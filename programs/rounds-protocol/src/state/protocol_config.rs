use anchor_lang::prelude::*;

/// ProtocolConfig — global singleton PDA
/// Seeds: [b"config"]
/// Created once at initialize_protocol.
/// Read by every single instruction via is_paused check.
/// Written only by admin instructions.
#[account]
#[derive(Default)]
pub struct ProtocolConfig {
    /// The admin public key.
    /// Only this keypair can call admin-gated instructions:
    /// pause_protocol, unpause_protocol, update_config,
    /// withdraw_treasury.
    pub admin: Pubkey,

    /// Protocol fee in basis points.
    /// Applied to every pot disbursement.
    /// 50 bps = 0.5% · 100 bps = 1%
    /// Deducted from pot before recipient receives net amount.
    pub protocol_fee_bps: u16,

    /// Emergency pause flag.
    /// When true, every instruction checks this first and
    /// reverts immediately. Funds stay locked in vaults.
    /// Only admin can flip this.
    pub is_paused: bool,

    /// PDA bump seed.
    /// Stored so we never need to recompute it.
    pub bump: u8,
}

impl ProtocolConfig {
    /// Account discriminator:    8
    /// admin (Pubkey):          32
    /// protocol_fee_bps (u16):   2
    /// is_paused (bool):         1
    /// bump (u8):                1
    /// ─────────────────────────
    /// Total:                   44
    pub const LEN: usize = 8 + 32 + 2 + 1 + 1;
}