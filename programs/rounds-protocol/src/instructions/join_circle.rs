use anchor_lang::prelude::*;
use anchor_spl::token_interface::{
    Mint, TokenAccount, TokenInterface,
    TransferChecked, transfer_checked,
};

use crate::constants::*;
use crate::errors::RoundsError;
use crate::state::{
    CircleAccount, CircleState, CollateralRecord,
    MemberAccount, ProtocolConfig,
};

/// Account validation struct for join_circle.
///
/// This is the most complex instruction in the protocol.
/// It handles position assignment, collateral calculation,
/// collateral transfer, first round contribution transfer,
/// premium routing to position 1, and MemberAccount +
/// CollateralRecord initialisation — all atomically.
///
/// The frontend must read CircleAccount before calling this
/// to show the member their position and total cost upfront.
/// Nothing is reserved until this transaction confirms.
#[derive(Accounts)]
pub struct JoinCircle<'info> {

    #[account(mut)]
    pub member: Signer<'info>,

    #[account(
        seeds = [b"config"],
        bump = protocol_config.bump,
        constraint = !protocol_config.is_paused
            @ RoundsError::ProtocolPaused,
    )]
    pub protocol_config: Box<Account<'info, ProtocolConfig>>,

    #[account(
        mut,
        seeds = [
            b"circle",
            circle_account.contribution_amount.to_le_bytes().as_ref(),
            &[circle_account.total_members],
            &[circle_account.frequency.clone() as u8],
            circle_account.usdc_mint.as_ref(),
            &[circle_account.nonce],
        ],
        bump = circle_account.bump,
        constraint = circle_account.state == CircleState::Open
            @ RoundsError::CircleNotOpen,
        constraint = circle_account.current_members < circle_account.total_members
            @ RoundsError::CircleFull,
    )]
    pub circle_account: Box<Account<'info, CircleAccount>>,

    #[account(
        init,
        payer = member,
        space = MemberAccount::LEN,
        seeds = [
            b"member",
            circle_account.key().as_ref(),
            member.key().as_ref(),
        ],
        bump,
    )]
    pub member_account: Box<Account<'info, MemberAccount>>,

    #[account(
        init,
        payer = member,
        space = CollateralRecord::LEN,
        seeds = [
            b"colrec",
            circle_account.key().as_ref(),
            member.key().as_ref(),
        ],
        bump,
    )]
    pub collateral_record: Box<Account<'info, CollateralRecord>>,

    #[account(
        mut,
        token::mint          = usdc_mint,
        token::authority     = member,
        token::token_program = token_program,
    )]
    pub member_token_account: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        mut,
        seeds = [b"collateral_vault", circle_account.key().as_ref()],
        bump,
        token::mint          = usdc_mint,
        token::authority     = circle_account,
        token::token_program = token_program,
    )]
    pub collateral_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        mut,
        seeds = [b"pot_vault", circle_account.key().as_ref()],
        bump,
        token::mint          = usdc_mint,
        token::authority     = circle_account,
        token::token_program = token_program,
    )]
    pub pot_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    pub usdc_mint: Box<InterfaceAccount<'info, Mint>>,

    pub system_program: Program<'info, System>,
    pub token_program:  Interface<'info, TokenInterface>,
}

/// join_circle
///
/// Assigns the next available position to the joining member,
/// calculates their exact payment requirement, validates their
/// balance, executes three atomic transfers, and initialises
/// their MemberAccount and CollateralRecord.
///
/// All three transfers happen in one transaction.
/// If any transfer fails the entire transaction reverts —
/// no partial state is ever written.
pub fn handler(ctx: Context<JoinCircle>) -> Result<()> {

    // Capture keys before any mutable borrows
    let circle_account_key = ctx.accounts.circle_account.key();

    let circle  = &mut ctx.accounts.circle_account;
    let decimals = ctx.accounts.usdc_mint.decimals;

    // ── Step 1: Assign position ────────────────────────────
    // Strictly by join order. First joiner = position 1.
    // current_members is 0 before the first join.
    let position: u8 = circle.current_members
        .checked_add(1)
        .ok_or(RoundsError::MathOverflow)?;

    // ── Step 2: Calculate collateral ──────────────────────
    // Remaining cycles after this member receives the pot.
    // Position 1 in a 10-member circle: (10 - 1) = 9 cycles
    // Position 5 in a 10-member circle: (10 - 5) = 5 cycles
    // Position 10 (last):               (10 - 10) = 0 cycles
    // Always a clean multiple of contribution_amount.
    let remaining_cycles = circle.total_members
        .checked_sub(position)
        .ok_or(RoundsError::MathUnderflow)? as u64;

    let collateral_amount: u64 = remaining_cycles
        .checked_mul(circle.contribution_amount)
        .ok_or(RoundsError::MathOverflow)?;

    // ── Step 3: Calculate premium ──────────────────────────
    // 10% of contribution_amount — hardcoded, not configurable.
    // Only positions 2-N pay the premium.
    // Position 1 pays zero premium (they receive it).
    let premium_amount: u64 = if position > 1 {
        circle.contribution_amount
            .checked_mul(PREMIUM_BPS)
            .ok_or(RoundsError::MathOverflow)?
            .checked_div(BPS_DENOMINATOR)
            .ok_or(RoundsError::MathOverflow)?
    } else {
        0
    };

    // ── Step 4: Calculate first round payment ─────────────
    // Position 1: contribution_amount only (no premium)
    // Position 2-N: contribution_amount + premium_amount
    let first_round_payment: u64 = circle.contribution_amount
        .checked_add(premium_amount)
        .ok_or(RoundsError::MathOverflow)?;

    // ── Step 5: Calculate total required from wallet ───────
    // Everything that leaves the member's wallet this tx.
    let total_required: u64 = collateral_amount
        .checked_add(first_round_payment)
        .ok_or(RoundsError::MathOverflow)?;

    // ── Step 6: Validate member has sufficient balance ─────
    require!(
        ctx.accounts.member_token_account.amount >= total_required,
        RoundsError::InsufficientBalance
    );

    // ── Step 7: Transfer collateral → CollateralVault ──────
    // Locked here. Released only at circle completion via
    // claim_collateral, or seized via process_default.
    // Position N (zero collateral) skips this transfer.
    if collateral_amount > 0 {
        transfer_checked(
            CpiContext::new(
                ctx.accounts.token_program.key(),
                TransferChecked {
                    from:      ctx.accounts.member_token_account.to_account_info(),
                    mint:      ctx.accounts.usdc_mint.to_account_info(),
                    to:        ctx.accounts.collateral_vault.to_account_info(),
                    authority: ctx.accounts.member.to_account_info(),
                },
            ),
            collateral_amount,
            decimals,
        )?;
    }

    // ── Step 8: Transfer contribution + premium → PotVault ─
    // For position 1: contribution_amount only (no premium)
    // For positions 2-N: contribution_amount + premium_amount
    //
    // Premium now sits in PotVault alongside the contribution.
    // Position 1 receives their compensation when they receive
    // the cycle 1 pot — which contains all premiums from all
    // joining members. This is fairer than direct routing
    // because premiums remain in a protocol-controlled vault
    // and are fully refundable if the circle is cancelled.
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
        first_round_payment, // contribution_amount + premium_amount
        decimals,
    )?;

    // ── Step 9: Initialise MemberAccount ─────────────────
    let member_acc              = &mut ctx.accounts.member_account;
    member_acc.circle           = circle_account_key;
    member_acc.member           = ctx.accounts.member.key();
    member_acc.position         = position;
    member_acc.collateral_locked = collateral_amount;
    member_acc.has_received_pot  = false;
    member_acc.is_defaulted      = false;
    member_acc.is_kicked         = false;
    member_acc.bump              = ctx.bumps.member_account;

    // ── Step 10: Initialise CollateralRecord ───────────────
    // total_locked is set once here and never changes.
    // It is the reference point for claim_collateral:
    //   claimable = total_locked - total_slashed
    let col_rec           = &mut ctx.accounts.collateral_record;
    col_rec.circle        = circle_account_key;
    col_rec.member        = ctx.accounts.member.key();
    col_rec.total_locked  = collateral_amount;
    col_rec.total_released = 0;
    col_rec.total_slashed  = 0;
    col_rec.claimed        = false;
    col_rec.bump           = ctx.bumps.collateral_record;

    // ── Step 11: Update CircleAccount ─────────────────────
    // Capture key before mutable borrow of circle

    circle.current_members = circle.current_members
        .checked_add(1)
        .ok_or(RoundsError::MathOverflow)?;

    circle.active_members = circle.active_members
        .checked_add(1)
        .ok_or(RoundsError::MathOverflow)?;

    // ── Step 12: Transition OPEN → READY if full ──────────
    // The last member to join triggers this automatically
    // in the same transaction — no separate instruction needed.
    if circle.current_members == circle.total_members {
        circle.state = CircleState::Ready;
        msg!(
            "Circle is now READY. All {} seats filled.",
            circle.total_members
        );
    }

    msg!(
        "Member {} joined at position {}. Collateral: {}. Premium: {}. Total paid: {}.",
        ctx.accounts.member.key(),
        position,
        collateral_amount,
        premium_amount,
        total_required,
    );

    Ok(())
}