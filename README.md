# Rounds Protocol

## Overview

Rounds Protocol is a decentralized Rotating Savings and Credit Association (ROSCA) built on Solana using Rust and Anchor.

The protocol enables groups of users to form onchain savings circles where members contribute USDC periodically and receive pooled funds according to a predefined payout schedule.

By replacing social trust with smart contract enforcement and collateral-backed guarantees, Rounds allows savings circles to operate transparently, autonomously, and without intermediaries.

Traditional ROSCAs such as Adashe, Esusu, Ajo, Susu, Chit Funds, Tandas, and Hui rely on social trust and manual coordination. Rounds brings this centuries-old financial primitive onchain, enabling participants anywhere in the world to save collectively with verifiable rules and automated enforcement.

---

# The Problem

Rotating savings groups are one of the most widely used financial tools globally.

However, traditional ROSCAs suffer from several limitations:

- Members can default after receiving their payout.
- Group organizers act as trusted intermediaries.
- Contributions are tracked manually.
- Records are often opaque and difficult to audit.
- Disputes require human intervention.
- Participants have limited recourse when members disappear.

These trust assumptions limit scalability and introduce operational risk.

Rounds solves these problems through smart contracts.

---

# How Rounds Works

A savings circle consists of a fixed number of members.

Each member agrees to:

1. Contribute a fixed amount of USDC every cycle.
2. Lock collateral when joining.
3. Receive exactly one payout during the lifetime of the circle.
4. Continue contributing until all members have received their payout.

The protocol enforces these rules automatically.

---

# Example

Assume:

- 10 members
- Contribution = 100 USDC
- Weekly frequency

Every member deposits:

- 100 USDC first contribution
- 100 USDC collateral

Total joining deposit:

200 USDC

When all 10 members join:

Cycle 1 pot:

10 × 100 USDC = 1,000 USDC

The member in Position 1 receives the first payout.

The circle then advances to Cycle 2.

All remaining active members contribute again.

Another 1,000 USDC pot is formed and paid to Position 2.

This process repeats until every active member has received their payout.

---

# Core Design Principles

## Trustless Operation

No organizer controls funds.

All assets are held inside program-owned vaults controlled by PDAs.

## Collateral Backing

Every member locks collateral when joining.

If a member defaults, their collateral can be seized and used to cover missed obligations.

## Permissionless Execution

Operational actions can be executed by anyone.

No centralized keeper infrastructure is required.

## Transparency

All circle state, payments, defaults, and payouts are recorded onchain.

## Deterministic Payout Order

Members receive payouts according to their assigned position.

Position assignment occurs when joining.

---

# Circle Lifecycle

## 1. Protocol Initialization

Admin deploys and initializes the protocol.

Creates:

- ProtocolConfig PDA
- Treasury Vault PDA

Stores:

- Admin address
- Protocol fee
- Pause status

---

## 2. Create Circle

A user creates a savings circle by specifying:

- Contribution amount
- Total member count
- Frequency
- USDC mint

Examples:

- 50 USDC weekly
- 20 USDC monthly
- 100 USDC biweekly

The protocol creates:

- CircleAccount PDA
- Collateral Vault PDA
- Pot Vault PDA

The circle enters:

Open State

---

## 3. Join Circle

Members join the circle.

Each member:

- Receives a position
- Creates a MemberAccount
- Deposits collateral
- Deposits first-cycle contribution

Funds are split automatically:

Contribution → Pot Vault

Collateral → Collateral Vault

When the circle reaches capacity:

Open → Ready

---

## 4. Start Circle

Anyone can start a ready circle.

The protocol:

- Transitions Ready → Active
- Sets Cycle 1
- Creates PaymentRecord for Cycle 1
- Marks all members as paid for Cycle 1

Cycle 1 contributions were already collected during joining.

No additional payments are required.

---

## 5. Disburse Pot

Anyone may trigger disbursement.

The protocol:

- Verifies all required contributions are present
- Deducts protocol fee
- Sends payout to current recipient
- Advances the circle

If final cycle:

Active → Completed

Otherwise:

Current cycle increments.

---

## 6. Future Contributions

For cycles 2 through N:

Members call:

pay_contribution

The contribution is transferred into the Pot Vault.

PaymentRecord is updated.

When all required payments are recorded:

disburse_pot may execute.

---

## 7. Default Handling

If a member fails to contribute before the deadline:

process_default may be called.

The protocol:

- Marks the member as kicked
- Seizes collateral
- Covers the missed contribution
- Updates payment tracking

The circle continues operating.

A single default cannot halt the group.

---

## 8. Circle Completion

After the final payout:

Circle State → Completed

The protocol records:

- Completion slot
- Final cycle

Remaining collateral becomes claimable.

---

## 9. Collateral Claims

After circle completion:

Members may reclaim remaining collateral.

This prevents collateral from remaining locked indefinitely.

---

# Account Architecture

## ProtocolConfig

Global protocol configuration.

Stores:

- Admin
- Fee rate
- Pause status

---

## CircleAccount

Primary circle state.

Stores:

- Contribution amount
- Member count
- Active members
- Frequency
- Current cycle
- Deadlines
- Circle state

---

## MemberAccount

One account per member per circle.

Stores:

- Wallet address
- Position
- Collateral amount
- Default count
- Payout status
- Kick status

---

## PaymentRecord

Tracks payment status for a single cycle.

Uses a compact u64 bitmask.

Example:

Position 1 paid → Bit 0

Position 2 paid → Bit 1

Position 3 paid → Bit 2

This allows payment verification with minimal storage.

---

# Vault Architecture

## Pot Vault

Holds contribution funds.

Used to pay cycle recipients.

Empties after every disbursement.

---

## Collateral Vault

Holds member collateral.

Used only when:

- Default occurs
- Circle completes

---

## Treasury Vault

Receives protocol fees.

Controlled exclusively by protocol governance.

---

# States

## Open

Accepting members.

## Ready

Circle full.

Waiting to start.

## Active

Currently operating.

## Completed

All payouts completed.

## Cancelled

Circle failed to fill before deadline.

---

# Security Model

Rounds uses multiple layers of protection:

- PDA ownership
- Seed verification
- Anchor account constraints
- Collateral-backed guarantees
- Permissionless execution
- Overflow-safe arithmetic
- Explicit state transitions
- Token mint validation
- Authority validation

No participant can:

- Skip contribution requirements
- Claim multiple payouts
- Modify payout order
- Withdraw protocol funds
- Steal collateral

without satisfying protocol rules.

---

# Protocol Fees

Each disbursement may include a protocol fee.

Fee calculation:

fee = pot × protocol_fee_bps / 10,000

The fee is routed to the Treasury Vault.

The remaining amount is sent to the recipient.

---

# Technology Stack

- Solana
- Rust
- Anchor Framework
- SPL Token-2022 Compatible Interfaces (SPL Token Interface)
- Program Derived Addresses (PDAs)

---

# Vision

Rounds transforms one of humanity's oldest financial coordination mechanisms into an open, transparent, and programmable financial primitive.

By combining cooperative savings with smart contract enforcement, Rounds enables trustless community finance at internet scale while preserving the simplicity that made ROSCAs successful for generations.

PROGRAM_ID = 7BBvnkQ4AKMFU6EfWvScSqi69eu9TjLoDzpmzG8ZeFhN

## Rounds protocol tests suit

<p align="center">
    <img src="/assets/Rounds-protocol-tests.png" alt="Rounds protocol tests suit">
</p>

## Important docs

## Turbin3 Capstone Project Definition & Market Analysis

📄 [Letter of intent](assets/Turbin3_capstone_project_definition_and_market_analysis.pdf)

## User Stories & On-Chain Requirements

📄 [User stories](assets/rounds_protocol_user_stories_and_onchain_requirements.pdf)

## Architecture Design

📄 [Architectural diagram](assets/Rounds%20Protocol%20—%20Architecture%20Diagrams.pdf)
