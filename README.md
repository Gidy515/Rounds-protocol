Here is the updated README in plain text:

---

# Rounds Protocol

## Overview

Rounds Protocol is a trustless, non-custodial Rotating Savings and Credit Association (ROSCA) built on Solana using Rust and the Anchor framework.

The protocol enables groups of participants to form onchain savings circles where members contribute USDC periodically and receive the pooled pot according to a predefined payout schedule — enforced entirely by smart contracts with no human intermediary.

By replacing social trust with collateral-backed smart contract enforcement, Rounds allows savings circles to operate transparently, autonomously, and at internet scale. Locked collateral earns yield via Kamino Finance, turning a traditionally zero-sum savings instrument into a positive-sum one where participants earn while they save.

Traditional ROSCAs such as Adashe, Esusu, Ajo, Susu, Chit Funds, Tandas, and Hui rely on social trust and manual coordination. Rounds brings this centuries-old financial primitive onchain, enabling participants anywhere in the world to save collectively with verifiable rules, automated enforcement, and yield on idle capital.

---

## The Problem

Rotating savings groups are one of the most widely used financial tools globally, facilitating hundreds of billions of dollars in annual economic activity. Despite this scale, they remain almost entirely absent from decentralized finance.

Traditional ROSCAs suffer from several structural limitations:

Members can default after receiving their payout with no enforceable consequence. Group organizers act as trusted intermediaries who hold funds and coordinate manually. Contributions are tracked informally with no audit trail. Records are opaque and difficult to verify. Disputes require human intervention with no guaranteed resolution. Participants have limited recourse when members disappear. Capital sits idle throughout the circle — no yield, no productivity.

These trust assumptions limit scalability, introduce operational risk, and exclude ROSCAs from the broader DeFi ecosystem.

DeFi has built lending, trading, staking, liquid staking, perpetual markets, and structured products. It has not built a trustless primitive for cooperative savings and rotating liquidity. Rounds fills that gap.

---

## How Rounds Works

A savings circle consists of a fixed number of members — between 2 and 20 — who agree on a contribution amount and payout frequency before the circle begins.

Each member agrees to:

1. Contribute a fixed amount of USDC every cycle.
2. Lock collateral when joining, proportional to their payout position.
3. Receive exactly one pot payout during the lifetime of the circle.
4. Continue contributing until all members have received their payout.

Locked collateral earns yield via Kamino Finance throughout the circle's lifetime. Members earn passively while fulfilling their savings obligations.

The protocol enforces all rules automatically. No organizer, no intermediary, no trusted party.

---

## Collateral System

The collateral requirement for a member at position p in a circle with N members and contribution amount C is:

collateral_required = (N - p) × C

Position 1 locks the most collateral and receives the pot first. Position N locks zero collateral and receives the pot last. This sliding-scale design means early recipients put up proportionally more capital to compensate for the trust the group extends them.

The collateral invariant that always holds: a member's locked collateral is always greater than or equal to their remaining contribution obligations. This guarantees the pot is always fully funded regardless of member behavior.

If a member misses a payment, a permissionless keeper instruction seizes exactly one contribution worth of their collateral and deposits it into the pot. The pot recipient is paid in full. If a member exhausts their collateral through repeated defaults they are removed from the circle and the group restructures around the remaining members.

---

## Yield on Collateral

Phase 1 displays live yield projections using real APY data fetched from Kamino Finance's mainnet USDC lending market. Members can see exactly what their locked collateral would earn at current market rates.

Phase 2 deposits locked collateral directly into Kamino Finance K-Lend via CPI (Cross-Program Invocation). The protocol holds kUSDC receipt tokens on behalf of members. Yield accrues continuously throughout the circle lifetime. When a member claims collateral after circle completion, they receive their original USDC plus all accumulated yield — automatically, with no action required beyond the standard claim instruction.

At current Kamino USDC supply APY of approximately 5 to 9 percent, a 10-member 100 USDC circle with 4,500 USDC in total locked collateral earns between 225 and 405 USDC in annual yield. This turns the circle from a zero-sum savings rotation into a positive-sum instrument where every participant earns yield simply by participating.

This is the first ROSCA implementation where locked collateral earns yield while the circle runs.

---

## Example

Assume a 10-member circle with 100 USDC contributions and weekly frequency.

When members join, position 1 locks 900 USDC collateral plus pays 100 USDC as the cycle 1 contribution. Position 2 locks 800 USDC collateral and pays a joining premium. Each subsequent position locks progressively less collateral down to position 10 which locks zero collateral.

Total collateral locked across the circle: 4,500 USDC.

Every week, all active members contribute 100 USDC. The pot of 1,000 USDC is disbursed to the current cycle's recipient. The cycle advances. This repeats for 10 cycles until every member has received exactly one pot payout.

Throughout all 10 cycles, the 4,500 USDC in locked collateral is deployed into Kamino Finance earning yield. When each member claims their collateral after the circle completes, they receive their original collateral plus their share of the yield earned.

---

## Core Design Principles

### Trustless Operation

No organizer controls funds. All assets are held in smart contract vaults controlled by the program. The admin key cannot access member funds.

### Collateral Backing

Every member locks collateral when joining proportional to their payout position. If a member defaults, their collateral is seized automatically to cover the missed payment. The pot is always fully funded.

### Yield-Bearing Collateral

Locked collateral is productive. Rather than sitting idle, it earns yield via Kamino Finance throughout the circle's lifetime. Yield is returned to members when they claim collateral after circle completion.

### Permissionless Execution

All keeper actions — starting circles, processing defaults, disbursing pots — can be executed by any wallet. No centralized keeper infrastructure is required. This mirrors the liquidation bot model used in lending protocols like Aave and Kamino.

### Transparency

All circle state, payments, defaults, yields, and payouts are recorded onchain and publicly verifiable by anyone.

### Deterministic Payout Order

Members receive payouts according to their assigned position. Position assignment occurs at join time and is immutable.

### Sequential Circles

Multiple circles with identical parameters can coexist via a nonce seed structure. When an open circle exists for a given parameter set, new participants are directed to join it. When it fills, a new circle opens at the next nonce. This maximizes circle utilization while preventing parameter collisions.

---

## Circle Lifecycle

### 1. Protocol Initialization

Admin deploys and initializes the protocol. Creates the ProtocolConfig account and TreasuryVault. Stores the admin address, protocol fee, and pause status.

### 2. Create Circle

A user creates a savings circle by specifying the contribution amount, total member count, frequency (daily, weekly, biweekly, or monthly), and USDC mint.

The protocol creates the CircleAccount, CollateralVault, and PotVault for the circle. The circle enters Open state. The frontend automatically discovers the correct nonce for the new circle by checking existing circles at nonces 0 through 254 and directing the user to join any open circle before creating a new one.

### 3. Join Circle

Members join the circle and receive a sequential position. Each member creates a MemberAccount and CollateralRecord, deposits collateral into the CollateralVault, and deposits their first-cycle contribution into the PotVault. In Phase 2, collateral is immediately deployed into Kamino Finance to begin earning yield.

When the final seat fills, the circle transitions from Open to Ready.

### 4. Start Circle

Any wallet can start a Ready circle. The protocol transitions the circle from Ready to Active, sets cycle 1 as the current cycle, creates the PaymentRecord for cycle 1, and marks all members as paid for cycle 1 since contributions were collected at join time.

### 5. Disburse Pot

Any wallet can trigger disbursement once all active members have paid. The protocol verifies all PaymentRecord bits are set, deducts the protocol fee and routes it to the TreasuryVault, sends the remaining pot to the current cycle's recipient, and advances the cycle counter. If this was the final cycle the circle transitions from Active to Completed.

### 6. Future Contributions

For cycles 2 through N, members call pay_contribution to transfer their contribution into the PotVault. The PaymentRecord is updated. When all required payments are recorded, disburse_pot may execute.

### 7. Default Handling

If a member fails to contribute before the cycle deadline, process_default may be called by any wallet. The protocol verifies the deadline has passed and the member has not paid, seizes exactly one contribution worth of their collateral from the CollateralVault and transfers it to the PotVault, sets the member's bit in the PaymentRecord, and checks if the member should be kicked. A member is kicked only when their collateral reaches zero and rounds remain. A single default does not remove a member — they must exhaust all their collateral through repeated defaults before being removed.

### 8. Circle Completion

After the final payout the circle state transitions to Completed. In Phase 2 the protocol initiates withdrawal of all collateral positions from Kamino Finance, collecting principal plus accumulated yield.

### 9. Collateral Claims

After circle completion, members call claim_collateral to recover their remaining locked collateral plus any yield earned. Members who never defaulted recover 100 percent of their collateral plus yield. Members who defaulted recover their remaining collateral after seizures plus yield on the remaining amount. Members who were kicked recover nothing.

### 10. Cancellation

An Open circle can be cancelled if the caller is the only member (solo cancel, no deadline required) or if the cancel deadline of approximately 24 hours has passed and the circle has not filled. The cancel instruction returns all funds to the caller and transitions the circle to Cancelled state.

---

## Account Architecture

### ProtocolConfig

Global protocol configuration. Stores the admin address, protocol fee in basis points (maximum 10 percent), and pause status.

### CircleAccount

Primary circle state. Stores the contribution amount, total and active member counts, payout frequency, current cycle, payment deadline, cancel deadline, circle state, creation slot, start slot, completion slot, and nonce.

### MemberAccount

One account per member per circle. Stores the member's wallet address, payout position, current collateral locked, payout received status, default status, and kick status.

### CollateralRecord

Audit trail for a member's collateral movements. Stores total collateral locked at join time, total collateral released to member, and total collateral seized due to defaults. The invariant total_locked equals total_released plus total_slashed plus current collateral_locked is permanently verifiable onchain.

### PaymentRecord

Tracks payment status for a single cycle using a compact 64-bit bitmask. Position p corresponds to bit p minus 1. When set, the member has paid either voluntarily or via default processing.

---

## Vault Architecture

### Pot Vault

Holds contribution funds for the current cycle. Filled by member contributions and collateral seizures. Empties completely after every disbursement.

### Collateral Vault

Holds all member collateral for the circle. In Phase 2, balance is deployed to Kamino Finance and the vault holds kUSDC receipt tokens instead of USDC directly. Moves only on default (partial seizure to Pot Vault) or circle completion (full withdrawal from Kamino and return to members).

### Treasury Vault

Receives protocol fees from every disbursement. Controlled exclusively by the protocol admin.

---

## Yield Integration

### Phase 1 — Live Projections

The frontend displays real-time yield projections for every member's locked collateral. APY data is fetched live from Kamino Finance's mainnet USDC lending market API. Projections show what collateral would earn at current market rates, estimated yield earned so far in active circles, and projected annual yield.

### Phase 2 — On-Chain Yield via Kamino CPI

The join_circle instruction CPIs into Kamino Finance K-Lend to deposit USDC collateral. The program holds kUSDC receipt tokens in the CollateralVault on behalf of members. Yield accrues to the kUSDC position automatically. The claim_collateral instruction CPIs into Kamino to redeem kUSDC for USDC plus accumulated yield and transfers the full amount to the member. No member action is required beyond the standard join and claim flow.

### Why Kamino Finance

Kamino is the largest lending protocol on Solana with approximately 3.2 billion USD in TVL. The USDC supply APY has ranged between 4 and 9 percent in 2026. Kamino has been audited by OtterSec, Halborn, and Offside Labs and has maintained a strong security record. The USDC market is deep and liquid, making it reliable infrastructure for collateral yield rather than a speculative yield source.

---

## Circle States

Open — accepting members.
Ready — circle full, waiting to start.
Active — cycles running, contributions and disbursements in progress.
Completed — all payouts completed, collateral claimable.
Cancelled — circle failed to fill before the cancel deadline.

---

## Security Model

Rounds uses multiple layers of protection including program-derived account ownership, seed verification on all accounts, Anchor account constraint validation, collateral-backed guarantees enforced at every state transition, permissionless execution with onchain eligibility checks, overflow-safe arithmetic throughout, explicit state machine with validated transitions, token mint validation on all transfers, and authority validation on member-specific instructions.

No participant can skip contribution requirements, claim multiple payouts, modify payout order, withdraw protocol funds, steal collateral, or front-run disbursements without satisfying protocol rules enforced by the smart contract.

---

## Protocol Fees

Each disbursement includes a protocol fee calculated as pot times protocol_fee_bps divided by 10,000. The fee is routed to the TreasuryVault. The remaining amount is sent to the recipient. The maximum protocol fee is 10 percent (1000 basis points). The default fee at launch is 50 basis points (0.5 percent).

---

## Technology Stack

Solana blockchain, Rust programming language, Anchor framework version 0.30 and above, SPL Token 2022 compatible interfaces, program-derived addresses for all accounts and vaults, Kamino Finance K-Lend for collateral yield (Phase 2), Next.js 15 and TypeScript for the frontend, Solana Wallet Adapter for wallet connectivity.

---

## Test Suite

The protocol ships with 39 tests covering the full circle lifecycle from initialization through collateral claims, cancel flows, default processing and collateral seizure, member kick logic, security edge cases including double join and unauthorized instructions, nonce seed structure with sequential circle creation, and duplicate nonce rejection.

All 39 tests pass on localnet.

---

## Deployed Contracts

Program ID: 7BBvnkQ4AKMFU6EfWvScSqi69eu9TjLoDzpmzG8ZeFhN

Protocol Config: GA1F8dEDGnzBPhmrktT4MQAZLBWbLJDA1jHUuzvRSJZ7

Treasury Vault: 7j4VdqEGyjBdsYdy2E2oUuGf7pwnHERfxTLfpHwEYYGx

USDC Mint (devnet): 6dLsmJXz5P9eoWDtmyoYEZigejtN3tyBiZMpEiLsD7sh

Network: Solana Devnet — Mainnet launch pending security audit.

---

## Vision

Rounds transforms one of humanity's oldest financial coordination mechanisms into an open, transparent, programmable, and yield-bearing financial primitive.

ROSCAs have operated for centuries without blockchain infrastructure not because participants do not want it but because no one built the right primitive. Rounds builds it.

The immediate goal is a trustless ROSCA on Solana with yield-bearing collateral. The longer-term vision is a composable primitive that supports circle positions as transferable NFTs, multi-asset collateral including tokenized RWAs, undercollateralized circles for identity-verified participants, keeper incentive markets, and mobile-first interfaces targeting the communities that use ROSCAs most heavily across West Africa, East Africa, and the global diaspora.

DeFi has brought lending, trading, staking, and derivatives onchain. Rounds brings cooperative savings.

---

## Important Docs

Letter of intent: assets/Turbin3_capstone_project_definition_and_market_analysis.pdf

User stories and onchain requirements: assets/rounds_protocol_user_stories_and_onchain_requirements.pdf

Architecture diagrams: assets/Rounds Protocol Architecture Diagrams.pdf

---

Rounds Protocol — Trustless Rotating Savings on Solana.

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

<p align="center">
<img src="/assets/Screenshot from 2026-07-03 23-01-50.png">
</p>

<p align="center">
<img src="/assets/Screenshot from 2026-07-03 23-02-03.png">
</p>
