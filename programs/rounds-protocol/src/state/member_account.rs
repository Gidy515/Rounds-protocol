use anchor_lang::prelude::*;

/// MemberAccount — per-member per-circle state PDA.
/// Seeds: [b"member", circle_pubkey.as_ref(), member_pubkey.as_ref()]
///
/// Created in join_circle. Lives for the circle lifetime.
/// Tracks the live state of this member's participation.
/// One per member per circle — impossible to fake or collide
/// because both circle and member pubkeys are in the seeds.
///
/// IMPORTANT: This is live state only.
/// The full collateral history (total_locked, total_slashed,
/// total_released) lives in CollateralRecord — the audit trail.
#[account]
pub struct MemberAccount {
    /// The circle this member belongs to.
    /// Used for cross-account validation.
    pub circle: Pubkey,

    /// The member's wallet public key.
    /// Used for permission checks and premium routing.
    pub member: Pubkey,

    /// Position in the payout order. 1-indexed.
    /// Assigned at join time from current_members + 1.
    /// Position 1 = receives pot in cycle 1.
    /// Position N = receives pot in final cycle.
    /// May be updated if a prior member is kicked —
    /// positions shift down to fill the gap.
    pub position: u8,

    /// Current locked collateral in USDC lamports.
    /// Set at join time as (total_members - position) × contribution_amount.
    /// Decrements by exactly contribution_amount per default deduction.
    /// Always a clean multiple of contribution_amount.
    /// Reaches 0 only through default deductions — never from
    /// normal operation (collateral is returned via claim_collateral
    /// at completion, not decremented during the circle).
    pub collateral_locked: u64,

    /// True once this member has received their pot disbursement.
    /// Set in disburse_pot when current_cycle matches their position.
    /// Used in the final completion check — all members must have
    /// either received their pot or been kicked before COMPLETED.
    pub has_received_pot: bool,

    /// True if this member has missed at least one payment.
    /// Set in process_default.
    /// Does NOT mean they are kicked — just that they have defaulted
    /// at least once. They can still pay future cycles if collateral
    /// covers the missed one(s).
    pub is_defaulted: bool,

    /// True if this member has been permanently removed from the circle.
    /// Set in process_default when:
    ///   collateral_locked == 0 AND rounds still remain.
    /// Once kicked, member cannot pay, cannot receive pot,
    /// and cannot claim collateral (nothing left to claim).
    /// active_members decrements in CircleAccount when this is set.
    pub is_kicked: bool,

    /// PDA bump seed. Stored to avoid recomputation.
    pub bump: u8,
}

impl MemberAccount {
    /// Account discriminator:      8
    /// circle (Pubkey):           32
    /// member (Pubkey):           32
    /// position (u8):              1
    /// collateral_locked (u64):    8
    /// has_received_pot (bool):    1
    /// is_defaulted (bool):        1
    /// is_kicked (bool):           1
    /// bump (u8):                  1
    /// ───────────────────────────
    /// Total:                     85
    pub const LEN: usize = 8 + 32 + 32 + 1 + 8 + 1 + 1 + 1 + 1;
}