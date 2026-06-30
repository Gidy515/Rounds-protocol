use anchor_lang::prelude::*;

/// CircleState — the five valid states a circle can be in.
/// Transitions are strictly enforced — no instruction can
/// move a circle to an invalid state or skip a state.
#[derive(AnchorSerialize, AnchorDeserialize, Clone, PartialEq, Default, Debug)]
pub enum CircleState {
    /// Circle has been created and is accepting members.
    /// join_circle can be called.
    /// Transitions to Ready when final seat fills.
    /// Transitions to Cancelled if 24hr timeout passes unfilled.
    #[default]
    Open,

    /// All seats are filled. Awaiting start_circle call.
    /// No new members can join.
    /// Transitions to Active when start_circle is called.
    Ready,

    /// Circle is running. Cycles are in progress.
    /// pay_contribution, disburse_pot, process_default
    /// are all valid in this state.
    /// Transitions to Completed after final disbursement.
    Active,

    /// All cycles have been disbursed.
    /// claim_collateral is now available to all members.
    /// Terminal state.
    Completed,

    /// Circle never filled within the 24hr cancel window.
    /// Members who joined can call claim_collateral
    /// to recover their locked funds.
    /// Terminal state.
    Cancelled,
}

/// PayoutFrequency — the four cycle duration options.
/// Mapped to Solana slot counts internally.
/// Users select a human-readable label.
/// The program stores both the enum and the derived slot count.
#[derive(AnchorSerialize, AnchorDeserialize, Clone, PartialEq, Default, Debug)]
pub enum PayoutFrequency {
    /// ~216,000 slots (~24 hours at 400ms/slot)
    Daily,

    /// ~1,512,000 slots (~7 days)
    Weekly,

    /// ~3,024,000 slots (~14 days)
    Biweekly,

    /// ~6,480,000 slots (~30 days)
    #[default]
    Monthly,
}

/// CircleAccount — per-circle state PDA.
/// Seeds: [b"circle", contribution_amount.to_le_bytes(),
///          &[total_members], &[frequency as u8],
///          usdc_mint.as_ref()]
///
/// The seed fingerprint is the duplicate prevention mechanism.
/// Two circles with identical parameters resolve to the same
/// PDA address. If that address is already in Open state,
/// create_circle returns a descriptive error.
#[account]
#[derive(Default)]
pub struct CircleAccount {
    /// Fixed contribution amount per cycle in USDC lamports.
    /// 6 decimal places: 100 USDC = 100_000_000
    /// Set at creation. Never changes.
    pub contribution_amount: u64,

    /// Total number of seats in this circle.
    /// Set at creation. Never changes. Range: 2–20.
    pub total_members: u8,

    /// Number of members currently active (not kicked).
    /// Starts equal to total_members once circle fills.
    /// Decrements when a member is kicked via process_default.
    /// This is what disburse_pot uses for pot calculation
    /// and all-paid verification — not total_members.
    pub active_members: u8,

    /// Number of members who have joined so far.
    /// Used to assign positions (position = current_members + 1).
    /// Increments on every join_circle call.
    pub current_members: u8,

    /// Selected payout frequency.
    /// Human-readable label chosen by creator.
    pub frequency: PayoutFrequency,

    /// Cycle duration in slots derived from frequency.
    /// Daily:     216_000
    /// Weekly:    1_512_000
    /// Biweekly:  3_024_000
    /// Monthly:   6_480_000
    /// Stored here so every instruction can read it directly
    /// without recomputing from frequency enum.
    pub cycle_duration_slots: u64,

    /// USDC mint address for this circle.
    /// All collateral and contributions must use this mint.
    /// Validated on every join and pay instruction.
    pub usdc_mint: Pubkey,

    /// Current circle state.
    /// Enforced as a constraint on every instruction.
    pub state: CircleState,

    /// Current active cycle number. 1-indexed.
    /// 0 before the circle starts.
    /// Increments in disburse_pot after each disbursement.
    /// When current_cycle > active_members → Completed.
    pub current_cycle: u8,

    /// Slot deadline for the current cycle's contributions.
    /// Members must pay before this slot.
    /// Set in start_circle for cycle 1.
    /// Reset in disburse_pot for each subsequent cycle.
    /// If current slot > this value, process_default is valid.
    pub cycle_deadline_slot: u64,

    /// Slot after which cancel_circle can be called.
    /// Set in create_circle as:
    ///   Clock::get().slot + CANCEL_DEADLINE_SLOTS (216_000)
    /// Only relevant while state = Open.
    pub cancel_deadline_slot: u64,

    /// Slot at which the circle transitioned to Active.
    /// Set in start_circle. Zero before the circle starts.
    /// Used to derive the full circle timeline on the frontend.
    pub started_at_slot: u64,

    /// Slot at which the final cycle was disbursed.
    /// Set in disburse_pot when state transitions to Completed.
    /// Zero until circle completes.
    pub completed_at_slot: u64,

    /// PDA bump seed. Stored to avoid recomputation.
    pub bump: u8,

    /// Nonce — allows multiple circles with identical parameters.
    /// Increments when the previous circle at nonce N-1 is full.
    /// Frontend finds the correct nonce automatically.
    pub nonce: u8,
}

impl CircleAccount {
    /// Account discriminator:        8
    /// contribution_amount (u64):    8
    /// total_members (u8):           1
    /// active_members (u8):          1
    /// current_members (u8):         1
    /// frequency (enum u8):          1
    /// cycle_duration_slots (u64):   8
    /// usdc_mint (Pubkey):          32
    /// state (enum u8):              1
    /// current_cycle (u8):           1
    /// cycle_deadline_slot (u64):    8
    /// cancel_deadline_slot (u64):   8
    /// started_at_slot (u64):        8  ← new
    /// completed_at_slot (u64):      8  ← new
    /// bump (u8):                    1
    /// nonce (u8):                   1  ← new
    /// ─────────────────────────────
    /// Total:                       95
    pub const LEN: usize = 8 + 8 + 1 + 1 + 1 + 1 + 8 + 32 + 1 + 1 + 8 + 8 + 8 + 8 + 1 + 1;
}