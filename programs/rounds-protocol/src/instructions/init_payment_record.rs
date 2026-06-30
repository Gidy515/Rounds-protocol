use anchor_lang::prelude::*;

use crate::errors::RoundsError;
use crate::state::{CircleAccount, CircleState, PaymentRecord, ProtocolConfig};

/// Account validation struct for init_payment_record.
///
/// Permissionless keeper instruction.
/// Initialises the PaymentRecord for the next upcoming cycle.
/// Must be called after disburse_pot advances the cycle counter
/// and before members call pay_contribution for that cycle.
///
/// Any wallet can call this. The caller pays rent for the
/// new PaymentRecord account.
///
/// This instruction is a prerequisite for pay_contribution
/// on cycles 2 through N. Without it pay_contribution will
/// fail because the PaymentRecord account does not exist.
///
/// Typical call order per cycle:
///   1. disburse_pot (cycle N disbursed, cycle counter → N+1)
///   2. init_payment_record (creates record for cycle N+1)
///   3. pay_contribution × active_members (fills record)
///   4. disburse_pot (cycle N+1 disbursed)
#[derive(Accounts)]
#[instruction(cycle: u8)]
pub struct InitPaymentRecord<'info> {

    /// The caller — pays rent for the new PaymentRecord.
    /// Can be any wallet. No permissions required.
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

    /// CircleAccount — must be Active.
    /// The cycle parameter must match current_cycle —
    /// you can only initialise the record for the
    /// cycle that is currently active, not future ones.
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
        constraint = circle_account.current_cycle == cycle
            @ RoundsError::InvalidCycle,
    )]
    pub circle_account: Account<'info, CircleAccount>,

    /// PaymentRecord for the specified cycle — created here.
    /// Seeds: [b"payment", circle_account, &[cycle]]
    /// Initialised with paid_flags = 0.
    /// Anchor's init constraint prevents creating a record
    /// for a cycle that already has one.
    #[account(
        init,
        payer = caller,
        space = PaymentRecord::LEN,
        seeds = [
            b"payment",
            circle_account.key().as_ref(),
            &[cycle],
        ],
        bump,
    )]
    pub payment_record: Account<'info, PaymentRecord>,

    pub system_program: Program<'info, System>,
}

/// init_payment_record
///
/// Creates the PaymentRecord for the specified cycle number.
/// Initialises paid_flags to 0 — all members unpaid.
///
/// Must be called after disburse_pot has advanced the circle
/// to the target cycle and before any pay_contribution calls
/// for that cycle.
pub fn handler(
    ctx: Context<InitPaymentRecord>,
    cycle: u8,
) -> Result<()> {

    let payment      = &mut ctx.accounts.payment_record;
    payment.circle     = ctx.accounts.circle_account.key();
    payment.cycle      = cycle;
    payment.paid_flags = 0u64; // all members start unpaid
    payment.bump       = ctx.bumps.payment_record;

    msg!(
        "PaymentRecord initialised for cycle {}. Circle: {}.",
        cycle,
        ctx.accounts.circle_account.key(),
    );

    Ok(())
}