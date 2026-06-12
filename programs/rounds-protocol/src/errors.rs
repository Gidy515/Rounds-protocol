/// ─────────────────────────────────────────────────────────
/// errors.rs
/// All RoundsError codes with human-readable messages.
/// Every error message is written for the end user —
/// not for the developer. A wallet or frontend should be
/// able to display these directly without translation.
/// ─────────────────────────────────────────────────────────

use anchor_lang::prelude::*;

#[error_code]
pub enum RoundsError {

    // ── Protocol-level errors ────────────────────────────

    #[msg("The protocol is currently paused. Please try again later.")]
    ProtocolPaused,

    #[msg("Only the protocol admin can call this instruction.")]
    Unauthorized,

    #[msg("Protocol fee cannot exceed 10%.")]
    FeeTooHigh,

    // ── Circle creation errors ───────────────────────────

    #[msg("A circle with these exact settings is already open and accepting members. Please join that circle instead.")]
    CircleAlreadyOpen,

    #[msg("Member count must be between 2 and 20.")]
    InvalidMemberCount,

    #[msg("Contribution amount must be at least 1 USDC.")]
    ContributionTooLow,

    #[msg("Invalid payout frequency selected.")]
    InvalidFrequency,

    // ── Circle state errors ──────────────────────────────

    #[msg("This circle is no longer accepting members.")]
    CircleNotOpen,

    #[msg("This circle is not ready to start yet. All seats must be filled first.")]
    CircleNotReady,

    #[msg("This circle is not currently active.")]
    CircleNotActive,

    #[msg("This circle has not completed yet. All cycles must be disbursed first.")]
    CircleNotComplete,

    #[msg("This circle has already been cancelled.")]
    CircleAlreadyCancelled,

    #[msg("The 24-hour cancel window has not passed yet. The circle can only be cancelled if it has not filled within 24 hours of creation.")]
    CancelWindowNotPassed,

    // ── Joining errors ───────────────────────────────────

    #[msg("This circle is full. No more members can join.")]
    CircleFull,

    #[msg("Cycle 1 contributions were collected at join time and cannot be paid again.")]
    Cycle1AlreadyFunded,

    #[msg("You have already joined this circle.")]
    AlreadyMember,

    #[msg("Insufficient USDC balance to join this circle. Please check your wallet and try again.")]
    InsufficientBalance,

    #[msg("Position 1 wallet account is required for premium routing but was not provided.")]
    Position1WalletMissing,

    // ── Contribution errors ──────────────────────────────

    #[msg("You have already paid your contribution for this cycle.")]
    AlreadyPaid,

    #[msg("The payment deadline for this cycle has passed. Your contribution cannot be accepted.")]
    DeadlinePassed,

    #[msg("The deadline for this cycle has not passed yet. Default processing is not available.")]
    DeadlineNotPassed,

    #[msg("This member has already paid their contribution for this cycle.")]
    MemberAlreadyPaid,

    #[msg("You have been removed from this circle due to collateral exhaustion and cannot make further payments.")]
    MemberKicked,

    #[msg("You are not an active member of this circle.")]
    NotAMember,

    #[msg("You have not joined this circle and cannot make contributions.")]
    InvalidPaymentRecord,

    // ── Disbursement errors ──────────────────────────────

    #[msg("Not all active members have paid their contribution for this cycle yet.")]
    NotAllMembersPaid,

    #[msg("This member is not the designated recipient for the current cycle.")]
    WrongRecipient,

    #[msg("The pot for this cycle has already been disbursed.")]
    AlreadyDisbursed,

    // ── Default and kick errors ──────────────────────────

    #[msg("This member has not missed a payment. Default processing cannot be triggered.")]
    MemberNotDefaulted,

    #[msg("This member has already been removed from the circle.")]
    MemberAlreadyKicked,

    // ── Collateral claim errors ──────────────────────────

    #[msg("You have already claimed your collateral for this circle.")]
    AlreadyClaimed,

    #[msg("There is no collateral to claim. Your locked amount was fully used to cover missed payments.")]
    NothingToClaim,

    #[msg("Unauthorized collateral claim. You can only claim your own collateral.")]
    UnauthorizedClaim,

    // ── Member permission errors ─────────────────────────

    #[msg("Unauthorized. This instruction can only be called by the member who owns this account.")]
    UnauthorizedMember,

    // ── Math errors ──────────────────────────────────────

    #[msg("A calculation error occurred. This is likely a bug — please report it.")]
    MathOverflow,

    #[msg("A calculation error occurred. This is likely a bug — please report it.")]
    MathUnderflow,

    #[msg("The specified cycle does not match the circle's current active cycle.")]
    InvalidCycle,

    // ── Invariant errors ─────────────────────────────────

    #[msg("Collateral invariant violated. Locked collateral cannot be less than remaining obligations. This is a critical protocol error.")]
    InvariantViolation,

    // ── Account errors ───────────────────────────────────

    #[msg("Invalid USDC mint. This circle only accepts the configured USDC token.")]
    InvalidMint,

    #[msg("Invalid token account. The provided account does not match the expected owner or mint.")]
    InvalidTokenAccount,

    // ── Treasury errors ───────────────────────────────────
    #[msg("Protocol is not currently paused.")]
    ProtocolNotPaused,

    #[msg("Withdrawal amount must be greater than zero.")]
    InvalidWithdrawAmount,

    #[msg("Withdrawal amount exceeds treasury vault balance.")]
    InsufficientTreasuryBalance,
}