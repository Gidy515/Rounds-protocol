use anchor_lang::prelude::*;
use anchor_spl::token_interface::{
    Mint, TokenAccount, TokenInterface,
    TransferChecked, transfer_checked,
};

use crate::errors::RoundsError;
use crate::state::{
    CircleAccount, CircleState, MemberAccount,
    PaymentRecord, ProtocolConfig,
};

/// Account validation struct for disburse_pot.
///
/// Permissionless keeper instruction.
/// Anyone can call this once all active members have paid
/// for the current cycle.
///
/// Transfers the protocol fee to TreasuryVault then
/// transfers the net pot to the current cycle's recipient.
/// Advances the cycle counter and sets the next deadline.
/// On the final cycle transitions the circle to Completed.
#[derive(Accounts)]
pub struct DisbursePot<'info> {

    /// The caller — any wallet. No special permissions.
    /// Does not pay rent — no new accounts created here.
    pub caller: Signer<'info>,

    /// ProtocolConfig — checked for is_paused.
    /// Also provides protocol_fee_bps for fee calculation.
    #[account(
        seeds = [b"config"],
        bump = protocol_config.bump,
        constraint = !protocol_config.is_paused
            @ RoundsError::ProtocolPaused,
    )]
    pub protocol_config: Box<Account<'info, ProtocolConfig>>,

    /// CircleAccount — must be Active.
    /// current_cycle incremented here.
    /// cycle_deadline_slot updated here.
    /// State transitions to Completed on final cycle.
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
    pub circle_account: Box<Account<'info, CircleAccount>>,

    /// PaymentRecord for the current cycle.
    /// All active member bits must be set before
    /// disbursement can proceed.
    #[account(
        seeds = [
            b"payment",
            circle_account.key().as_ref(),
            &[circle_account.current_cycle],
        ],
        bump = payment_record.bump,
    )]
    pub payment_record: Box<Account<'info, PaymentRecord>>,

    /// Recipient MemberAccount.
    /// Position must match current_cycle.
    /// Must not have already received the pot.
    #[account(
        mut,
        seeds = [
            b"member",
            circle_account.key().as_ref(),
            recipient.key().as_ref(),
        ],
        bump = recipient_member_account.bump,
        constraint = recipient_member_account.position
            == circle_account.current_cycle
            @ RoundsError::WrongRecipient,
        constraint = !recipient_member_account.has_received_pot
            @ RoundsError::AlreadyDisbursed,
        constraint = !recipient_member_account.is_kicked
            @ RoundsError::MemberKicked,
    )]
    pub recipient_member_account: Box<Account<'info, MemberAccount>>,

    /// Recipient wallet — receives the net pot.
    /// Validated via recipient_member_account.member check
    /// in the handler to ensure the correct wallet is paid.
    /// CHECK: verified in handler against
    /// recipient_member_account.member
    #[account(mut)]
    pub recipient: SystemAccount<'info>,

    /// Recipient's USDC token account.
    /// Net pot lands here. Validated against circle mint.
    #[account(
        mut,
        token::mint          = usdc_mint,
        token::authority     = recipient,
        token::token_program = token_program,
    )]
    pub recipient_token_account: Box<InterfaceAccount<'info, TokenAccount>>,

    /// PotVault — source of the disbursement.
    /// Seeds: [b"pot_vault", circle_account]
    /// Balance should equal active_members × contribution_amount
    /// at this point.
    #[account(
        mut,
        seeds = [b"pot_vault", circle_account.key().as_ref()],
        bump,
        token::mint          = usdc_mint,
        token::authority     = circle_account,
        token::token_program = token_program,
    )]
    pub pot_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    /// TreasuryVault — receives the protocol fee.
    /// Seeds: [b"treasury", protocol_config]
    #[account(
        mut,
        seeds = [
            b"treasury",
            protocol_config.key().as_ref(),
        ],
        bump,
        token::mint          = usdc_mint,
        token::authority     = protocol_config,
        token::token_program = token_program,
    )]
    pub treasury_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    /// USDC mint — required by transfer_checked.
    pub usdc_mint: Box<InterfaceAccount<'info, Mint>>,

    pub token_program:  Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
}

/// disburse_pot
///
/// Verifies all active members have paid for the current cycle,
/// deducts the protocol fee, transfers the net pot to the
/// designated cycle recipient, then advances the circle state.
///
/// On the final cycle:
///   - Sets completed_at_slot
///   - Transitions state to Completed
///
/// On all other cycles:
///   - Increments current_cycle
///   - Sets the next cycle_deadline_slot
///   - Initialises the PaymentRecord for the next cycle
pub fn handler(ctx: Context<DisbursePot>) -> Result<()> {

    // Capture all values before mutable borrows
    //let circle_key          = ctx.accounts.circle_account.key();
    let active_members      = ctx.accounts.circle_account.active_members;
    let contribution_amount = ctx.accounts.circle_account.contribution_amount;
    let current_cycle       = ctx.accounts.circle_account.current_cycle;
    let cycle_duration_slots = ctx.accounts.circle_account.cycle_duration_slots;
    let protocol_fee_bps    = ctx.accounts.protocol_config.protocol_fee_bps as u64;
    let decimals            = ctx.accounts.usdc_mint.decimals;
    let recipient_key       = ctx.accounts.recipient_member_account.member;

    // ── Check 1: Verify recipient wallet matches record ────
    // Ensures the recipient account passed in actually
    // belongs to the member designated for this cycle.
    require!(
        ctx.accounts.recipient.key() == recipient_key,
        RoundsError::WrongRecipient
    );

    // ── Check 2: Verify all active members have paid ───────
    // Build the expected bitmask for all active members.
    // Every bit from 0 to active_members-1 must be set.
    // Kicked members have their bits pre-set by process_default
    // so they do not block disbursement.
    let expected_mask: u64 = if active_members == 64 {
        u64::MAX
    } else {
        (1u64 << active_members)
            .checked_sub(1)
            .ok_or(RoundsError::MathOverflow)?
    };

    require!(
        ctx.accounts.payment_record.paid_flags & expected_mask == expected_mask,
        RoundsError::NotAllMembersPaid
    );

    // ── Calculate pot total and fee ────────────────────────
    // Pot total = what all active members contributed.
    // This is what should be sitting in PotVault right now.
    //let pot_total: u64 = (active_members as u64)
      //  .checked_mul(contribution_amount)
       // .ok_or(RoundsError::MathOverflow)?;

        // ── Calculate pot total and fee ────────────────────────
    // For cycle 1: PotVault holds contributions + premiums
    // from all members. Read the actual vault balance.
    //
    // For cycles 2-N: PotVault holds exactly
    // active_members × contribution_amount.
    // We compute this rather than reading the balance
    // to avoid any rounding or dust issues.
    let pot_total: u64 = if current_cycle == 1 {
        // Read actual vault balance — includes all premiums
        // deposited by positions 2-N at join time.
        ctx.accounts.pot_vault.amount
    } else {
        // Computed — always active_members × contribution_amount
        (active_members as u64)
            .checked_mul(contribution_amount)
            .ok_or(RoundsError::MathOverflow)?
    };   

    // Protocol fee deducted before recipient gets anything.
    // fee = pot_total * fee_bps / 10_000
    let fee_amount: u64 = pot_total
        .checked_mul(protocol_fee_bps)
        .ok_or(RoundsError::MathOverflow)?
        .checked_div(10_000)
        .ok_or(RoundsError::MathOverflow)?;

    // Net pot = everything the recipient actually receives.
    let net_pot: u64 = pot_total
        .checked_sub(fee_amount)
        .ok_or(RoundsError::MathUnderflow)?;

    // ── PDA signer seeds for PotVault transfers ────────────
    // PotVault authority is circle_account PDA.
    // We need its seeds to sign the CPI transfers.
    let contribution_amount_bytes = contribution_amount.to_le_bytes();
    let total_members_byte        = [ctx.accounts.circle_account.total_members];
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

    // ── Transfer fee → TreasuryVault ───────────────────────
    // Fee is taken first before any pot transfer.
    // If fee is zero (protocol_fee_bps = 0) skip transfer.
    if fee_amount > 0 {
        transfer_checked(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.key(),
                TransferChecked {
                    from:      ctx.accounts.pot_vault.to_account_info(),
                    mint:      ctx.accounts.usdc_mint.to_account_info(),
                    to:        ctx.accounts.treasury_vault.to_account_info(),
                    authority: ctx.accounts.circle_account.to_account_info(),
                },
                &[circle_signer_seeds],
            ),
            fee_amount,
            decimals,
        )?;
    }

    // ── Transfer net pot → recipient token account ─────────
    // Full remaining pot after fee deduction.
    // No deductions. No auto-locking. Every unit is theirs.
    transfer_checked(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.key(),
            TransferChecked {
                from:      ctx.accounts.pot_vault.to_account_info(),
                mint:      ctx.accounts.usdc_mint.to_account_info(),
                to:        ctx.accounts.recipient_token_account.to_account_info(),
                authority: ctx.accounts.circle_account.to_account_info(),
            },
            &[circle_signer_seeds],
        ),
        net_pot,
        decimals,
    )?;

    // ── Mark recipient as paid ─────────────────────────────
    ctx.accounts.recipient_member_account.has_received_pot = true;

    // ── Advance circle state ───────────────────────────────
    let circle = &mut ctx.accounts.circle_account;
    let clock   = Clock::get()?;

    // Check if this was the final cycle.
    // Final cycle = current_cycle equals active_members count.
    // After disbursement every active member has received
    // their pot so the circle is complete.
    if current_cycle >= active_members {

        // ── Final cycle → Completed ────────────────────────
        circle.state             = CircleState::Completed;
        circle.completed_at_slot = clock.slot;

        msg!(
            "Circle COMPLETED. Final cycle {} disbursed. \
             Pot: {}. Fee: {}. Net: {}. Completed at slot: {}.",
            current_cycle,
            pot_total,
            fee_amount,
            net_pot,
            circle.completed_at_slot,
        );

    } else {

        // ── Advance to next cycle ──────────────────────────
        circle.current_cycle = current_cycle
            .checked_add(1)
            .ok_or(RoundsError::MathOverflow)?;

        circle.cycle_deadline_slot = clock
            .slot
            .checked_add(cycle_duration_slots)
            .ok_or(RoundsError::MathOverflow)?;

        msg!(
            "Cycle {} disbursed. Pot: {}. Fee: {}. Net: {}. \
             Next cycle: {}. Next deadline slot: {}.",
            current_cycle,
            pot_total,
            fee_amount,
            net_pot,
            circle.current_cycle,
            circle.cycle_deadline_slot,
        );
    }

    Ok(())
}