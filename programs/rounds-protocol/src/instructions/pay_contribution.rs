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

/// Account validation struct for pay_contribution.
///
/// Called by members for cycles 2 through N.
/// Cycle 1 is prefunded at join_circle time — this
/// instruction is never called for cycle 1.
///
/// Accepts exactly contribution_amount from the member.
/// No premium. No collateral. Clean and simple.
#[derive(Accounts)]
pub struct PayContribution<'info> {

    /// The paying member. Must be an active member
    /// of this circle who has not been kicked.
    #[account(mut)]
    pub member: Signer<'info>,

    /// ProtocolConfig — checked for is_paused.
    #[account(
        seeds = [b"config"],
        bump = protocol_config.bump,
        constraint = !protocol_config.is_paused
            @ RoundsError::ProtocolPaused,
    )]
    pub protocol_config: Account<'info, ProtocolConfig>,

    /// CircleAccount — must be Active.
    /// Deadline slot must not have passed.
    #[account(
        seeds = [
            b"circle",
            circle_account.contribution_amount.to_le_bytes().as_ref(),
            &[circle_account.total_members],
            &[circle_account.frequency.clone() as u8],
            circle_account.usdc_mint.as_ref(),
            &[circle_account.nonce],
        ],
        bump = circle_account.bump,
        constraint = circle_account.state == CircleState::Active
            @ RoundsError::CircleNotActive,
    )]
    pub circle_account: Box<Account<'info, CircleAccount>>,

    /// MemberAccount — validated as belonging to this member
    /// in this circle. Member must not be kicked.
    #[account(
        seeds = [
            b"member",
            circle_account.key().as_ref(),
            member.key().as_ref(),
        ],
        bump = member_account.bump,
        constraint = member_account.member == member.key()
            @ RoundsError::UnauthorizedMember,
        constraint = !member_account.is_kicked
            @ RoundsError::MemberKicked,
    )]
    pub member_account: Account<'info, MemberAccount>,

    /// PaymentRecord for the current cycle.
    /// Seeds: [b"payment", circle_account, current_cycle]
    /// Bitmask updated here to mark this member as paid.
    #[account(
        mut,
        seeds = [
            b"payment",
            circle_account.key().as_ref(),
            &[circle_account.current_cycle],
        ],
        bump = payment_record.bump,

        constraint = payment_record.circle == circle_account.key()
        @ RoundsError::InvalidPaymentRecord,

        constraint = payment_record.cycle == circle_account.current_cycle
        @ RoundsError::InvalidCycle,
    )]
    pub payment_record: Account<'info, PaymentRecord>,

    /// Member's USDC token account.
    /// Source of the contribution payment.
    /// Validated against the circle's configured mint.
    #[account(
        mut,
        token::mint          = usdc_mint,
        token::authority     = member,
        token::token_program = token_program,
    )]
    pub member_token_account: InterfaceAccount<'info, TokenAccount>,

    /// PotVault — receives the contribution.
    /// Seeds: [b"pot_vault", circle_account]
    #[account(
        mut,
        seeds = [b"pot_vault", circle_account.key().as_ref()],
        bump,
        token::mint          = usdc_mint,
        token::authority     = circle_account,
        token::token_program = token_program,
    )]
    pub pot_vault: InterfaceAccount<'info, TokenAccount>,

    /// USDC mint — required by transfer_checked.
    pub usdc_mint: InterfaceAccount<'info, Mint>,

    pub token_program: Interface<'info, TokenInterface>,
}

/// pay_contribution
///
/// Accepts exactly contribution_amount from the calling member
/// and transfers it to the PotVault. Updates the PaymentRecord
/// bitmask to mark this member as paid for the current cycle.
///
/// Called for cycles 2 through N only.
/// Cycle 1 was prefunded at join_circle time.
///
/// Rejects payment if:
/// - Circle is not Active
/// - Deadline slot has passed (use process_default instead)
/// - Member has already paid this cycle
/// - Member has been kicked
pub fn handler(ctx: Context<PayContribution>) -> Result<()> {

    // Capture values before mutable borrow
    let contribution_amount = ctx.accounts.circle_account.contribution_amount;
    let cycle_deadline_slot = ctx.accounts.circle_account.cycle_deadline_slot;
    let current_cycle       = ctx.accounts.circle_account.current_cycle;
    let decimals            = ctx.accounts.usdc_mint.decimals;
    let position            = ctx.accounts.member_account.position;

     // ── Validate PaymentRecord integrity ────────────────

     require!(
        ctx.accounts.payment_record.cycle
            == ctx.accounts.circle_account.current_cycle,
        RoundsError::InvalidCycle
    );

    require!(
        ctx.accounts.payment_record.circle
            == ctx.accounts.circle_account.key(),
        RoundsError::InvalidPaymentRecord
    );

    let clock = Clock::get()?;

    // ── Check 1: Deadline has not passed ──────────────────
    // If the deadline has passed this member is a defaulter.
    // They cannot pay through this instruction.
    // process_default must be called first to handle the
    // missed cycle before any further payments are accepted.
    require!(
        clock.slot <= cycle_deadline_slot,
        RoundsError::DeadlinePassed
    );

    // ── Check 2: Current cycle is not cycle 1 ─────────────
    // Cycle 1 was prefunded at join time. Calling
    // pay_contribution for cycle 1 would double-pay.
    require!(
        current_cycle > 1,
        RoundsError::Cycle1AlreadyFunded
    );  // Cycle 1 was already paid at join time

    // ── Check 3: Member has not already paid this cycle ────
    // Check the bitmask. Bit at (position - 1) must be 0.
    // position 1 → bit 0
    // position 2 → bit 1
    // position N → bit N-1
    let bit: u64 = 1u64
        .checked_shl((position - 1) as u32)
        .ok_or(RoundsError::MathOverflow)?;

    require!(
        ctx.accounts.payment_record.paid_flags & bit == 0,
        RoundsError::AlreadyPaid
    );

    // ── Transfer contribution → PotVault ──────────────────
    // Exact contribution_amount. No premium. No collateral.
    // Rejects if member's balance is insufficient —
    // transfer_checked will error naturally in that case.
    transfer_checked(
        CpiContext::new(
            ctx.accounts.token_program.key(),
            TransferChecked {
                from:      ctx.accounts.member_token_account.to_account_info(),
                mint:      ctx.accounts.usdc_mint.to_account_info(),
                to:        ctx.accounts.pot_vault.to_account_info(),
                authority: ctx.accounts.member.to_account_info(),
            },
        ),
        contribution_amount,
        decimals,
    )?;

    // ── Update PaymentRecord bitmask ───────────────────────
    // Set the member's bit to 1 — marks them as paid.
    // When all active member bits are set disburse_pot
    // can fire for this cycle.
    ctx.accounts.payment_record.paid_flags |= bit;

    msg!(
        "Member at position {} paid cycle {}. Amount: {}.",
        position,
        current_cycle,
        contribution_amount,
    );

    Ok(())
}