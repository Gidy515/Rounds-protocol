use anchor_lang::prelude::*;

use crate::errors::RoundsError;
use crate::state::{CircleAccount, CircleState, PaymentRecord, ProtocolConfig};

/// Account validation struct for start_circle.
///
/// Permissionless keeper instruction.
/// Anyone can call this once the circle is in Ready state.
/// No funds move in this instruction — it is purely a state
/// transition that sets the clock running for cycle 1.
#[derive(Accounts)]
pub struct StartCircle<'info> {

    /// The caller — pays rent for the cycle 1 PaymentRecord.
    /// Can be any wallet. No special permissions required.
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

    /// CircleAccount — must be in Ready state.
    /// Transitions to Active in this instruction.
    /// current_cycle set to 1.
    /// cycle_deadline_slot set to now + cycle_duration_slots.
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
        constraint = circle_account.state == CircleState::Ready
            @ RoundsError::CircleNotReady,
    )]
    pub circle_account: Account<'info, CircleAccount>,

    /// PaymentRecord PDA for cycle 1 — created here.
    ///
    /// Seeds: [b"payment", circle_account, &[1u8]]
    ///
    /// Tracks who has paid in cycle 1 via a u64 bitmask.
    /// Cycle 1 is special — all contributions were already
    /// paid at join time via join_circle. So we initialise
    /// paid_flags with ALL bits set for active members,
    /// meaning cycle 1 is immediately ready for disburse_pot.
    ///
    /// This avoids requiring members to call pay_contribution
    /// for a cycle they have already funded.
    #[account(
        init,
        payer = caller,
        space = PaymentRecord::LEN,
        seeds = [
            b"payment",
            circle_account.key().as_ref(),
            &[1u8],
        ],
        bump,
    )]
    pub payment_record: Account<'info, PaymentRecord>,

    pub system_program: Program<'info, System>,
}

/// start_circle
///
/// Transitions the circle from Ready to Active.
/// Sets cycle 1 as the current cycle.
/// Sets the deadline slot for cycle 1.
/// Initialises the PaymentRecord for cycle 1 with all
/// active member bits pre-set — because all contributions
/// were already paid at join time.
///
/// After this instruction disburse_pot can be called
/// immediately for cycle 1 since all members have already
/// paid their contributions via join_circle.
pub fn handler(ctx: Context<StartCircle>) -> Result<()> {

    // Capture values before mutable borrow
    let circle_key = ctx.accounts.circle_account.key();
    let active_members       = ctx.accounts.circle_account.active_members;
    let cycle_duration_slots = ctx.accounts.circle_account.cycle_duration_slots;

    let circle = &mut ctx.accounts.circle_account;
    let clock   = Clock::get()?;

    // ── Transition to Active ───────────────────────────────
    circle.state         = CircleState::Active;
    circle.current_cycle = 1;

    // ── Set cycle 1 deadline ───────────────────────────────
    // Members have cycle_duration_slots from now to pay.
    // For cycle 1 this is technically already funded but
    // the deadline is set for protocol consistency —
    // it defines when disburse_pot must be called by.
    circle.cycle_deadline_slot = clock
        .slot
        .checked_add(cycle_duration_slots)
        .ok_or(RoundsError::MathOverflow)?;
    
    // Record when the circle started
    circle.started_at_slot = clock.slot;

    // ── Initialise PaymentRecord for cycle 1 ──────────────
    // All contributions for cycle 1 were collected at
    // join_circle time. We pre-set the bitmask here so
    // disburse_pot can fire immediately without waiting
    // for members to call pay_contribution again.
    //
    // Build the expected mask for all active members:
    // active_members = 10 → mask = (1 << 10) - 1 = 0b1111111111
    // Every bit from 0 to active_members-1 is set.
    let full_mask: u64 = if active_members == 64 {
        // Edge case: u64 shift overflow protection.
        // 64 members fills the entire u64.
        u64::MAX
    } else {
        (1u64 << active_members)
            .checked_sub(1)
            .ok_or(RoundsError::MathOverflow)?
    };

    let payment              = &mut ctx.accounts.payment_record;
    payment.circle           = circle_key;
    payment.cycle            = 1;
    payment.paid_flags       = full_mask;
    payment.bump             = ctx.bumps.payment_record;

    msg!(
        "Circle started. State: Active. Cycle: 1. Deadline slot: {}. Active members: {}.",
        circle.cycle_deadline_slot,
        active_members,
    );

    Ok(())
}