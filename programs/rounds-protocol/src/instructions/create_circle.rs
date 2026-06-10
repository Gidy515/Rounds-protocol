use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{Mint, TokenAccount, TokenInterface},
};

use crate::constants::*;
use crate::errors::RoundsError;
use crate::state::{CircleAccount, CircleState, PayoutFrequency, ProtocolConfig};

/// Account validation struct for create_circle.
#[derive(Accounts)]
#[instruction(
    contribution_amount: u64,
    total_members: u8,
    frequency: PayoutFrequency,
)]
pub struct CreateCircle<'info> {

    /// The circle creator — automatically becomes position 1.
    /// Pays rent for CircleAccount, CollateralVault, PotVault.
    #[account(mut)]
    pub creator: Signer<'info>,

    /// ProtocolConfig — read to check is_paused.
    /// Seeds: [b"config"]
    #[account(
        seeds = [b"config"],
        bump = protocol_config.bump,
        constraint = !protocol_config.is_paused
            @ RoundsError::ProtocolPaused,
    )]
    pub protocol_config: Account<'info, ProtocolConfig>,

    /// CircleAccount PDA — the circle's source of truth.
    ///
    /// Seeds are derived from the circle's parameters:
    /// [b"circle", contribution_amount, total_members,
    ///  frequency as u8, usdc_mint pubkey]
    ///
    /// This seed fingerprint is the duplicate prevention
    /// mechanism. Two circles with identical parameters
    /// resolve to the same PDA address. Anchor's `init`
    /// constraint will reject the transaction if the
    /// account already exists — meaning an open circle
    /// with these exact settings already exists.
    ///
    /// If you hit this error, join that circle instead.
    #[account(
        init,
        payer = creator,
        space = CircleAccount::LEN,
        seeds = [
            b"circle",
            contribution_amount.to_le_bytes().as_ref(),
            &[total_members],
            &[frequency.clone() as u8],
            usdc_mint.key().as_ref(),
        ],
        bump,
    )]
    pub circle_account: Account<'info, CircleAccount>,

    /// CollateralVault — USDC token account PDA.
    /// Holds all member collateral for this circle.
    /// Seeds: [b"collateral_vault", circle_account pubkey]
    /// Authority: circle_account PDA — program controlled.
    /// Never touched during normal operation.
    /// Only moves on default deduction or circle completion.
    #[account(
        init,
        payer = creator,
        associated_token::mint          = usdc_mint,
        associated_token::authority     = circle_account,
        associated_token::token_program = token_program,
    )]
    pub collateral_vault: InterfaceAccount<'info, TokenAccount>,

    /// PotVault — USDC token account PDA.
    /// Accumulates cycle contributions before disbursement.
    /// Seeds: [b"pot_vault", circle_account pubkey]
    /// Authority: circle_account PDA — program controlled.
    /// Fills each cycle, zeroes completely after disburse_pot.
    #[account(
        init,
        payer = creator,
        associated_token::mint          = usdc_mint,
        associated_token::authority     = circle_account,
        associated_token::token_program = token_program,
    )]
    pub pot_vault: InterfaceAccount<'info, TokenAccount>,

    /// USDC mint — validated against token program.
    /// Stored in CircleAccount so every subsequent instruction
    /// can verify incoming token accounts against this mint.
    #[account(
        mint::token_program = token_program
    )]
    pub usdc_mint: InterfaceAccount<'info, Mint>,

    pub system_program:            Program<'info, System>,
    pub token_program:             Interface<'info, TokenInterface>,
    pub associated_token_program:  Program<'info, AssociatedToken>,
}

/// create_circle
///
/// Creates a new savings circle with the given parameters.
/// The creator is automatically position 1 — they must call
/// join_circle immediately after to lock their collateral
/// and take their seat.
///
/// Duplicate prevention: CircleAccount PDA is derived from
/// the parameter fingerprint. If an identical open circle
/// already exists, Anchor's init constraint rejects the
/// transaction before any state is written.
pub fn handler(
    ctx: Context<CreateCircle>,
    contribution_amount: u64,
    total_members: u8,
    frequency: PayoutFrequency,
) -> Result<()> {

    // ── Validate parameters ────────────────────────────────
    require!(
        total_members >= MIN_MEMBERS && total_members <= MAX_MEMBERS,
        RoundsError::InvalidMemberCount
        // "Member count must be between 2 and 20"
    );

    require!(
        contribution_amount >= MIN_CONTRIBUTION_AMOUNT,
        RoundsError::ContributionTooLow
        // "Contribution amount must be at least 1 USDC"
    );

    // ── Map frequency to cycle duration in slots ───────────
    // Users select a human-readable label.
    // The program stores the derived slot count so every
    // subsequent instruction can read it directly without
    // recomputing from the enum.
    let cycle_duration_slots: u64 = match frequency {
        PayoutFrequency::Daily    => SLOTS_PER_DAY,
        PayoutFrequency::Weekly   => SLOTS_PER_WEEK,
        PayoutFrequency::Biweekly => SLOTS_PER_BIWEEK,
        PayoutFrequency::Monthly  => SLOTS_PER_MONTH,
    };

    // ── Set cancel deadline ────────────────────────────────
    // If the circle does not fill within 24 hours of creation
    // any wallet can call cancel_circle to return all funds.
    let clock = Clock::get()?;
    let cancel_deadline_slot = clock
        .slot
        .checked_add(CANCEL_DEADLINE_SLOTS)
        .ok_or(RoundsError::MathOverflow)?;

    // ── Initialise CircleAccount ───────────────────────────
    
    let circle = &mut ctx.accounts.circle_account;

    circle.started_at_slot   = 0; // set in start_circle when circle starts
    circle.completed_at_slot = 0; // set in disburse_pot when circle completes

    circle.contribution_amount  = contribution_amount;
    circle.total_members        = total_members;
    circle.active_members       = 0; // increments as members join
    circle.current_members      = 0; // increments as members join
    circle.frequency            = frequency;
    circle.cycle_duration_slots = cycle_duration_slots;
    circle.usdc_mint            = ctx.accounts.usdc_mint.key();
    circle.state                = CircleState::Open;
    circle.current_cycle        = 0; // set to 1 when start_circle fires
    circle.cycle_deadline_slot  = 0; // set in start_circle
    circle.cancel_deadline_slot = cancel_deadline_slot;
    circle.bump                 = ctx.bumps.circle_account;

    msg!(
        "Circle created. Amount: {} lamports. Members: {}. Frequency: {:?}. Cancel deadline slot: {}.",
        contribution_amount,
        total_members,
        circle.frequency,
        cancel_deadline_slot,
    );

    Ok(())
}