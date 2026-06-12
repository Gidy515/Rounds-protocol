use anchor_lang::prelude::*;

use crate::errors::RoundsError;
use crate::state::ProtocolConfig;

/// Account validation struct for unpause_protocol.
///
/// Admin only. Sets is_paused to false in ProtocolConfig.
/// All instructions resume normal execution immediately
/// after this transaction confirms.
///
/// No circle state is modified. Circles that were mid-cycle
/// when the pause occurred resume from exactly where they
/// left off. Deadline slots may need manual extension via
/// update_config if the pause duration was significant —
/// that is a post-MVP governance concern.
#[derive(Accounts)]
pub struct UnpauseProtocol<'info> {

    /// Must be the admin stored in ProtocolConfig.
    #[account(
        constraint = admin.key() == protocol_config.admin
            @ RoundsError::Unauthorized,
    )]
    pub admin: Signer<'info>,

    /// ProtocolConfig — is_paused flipped to false here.
    #[account(
        mut,
        seeds = [b"config"],
        bump = protocol_config.bump,
    )]
    pub protocol_config: Account<'info, ProtocolConfig>,
}

/// unpause_protocol
///
/// Sets is_paused = false in ProtocolConfig.
/// All protocol activity resumes immediately.
pub fn handler(ctx: Context<UnpauseProtocol>) -> Result<()> {

    require!(
        ctx.accounts.protocol_config.is_paused,
        RoundsError::ProtocolNotPaused
        // Not currently paused — revert to avoid confusion
    );

    ctx.accounts.protocol_config.is_paused = false;

    msg!(
        "Protocol unpaused by admin {}.",
        ctx.accounts.admin.key()
    );

    Ok(())
}