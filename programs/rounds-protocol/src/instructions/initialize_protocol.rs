use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,  
    token_interface::
    {Mint, TokenAccount, TokenInterface},
};

use crate::constants::*;
use crate::errors::RoundsError;
use crate::state::ProtocolConfig;

/// Account validation struct for initialize_protocol.
#[derive(Accounts)]
pub struct InitializeProtocol<'info> {

    /// The deployer wallet.
    /// Pays rent for ProtocolConfig and TreasuryVault.
    /// Becomes the protocol admin stored in ProtocolConfig.
    /// Must sign the transaction.
    #[account(mut)]
    pub admin: Signer<'info>,

    /// ProtocolConfig PDA — global singleton.
    /// Seeds: [b"config"]
    /// Created here. Will exist for the lifetime of the protocol.
    /// init ensures this can only ever be called once —
    /// if the account already exists the instruction reverts.
    #[account(
        init,
        payer = admin,
        space = ProtocolConfig::LEN,
        seeds = [b"config"],
        bump,
    )]
    pub protocol_config: Account<'info, ProtocolConfig>,

    /// TreasuryVault — USDC token account PDA.
    /// Seeds: [b"treasury", protocol_config.key()]
    /// Receives protocol fees from every disburse_pot call.
    /// Owned by the protocol — no private key controls it.
    /// Only withdraw_treasury (admin only) can move funds out.
    #[account(
        init,
        payer = admin,
        associated_token::mint = usdc_mint,
        associated_token::authority = protocol_config,
        associated_token::token_program = token_program,
    )]
    pub treasury_vault: InterfaceAccount<'info, TokenAccount>,

    /// USDC mint account.
    /// Passed in so the TreasuryVault is initialised with
    /// the correct mint. Validated to be a valid mint account.
    #[account(
        mint::token_program = token_program
    )]
    pub usdc_mint: InterfaceAccount<'info, Mint>,

    pub system_program: Program<'info, System>,
    pub token_program: Interface<'info, TokenInterface>,

    /// Required for token account rent exemption calculation.
    //pub rent: Sysvar<'info, Rent>,
    pub associated_token_program: Program<'info, AssociatedToken>,
}


/// initialize_protocol
///
/// Called once by the deployer immediately after the program
/// is deployed. Creates the ProtocolConfig PDA and the
/// TreasuryVault token account PDA.
///
/// This is the only instruction that cannot check is_paused
/// because ProtocolConfig does not exist yet when it runs.
///
/// After this instruction succeeds the protocol is live.
/// No circle can be created until this has been called.
pub fn handler(
    ctx: Context<InitializeProtocol>,
    protocol_fee_bps: u16,
) -> Result<()> {

    // Validate fee does not exceed the protocol maximum
    require!(
        protocol_fee_bps <= MAX_PROTOCOL_FEE_BPS,
        RoundsError::FeeTooHigh
    );

    // Initialise ProtocolConfig
    let config = &mut ctx.accounts.protocol_config;
    config.admin            = ctx.accounts.admin.key();
    config.protocol_fee_bps = protocol_fee_bps;
    config.is_paused        = false;
    config.bump             = ctx.bumps.protocol_config;

    msg!(
        "Rounds Protocol initialised. Admin: {}. Fee: {} bps.",
        config.admin,
        config.protocol_fee_bps
    );

    Ok(())
}