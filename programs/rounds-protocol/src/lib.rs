pub mod constants;
pub mod instructions;
pub mod state;
pub mod errors;

use anchor_lang::prelude::*;

/*pub use constants::*;
pub use instructions::*;
pub use state::*;
pub use errors::RoundsError;*/


declare_id!("F4kSCH4tGiakWt25cGqKqrEjEobafeZVre2PJqcEfspg");

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

    // ── Permissionless keeper instructions ───────────────

    pub fn create_circle(
        ctx: Context<CreateCircle>,
        contribution_amount: u64,
        total_members: u8,
        frequency: state::PayoutFrequency,
    ) -> Result<()> {
        instructions::create_circle::handler(
            ctx,
            contribution_amount,
            total_members,
            frequency,
        )
    }

    pub fn start_circle(
        ctx: Context<StartCircle>,
    ) -> Result<()> {
        instructions::start_circle::handler(ctx)
    }

    pub fn disburse_pot(
        ctx: Context<DisbursePot>,
    ) -> Result<()> {
        instructions::disburse_pot::handler(ctx)
    }

    pub fn process_default(
        ctx: Context<ProcessDefault>,
    ) -> Result<()> {
        instructions::process_default::handler(ctx)
    }

    pub fn cancel_circle(
        ctx: Context<CancelCircle>,
    ) -> Result<()> {
        instructions::cancel_circle::handler(ctx)
    }

    // ── Member instructions ──────────────────────────────

    pub fn join_circle(
        ctx: Context<JoinCircle>,
    ) -> Result<()> {
        instructions::join_circle::handler(ctx)
    }

    pub fn pay_contribution(
        ctx: Context<PayContribution>,
    ) -> Result<()> {
        instructions::pay_contribution::handler(ctx)
    }

    pub fn claim_collateral(
        ctx: Context<ClaimCollateral>,
    ) -> Result<()> {
        instructions::claim_collateral::handler(ctx)
    }
}
