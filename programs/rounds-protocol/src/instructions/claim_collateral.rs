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

/// Account validation struct for claim_collateral.
///
/// Called by each member independently after the circle
/// reaches Completed or Cancelled state.
///
/// Every member calls this for themselves — there is no
/// batch claim. Each member's claimable amount is calculated
/// from their own CollateralRecord:
///   claimable = total_locked - total_slashed
///
/// Members who never defaulted receive their full original
/// collateral. Members who defaulted K times receive their
/// collateral minus K × contribution_amount. Members who
/// were kicked receive zero — their collateral was fully
/// consumed by default deductions.
///
/// Position N members locked zero collateral at join time.
/// They can still call this instruction — it simply
/// transfers zero and marks their record as claimed.
/// This keeps the protocol state consistent.
#[derive(Accounts)]
pub struct ClaimCollateral<'info> {

    /// The claiming member. Must be the owner of the
    /// MemberAccount being claimed against.
    #[account(mut)]
    pub member: Signer<'info>,

    /// ProtocolConfig — checked for is_paused.
    #[account(
        seeds = [b"config"],
        bump = protocol_config.bump,
        constraint = !protocol_config.is_paused
            @ RoundsError::ProtocolPaused,
    )]
    pub protocol_config: Box<Account<'info, ProtocolConfig>>,

    /// CircleAccount — must be Completed or Cancelled.
    /// No other state allows collateral claims.
    #[account(
        seeds = [
            b"circle",
            circle_account.contribution_amount.to_le_bytes().as_ref(),
            &[circle_account.total_members],
            &[circle_account.frequency.clone() as u8],
            circle_account.usdc_mint.as_ref(),
        ],
        bump = circle_account.bump,
        constraint = (
            circle_account.state == CircleState::Completed ||
            circle_account.state == CircleState::Cancelled
        ) @ RoundsError::CircleNotComplete,
    )]
    pub circle_account: Box<Account<'info, CircleAccount>>,

    /// MemberAccount — validated as belonging to this member
    /// in this circle. Confirms this wallet is a legitimate
    /// participant with a collateral record to claim.
    #[account(
        seeds = [
            b"member",
            circle_account.key().as_ref(),
            member.key().as_ref(),
        ],
        bump = member_account.bump,
        constraint = member_account.member == member.key()
            @ RoundsError::UnauthorizedClaim,
        constraint = member_account.circle == circle_account.key()
            @ RoundsError::UnauthorizedClaim,
    )]
    pub member_account: Box<Account<'info, MemberAccount>>,

    /// CollateralRecord — the permanent audit trail.
    /// Provides total_locked and total_slashed for the
    /// claimable amount calculation.
    /// claimed flag set to true here to prevent double-claim.
    #[account(
        mut,
        seeds = [
            b"colrec",
            circle_account.key().as_ref(),
            member.key().as_ref(),
        ],
        bump = collateral_record.bump,
        constraint = !collateral_record.claimed
            @ RoundsError::AlreadyClaimed,
    )]
    pub collateral_record: Box<Account<'info, CollateralRecord>>,

    /// CollateralVault — source of the collateral return.
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
    pub collateral_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    /// Member's USDC token account — receives the collateral.
    /// Validated against the circle's configured mint.
    #[account(
        mut,
        token::mint          = usdc_mint,
        token::authority     = member,
        token::token_program = token_program,
    )]
    pub member_token_account: Box<InterfaceAccount<'info, TokenAccount>>,

    /// USDC mint — required by transfer_checked.
    pub usdc_mint: Box<InterfaceAccount<'info, Mint>>,

    pub token_program:  Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
}

/// claim_collateral
///
/// Returns locked collateral to the calling member after
/// the circle has completed or been cancelled.
///
/// Claimable amount = total_locked - total_slashed
///
/// This is always a clean multiple of contribution_amount
/// because every deduction in process_default is exactly
/// one contribution_amount. No partial amounts are possible.
///
/// The instruction succeeds even if claimable is zero —
/// it marks the record as claimed and exits cleanly.
/// This handles position N members (zero collateral) and
/// fully kicked members without reverting.
pub fn handler(ctx: Context<ClaimCollateral>) -> Result<()> {

    // Capture values before mutable borrows
    let total_locked     = ctx.accounts.collateral_record.total_locked;
    let total_slashed    = ctx.accounts.collateral_record.total_slashed;
    let contribution_amount = ctx.accounts.circle_account.contribution_amount;
    let total_members    = ctx.accounts.circle_account.total_members;
    let decimals         = ctx.accounts.usdc_mint.decimals;

    // ── Calculate claimable amount ─────────────────────────
    // total_locked was set once at join_circle time.
    // total_slashed accumulated via process_default calls.
    // The difference is what this member is owed back.
    //
    // For a member who never defaulted:
    //   claimable = total_locked - 0 = full original collateral
    //
    // For a member who defaulted K times:
    //   claimable = total_locked - (K × contribution_amount)
    //
    // For a kicked member:
    //   total_slashed = total_locked → claimable = 0
    //
    // For position N (zero collateral):
    //   total_locked = 0 → claimable = 0
    let claimable: u64 = total_locked
        .checked_sub(total_slashed)
        .ok_or(RoundsError::MathUnderflow)?;

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

    // ── Transfer collateral back to member ─────────────────
    // Only execute the transfer if there is something to send.
    // Position N and kicked members skip the transfer but
    // still get their record marked as claimed below.
    if claimable > 0 {
        transfer_checked(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.key(),
                TransferChecked {
                    from:      ctx.accounts.collateral_vault.to_account_info(),
                    mint:      ctx.accounts.usdc_mint.to_account_info(),
                    to:        ctx.accounts.member_token_account.to_account_info(),
                    authority: ctx.accounts.circle_account.to_account_info(),
                },
                &[circle_signer_seeds],
            ),
            claimable,
            decimals,
        )?;
    }

    // ── Update CollateralRecord ────────────────────────────
    // Mark as claimed — prevents double-claiming.
    // Set total_released to the claimable amount for the
    // full audit trail: locked = released + slashed.
    let col_rec            = &mut ctx.accounts.collateral_record;
    col_rec.total_released = claimable;
    col_rec.claimed        = true;

    msg!(
        "Collateral claimed. Total locked: {}. Total slashed: {}. \
         Returned: {}. Member: {}.",
        total_locked,
        total_slashed,
        claimable,
        ctx.accounts.member.key(),
    );

    Ok(())
}