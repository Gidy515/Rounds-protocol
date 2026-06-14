use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{Mint, TokenAccount, TokenInterface, TransferChecked, transfer_checked},
};

use crate::errors::RoundsError;
use crate::state::ProtocolConfig;

/// Account validation struct for withdraw_treasury.
///
/// Admin only. Moves accumulated protocol fees from
/// TreasuryVault to a destination wallet of the admin's
/// choosing.
///
/// The amount parameter allows partial withdrawals —
/// the admin can withdraw any amount up to the full
/// TreasuryVault balance. This supports treasury
/// management strategies like periodic fee collection
/// without emptying the vault entirely.
#[derive(Accounts)]
pub struct WithdrawTreasury<'info> {

    /// Must be the admin stored in ProtocolConfig.
    #[account(
        constraint = admin.key() == protocol_config.admin
            @ RoundsError::Unauthorized,
    )]
    pub admin: Signer<'info>,

    /// ProtocolConfig — provides admin validation and
    /// serves as TreasuryVault authority.
    /// Seeds: [b"config"]
    #[account(
        seeds = [b"config"],
        bump = protocol_config.bump,
    )]
    pub protocol_config: Box<Account<'info, ProtocolConfig>>,

    /// TreasuryVault — source of the withdrawal.
    /// Seeds: [b"treasury", protocol_config]
    /// Authority: protocol_config PDA.
    //#[account(
    //    mut,
    //    seeds = [
    //        b"treasury",
    //        protocol_config.key().as_ref(),
    //    ],
    //    bump,
   //     token::mint          = usdc_mint,
    //    token::authority     = protocol_config,
    //    token::token_program = token_program,
   // )]
   // pub treasury_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        mut,
        associated_token::mint          = usdc_mint,
        associated_token::authority     = protocol_config,
        associated_token::token_program = token_program,
    )]
    pub treasury_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    /// Destination token account — receives the withdrawn fees.
    /// Must hold the same mint as TreasuryVault.
    /// Admin chooses this address — can be any valid USDC ATA.
    #[account(
        mut,
        token::mint          = usdc_mint,
        token::token_program = token_program,
    )]
    pub destination: Box<InterfaceAccount<'info, TokenAccount>>,

    /// USDC mint — required by transfer_checked.
    pub usdc_mint: Box<InterfaceAccount<'info, Mint>>,

    pub token_program: Interface<'info, TokenInterface>,
    pub associated_token_program: Program<'info, AssociatedToken>,
}

/// withdraw_treasury
///
/// Transfers the specified amount from TreasuryVault to the
/// destination token account. Admin controlled.
///
/// Amount must be greater than zero and not exceed the
/// current TreasuryVault balance.
pub fn handler(
    ctx: Context<WithdrawTreasury>,
    amount: u64,
) -> Result<()> {

    let treasury_balance = ctx.accounts.treasury_vault.amount;
    let decimals         = ctx.accounts.usdc_mint.decimals;

    // ── Validate amount ────────────────────────────────────
    require!(
        amount > 0,
        RoundsError::InvalidWithdrawAmount
    );

    require!(
        amount <= treasury_balance,
        RoundsError::InsufficientTreasuryBalance
    );

    // ── PDA signer seeds for TreasuryVault transfer ────────
    // TreasuryVault authority is protocol_config PDA.
    let config_bump = [ctx.accounts.protocol_config.bump];

    let config_signer_seeds: &[&[u8]] = &[
        b"config",
        config_bump.as_ref(),
    ];

    // ── Transfer fees → destination ────────────────────────
    transfer_checked(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.key(),
            TransferChecked {
                from:      ctx.accounts.treasury_vault.to_account_info(),
                mint:      ctx.accounts.usdc_mint.to_account_info(),
                to:        ctx.accounts.destination.to_account_info(),
                authority: ctx.accounts.protocol_config.to_account_info(),
            },
            &[config_signer_seeds],
        ),
        amount,
        decimals,
    )?;

    msg!(
        "Treasury withdrawal by admin {}. Amount: {}. \
         Remaining balance: {}.",
        ctx.accounts.admin.key(),
        amount,
        treasury_balance
            .checked_sub(amount)
            .unwrap_or(0),
    );

    Ok(())
}