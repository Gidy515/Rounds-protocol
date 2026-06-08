use anchor_lang::prelude::*;

/// PaymentRecord — per-cycle payment tracking PDA.
/// Seeds: [b"payment", circle_pubkey.as_ref(), &[cycle_number]]
///
/// One account per cycle per circle.
/// Created in start_circle for cycle 1.
/// Created in disburse_pot for each subsequent cycle.
///
/// WHY A BITMASK?
/// A u64 bitmask is the most efficient way to track N members'
/// payment status in a single account read.
/// Bit 0 = position 1 paid
/// Bit 1 = position 2 paid
/// Bit N-1 = position N paid
///
/// To check if position P has paid:
///   let bit = 1u64 << (P - 1);
///   paid_flags & bit != 0
///
/// To check if ALL active members have paid:
///   let expected = (1u64 << active_members) - 1;
///   paid_flags & expected == expected
///
/// Supports up to 64 members (u64 has 64 bits).
/// Our max is 20 members so this is more than sufficient.
///
/// Bits for kicked members are pre-set by process_default
/// so disburse_pot never blocks waiting for a kicked member.
///
/// LIFECYCLE:
/// Created fresh each cycle with paid_flags = 0.
/// Fills up as members pay or are processed as defaulters.
/// Once disburse_pot fires, this record is historical.
/// It is never deleted — it becomes the permanent payment
/// history for that cycle, queryable by anyone.
#[account]
pub struct PaymentRecord {
    /// The circle this record belongs to.
    pub circle: Pubkey,

    /// The cycle number this record tracks. 1-indexed.
    pub cycle: u8,

    /// Bitmask tracking payment status for this cycle.
    /// Bit N = 1 means position N+1 has been covered.
    /// Covered means: paid via pay_contribution OR
    /// covered by collateral seizure in process_default.
    /// Starts at 0. Fills toward the expected_mask value.
    /// When paid_flags & expected_mask == expected_mask:
    ///   disburse_pot can fire.
    pub paid_flags: u64,

    /// PDA bump seed.
    pub bump: u8,
}

impl PaymentRecord {
    /// Account discriminator:  8
    /// circle (Pubkey):       32
    /// cycle (u8):             1
    /// paid_flags (u64):       8
    /// bump (u8):              1
    /// ────────────────────────
    /// Total:                 50
    pub const LEN: usize = 8 + 32 + 1 + 8 + 1;
}