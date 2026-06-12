use anchor_lang::prelude::*;

use crate::errors::RoundsError;
use crate::state::ProtocolConfig;

/// Account validation struct for pause_protocol.
///
/// Admin only. Sets is_paused to true in ProtocolConfig.
/// Every other instruction checks this flag first and
/// reverts immediately if true.
///
/// Funds stay exactly where they are in their vault accounts.
/// No funds move. No circle state changes.
/// Takes effect on the next instruction call after this
/// transaction confirms.
#[derive(Accounts)]
pub struct PauseProtocol<'info> {

    /// Must be the admin stored in ProtocolConfig.
    /// Only this keypair can pause the protocol.
    #[account(
        constraint = admin.key() == protocol_config.admin
            @ RoundsError::Unauthorized,
    )]
    pub admin: Signer<'info>,

    /// ProtocolConfig — is_paused flipped to true here.
    #[account(
        mut,
        seeds = [b"config"],
        bump = protocol_config.bump,
    )]
    pub protocol_config: Account<'info, ProtocolConfig>,
}

/// pause_protocol
///
/// Sets is_paused = true in ProtocolConfig.
/// Halts all protocol activity immediately after this
/// transaction confirms. All vault balances are unaffected.
pub fn handler(ctx: Context<PauseProtocol>) -> Result<()> {

    require!(
        !ctx.accounts.protocol_config.is_paused,
        RoundsError::ProtocolPaused
        // Already paused — no-op would be confusing, revert instead
    );

    ctx.accounts.protocol_config.is_paused = true;

    msg!(
        "Protocol paused by admin {}.",
        ctx.accounts.admin.key()
    );

    Ok(())
}