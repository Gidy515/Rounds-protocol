pub mod constants;
pub mod instructions;
pub mod state;
pub mod errors;

use anchor_lang::prelude::*;

pub use constants::*;
pub use instructions::*;
pub use state::*;
pub use errors::RoundsError;

declare_id!("7BBvnkQ4AKMFU6EfWvScSqi69eu9TjLoDzpmzG8ZeFhN");

#[program]
pub mod rounds_protocol {
    use super::*;

    // ── Admin instructions ───────────────────────────────
   pub fn initialize_protocol(
        ctx: Context<InitializeProtocol>,
        protocol_fee_bps: u16,
    ) -> Result<()> {
        instructions::initialize_protocol::handler(ctx, protocol_fee_bps)
    }
    
    // ── Permissionless keeper instructions ───────────────
    pub fn create_circle(
        ctx: Context<CreateCircle>,
        contribution_amount: u64,
        total_members: u8,
        frequency: PayoutFrequency,
        nonce: u8,
    ) -> Result<()> {
        instructions::create_circle::handler(ctx, contribution_amount, total_members, frequency, nonce)
    }

        // ── Member instructions ──────────────────────────────
    pub fn join_circle(
        ctx: Context<JoinCircle>,
    ) -> Result<()> {
        instructions::join_circle::handler(ctx)
    }

    // ── Permissionless keeper instructions ───────────────
    pub fn start_circle(
        ctx: Context<StartCircle>,
    ) -> Result<()> {
        instructions::start_circle::handler(ctx)
    }

    // ── Member instructions ──────────────────────────────
    pub fn pay_contribution(
        ctx: Context<PayContribution>,
    ) -> Result<()> {
        instructions::pay_contribution::handler(ctx)
    }

    // ── Permissionless keeper instructions ───────────────
    pub fn disburse_pot(
        ctx: Context<DisbursePot>,
    ) -> Result<()> {
        instructions::disburse_pot::handler(ctx)
    }

    // ── Permissionless keeper instructions ───────────────
    pub fn process_default(
        ctx: Context<ProcessDefault>,
    ) -> Result<()> {
        instructions::process_default::handler(ctx)
    }

        // ── Member instructions ──────────────────────────────
    pub fn claim_collateral(
        ctx: Context<ClaimCollateral>,
    ) -> Result<()> {
        instructions::claim_collateral::handler(ctx)
    }

    // ── Permissionless keeper instructions ───────────────
    pub fn cancel_circle(
        ctx: Context<CancelCircle>,
    ) -> Result<()> {
        instructions::cancel_circle::handler(ctx)
    }

    pub fn init_payment_record(
        ctx: Context<InitPaymentRecord>,
        cycle: u8,
    ) -> Result<()> {
        instructions::init_payment_record::handler(ctx, cycle)
    }

    // ── Admin instructions ───────────────────────────────
    pub fn pause_protocol(
        ctx: Context<PauseProtocol>,
    ) -> Result<()> {
        instructions::pause_protocol::handler(ctx)
    }

    pub fn unpause_protocol(
        ctx: Context<UnpauseProtocol>,
    ) -> Result<()> {
        instructions::unpause_protocol::handler(ctx)
    }

    pub fn update_config(
        ctx: Context<UpdateConfig>,
        new_fee_bps: u16,
    ) -> Result<()> {
        instructions::update_config::handler(ctx, new_fee_bps)
    }

    pub fn withdraw_treasury(
        ctx: Context<WithdrawTreasury>,
        amount: u64,
    ) -> Result<()> {
        instructions::withdraw_treasury::handler(ctx, amount)
    }

}
