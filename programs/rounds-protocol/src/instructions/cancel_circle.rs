use anchor_lang::prelude::*;
use anchor_spl::token_interface::{
    Mint, TokenAccount, TokenInterface,
    TransferChecked, transfer_checked,
};

use crate::errors::RoundsError;
use crate::state::{
    CircleAccount, CircleState, CollateralRecord,
    MemberAccount, ProtocolConfig,
};
use crate::constants::*;

/// Account validation struct for cancel_circle.
///
/// Called by any member who joined the circle.
/// Atomically:
///   1. Validates cancellation conditions
///   2. Transitions circle to Cancelled state
///   3. Returns caller's locked collateral from CollateralVault
///   4. Returns caller's first round contribution from PotVault
///
/// Two valid cancellation conditions:
///   A. Only position 1 has joined (current_members == 1)
///      — creator can cancel immediately, no wait required
///   B. 24-hour cancel deadline has passed and circle is not full
///      — any joined member can trigger cancellation
///
/// Only joined members can call this instruction.
/// Each member calls it once for themselves.
/// Premium payments already received by position 1 are
/// non-refundable — they transferred directly to position 1's
/// wallet at join time and cannot be clawed back.
#[derive(Accounts)]
pub struct CancelCircle<'info> {

    /// The caller — must be a joined member of this circle.
    /// Receives their collateral and contribution back.
    #[account(mut)]
    pub caller: Signer<'info>,

    /// ProtocolConfig — checked for is_paused.
    #[account(
        seeds = [b"config"],
        bump = protocol_config.bump,
        constraint = !protocol_config.is_paused
            @ RoundsError::ProtocolPaused,
    )]
    pub protocol_config: Account<'info, ProtocolConfig>,

    /// CircleAccount — must be Open or already Cancelled.
    /// Open: first caller transitions it to Cancelled.
    /// Cancelled: subsequent callers can still recover funds.
    ///
    /// Cancellation conditions checked in handler —
    /// not in account constraints because they involve
    /// multiple conditions with different logic paths.
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
        constraint = (
            circle_account.state == CircleState::Open ||
            circle_account.state == CircleState::Cancelled
        ) @ RoundsError::CircleNotOpen,
    )]
    pub circle_account: Account<'info, CircleAccount>,

    /// Caller's MemberAccount — proves they are a legitimate
    /// joined member of this circle.
    /// Seeds validate both circle and member pubkey.
    /// claimed flag checked and set to prevent double-refund.
    #[account(
        seeds = [
            b"member",
            circle_account.key().as_ref(),
            caller.key().as_ref(),
        ],
        bump = caller_member_account.bump,
        constraint = caller_member_account.member == caller.key()
            @ RoundsError::NotAMember,
        constraint = caller_member_account.circle == circle_account.key()
            @ RoundsError::NotAMember,
    )]
    pub caller_member_account: Account<'info, MemberAccount>,

    /// Caller's CollateralRecord — used to calculate
    /// claimable collateral and prevent double-refund.
    /// claimed flag set here atomically with the transfer.
    #[account(
        mut,
        seeds = [
            b"colrec",
            circle_account.key().as_ref(),
            caller.key().as_ref(),
        ],
        bump = caller_collateral_record.bump,
        constraint = !caller_collateral_record.claimed
            @ RoundsError::AlreadyClaimed,
    )]
    pub caller_collateral_record: Account<'info, CollateralRecord>,

    /// CollateralVault — returns caller's locked collateral.
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

    /// PotVault — returns caller's first round contribution.
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

    /// Caller's USDC token account — receives all returned funds.
    /// Validated against the circle's configured mint.
    #[account(
        mut,
        token::mint          = usdc_mint,
        token::authority     = caller,
        token::token_program = token_program,
    )]
    pub caller_token_account: InterfaceAccount<'info, TokenAccount>,

    /// USDC mint — required by transfer_checked.
    pub usdc_mint: InterfaceAccount<'info, Mint>,

    pub token_program:  Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
}

/// cancel_circle
///
/// Validates cancellation conditions, transitions the circle
/// to Cancelled state if not already cancelled, then returns
/// the caller's locked collateral and first round contribution
/// atomically in a single transaction.
///
/// Valid cancellation conditions:
///   A. current_members == 1 (only creator joined)
///   B. cancel_deadline_slot passed AND circle not full
///
/// Security guarantees:
///   - Only joined members can call this
///   - Each member can only recover funds once
///   - Collateral and contribution returned in one atomic tx
///   - Circle state set to Cancelled on first valid call
///   - Subsequent callers recover their own funds from
///     the already-Cancelled circle without re-checking
///     cancellation conditions
///   - Premium received by position 1 is non-refundable
///     (transferred directly at join time, not held in vault)
pub fn handler(ctx: Context<CancelCircle>) -> Result<()> {

    // Capture all values before mutable borrows
    let contribution_amount  = ctx.accounts.circle_account.contribution_amount;
    let total_members        = ctx.accounts.circle_account.total_members;
    let current_members      = ctx.accounts.circle_account.current_members;
    let cancel_deadline_slot = ctx.accounts.circle_account.cancel_deadline_slot;
    let total_locked         = ctx.accounts.caller_collateral_record.total_locked;
    let total_slashed        = ctx.accounts.caller_collateral_record.total_slashed;
    let current_state        = ctx.accounts.circle_account.state.clone();
    let decimals             = ctx.accounts.usdc_mint.decimals;

    let clock = Clock::get()?;

    // ── Step 1: Validate cancellation conditions ───────────
    // Only run this check if the circle is still Open.
    // If it is already Cancelled a previous caller already
    // validated and transitioned it — skip re-validation.
    if current_state == CircleState::Open {

        // Condition A: only position 1 has joined.
        // Creator can cancel immediately — no time requirement.
        let only_creator_joined = current_members == 1;

        // Condition B: cancel deadline has passed and
        // the circle never filled completely.
        let deadline_passed_unfilled =
            clock.slot > cancel_deadline_slot &&
            current_members < total_members;

        // At least one condition must be true.
        require!(
            only_creator_joined || deadline_passed_unfilled,
            RoundsError::CancelWindowNotPassed
        );

        // ── Transition circle to Cancelled ─────────────────
        ctx.accounts.circle_account.state = CircleState::Cancelled;

        msg!(
            "Circle cancelled. Condition: {}. Members joined: {}/{}.",
            if only_creator_joined { "solo creator" } else { "deadline elapsed" },
            current_members,
            total_members,
        );
    }

    // ── Step 2: Calculate collateral refund ───────────────
    // claimable = total_locked - total_slashed
    // For a cancelled circle total_slashed is always 0
    // because process_default can only fire on Active circles.
    // But we use the formula consistently for correctness.
    let collateral_refund: u64 = total_locked
        .checked_sub(total_slashed)
        .ok_or(RoundsError::MathUnderflow)?;

    // ── Step 3: PDA signer seeds for vault transfers ───────
    // Both vaults are authorised by the circle_account PDA.
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

    let caller_position = ctx.accounts.caller_member_account.position;

    // ── Step 4: Return collateral → caller ────────────────
    // Position N members locked zero collateral — skip.
    // All other members get their full collateral back
    // because process_default never fires on Open circles.
    if collateral_refund > 0 {
        transfer_checked(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.key(),
                TransferChecked {
                    from:      ctx.accounts.collateral_vault.to_account_info(),
                    mint:      ctx.accounts.usdc_mint.to_account_info(),
                    to:        ctx.accounts.caller_token_account.to_account_info(),
                    authority: ctx.accounts.circle_account.to_account_info(),
                },
                &[circle_signer_seeds],
            ),
            collateral_refund,
            decimals,
        )?;
    }

    // ── Step 5: Return contribution + premium → caller ────
    // Positions 2-N paid contribution_amount + premium_amount
    // into PotVault at join time. Return the full amount.
    // Position 1 paid contribution_amount only (no premium).
    // The first_round_payment captures both cases correctly.
    let first_round_refund: u64 = if caller_position == 1 {
        contribution_amount
    } else {
        // contribution + premium
        contribution_amount
            .checked_add(
                contribution_amount
                    .checked_mul(PREMIUM_BPS)
                    .ok_or(RoundsError::MathOverflow)?
                    .checked_div(BPS_DENOMINATOR)
                    .ok_or(RoundsError::MathOverflow)?
            )
            .ok_or(RoundsError::MathOverflow)?
    };

    transfer_checked(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.key(),
            TransferChecked {
                from:      ctx.accounts.pot_vault.to_account_info(),
                mint:      ctx.accounts.usdc_mint.to_account_info(),
                to:        ctx.accounts.caller_token_account.to_account_info(),
                authority: ctx.accounts.circle_account.to_account_info(),
            },
            &[circle_signer_seeds],
        ),
        first_round_refund,
        decimals,
    )?;

    // ── Step 6: Update CollateralRecord ───────────────────
    // Mark as claimed — prevents double-refund.
    // Set total_released for complete audit trail.
    let col_rec            = &mut ctx.accounts.caller_collateral_record;
    col_rec.total_released = collateral_refund;
    col_rec.claimed        = true;

    msg!(
        "Funds returned to {}. Collateral: {}. Contribution+Premium: {}. Total: {}.",
        ctx.accounts.caller.key(),
        collateral_refund,
        first_round_refund,
        collateral_refund
            .checked_add(first_round_refund)
            .unwrap_or(0),
    );

    Ok(())
}