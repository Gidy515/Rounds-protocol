use anchor_lang::prelude::*;
use anchor_spl::token_interface::{
    Mint, TokenAccount, TokenInterface,
    TransferChecked, transfer_checked,
};

use crate::errors::RoundsError;
use crate::state::{
    CircleAccount, CircleState, CollateralRecord,
    MemberAccount, PaymentRecord, ProtocolConfig,
};

/// Account validation struct for process_default.
///
/// Permissionless keeper instruction.
/// Any wallet can call this once the cycle deadline slot
/// has passed and a specific member has not paid.
///
/// This mirrors how liquidation bots work in Aave and Kamino —
/// the protocol does not care who calls it. The eligibility
/// conditions are enforced entirely by the smart contract.
/// If conditions are not met the transaction reverts.
/// If conditions are met the default is processed regardless
/// of who submitted the transaction.
///
/// No grace period. Consistent with standard lending protocol
/// behaviour. Collateral deduction fires the instant the
/// deadline slot is confirmed passed onchain.
#[derive(Accounts)]
pub struct ProcessDefault<'info> {

    /// The caller — any wallet. No special permissions.
    /// Does not pay rent — no new accounts are created here.
    pub caller: Signer<'info>,

    /// ProtocolConfig — checked for is_paused.
    #[account(
        seeds = [b"config"],
        bump = protocol_config.bump,
        constraint = !protocol_config.is_paused
            @ RoundsError::ProtocolPaused,
    )]
    pub protocol_config: Account<'info, ProtocolConfig>,

    /// CircleAccount — must be Active.
    /// active_members decremented if member is kicked.
    #[account(
        mut,
        seeds = [
            b"circle",
            circle_account.contribution_amount.to_le_bytes().as_ref(),
            &[circle_account.total_members],
            &[circle_account.frequency.clone() as u8],
            circle_account.usdc_mint.as_ref(),
        ],
        bump = circle_account.bump,
        constraint = circle_account.state == CircleState::Active
            @ RoundsError::CircleNotActive,
    )]
    pub circle_account: Account<'info, CircleAccount>,

    /// Defaulting member's MemberAccount.
    /// Seeds: [b"member", circle_account, defaulter]
    /// Must not already be kicked.
    #[account(
        mut,
        seeds = [
            b"member",
            circle_account.key().as_ref(),
            defaulter.key().as_ref(),
        ],
        bump = defaulter_member_account.bump,
        constraint = !defaulter_member_account.is_kicked
            @ RoundsError::MemberAlreadyKicked,
    )]
    pub defaulter_member_account: Account<'info, MemberAccount>,

    /// Defaulting member's CollateralRecord.
    /// total_slashed incremented here.
    #[account(
        mut,
        seeds = [
            b"colrec",
            circle_account.key().as_ref(),
            defaulter.key().as_ref(),
        ],
        bump = defaulter_collateral_record.bump,
    )]
    pub defaulter_collateral_record: Account<'info, CollateralRecord>,

    /// PaymentRecord for the current cycle.
    /// Defaulter's bit is set here so disburse_pot
    /// sees the cycle as fully covered and can proceed.
    #[account(
        mut,
        seeds = [
            b"payment",
            circle_account.key().as_ref(),
            &[circle_account.current_cycle],
        ],
        bump = payment_record.bump,
    )]
    pub payment_record: Account<'info, PaymentRecord>,

    /// CollateralVault — source of the seized collateral.
    /// Seeds: [b"collateral_vault", circle_account]
    /// Authority: circle_account PDA.
    #[account(
        mut,
        seeds = [b"collateral_vault", circle_account.key().as_ref()],
        bump,
        token::mint          = usdc_mint,
        token::authority     = circle_account,
        token::token_program = token_program,
    )]
    pub collateral_vault: InterfaceAccount<'info, TokenAccount>,

    /// PotVault — receives the seized collateral amount.
    /// Seeds: [b"pot_vault", circle_account]
    /// Authority: circle_account PDA.
    #[account(
        mut,
        seeds = [b"pot_vault", circle_account.key().as_ref()],
        bump,
        token::mint          = usdc_mint,
        token::authority     = circle_account,
        token::token_program = token_program,
    )]
    pub pot_vault: InterfaceAccount<'info, TokenAccount>,

    /// The defaulting member's wallet.
    /// Used only for PDA seed validation.
    /// No funds go to or from this account in this instruction.
    /// CHECK: identity verified via MemberAccount PDA seeds.
    pub defaulter: UncheckedAccount<'info>,

    /// USDC mint — required by transfer_checked.
    pub usdc_mint: InterfaceAccount<'info, Mint>,

    pub token_program: Interface<'info, TokenInterface>,
}

/// process_default
///
/// Called permissionlessly after a cycle deadline passes
/// without a specific member paying their contribution.
///
/// Deducts exactly one contribution_amount from the
/// defaulter's locked collateral and transfers it to
/// the PotVault to cover their missed payment.
///
/// Sets the defaulter's bit in the PaymentRecord bitmask
/// so disburse_pot sees the cycle as fully covered.
///
/// Kick check: if collateral reaches zero and rounds
/// remain the member is permanently removed. The circle
/// restructures to N-minus-1 active members and continues.
///
/// The circle never pauses, never skips a cycle, and
/// remaining members are completely unaffected.
pub fn handler(ctx: Context<ProcessDefault>) -> Result<()> {

    // Capture values before mutable borrows
    let contribution_amount  = ctx.accounts.circle_account.contribution_amount;
    let current_cycle        = ctx.accounts.circle_account.current_cycle;
    let cycle_deadline_slot  = ctx.accounts.circle_account.cycle_deadline_slot;
    let active_members       = ctx.accounts.circle_account.active_members;
    let total_members        = ctx.accounts.circle_account.total_members;
    let position             = ctx.accounts.defaulter_member_account.position;
    let collateral_locked    = ctx.accounts.defaulter_member_account.collateral_locked;
    let decimals             = ctx.accounts.usdc_mint.decimals;

    let clock = Clock::get()?;

    // ── Check 1: Deadline has passed ──────────────────────
    // The cycle deadline must have passed before default
    // processing can be triggered. If the deadline has not
    // passed the member still has time to pay normally via
    // pay_contribution.
    require!(
        clock.slot > cycle_deadline_slot,
        RoundsError::DeadlineNotPassed
    );

    // ── Check 2: Member has not paid this cycle ────────────
    // Verify the defaulter's bit in the PaymentRecord
    // bitmask is still 0 — meaning they have not paid.
    // If their bit is already set they either paid or were
    // already processed as a defaulter this cycle.
    let bit: u64 = 1u64
        .checked_shl((position - 1) as u32)
        .ok_or(RoundsError::MathOverflow)?;

    require!(
        ctx.accounts.payment_record.paid_flags & bit == 0,
        RoundsError::MemberAlreadyPaid
    );

    // ── Check 3: Member has collateral to seize ────────────
    // Collateral must be sufficient to cover exactly one
    // contribution_amount. Because collateral is always a
    // clean multiple of contribution_amount this check
    // should always pass for non-kicked members — but we
    // verify explicitly as a safety invariant.
    require!(
        collateral_locked >= contribution_amount,
        RoundsError::InvariantViolation
    );

    // ── Seize exactly one contribution_amount ─────────────
    // Deduct from MemberAccount live state.
    let new_collateral_locked = collateral_locked
        .checked_sub(contribution_amount)
        .ok_or(RoundsError::MathUnderflow)?;

    ctx.accounts.defaulter_member_account.collateral_locked = new_collateral_locked;
    ctx.accounts.defaulter_member_account.is_defaulted      = true;

    // ── Update CollateralRecord audit trail ────────────────
    ctx.accounts.defaulter_collateral_record.total_slashed =
        ctx.accounts.defaulter_collateral_record.total_slashed
            .checked_add(contribution_amount)
            .ok_or(RoundsError::MathOverflow)?;

    // ── PDA signer seeds for CollateralVault transfers ─────
    // CollateralVault authority is circle_account PDA.
    let contribution_amount_bytes = contribution_amount.to_le_bytes();
    let total_members_byte        = [total_members];
    let frequency_byte            = [ctx.accounts.circle_account.frequency.clone() as u8];
    let circle_bump               = [ctx.accounts.circle_account.bump];

    let circle_signer_seeds: &[&[u8]] = &[
        b"circle",
        contribution_amount_bytes.as_ref(),
        total_members_byte.as_ref(),
        frequency_byte.as_ref(),
        ctx.accounts.circle_account.usdc_mint.as_ref(),
        circle_bump.as_ref(),
    ];

    // ── Transfer seized amount: CollateralVault → PotVault ─
    // The seized contribution_amount covers the defaulter's
    // missed payment for this cycle. The pot is now complete
    // and disburse_pot can proceed normally.
    transfer_checked(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.key(),
            TransferChecked {
                from:      ctx.accounts.collateral_vault.to_account_info(),
                mint:      ctx.accounts.usdc_mint.to_account_info(),
                to:        ctx.accounts.pot_vault.to_account_info(),
                authority: ctx.accounts.circle_account.to_account_info(),
            },
            &[circle_signer_seeds],
        ),
        contribution_amount,
        decimals,
    )?;

    // ── Set defaulter's bit in PaymentRecord ───────────────
    // Marks this member's slot as covered for this cycle.
    // disburse_pot checks all bits — this ensures the cycle
    // can disburse without waiting for the defaulter to pay.
    ctx.accounts.payment_record.paid_flags |= bit;

    // ── Kick check ─────────────────────────────────────────
    // A member is kicked when BOTH conditions are true:
    // 1. Their collateral has reached zero
    // 2. Rounds still remain after this cycle
    //
    // If collateral reaches zero but this is the final cycle
    // no kick occurs — the circle is about to complete anyway.
    //
    // rounds_remaining = total payout rounds left after this
    // cycle. We use active_members as the total since kicked
    // members no longer count toward the payout schedule.
    let rounds_remaining = active_members
        .checked_sub(current_cycle)
        .unwrap_or(0);

    if new_collateral_locked == 0 && rounds_remaining > 0 {

        // ── Kick the member ────────────────────────────────
        ctx.accounts.defaulter_member_account.is_kicked = true;

        // ── Decrement active member count ──────────────────
        // This shrinks the expected bitmask in future cycles
        // and reduces the pot size calculated by disburse_pot.
        ctx.accounts.circle_account.active_members =
            ctx.accounts.circle_account.active_members
                .checked_sub(1)
                .ok_or(RoundsError::MathUnderflow)?;

        msg!(
            "Member at position {} has been kicked. \
             Collateral exhausted. Active members now: {}.",
            position,
            ctx.accounts.circle_account.active_members,
        );

    } else {

        msg!(
            "Default processed for position {}. \
             Collateral remaining: {}. Rounds remaining: {}.",
            position,
            new_collateral_locked,
            rounds_remaining,
        );
    }

    msg!(
        "Cycle {} covered for position {} via collateral seizure. \
         Amount seized: {}.",
        current_cycle,
        position,
        contribution_amount,
    );

    Ok(())
}