use anchor_lang::prelude::*;

// #[constant]
// pub const SEED: &str = "anchor";

/// ─────────────────────────────────────────────────────────
/// constants.rs
/// Protocol-wide constants. Every magic number in the program
/// lives here. Nothing is hardcoded inline in any instruction.
/// ─────────────────────────────────────────────────────────

/// Premium percentage paid by positions 2-N to position 1.
/// Hardcoded at 10% for all circles — not configurable.
/// Applied as: contribution_amount * PREMIUM_BPS / 10_000
/// 1000 bps = 10%
pub const PREMIUM_BPS: u64 = 1_000;

/// Denominator for all basis point calculations.
pub const BPS_DENOMINATOR: u64 = 10_000;

/// Cancel window in slots. A circle that has not filled
/// within this many slots of creation can be cancelled
/// by any wallet via cancel_circle.
/// 216_000 slots × 400ms/slot ≈ 24 hours.
pub const CANCEL_DEADLINE_SLOTS: u64 = 216_000;

/// Cycle duration in slots for each payout frequency.
/// Derived from: duration_in_seconds / 0.4 (slot time)
/// Used in create_circle to populate cycle_duration_slots
/// from the creator's selected PayoutFrequency enum.

/// Daily: 86_400 seconds / 0.4 = 216_000 slots
pub const SLOTS_PER_DAY: u64 = 216_000;

/// Weekly: 604_800 seconds / 0.4 = 1_512_000 slots
pub const SLOTS_PER_WEEK: u64 = 1_512_000;

/// Biweekly: 1_209_600 seconds / 0.4 = 3_024_000 slots
pub const SLOTS_PER_BIWEEK: u64 = 3_024_000;

/// Monthly: 2_592_000 seconds / 0.4 = 6_480_000 slots
/// Based on 30-day month.
pub const SLOTS_PER_MONTH: u64 = 6_480_000;

/// Minimum contribution amount in USDC lamports.
/// USDC has 6 decimal places: 1 USDC = 1_000_000 lamports.
/// Minimum is 1 USDC to prevent dust circles.
pub const MIN_CONTRIBUTION_AMOUNT: u64 = 1_000_000;

/// Minimum number of members per circle.
/// A circle needs at least 2 people to make sense.
pub const MIN_MEMBERS: u8 = 2;

/// Maximum number of members per circle.
/// Capped at 20 for MVP. PaymentRecord uses a u64 bitmask
/// which supports up to 64 members — so this can be raised
/// post-MVP without any account structure changes.
pub const MAX_MEMBERS: u8 = 20;

/// Maximum protocol fee in basis points.
/// Prevents admin from setting an abusive fee rate.
/// 1000 bps = 10% maximum.
pub const MAX_PROTOCOL_FEE_BPS: u16 = 1_000;
