use anchor_lang::prelude::*;

use crate::constants::MAX_PROTOCOL_FEE_BPS;
use crate::errors::RoundsError;
use crate::state::ProtocolConfig;

/// Account validation struct for update_config.
///
/// Admin only. Updates protocol fee basis points.
/// Change applies to all subsequent disburse_pot calls
/// across all circles from this point forward.
/// Circles already in progress are not retroactively affected
/// — the fee is calculated at disbursement time from the
/// current ProtocolConfig value.
#[derive(Accounts)]
pub struct UpdateConfig<'info> {

    /// Must be the admin stored in ProtocolConfig.
    #[account(
        constraint = admin.key() == protocol_config.admin
            @ RoundsError::Unauthorized,
    )]
    pub admin: Signer<'info>,

    /// ProtocolConfig — protocol_fee_bps updated here.
    #[account(
        mut,
        seeds = [b"config"],
        bump = protocol_config.bump,
    )]
    pub protocol_config: Account<'info, ProtocolConfig>,
}

/// update_config
///
/// Updates the protocol fee in basis points.
/// Validated against MAX_PROTOCOL_FEE_BPS (1000 = 10%).
/// New fee applies to all subsequent pot disbursements.
pub fn handler(
    ctx: Context<UpdateConfig>,
    new_fee_bps: u16,
) -> Result<()> {

    require!(
        new_fee_bps <= MAX_PROTOCOL_FEE_BPS,
        RoundsError::FeeTooHigh
    );

    let old_fee = ctx.accounts.protocol_config.protocol_fee_bps;

    ctx.accounts.protocol_config.protocol_fee_bps = new_fee_bps;

    msg!(
        "Protocol fee updated by admin {}. Old: {} bps. New: {} bps.",
        ctx.accounts.admin.key(),
        old_fee,
        new_fee_bps,
    );

    Ok(())
}