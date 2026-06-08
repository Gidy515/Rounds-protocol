use anchor_lang::prelude::*;

/// CollateralRecord — permanent collateral audit trail PDA.
/// Seeds: [b"colrec", circle_pubkey.as_ref(), member_pubkey.as_ref()]
///
/// Created in join_circle alongside MemberAccount.
/// MemberAccount tracks live state (what is locked right now).
/// CollateralRecord tracks history (what happened over time).
///
/// WHY TWO ACCOUNTS?
/// MemberAccount.collateral_locked decrements on each default.
/// By circle completion it no longer reflects the original amount.
/// CollateralRecord preserves total_locked permanently so
/// claim_collateral can always compute the correct return:
///   claimable = total_locked - total_slashed
///
/// This is also the account queried for transparency —
/// members and auditors can verify the full collateral history
/// for any participant in any circle at any time.
#[account]
pub struct CollateralRecord {
    /// The circle this record belongs to.
    pub circle: Pubkey,

    /// The member this record belongs to.
    pub member: Pubkey,

    /// Total collateral locked at join time.
    /// Set once in join_circle. Never changes after that.
    /// Formula: (total_members - position) × contribution_amount
    /// For position N (last): 0
    /// For position 1 (first): (N-1) × contribution_amount
    pub total_locked: u64,

    /// Total collateral released back to the member.
    /// Set in claim_collateral to the claimable amount.
    /// 0 until claim_collateral is called.
    /// Should equal total_locked - total_slashed after claim.
    pub total_released: u64,

    /// Total collateral seized through default deductions.
    /// Increments by exactly contribution_amount per default.
    /// Always a clean multiple of contribution_amount.
    /// If member never defaulted: 0.
    /// If member was kicked: equals total_locked.
    pub total_slashed: u64,

    /// True after claim_collateral has been successfully called.
    /// Prevents double-claiming.
    /// Also used to verify circuit is complete in analytics.
    pub claimed: bool,

    /// PDA bump seed.
    pub bump: u8,
}

impl CollateralRecord {
    /// Account discriminator:     8
    /// circle (Pubkey):          32
    /// member (Pubkey):          32
    /// total_locked (u64):        8
    /// total_released (u64):      8
    /// total_slashed (u64):       8
    /// claimed (bool):            1
    /// bump (u8):                 1
    /// ──────────────────────────
    /// Total:                    98
    pub const LEN: usize = 8 + 32 + 32 + 8 + 8 + 8 + 1 + 1;
}