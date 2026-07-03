use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{Mint, TokenAccount, TokenInterface},
};

use crate::constants::*;
use crate::errors::RoundsError;
use crate::state::{CircleAccount, CircleState, PayoutFrequency, ProtocolConfig};

#[derive(Accounts)]
#[instruction(
    contribution_amount: u64,
    total_members: u8,
    frequency: PayoutFrequency,
    nonce: u8,
)]
pub struct CreateCircle<'info> {
    #[account(mut)]
    pub creator: Signer<'info>,

    #[account(
        seeds = [b"config"],
        bump = protocol_config.bump,
        constraint = !protocol_config.is_paused
            @ RoundsError::ProtocolPaused,
    )]
    pub protocol_config: Account<'info, ProtocolConfig>,

    /// CircleAccount PDA.
    /// Seeds now include a nonce — allows multiple circles
    /// with identical parameters to coexist sequentially.
    /// Frontend finds the correct nonce automatically:
    ///   nonce 0 = first circle, nonce 1 = second, etc.
    /// A new circle at nonce N is only valid when nonce N-1
    /// is no longer Open (full, active, completed, or cancelled).
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
            &[nonce],
        ],
        bump,
    )]
    pub circle_account: Account<'info, CircleAccount>,

    #[account(
        init,
        payer = creator,
        seeds = [b"collateral_vault", circle_account.key().as_ref()],
        bump,
        token::mint          = usdc_mint,
        token::authority     = circle_account,
        token::token_program = token_program,
    )]
    pub collateral_vault: InterfaceAccount<'info, TokenAccount>,

    #[account(
        init,
        payer = creator,
        seeds = [b"pot_vault", circle_account.key().as_ref()],
        bump,
        token::mint          = usdc_mint,
        token::authority     = circle_account,
        token::token_program = token_program,
    )]
    pub pot_vault: InterfaceAccount<'info, TokenAccount>,

    #[account(
        mint::token_program = token_program
    )]
    pub usdc_mint: InterfaceAccount<'info, Mint>,

    pub system_program:           Program<'info, System>,
    pub token_program:            Interface<'info, TokenInterface>,
    pub associated_token_program: Program<'info, AssociatedToken>,
}

pub fn handler(
    ctx: Context<CreateCircle>,
    contribution_amount: u64,
    total_members: u8,
    frequency: PayoutFrequency,
    nonce: u8,
) -> Result<()> {

    require!(
        total_members >= MIN_MEMBERS && total_members <= MAX_MEMBERS,
        RoundsError::InvalidMemberCount
    );

    require!(
        contribution_amount >= MIN_CONTRIBUTION_AMOUNT,
        RoundsError::ContributionTooLow
    );

    let cycle_duration_slots: u64 = match frequency {
        PayoutFrequency::Daily    => SLOTS_PER_DAY,
        PayoutFrequency::Weekly   => SLOTS_PER_WEEK,
        PayoutFrequency::Biweekly => SLOTS_PER_BIWEEK,
        PayoutFrequency::Monthly  => SLOTS_PER_MONTH,
    };

    let clock = Clock::get()?;
    let cancel_deadline_slot = clock
        .slot
        .checked_add(CANCEL_DEADLINE_SLOTS)
        .ok_or(RoundsError::MathOverflow)?;

    let circle = &mut ctx.accounts.circle_account;

    circle.started_at_slot    = 0;
    circle.completed_at_slot  = 0;
    circle.contribution_amount  = contribution_amount;
    circle.total_members        = total_members;
    circle.active_members       = 0;
    circle.current_members      = 0;
    circle.frequency            = frequency;
    circle.cycle_duration_slots = cycle_duration_slots;
    circle.usdc_mint            = ctx.accounts.usdc_mint.key();
    circle.state                = CircleState::Open;
    circle.current_cycle        = 0;
    circle.cycle_deadline_slot  = 0;
    circle.cancel_deadline_slot = cancel_deadline_slot;
    circle.bump                 = ctx.bumps.circle_account;
    circle.nonce                = nonce;

    msg!(
        "Circle created. Amount: {} lamports. Members: {}. Frequency: {:?}. Nonce: {}. Cancel deadline slot: {}.",
        contribution_amount,
        total_members,
        circle.frequency,
        nonce,
        cancel_deadline_slot,
    );

    Ok(())
}