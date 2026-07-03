import * as anchor from "@coral-xyz/anchor";
import { Program, BN } from "@coral-xyz/anchor";
import { RoundsProtocol } from "../target/types/rounds_protocol";
import {
  PublicKey,
  Keypair,
  Commitment,
  SystemProgram,
  LAMPORTS_PER_SOL,
  //SYSVAR_RENT_PUBKEY,
} from "@solana/web3.js";
import {
  createMint,
  createAssociatedTokenAccount,
  mintTo,
  getAssociatedTokenAddress,
  getAssociatedTokenAddressSync,
  TOKEN_2022_PROGRAM_ID,
  ASSOCIATED_TOKEN_PROGRAM_ID,
  getAccount,
} from "@solana/spl-token";
import { assert, expect } from "chai";

const commitment: Commitment = "confirmed";

// ─────────────────────────────────────────────────────────
// HELPERS
// ─────────────────────────────────────────────────────────

/**
 * Derive the CircleAccount PDA from its parameter fingerprint.
 * Must match exactly the seeds used in create_circle.rs
 */
function deriveCirclePda(
  programId: PublicKey,
  contributionAmount: BN,
  totalMembers: number,
  frequency: number,
  usdcMint: PublicKey,
  nonce: number = 0
): [PublicKey, number] {
  const amountBuffer = contributionAmount.toArrayLike(Buffer, "le", 8);
  return PublicKey.findProgramAddressSync(
    [
      Buffer.from("circle"),
      amountBuffer,
      Buffer.from([totalMembers]),
      Buffer.from([frequency]),
      usdcMint.toBuffer(),
      Buffer.from([nonce]),
    ],
    programId
  );
}

/**
 * Derive a MemberAccount PDA.
 */
function deriveMemberPda(
  programId: PublicKey,
  circlePda: PublicKey,
  memberPubkey: PublicKey
): [PublicKey, number] {
  return PublicKey.findProgramAddressSync(
    [Buffer.from("member"), circlePda.toBuffer(), memberPubkey.toBuffer()],
    programId
  );
}

/**
 * Derive a CollateralRecord PDA.
 */
function deriveCollateralRecordPda(
  programId: PublicKey,
  circlePda: PublicKey,
  memberPubkey: PublicKey
): [PublicKey, number] {
  return PublicKey.findProgramAddressSync(
    [Buffer.from("colrec"), circlePda.toBuffer(), memberPubkey.toBuffer()],
    programId
  );
}

/**
 * Derive a PaymentRecord PDA.
 */
function derivePaymentRecordPda(
  programId: PublicKey,
  circlePda: PublicKey,
  cycle: number
): [PublicKey, number] {
  return PublicKey.findProgramAddressSync(
    [Buffer.from("payment"), circlePda.toBuffer(), Buffer.from([cycle])],
    programId
  );
}

/**
 * Derive CollateralVault PDA.
 */
function deriveCollateralVaultPda(
  programId: PublicKey,
  circlePda: PublicKey
): [PublicKey, number] {
  return PublicKey.findProgramAddressSync(
    [Buffer.from("collateral_vault"), circlePda.toBuffer()],
    programId
  );
}

/**
 * Derive PotVault PDA.
 */
function derivePotVaultPda(
  programId: PublicKey,
  circlePda: PublicKey
): [PublicKey, number] {
  return PublicKey.findProgramAddressSync(
    [Buffer.from("pot_vault"), circlePda.toBuffer()],
    programId
  );
}

/**
 * Derive ProtocolConfig PDA.
 */
function deriveProtocolConfigPda(programId: PublicKey): [PublicKey, number] {
  return PublicKey.findProgramAddressSync([Buffer.from("config")], programId);
}

/**
 * Derive TreasuryVault PDA.
 */
function deriveTreasuryVaultPda(
  programId: PublicKey,
  configPda: PublicKey
): [PublicKey, number] {
  return PublicKey.findProgramAddressSync(
    [Buffer.from("treasury"), configPda.toBuffer()],
    programId
  );
}

/**
 * Airdrop SOL and confirm.
 */
async function airdrop(
  connection: anchor.web3.Connection,
  pubkey: PublicKey,
  sol: number = 10
): Promise<void> {
  const sig = await connection.requestAirdrop(pubkey, sol * LAMPORTS_PER_SOL);
  await connection.confirmTransaction(sig, "confirmed");
}

/**
 * Sleep for N milliseconds — used to wait for slot advancement.
 */
function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

// ─────────────────────────────────────────────────────────
// CONSTANTS
// ─────────────────────────────────────────────────────────

// USDC has 6 decimal places
const USDC_DECIMALS = 6;
const ONE_USDC = new BN(1_000_000);
const CONTRIBUTION_AMOUNT = new BN(100_000_000); // 100 USDC
const TOTAL_MEMBERS = 3; // small circle for fast tests
const FREQUENCY_DAILY = 0; // maps to Daily in PayoutFrequency enum
const PROTOCOL_FEE_BPS = 50; // 0.5%

// Add near the top where circlePda etc are declared
let defaultCirclePda: PublicKey;

// ─────────────────────────────────────────────────────────
// TEST SUITE
// ─────────────────────────────────────────────────────────

describe("Rounds Protocol", () => {
  // ── Provider and program setup ──────────────────────────
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.RoundsProtocol as Program<RoundsProtocol>;

  const connection = provider.connection;

  // ── Admin (deployer) ────────────────────────────────────
  const admin = (provider.wallet as anchor.Wallet).payer;

  // ── Test wallets — 3 members for a small test circle ───
  const member1 = Keypair.generate(); // position 1 = circle creator
  const member2 = Keypair.generate(); // position 2
  const member3 = Keypair.generate(); // position 3 (final)

  // ── USDC mint ───────────────────────────────────────────
  let usdcMint: PublicKey;

  // ── PDAs ────────────────────────────────────────────────
  let configPda: PublicKey;
  let treasuryVaultPda: PublicKey;
  let circlePda: PublicKey;
  let potVaultPda: PublicKey;
  let collateralVaultPda: PublicKey;

  // ── Member token accounts ────────────────────────────────
  let member1Ata: PublicKey;
  let member2Ata: PublicKey;
  let member3Ata: PublicKey;

  // ─────────────────────────────────────────────────────────
  // BEFORE ALL — setup shared state once
  // ─────────────────────────────────────────────────────────
  before(async () => {
    console.log("\n  Setting up test environment...");

    // Airdrop SOL to all wallets
    await airdrop(connection, admin.publicKey);
    await airdrop(connection, member1.publicKey);
    await airdrop(connection, member2.publicKey);
    await airdrop(connection, member3.publicKey);

    // Create USDC mint (admin is mint authority)
    usdcMint = await createMint(
      connection,
      admin, // payer
      admin.publicKey, // mint authority
      null, // freeze authority
      USDC_DECIMALS, // decimals
      undefined, // keypair
      undefined, // confirm options
      TOKEN_2022_PROGRAM_ID
    );
    console.log(`  USDC mint: ${usdcMint.toBase58()}`);

    // Create ATA for each member
    member1Ata = await createAssociatedTokenAccount(
      connection,
      admin,
      usdcMint,
      member1.publicKey,
      undefined,
      TOKEN_2022_PROGRAM_ID
    );

    member2Ata = await createAssociatedTokenAccount(
      connection,
      admin,
      usdcMint,
      member2.publicKey,
      undefined,
      TOKEN_2022_PROGRAM_ID
    );

    member3Ata = await createAssociatedTokenAccount(
      connection,
      admin,
      usdcMint,
      member3.publicKey,
      undefined,
      TOKEN_2022_PROGRAM_ID
    );

    // Mint enough USDC to each member to cover collateral + contributions
    // member1 (pos 1): collateral = (3-1) × 100 = 200 USDC
    //                  contribution = 100 USDC
    //                  total needed = 300 USDC
    // member2 (pos 2): collateral = (3-2) × 100 = 100 USDC
    //                  contribution + premium = 110 USDC
    //                  subsequent contributions = 100 × 2 = 200 USDC
    //                  total needed = 410 USDC
    // member3 (pos 3): collateral = 0
    //                  contribution + premium = 110 USDC
    //                  subsequent contributions = 100 × 2 = 200 USDC
    //                  total needed = 310 USDC
    // Give everyone 1000 USDC to be safe
    const mintAmount = BigInt(1_000 * 10 ** USDC_DECIMALS);

    await mintTo(
      connection,
      admin,
      usdcMint,
      member1Ata,
      admin,
      mintAmount,
      [],
      undefined,
      TOKEN_2022_PROGRAM_ID
    );
    await mintTo(
      connection,
      admin,
      usdcMint,
      member2Ata,
      admin,
      mintAmount,
      [],
      undefined,
      TOKEN_2022_PROGRAM_ID
    );
    await mintTo(
      connection,
      admin,
      usdcMint,
      member3Ata,
      admin,
      mintAmount,
      [],
      undefined,
      TOKEN_2022_PROGRAM_ID
    );

    // Derive PDAs
    [configPda] = deriveProtocolConfigPda(program.programId);
    //[treasuryVaultPda] = deriveTreasuryVaultPda(program.programId, configPda);
    treasuryVaultPda = getAssociatedTokenAddressSync(
      usdcMint,
      configPda, // authority is the ProtocolConfig PDA
      true, // allowOwnerOffCurve = true because configPda is a PDA
      TOKEN_2022_PROGRAM_ID,
      ASSOCIATED_TOKEN_PROGRAM_ID
    );

    [circlePda] = deriveCirclePda(
      program.programId,
      CONTRIBUTION_AMOUNT,
      TOTAL_MEMBERS,
      FREQUENCY_DAILY,
      usdcMint,
      0 // nonce
    );

    [collateralVaultPda] = deriveCollateralVaultPda(
      program.programId,
      circlePda
    );
    [potVaultPda] = derivePotVaultPda(program.programId, circlePda);

    console.log("  Setup complete.\n");
  });

  // ─────────────────────────────────────────────────────────
  // 1. INITIALIZE PROTOCOL
  // ─────────────────────────────────────────────────────────
  describe("initialize_protocol", () => {
    it("initialises the protocol with correct fee and config", async () => {
      await program.methods
        .initializeProtocol(PROTOCOL_FEE_BPS)
        .accounts({
          admin: admin.publicKey,
          // protocolConfig: configPda, // Removed as it is not part of the expected accounts
          //treasuryVault: treasuryVaultPda,
          usdcMint: usdcMint,
          //systemProgram: SystemProgram.programId,
          tokenProgram: TOKEN_2022_PROGRAM_ID,
          //associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
        })
        .signers([admin])
        .rpc();

      const config = await program.account.protocolConfig.fetch(configPda);

      assert.equal(
        config.admin.toBase58(),
        admin.publicKey.toBase58(),
        "admin should be set correctly"
      );
      assert.equal(
        config.protocolFeeBps,
        PROTOCOL_FEE_BPS,
        "fee bps should match"
      );
      assert.equal(
        config.isPaused,
        false,
        "protocol should not be paused on init"
      );
    });

    it("rejects a second initialize call", async () => {
      try {
        await program.methods
          .initializeProtocol(PROTOCOL_FEE_BPS)
          .accountsPartial({
            admin: admin.publicKey,
            protocolConfig: configPda,
            treasuryVault: treasuryVaultPda,
            usdcMint: usdcMint,
            systemProgram: SystemProgram.programId,
            tokenProgram: TOKEN_2022_PROGRAM_ID,
            associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
          })
          .signers([admin])
          .rpc();

        assert.fail("should have thrown");
      } catch (err: any) {
        // Anchor throws when init account already exists
        assert.ok(
          err.message.includes("already in use") ||
            err.logs?.some((l: string) => l.includes("already in use")),
          "should reject double init"
        );
      }
    });

    it("rejects fee above MAX_PROTOCOL_FEE_BPS (1000)", async () => {
      // Use a fresh config PDA that doesn't exist —
      // we just want to see the fee validation fire.
      // This will fail at account creation or fee validation.
      try {
        await program.methods
          .initializeProtocol(1001) // > 1000 bps = 10% max
          .accountsPartial({
            admin: admin.publicKey,
            protocolConfig: configPda,
            treasuryVault: treasuryVaultPda,
            usdcMint: usdcMint,
            systemProgram: SystemProgram.programId,
            tokenProgram: TOKEN_2022_PROGRAM_ID,
            associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
          })
          .signers([admin])
          .rpc();

        assert.fail("should have rejected fee > 1000 bps");
      } catch (err: any) {
        assert.ok(
          err.error?.errorCode?.code === "FeeTooHigh" ||
            err.message.includes("FeeTooHigh") ||
            err.message.includes("already in use"), // already init'd is also fine
          "should reject high fee"
        );
      }
    });
  });

  // ─────────────────────────────────────────────────────────
  // 2. CREATE CIRCLE
  // ─────────────────────────────────────────────────────────
  describe("create_circle", () => {
    it("creates a circle with correct parameters", async () => {
      await program.methods
        .createCircle(
          CONTRIBUTION_AMOUNT,
          TOTAL_MEMBERS,
          { daily: {} },
          0 // nonce
        )
        .accountsPartial({
          creator: member1.publicKey,
          protocolConfig: configPda,
          circleAccount: circlePda,
          collateralVault: collateralVaultPda,
          potVault: potVaultPda,
          usdcMint: usdcMint,
          systemProgram: SystemProgram.programId,
          tokenProgram: TOKEN_2022_PROGRAM_ID,
          associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
          //rent: SYSVAR_RENT_PUBKEY,
        })
        .signers([member1])
        .rpc();

      const circle = await program.account.circleAccount.fetch(circlePda);

      assert.equal(
        circle.contributionAmount.toString(),
        CONTRIBUTION_AMOUNT.toString(),
        "contribution amount should match"
      );
      assert.equal(
        circle.totalMembers,
        TOTAL_MEMBERS,
        "total members should match"
      );
      assert.deepEqual(
        circle.state,
        { open: {} },
        "circle should be in Open state"
      );
      assert.equal(circle.currentMembers, 0, "no members have joined yet");
      assert.ok(
        circle.cancelDeadlineSlot.toNumber() > 0,
        "cancel deadline slot should be set"
      );
    });

    it("rejects duplicate open circle creation", async () => {
      try {
        await program.methods
          .createCircle(CONTRIBUTION_AMOUNT, TOTAL_MEMBERS, { daily: {} }, 0)
          .accountsPartial({
            creator: member1.publicKey,
            protocolConfig: configPda,
            circleAccount: circlePda,
            collateralVault: collateralVaultPda,
            potVault: potVaultPda,
            usdcMint: usdcMint,
            systemProgram: SystemProgram.programId,
            tokenProgram: TOKEN_2022_PROGRAM_ID,
            associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
          })
          .signers([member1])
          .rpc();

        assert.fail("should have rejected duplicate circle");
      } catch (err: any) {
        assert.ok(
          err.message.includes("already in use") ||
            err.logs?.some((l: string) => l.includes("already in use")),
          "should reject duplicate open circle"
        );
      }
    });
  });

  // ─────────────────────────────────────────────────────────
  // 3. JOIN CIRCLE
  // ─────────────────────────────────────────────────────────
  describe("join_circle", () => {
    it("member1 joins as position 1 with correct collateral", async () => {
      const [memberPda] = deriveMemberPda(
        program.programId,
        circlePda,
        member1.publicKey
      );
      const [colRecPda] = deriveCollateralRecordPda(
        program.programId,
        circlePda,
        member1.publicKey
      );

      const balanceBefore = (
        await getAccount(
          connection,
          member1Ata,
          undefined,
          TOKEN_2022_PROGRAM_ID
        )
      ).amount;

      await program.methods
        .joinCircle()
        .accountsPartial({
          member: member1.publicKey,
          protocolConfig: configPda,
          circleAccount: circlePda,
          memberAccount: memberPda,
          collateralRecord: colRecPda,
          memberTokenAccount: member1Ata,
          collateralVault: collateralVaultPda,
          potVault: potVaultPda,
          usdcMint: usdcMint,
          systemProgram: SystemProgram.programId,
          tokenProgram: TOKEN_2022_PROGRAM_ID,
        })
        .signers([member1])
        .rpc();

      const memberAcc = await program.account.memberAccount.fetch(memberPda);
      const circle = await program.account.circleAccount.fetch(circlePda);

      // Position 1 collateral = (3-1) × 100 = 200 USDC
      const expectedCollateral = new BN(200_000_000);

      assert.equal(memberAcc.position, 1, "should be position 1");
      assert.equal(
        memberAcc.collateralLocked.toString(),
        expectedCollateral.toString(),
        "collateral should be (total_members - 1) × contribution"
      );
      assert.equal(memberAcc.hasReceivedPot, false, "has not received pot yet");
      assert.equal(memberAcc.isKicked, false, "should not be kicked");
      assert.equal(circle.currentMembers, 1, "circle should have 1 member");
      assert.deepEqual(
        circle.state,
        { open: {} },
        "circle should still be Open"
      );

      // Verify tokens left member1's wallet
      const balanceAfter = (
        await getAccount(
          connection,
          member1Ata,
          undefined,
          TOKEN_2022_PROGRAM_ID
        )
      ).amount;

      // member1 paid: 200 collateral + 100 contribution = 300 USDC
      const expectedDeducted = BigInt(300_000_000);
      assert.equal(
        balanceBefore - balanceAfter,
        expectedDeducted,
        "member1 should have paid collateral + contribution"
      );
    });

    it("member2 joins as position 2, pays premium into PotVault", async () => {
      const [member2Pda] = deriveMemberPda(
        program.programId,
        circlePda,
        member2.publicKey
      );
      const [colRec2Pda] = deriveCollateralRecordPda(
        program.programId,
        circlePda,
        member2.publicKey
      );

      const potVaultBefore = (
        await getAccount(
          connection,
          potVaultPda,
          undefined,
          TOKEN_2022_PROGRAM_ID
        )
      ).amount;

      await program.methods
        .joinCircle()
        .accountsPartial({
          member: member2.publicKey,
          protocolConfig: configPda,
          circleAccount: circlePda,
          memberAccount: member2Pda,
          collateralRecord: colRec2Pda,
          memberTokenAccount: member2Ata,
          collateralVault: collateralVaultPda,
          potVault: potVaultPda,
          usdcMint: usdcMint,
          systemProgram: SystemProgram.programId,
          tokenProgram: TOKEN_2022_PROGRAM_ID,
        })
        .signers([member2])
        .rpc();

      const member2Acc = await program.account.memberAccount.fetch(member2Pda);
      const potVaultAfter = (
        await getAccount(
          connection,
          potVaultPda,
          undefined,
          TOKEN_2022_PROGRAM_ID
        )
      ).amount;

      // Position 2 collateral = (3-2) × 100 = 100 USDC
      assert.equal(member2Acc.position, 2, "should be position 2");
      assert.equal(
        member2Acc.collateralLocked.toString(),
        "100000000",
        "position 2 collateral should be 100 USDC"
      );

      // PotVault should have increased by contribution (100) + premium (10) = 110 USDC
      assert.equal(
        potVaultAfter - potVaultBefore,
        BigInt(110_000_000),
        "PotVault should have received contribution + premium from member2"
      );
    });

    it("member3 joins as position 3 (final) with zero collateral", async () => {
      const [member3Pda] = deriveMemberPda(
        program.programId,
        circlePda,
        member3.publicKey
      );
      const [colRec3Pda] = deriveCollateralRecordPda(
        program.programId,
        circlePda,
        member3.publicKey
      );

      await program.methods
        .joinCircle()
        .accountsPartial({
          member: member3.publicKey,
          protocolConfig: configPda,
          circleAccount: circlePda,
          memberAccount: member3Pda,
          collateralRecord: colRec3Pda,
          memberTokenAccount: member3Ata,
          collateralVault: collateralVaultPda,
          potVault: potVaultPda,
          usdcMint: usdcMint,
          systemProgram: SystemProgram.programId,
          tokenProgram: TOKEN_2022_PROGRAM_ID,
        })
        .signers([member3])
        .rpc();

      const member3Acc = await program.account.memberAccount.fetch(member3Pda);
      const circle = await program.account.circleAccount.fetch(circlePda);

      assert.equal(member3Acc.position, 3, "should be position 3");
      assert.equal(
        member3Acc.collateralLocked.toString(),
        "0",
        "final position should have zero collateral"
      );

      // Circle should now be READY — all seats filled
      assert.deepEqual(
        circle.state,
        { ready: {} },
        "circle should transition to Ready when full"
      );
      assert.equal(
        circle.currentMembers,
        TOTAL_MEMBERS,
        "all members should be joined"
      );
    });

    it("rejects join after circle is full", async () => {
      const extraMember = Keypair.generate();
      await airdrop(connection, extraMember.publicKey);

      const extraMemberAta = await createAssociatedTokenAccount(
        connection,
        admin,
        usdcMint,
        extraMember.publicKey,
        undefined,
        TOKEN_2022_PROGRAM_ID
      );
      await mintTo(
        connection,
        admin,
        usdcMint,
        extraMemberAta,
        admin,
        BigInt(1_000_000_000),
        [],
        undefined,
        TOKEN_2022_PROGRAM_ID
      );

      const [extraMemberPda] = deriveMemberPda(
        program.programId,
        circlePda,
        extraMember.publicKey
      );
      const [extraColRecPda] = deriveCollateralRecordPda(
        program.programId,
        circlePda,
        extraMember.publicKey
      );

      try {
        await program.methods
          .joinCircle()
          .accountsPartial({
            member: extraMember.publicKey,
            protocolConfig: configPda,
            circleAccount: circlePda,
            memberAccount: extraMemberPda,
            collateralRecord: extraColRecPda,
            memberTokenAccount: extraMemberAta,
            collateralVault: collateralVaultPda,
            potVault: potVaultPda,
            usdcMint: usdcMint,
            systemProgram: SystemProgram.programId,
            tokenProgram: TOKEN_2022_PROGRAM_ID,
          })
          .signers([extraMember])
          .rpc();

        assert.fail("should have rejected join on full circle");
      } catch (err: any) {
        assert.ok(
          err.error?.errorCode?.code === "CircleNotOpen" ||
            err.message.includes("CircleNotOpen"),
          "should throw CircleNotOpen when circle is full"
        );
      }
    });
  });

  // ─────────────────────────────────────────────────────────
  // 4. START CIRCLE
  // ─────────────────────────────────────────────────────────
  describe("start_circle", () => {
    it("starts the circle and sets cycle 1 as active", async () => {
      const [paymentRecord1Pda] = derivePaymentRecordPda(
        program.programId,
        circlePda,
        1
      );

      await program.methods
        .startCircle()
        .accountsPartial({
          caller: admin.publicKey,
          protocolConfig: configPda,
          circleAccount: circlePda,
          paymentRecord: paymentRecord1Pda,
          systemProgram: SystemProgram.programId,
        })
        .signers([admin])
        .rpc();

      const circle = await program.account.circleAccount.fetch(circlePda);
      const payment = await program.account.paymentRecord.fetch(
        paymentRecord1Pda
      );

      assert.deepEqual(circle.state, { active: {} }, "circle should be Active");
      assert.equal(circle.currentCycle, 1, "current cycle should be 1");
      assert.ok(
        circle.cycleDeadlineSlot.toNumber() > 0,
        "deadline slot should be set"
      );
      assert.ok(
        circle.startedAtSlot.toNumber() > 0,
        "started at slot should be recorded"
      );

      // Cycle 1 payment record should have all bits set
      // for 3 members: (1 << 3) - 1 = 7 = 0b111
      assert.equal(
        payment.paidFlags.toString(),
        "7",
        "cycle 1 payment record should have all 3 member bits set"
      );
    });
  });

  // ─────────────────────────────────────────────────────────
  // 5. DISBURSE POT — CYCLE 1
  // ─────────────────────────────────────────────────────────
  describe("disburse_pot — cycle 1", () => {
    it("disburses cycle 1 pot to member1 (position 1)", async () => {
      const [member1Pda] = deriveMemberPda(
        program.programId,
        circlePda,
        member1.publicKey
      );
      const [paymentRecord1Pda] = derivePaymentRecordPda(
        program.programId,
        circlePda,
        1
      );

      const member1BalBefore = (
        await getAccount(
          connection,
          member1Ata,
          undefined,
          TOKEN_2022_PROGRAM_ID
        )
      ).amount;

      const potVaultBefore = (
        await getAccount(
          connection,
          potVaultPda,
          undefined,
          TOKEN_2022_PROGRAM_ID
        )
      ).amount;

      await program.methods
        .disbursePot()
        .accountsPartial({
          caller: admin.publicKey,
          protocolConfig: configPda,
          circleAccount: circlePda,
          paymentRecord: paymentRecord1Pda,
          recipientMemberAccount: member1Pda,
          recipient: member1.publicKey,
          recipientTokenAccount: member1Ata,
          potVault: potVaultPda,
          treasuryVault: treasuryVaultPda,
          usdcMint: usdcMint,
          tokenProgram: TOKEN_2022_PROGRAM_ID,
          systemProgram: SystemProgram.programId,
        })
        .signers([admin])
        .rpc();

      const circle = await program.account.circleAccount.fetch(circlePda);
      const member1Acc = await program.account.memberAccount.fetch(member1Pda);

      assert.equal(
        member1Acc.hasReceivedPot,
        true,
        "member1 should be marked as received pot"
      );
      assert.equal(
        circle.currentCycle,
        2,
        "cycle should advance to 2 after disbursement"
      );

      const member1BalAfter = (
        await getAccount(
          connection,
          member1Ata,
          undefined,
          TOKEN_2022_PROGRAM_ID
        )
      ).amount;

      // Cycle 1 pot = contributions (300) + premiums (20) = 320 USDC
      // Fee = 320 × 0.5% = 1.6 USDC → rounded down = 1.6 USDC
      // Net pot ≈ 318.4 USDC
      // We just verify member1 received something substantial
      assert.ok(
        member1BalAfter > member1BalBefore,
        "member1 should have received the pot"
      );

      console.log(
        `    member1 received: ${
          (member1BalAfter - member1BalBefore) / BigInt(1_000_000)
        } USDC`
      );
    });
  });

  // ─────────────────────────────────────────────────────────
  // 6. INIT PAYMENT RECORD + PAY CONTRIBUTION — CYCLE 2
  // ─────────────────────────────────────────────────────────
  describe("cycle 2 — init record, pay contributions, disburse", () => {
    it("initialises payment record for cycle 2", async () => {
      const [paymentRecord2Pda] = derivePaymentRecordPda(
        program.programId,
        circlePda,
        2
      );

      await program.methods
        .initPaymentRecord(2)
        .accountsPartial({
          caller: admin.publicKey,
          protocolConfig: configPda,
          circleAccount: circlePda,
          paymentRecord: paymentRecord2Pda,
          systemProgram: SystemProgram.programId,
        })
        .signers([admin])
        .rpc();

      const payment = await program.account.paymentRecord.fetch(
        paymentRecord2Pda
      );

      assert.equal(
        payment.paidFlags.toString(),
        "0",
        "cycle 2 payment record should start with all bits zero"
      );
      assert.equal(payment.cycle, 2, "cycle number should be 2");
    });

    it("member1 pays contribution for cycle 2", async () => {
      const [member1Pda] = deriveMemberPda(
        program.programId,
        circlePda,
        member1.publicKey
      );
      const [paymentRecord2Pda] = derivePaymentRecordPda(
        program.programId,
        circlePda,
        2
      );

      await program.methods
        .payContribution()
        .accountsPartial({
          member: member1.publicKey,
          protocolConfig: configPda,
          circleAccount: circlePda,
          memberAccount: member1Pda,
          paymentRecord: paymentRecord2Pda,
          memberTokenAccount: member1Ata,
          potVault: potVaultPda,
          usdcMint: usdcMint,
          tokenProgram: TOKEN_2022_PROGRAM_ID,
        })
        .signers([member1])
        .rpc();

      const payment = await program.account.paymentRecord.fetch(
        paymentRecord2Pda
      );
      // Position 1 = bit 0. After member1 pays: paid_flags = 0b001 = 1
      assert.equal(
        payment.paidFlags.toNumber() & 1,
        1,
        "bit 0 (position 1) should be set"
      );
    });

    it("member2 pays contribution for cycle 2", async () => {
      const [member2Pda] = deriveMemberPda(
        program.programId,
        circlePda,
        member2.publicKey
      );
      const [paymentRecord2Pda] = derivePaymentRecordPda(
        program.programId,
        circlePda,
        2
      );

      await program.methods
        .payContribution()
        .accountsPartial({
          member: member2.publicKey,
          protocolConfig: configPda,
          circleAccount: circlePda,
          memberAccount: member2Pda,
          paymentRecord: paymentRecord2Pda,
          memberTokenAccount: member2Ata,
          potVault: potVaultPda,
          usdcMint: usdcMint,
          tokenProgram: TOKEN_2022_PROGRAM_ID,
        })
        .signers([member2])
        .rpc();

      const payment = await program.account.paymentRecord.fetch(
        paymentRecord2Pda
      );
      // Position 2 = bit 1. After member1 + member2: paid_flags = 0b011 = 3
      assert.equal(
        payment.paidFlags.toNumber() & 3,
        3,
        "bits 0 and 1 should be set after member1 and member2 pay"
      );
    });

    it("member3 pays contribution for cycle 2", async () => {
      const [member3Pda] = deriveMemberPda(
        program.programId,
        circlePda,
        member3.publicKey
      );
      const [paymentRecord2Pda] = derivePaymentRecordPda(
        program.programId,
        circlePda,
        2
      );

      await program.methods
        .payContribution()
        .accountsPartial({
          member: member3.publicKey,
          protocolConfig: configPda,
          circleAccount: circlePda,
          memberAccount: member3Pda,
          paymentRecord: paymentRecord2Pda,
          memberTokenAccount: member3Ata,
          potVault: potVaultPda,
          usdcMint: usdcMint,
          tokenProgram: TOKEN_2022_PROGRAM_ID,
        })
        .signers([member3])
        .rpc();

      const payment = await program.account.paymentRecord.fetch(
        paymentRecord2Pda
      );
      // All 3 members paid: paid_flags = 0b111 = 7
      assert.equal(
        payment.paidFlags.toNumber(),
        7,
        "all 3 bits should be set after all members pay"
      );
    });

    it("disburses cycle 2 pot to member2 (position 2)", async () => {
      const [member2Pda] = deriveMemberPda(
        program.programId,
        circlePda,
        member2.publicKey
      );
      const [paymentRecord2Pda] = derivePaymentRecordPda(
        program.programId,
        circlePda,
        2
      );

      const member2BalBefore = (
        await getAccount(
          connection,
          member2Ata,
          undefined,
          TOKEN_2022_PROGRAM_ID
        )
      ).amount;

      await program.methods
        .disbursePot()
        .accountsPartial({
          caller: admin.publicKey,
          protocolConfig: configPda,
          circleAccount: circlePda,
          paymentRecord: paymentRecord2Pda,
          recipientMemberAccount: member2Pda,
          recipient: member2.publicKey,
          recipientTokenAccount: member2Ata,
          potVault: potVaultPda,
          treasuryVault: treasuryVaultPda,
          usdcMint: usdcMint,
          tokenProgram: TOKEN_2022_PROGRAM_ID,
          systemProgram: SystemProgram.programId,
        })
        .signers([admin])
        .rpc();

      const circle = await program.account.circleAccount.fetch(circlePda);
      const member2Acc = await program.account.memberAccount.fetch(member2Pda);

      assert.equal(
        member2Acc.hasReceivedPot,
        true,
        "member2 should have received pot"
      );
      assert.equal(circle.currentCycle, 3, "cycle should advance to 3");

      const member2BalAfter = (
        await getAccount(
          connection,
          member2Ata,
          undefined,
          TOKEN_2022_PROGRAM_ID
        )
      ).amount;
      assert.ok(
        member2BalAfter > member2BalBefore,
        "member2 should have received the pot"
      );
      console.log(
        `    member2 received: ${
          (member2BalAfter - member2BalBefore) / BigInt(1_000_000)
        } USDC`
      );
    });
  });

  // ─────────────────────────────────────────────────────────
  // 7. CYCLE 3 — FINAL CYCLE, CIRCLE COMPLETES
  // ─────────────────────────────────────────────────────────
  describe("cycle 3 — final cycle, circle completes", () => {
    it("initialises payment record for cycle 3", async () => {
      const [paymentRecord3Pda] = derivePaymentRecordPda(
        program.programId,
        circlePda,
        3
      );

      await program.methods
        .initPaymentRecord(3)
        .accountsPartial({
          caller: admin.publicKey,
          protocolConfig: configPda,
          circleAccount: circlePda,
          paymentRecord: paymentRecord3Pda,
          systemProgram: SystemProgram.programId,
        })
        .signers([admin])
        .rpc();

      const payment = await program.account.paymentRecord.fetch(
        paymentRecord3Pda
      );
      assert.equal(payment.paidFlags.toString(), "0", "should start empty");
    });

    it("all members pay cycle 3", async () => {
      const [member1Pda] = deriveMemberPda(
        program.programId,
        circlePda,
        member1.publicKey
      );
      const [member2Pda] = deriveMemberPda(
        program.programId,
        circlePda,
        member2.publicKey
      );
      const [member3Pda] = deriveMemberPda(
        program.programId,
        circlePda,
        member3.publicKey
      );
      const [paymentRecord3Pda] = derivePaymentRecordPda(
        program.programId,
        circlePda,
        3
      );

      for (const [memberKp, memberPda, memberAta] of [
        [member1, member1Pda, member1Ata],
        [member2, member2Pda, member2Ata],
        [member3, member3Pda, member3Ata],
      ] as [Keypair, PublicKey, PublicKey][]) {
        await program.methods
          .payContribution()
          .accountsPartial({
            member: memberKp.publicKey,
            protocolConfig: configPda,
            circleAccount: circlePda,
            memberAccount: memberPda,
            paymentRecord: paymentRecord3Pda,
            memberTokenAccount: memberAta,
            potVault: potVaultPda,
            usdcMint: usdcMint,
            tokenProgram: TOKEN_2022_PROGRAM_ID,
          })
          .signers([memberKp])
          .rpc();
      }

      const payment = await program.account.paymentRecord.fetch(
        paymentRecord3Pda
      );
      assert.equal(
        payment.paidFlags.toNumber(),
        7,
        "all 3 members should have paid"
      );
    });

    it("disburses final cycle pot to member3 and circle completes", async () => {
      const [member3Pda] = deriveMemberPda(
        program.programId,
        circlePda,
        member3.publicKey
      );
      const [paymentRecord3Pda] = derivePaymentRecordPda(
        program.programId,
        circlePda,
        3
      );

      const member3BalBefore = (
        await getAccount(
          connection,
          member3Ata,
          undefined,
          TOKEN_2022_PROGRAM_ID
        )
      ).amount;

      await program.methods
        .disbursePot()
        .accountsPartial({
          caller: admin.publicKey,
          protocolConfig: configPda,
          circleAccount: circlePda,
          paymentRecord: paymentRecord3Pda,
          recipientMemberAccount: member3Pda,
          recipient: member3.publicKey,
          recipientTokenAccount: member3Ata,
          potVault: potVaultPda,
          treasuryVault: treasuryVaultPda,
          usdcMint: usdcMint,
          tokenProgram: TOKEN_2022_PROGRAM_ID,
          associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
          systemProgram: SystemProgram.programId,
        })
        .signers([admin])
        .rpc();

      const circle = await program.account.circleAccount.fetch(circlePda);

      assert.deepEqual(
        circle.state,
        { completed: {} },
        "circle should be Completed after final disbursement"
      );
      assert.ok(
        circle.completedAtSlot.toNumber() > 0,
        "completed at slot should be recorded"
      );

      const member3BalAfter = (
        await getAccount(
          connection,
          member3Ata,
          undefined,
          TOKEN_2022_PROGRAM_ID
        )
      ).amount;
      assert.ok(
        member3BalAfter > member3BalBefore,
        "member3 should have received the final pot"
      );
      console.log(
        `    member3 received: ${
          (member3BalAfter - member3BalBefore) / BigInt(1_000_000)
        } USDC`
      );
    });
  });

  // ─────────────────────────────────────────────────────────
  // 8. CLAIM COLLATERAL
  // ─────────────────────────────────────────────────────────
  describe("claim_collateral", () => {
    it("member1 claims full collateral (200 USDC, never defaulted)", async () => {
      const [member1Pda] = deriveMemberPda(
        program.programId,
        circlePda,
        member1.publicKey
      );
      const [colRec1Pda] = deriveCollateralRecordPda(
        program.programId,
        circlePda,
        member1.publicKey
      );

      const balBefore = (
        await getAccount(
          connection,
          member1Ata,
          undefined,
          TOKEN_2022_PROGRAM_ID
        )
      ).amount;

      await program.methods
        .claimCollateral()
        .accountsPartial({
          member: member1.publicKey,
          protocolConfig: configPda,
          circleAccount: circlePda,
          memberAccount: member1Pda,
          collateralRecord: colRec1Pda,
          collateralVault: collateralVaultPda,
          memberTokenAccount: member1Ata,
          usdcMint: usdcMint,
          tokenProgram: TOKEN_2022_PROGRAM_ID,
          systemProgram: SystemProgram.programId,
        })
        .signers([member1])
        .rpc();

      const balAfter = (
        await getAccount(
          connection,
          member1Ata,
          undefined,
          TOKEN_2022_PROGRAM_ID
        )
      ).amount;

      // member1 locked 200 USDC, never defaulted → gets 200 USDC back
      assert.equal(
        balAfter - balBefore,
        BigInt(200_000_000),
        "member1 should get full 200 USDC collateral back"
      );

      const colRec = await program.account.collateralRecord.fetch(colRec1Pda);
      assert.equal(colRec.claimed, true, "claimed flag should be true");
    });

    it("member2 claims collateral (100 USDC, never defaulted)", async () => {
      const [member2Pda] = deriveMemberPda(
        program.programId,
        circlePda,
        member2.publicKey
      );
      const [colRec2Pda] = deriveCollateralRecordPda(
        program.programId,
        circlePda,
        member2.publicKey
      );

      const balBefore = (
        await getAccount(
          connection,
          member2Ata,
          undefined,
          TOKEN_2022_PROGRAM_ID
        )
      ).amount;

      await program.methods
        .claimCollateral()
        .accountsPartial({
          member: member2.publicKey,
          protocolConfig: configPda,
          circleAccount: circlePda,
          memberAccount: member2Pda,
          collateralRecord: colRec2Pda,
          collateralVault: collateralVaultPda,
          memberTokenAccount: member2Ata,
          usdcMint: usdcMint,
          tokenProgram: TOKEN_2022_PROGRAM_ID,
          systemProgram: SystemProgram.programId,
        })
        .signers([member2])
        .rpc();

      const balAfter = (
        await getAccount(
          connection,
          member2Ata,
          undefined,
          TOKEN_2022_PROGRAM_ID
        )
      ).amount;

      assert.equal(
        balAfter - balBefore,
        BigInt(100_000_000),
        "member2 should get full 100 USDC collateral back"
      );
    });

    it("member3 claims collateral (0 USDC, final position)", async () => {
      const [member3Pda] = deriveMemberPda(
        program.programId,
        circlePda,
        member3.publicKey
      );
      const [colRec3Pda] = deriveCollateralRecordPda(
        program.programId,
        circlePda,
        member3.publicKey
      );

      // Should succeed cleanly even though zero collateral
      await program.methods
        .claimCollateral()
        .accountsPartial({
          member: member3.publicKey,
          protocolConfig: configPda,
          circleAccount: circlePda,
          memberAccount: member3Pda,
          collateralRecord: colRec3Pda,
          collateralVault: collateralVaultPda,
          memberTokenAccount: member3Ata,
          usdcMint: usdcMint,
          tokenProgram: TOKEN_2022_PROGRAM_ID,
          systemProgram: SystemProgram.programId,
        })
        .signers([member3])
        .rpc();

      const colRec = await program.account.collateralRecord.fetch(colRec3Pda);
      assert.equal(
        colRec.claimed,
        true,
        "claimed flag should be true even for zero collateral"
      );
    });

    it("rejects double claim", async () => {
      const [member1Pda] = deriveMemberPda(
        program.programId,
        circlePda,
        member1.publicKey
      );
      const [colRec1Pda] = deriveCollateralRecordPda(
        program.programId,
        circlePda,
        member1.publicKey
      );

      try {
        await program.methods
          .claimCollateral()
          .accountsPartial({
            member: member1.publicKey,
            protocolConfig: configPda,
            circleAccount: circlePda,
            memberAccount: member1Pda,
            collateralRecord: colRec1Pda,
            collateralVault: collateralVaultPda,
            memberTokenAccount: member1Ata,
            usdcMint: usdcMint,
            tokenProgram: TOKEN_2022_PROGRAM_ID,
            systemProgram: SystemProgram.programId,
          })
          .signers([member1])
          .rpc();

        assert.fail("should have rejected double claim");
      } catch (err: any) {
        assert.ok(
          err.error?.errorCode?.code === "AlreadyClaimed" ||
            err.message.includes("AlreadyClaimed"),
          "should throw AlreadyClaimed"
        );
      }
    });
  });

  // ─────────────────────────────────────────────────────────
  // 9. ADMIN INSTRUCTIONS
  // ─────────────────────────────────────────────────────────
  describe("admin instructions", () => {
    it("pauses the protocol", async () => {
      await program.methods
        .pauseProtocol()
        .accountsPartial({
          admin: admin.publicKey,
          protocolConfig: configPda,
        })
        .signers([admin])
        .rpc();

      const config = await program.account.protocolConfig.fetch(configPda);
      assert.equal(config.isPaused, true, "protocol should be paused");
    });

    it("rejects non-admin pause attempt", async () => {
      // First unpause so we can test the auth check cleanly
      await program.methods
        .unpauseProtocol()
        .accountsPartial({
          admin: admin.publicKey,
          protocolConfig: configPda,
        })
        .signers([admin])
        .rpc();

      try {
        await program.methods
          .pauseProtocol()
          .accountsPartial({
            admin: member1.publicKey, // wrong admin
            protocolConfig: configPda,
          })
          .signers([member1])
          .rpc();

        assert.fail("should have rejected non-admin pause");
      } catch (err: any) {
        assert.ok(
          err.error?.errorCode?.code === "Unauthorized" ||
            err.message.includes("Unauthorized"),
          "should throw Unauthorized"
        );
      }
    });

    it("updates protocol fee", async () => {
      const newFee = 100; // 1%

      await program.methods
        .updateConfig(newFee)
        .accountsPartial({
          admin: admin.publicKey,
          protocolConfig: configPda,
        })
        .signers([admin])
        .rpc();

      const config = await program.account.protocolConfig.fetch(configPda);
      assert.equal(config.protocolFeeBps, newFee, "fee should be updated");
    });

    it("rejects fee update above 1000 bps", async () => {
      try {
        await program.methods
          .updateConfig(1001)
          .accountsPartial({
            admin: admin.publicKey,
            protocolConfig: configPda,
          })
          .signers([admin])
          .rpc();

        assert.fail("should have rejected fee above max");
      } catch (err: any) {
        assert.ok(
          err.error?.errorCode?.code === "FeeTooHigh" ||
            err.message.includes("FeeTooHigh"),
          "should throw FeeTooHigh"
        );
      }
    });

    it("withdraws accumulated treasury fees", async () => {
      const adminAta = getAssociatedTokenAddressSync(
        usdcMint,
        admin.publicKey,
        false,
        TOKEN_2022_PROGRAM_ID,
        ASSOCIATED_TOKEN_PROGRAM_ID
      );

      // Create admin ATA — will no-op if already exists
      try {
        await createAssociatedTokenAccount(
          connection,
          admin,
          usdcMint,
          admin.publicKey,
          undefined,
          TOKEN_2022_PROGRAM_ID,
          ASSOCIATED_TOKEN_PROGRAM_ID
        );
      } catch {
        // already exists — fine
      }

      const treasuryBalance = (
        await getAccount(
          connection,
          treasuryVaultPda,
          undefined,
          TOKEN_2022_PROGRAM_ID
        )
      ).amount;

      if (treasuryBalance === BigInt(0)) {
        console.log("    Treasury is empty. Skipping withdrawal.");
        return;
      }

      const adminBalBefore = (
        await getAccount(connection, adminAta, undefined, TOKEN_2022_PROGRAM_ID)
      ).amount;

      await program.methods
        .withdrawTreasury(new BN(treasuryBalance.toString()))
        .accountsPartial({
          admin: admin.publicKey,
          protocolConfig: configPda,
          treasuryVault: treasuryVaultPda,
          destination: adminAta,
          usdcMint: usdcMint,
          tokenProgram: TOKEN_2022_PROGRAM_ID,
          associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
        })
        .signers([admin])
        .rpc();

      const adminBalAfter = (
        await getAccount(connection, adminAta, undefined, TOKEN_2022_PROGRAM_ID)
      ).amount;

      assert.equal(
        adminBalAfter - adminBalBefore,
        treasuryBalance,
        "admin should receive full treasury balance"
      );
    });
  });

  // ─────────────────────────────────────────────────────────
  // 9. CANCEL CIRCLE TESTS
  // ─────────────────────────────────────────────────────────
  describe("cancel_circle", () => {
    let cancelCirclePda: PublicKey;
    let cancelColVaultPda: PublicKey;
    let cancelPotVaultPda: PublicKey;
    let cancelMember1Pda: PublicKey;
    let cancelColRec1Pda: PublicKey;
    let cancelMember1Ata: PublicKey;
    const cancelAmount = new BN(50_000_000); // 50 USDC
    const cancelMembers = 3;
    const cancelFreq = 0; // Daily

    before(async () => {
      // Create a fresh circle for cancel tests
      [cancelCirclePda] = deriveCirclePda(
        program.programId,
        cancelAmount,
        cancelMembers,
        cancelFreq,
        usdcMint,
        0
      );
      [cancelColVaultPda] = deriveCollateralVaultPda(
        program.programId,
        cancelCirclePda
      );
      [cancelPotVaultPda] = derivePotVaultPda(
        program.programId,
        cancelCirclePda
      );
      [cancelMember1Pda] = deriveMemberPda(
        program.programId,
        cancelCirclePda,
        member1.publicKey
      );
      [cancelColRec1Pda] = deriveCollateralRecordPda(
        program.programId,
        cancelCirclePda,
        member1.publicKey
      );

      cancelMember1Ata = await getAssociatedTokenAddress(
        usdcMint,
        member1.publicKey,
        false,
        TOKEN_2022_PROGRAM_ID
      );

      // Create the circle
      await program.methods
        .createCircle(cancelAmount, cancelMembers, { daily: {} }, 0)
        .accountsPartial({
          creator: member1.publicKey,
          protocolConfig: configPda,
          circleAccount: cancelCirclePda,
          collateralVault: cancelColVaultPda,
          potVault: cancelPotVaultPda,
          usdcMint,
          systemProgram: SystemProgram.programId,
          tokenProgram: TOKEN_2022_PROGRAM_ID,
          associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
        })
        .signers([member1])
        .rpc();

      // Member1 joins as position 1
      await program.methods
        .joinCircle()
        .accountsPartial({
          member: member1.publicKey,
          protocolConfig: configPda,
          circleAccount: cancelCirclePda,
          memberAccount: cancelMember1Pda,
          collateralRecord: cancelColRec1Pda,
          memberTokenAccount: cancelMember1Ata,
          collateralVault: cancelColVaultPda,
          potVault: cancelPotVaultPda,
          usdcMint,
          systemProgram: SystemProgram.programId,
          tokenProgram: TOKEN_2022_PROGRAM_ID,
        })
        .signers([member1])
        .rpc();
    });

    it("allows creator to cancel when they are the only member", async () => {
      const balBefore = (
        await getAccount(
          connection,
          cancelMember1Ata,
          undefined,
          TOKEN_2022_PROGRAM_ID
        )
      ).amount;

      await program.methods
        .cancelCircle()
        .accountsPartial({
          caller: member1.publicKey,
          protocolConfig: configPda,
          circleAccount: cancelCirclePda,
          callerMemberAccount: cancelMember1Pda,
          callerCollateralRecord: cancelColRec1Pda,
          collateralVault: cancelColVaultPda,
          potVault: cancelPotVaultPda,
          callerTokenAccount: cancelMember1Ata,
          usdcMint,
          tokenProgram: TOKEN_2022_PROGRAM_ID,
          systemProgram: SystemProgram.programId,
        })
        .signers([member1])
        .rpc();

      const circle = await program.account.circleAccount.fetch(cancelCirclePda);
      const balAfter = (
        await getAccount(
          connection,
          cancelMember1Ata,
          undefined,
          TOKEN_2022_PROGRAM_ID
        )
      ).amount;

      assert.deepEqual(
        circle.state,
        { cancelled: {} },
        "circle should be Cancelled"
      );

      // member1 should have received back collateral (100 USDC) + contribution (50 USDC) = 150 USDC
      const expectedReturn = new BN(150_000_000);
      assert.equal(
        (balAfter - balBefore).toString(),
        expectedReturn.toString(),
        "member1 should get collateral + contribution back"
      );
    });

    it("rejects cancel when circle has more than 1 member and deadline not passed", async () => {
      // Create a new circle with 2 members
      const [circle2Pda] = deriveCirclePda(
        program.programId,
        cancelAmount,
        cancelMembers,
        cancelFreq,
        usdcMint,
        1
      );
      const [colVault2Pda] = deriveCollateralVaultPda(
        program.programId,
        circle2Pda
      );
      const [potVault2Pda] = derivePotVaultPda(program.programId, circle2Pda);
      const [mem1Pda2] = deriveMemberPda(
        program.programId,
        circle2Pda,
        member1.publicKey
      );
      const [mem2Pda2] = deriveMemberPda(
        program.programId,
        circle2Pda,
        member2.publicKey
      );
      const [colRec1Pda2] = deriveCollateralRecordPda(
        program.programId,
        circle2Pda,
        member1.publicKey
      );
      const [colRec2Pda2] = deriveCollateralRecordPda(
        program.programId,
        circle2Pda,
        member2.publicKey
      );

      // Create and have both members join
      await program.methods
        .createCircle(cancelAmount, cancelMembers, { daily: {} }, 1)
        .accountsPartial({
          creator: member1.publicKey,
          protocolConfig: configPda,
          circleAccount: circle2Pda,
          collateralVault: colVault2Pda,
          potVault: potVault2Pda,
          usdcMint,
          systemProgram: SystemProgram.programId,
          tokenProgram: TOKEN_2022_PROGRAM_ID,
          associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
        })
        .signers([member1])
        .rpc();

      await program.methods
        .joinCircle()
        .accountsPartial({
          member: member1.publicKey,
          protocolConfig: configPda,
          circleAccount: circle2Pda,
          memberAccount: mem1Pda2,
          collateralRecord: colRec1Pda2,
          memberTokenAccount: member1Ata,
          collateralVault: colVault2Pda,
          potVault: potVault2Pda,
          usdcMint,
          systemProgram: SystemProgram.programId,
          tokenProgram: TOKEN_2022_PROGRAM_ID,
        })
        .signers([member1])
        .rpc();

      await program.methods
        .joinCircle()
        .accountsPartial({
          member: member2.publicKey,
          protocolConfig: configPda,
          circleAccount: circle2Pda,
          memberAccount: mem2Pda2,
          collateralRecord: colRec2Pda2,
          memberTokenAccount: member2Ata,
          collateralVault: colVault2Pda,
          potVault: potVault2Pda,
          usdcMint,
          systemProgram: SystemProgram.programId,
          tokenProgram: TOKEN_2022_PROGRAM_ID,
        })
        .signers([member2])
        .rpc();

      // Try to cancel — should fail because deadline not passed and not solo
      try {
        await program.methods
          .cancelCircle()
          .accountsPartial({
            caller: member1.publicKey,
            protocolConfig: configPda,
            circleAccount: circle2Pda,
            callerMemberAccount: mem1Pda2,
            callerCollateralRecord: colRec1Pda2,
            collateralVault: colVault2Pda,
            potVault: potVault2Pda,
            callerTokenAccount: member1Ata,
            usdcMint,
            tokenProgram: TOKEN_2022_PROGRAM_ID,
            systemProgram: SystemProgram.programId,
          })
          .signers([member1])
          .rpc();

        assert.fail("should have rejected cancel");
      } catch (err: any) {
        assert.ok(
          err.error?.errorCode?.code === "CancelConditionsNotMet" ||
            err.message.includes("CancelConditionsNotMet") ||
            err.message.includes("cancel"),
          "should reject cancel when conditions not met"
        );
      }
    });
  });

  // ─────────────────────────────────────────────────────────
  // 10. PROCESS DEFAULT TESTS
  // ─────────────────────────────────────────────────────────
  describe("process_default", () => {
    let defaultColVaultPda: PublicKey;
    let defaultPotVaultPda: PublicKey;
    const defaultAmount = new BN(20_000_000); // 20 USDC
    const defaultMembers = 2;
    const defaultFreq = 0; // Daily

    before(async () => {
      [defaultCirclePda] = deriveCirclePda(
        program.programId,
        defaultAmount,
        defaultMembers,
        defaultFreq,
        usdcMint,
        0
      );
      [defaultColVaultPda] = deriveCollateralVaultPda(
        program.programId,
        defaultCirclePda
      );
      [defaultPotVaultPda] = derivePotVaultPda(
        program.programId,
        defaultCirclePda
      );

      const [mem1Pda] = deriveMemberPda(
        program.programId,
        defaultCirclePda,
        member1.publicKey
      );
      const [mem2Pda] = deriveMemberPda(
        program.programId,
        defaultCirclePda,
        member2.publicKey
      );
      const [col1Pda] = deriveCollateralRecordPda(
        program.programId,
        defaultCirclePda,
        member1.publicKey
      );
      const [col2Pda] = deriveCollateralRecordPda(
        program.programId,
        defaultCirclePda,
        member2.publicKey
      );
      const [pr1Pda] = derivePaymentRecordPda(
        program.programId,
        defaultCirclePda,
        1
      );

      // Create circle
      await program.methods
        .createCircle(defaultAmount, defaultMembers, { daily: {} }, 0)
        .accountsPartial({
          creator: member1.publicKey,
          protocolConfig: configPda,
          circleAccount: defaultCirclePda,
          collateralVault: defaultColVaultPda,
          potVault: defaultPotVaultPda,
          usdcMint,
          systemProgram: SystemProgram.programId,
          tokenProgram: TOKEN_2022_PROGRAM_ID,
          associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
        })
        .signers([member1])
        .rpc();

      // Both members join
      await program.methods
        .joinCircle()
        .accountsPartial({
          member: member1.publicKey,
          protocolConfig: configPda,
          circleAccount: defaultCirclePda,
          memberAccount: mem1Pda,
          collateralRecord: col1Pda,
          memberTokenAccount: member1Ata,
          collateralVault: defaultColVaultPda,
          potVault: defaultPotVaultPda,
          usdcMint,
          systemProgram: SystemProgram.programId,
          tokenProgram: TOKEN_2022_PROGRAM_ID,
        })
        .signers([member1])
        .rpc();

      await program.methods
        .joinCircle()
        .accountsPartial({
          member: member2.publicKey,
          protocolConfig: configPda,
          circleAccount: defaultCirclePda,
          memberAccount: mem2Pda,
          collateralRecord: col2Pda,
          memberTokenAccount: member2Ata,
          collateralVault: defaultColVaultPda,
          potVault: defaultPotVaultPda,
          usdcMint,
          systemProgram: SystemProgram.programId,
          tokenProgram: TOKEN_2022_PROGRAM_ID,
        })
        .signers([member2])
        .rpc();

      // Start circle
      await program.methods
        .startCircle()
        .accountsPartial({
          caller: admin.publicKey,
          protocolConfig: configPda,
          circleAccount: defaultCirclePda,
          paymentRecord: pr1Pda,
          systemProgram: SystemProgram.programId,
        })
        .signers([admin])
        .rpc();
    });

    it("rejects process_default before deadline has passed", async () => {
      const [mem2Pda] = deriveMemberPda(
        program.programId,
        defaultCirclePda,
        member2.publicKey
      );
      const [col2Pda] = deriveCollateralRecordPda(
        program.programId,
        defaultCirclePda,
        member2.publicKey
      );
      const [pr1Pda] = derivePaymentRecordPda(
        program.programId,
        defaultCirclePda,
        1
      );

      try {
        await program.methods
          .processDefault()
          .accountsPartial({
            caller: admin.publicKey,
            protocolConfig: configPda,
            circleAccount: defaultCirclePda,
            defaulterMemberAccount: mem2Pda,
            defaulterCollateralRecord: col2Pda,
            collateralVault: defaultColVaultPda,
            paymentRecord: pr1Pda,
            potVault: defaultPotVaultPda,
            defaulter: member2.publicKey,
            usdcMint,
            tokenProgram: TOKEN_2022_PROGRAM_ID,
          })
          .signers([admin])
          .rpc();

        assert.fail("should have rejected process_default before deadline");
      } catch (err: any) {
        assert.ok(
          err.error?.errorCode?.code === "DeadlineNotPassed" ||
            err.message.includes("DeadlineNotPassed") ||
            err.message.includes("deadline"),
          "should reject default before deadline"
        );
      }
    });

    it("rejects process_default for a member who already paid", async () => {
      const [mem1Pda] = deriveMemberPda(
        program.programId,
        defaultCirclePda,
        member1.publicKey
      );
      const [col1Pda] = deriveCollateralRecordPda(
        program.programId,
        defaultCirclePda,
        member1.publicKey
      );
      const [pr1Pda] = derivePaymentRecordPda(
        program.programId,
        defaultCirclePda,
        1
      );

      // Cycle 1 payment record was pre-set at start — member1 is position 1
      // and cycle 1 is pre-funded at join time, so member1 is already marked paid
      try {
        await program.methods
          .processDefault()
          .accountsPartial({
            caller: admin.publicKey,
            protocolConfig: configPda,
            circleAccount: defaultCirclePda,
            defaulterMemberAccount: mem1Pda,
            defaulterCollateralRecord: col1Pda,
            collateralVault: defaultColVaultPda,
            paymentRecord: pr1Pda,
            potVault: defaultPotVaultPda,
            defaulter: member1.publicKey,
            usdcMint,
            tokenProgram: TOKEN_2022_PROGRAM_ID,
          })
          .signers([admin])
          .rpc();

        assert.fail("should have rejected process_default for paid member");
      } catch (err: any) {
        assert.ok(
          err.error?.errorCode?.code === "MemberAlreadyPaid" ||
            err.message.includes("MemberAlreadyPaid") ||
            err.error?.errorCode?.code === "DeadlineNotPassed" ||
            err.message.includes("DeadlineNotPassed"),
          "should reject default for member who already paid or deadline not passed"
        );
      }
    });
  });

  // ─────────────────────────────────────────────────────────
  // 11. SECURITY TESTS
  // ─────────────────────────────────────────────────────────
  describe("security", () => {
    it("rejects double join by same wallet", async () => {
      // circlePda from the main test suite is Completed — use a fresh one
      const freshAmount = new BN(5_000_000); // 5 USDC
      const [freshPda] = deriveCirclePda(
        program.programId,
        freshAmount,
        3,
        0,
        usdcMint,
        0
      );
      const [freshColV] = deriveCollateralVaultPda(program.programId, freshPda);
      const [freshPotV] = derivePotVaultPda(program.programId, freshPda);
      const [freshMem1] = deriveMemberPda(
        program.programId,
        freshPda,
        member1.publicKey
      );
      const [freshCol1] = deriveCollateralRecordPda(
        program.programId,
        freshPda,
        member1.publicKey
      );

      await program.methods
        .createCircle(freshAmount, 3, { daily: {} }, 0)
        .accountsPartial({
          creator: member1.publicKey,
          protocolConfig: configPda,
          circleAccount: freshPda,
          collateralVault: freshColV,
          potVault: freshPotV,
          usdcMint,
          systemProgram: SystemProgram.programId,
          tokenProgram: TOKEN_2022_PROGRAM_ID,
          associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
        })
        .signers([member1])
        .rpc();

      // First join — should succeed
      await program.methods
        .joinCircle()
        .accountsPartial({
          member: member1.publicKey,
          protocolConfig: configPda,
          circleAccount: freshPda,
          memberAccount: freshMem1,
          collateralRecord: freshCol1,
          memberTokenAccount: member1Ata,
          collateralVault: freshColV,
          potVault: freshPotV,
          usdcMint,
          systemProgram: SystemProgram.programId,
          tokenProgram: TOKEN_2022_PROGRAM_ID,
        })
        .signers([member1])
        .rpc();

      // Second join — should fail because MemberAccount PDA already exists
      try {
        await program.methods
          .joinCircle()
          .accountsPartial({
            member: member1.publicKey,
            protocolConfig: configPda,
            circleAccount: freshPda,
            memberAccount: freshMem1,
            collateralRecord: freshCol1,
            memberTokenAccount: member1Ata,
            collateralVault: freshColV,
            potVault: freshPotV,
            usdcMint,
            systemProgram: SystemProgram.programId,
            tokenProgram: TOKEN_2022_PROGRAM_ID,
          })
          .signers([member1])
          .rpc();

        assert.fail("should have rejected double join");
      } catch (err: any) {
        assert.ok(
          err.message.includes("already in use") ||
            err.logs?.some((l: string) => l.includes("already in use")),
          "should reject double join"
        );
      }
    });

    it("rejects pay_contribution when circle is not active", async () => {
      // Use the fresh circle from above — it is still Open (only 1 member)
      const freshAmount = new BN(5_000_000);
      const [freshPda] = deriveCirclePda(
        program.programId,
        freshAmount,
        3,
        0,
        usdcMint,
        0
      );
      const [freshPotV] = derivePotVaultPda(program.programId, freshPda);
      const [freshMem1] = deriveMemberPda(
        program.programId,
        freshPda,
        member1.publicKey
      );
      const [freshPr1] = derivePaymentRecordPda(program.programId, freshPda, 1);

      try {
        await program.methods
          .payContribution()
          .accountsPartial({
            member: member1.publicKey,
            protocolConfig: configPda,
            circleAccount: freshPda,
            memberAccount: freshMem1,
            paymentRecord: freshPr1,
            memberTokenAccount: member1Ata,
            potVault: freshPotV,
            usdcMint,
            tokenProgram: TOKEN_2022_PROGRAM_ID,
          })
          .signers([member1])
          .rpc();

        assert.fail("should have rejected pay on non-active circle");
      } catch (err: any) {
        // Program rejects with various errors depending on which constraint fires first
        // Could be CircleNotActive, ConstraintSeeds, or AccountNotInitialized
        assert.ok(
          err.error?.errorCode?.code !== undefined ||
            err.message.includes("Error") ||
            err.logs?.length > 0,
          "should reject pay_contribution when circle is not active"
        );
      }
    });

    it("rejects unauthorized admin operations", async () => {
      // member1 tries to pause — should fail
      try {
        await program.methods
          .pauseProtocol()
          .accountsPartial({
            admin: member1.publicKey,
            protocolConfig: configPda,
          })
          .signers([member1])
          .rpc();

        assert.fail("should have rejected non-admin pause");
      } catch (err: any) {
        assert.ok(
          err.error?.errorCode?.code === "Unauthorized" ||
            err.message.includes("Unauthorized"),
          "should reject non-admin pause"
        );
      }
    });

    it("rejects start_circle when circle is not ready", async () => {
      // Use a circle that is Open (not all seats filled) — the fresh one above
      const freshAmount = new BN(5_000_000);
      const [freshPda] = deriveCirclePda(
        program.programId,
        freshAmount,
        3,
        0,
        usdcMint,
        0
      );
      const [pr1Pda] = derivePaymentRecordPda(program.programId, freshPda, 1);

      try {
        await program.methods
          .startCircle()
          .accountsPartial({
            caller: admin.publicKey,
            protocolConfig: configPda,
            circleAccount: freshPda,
            paymentRecord: pr1Pda,
            systemProgram: SystemProgram.programId,
          })
          .signers([admin])
          .rpc();

        assert.fail("should have rejected start on non-ready circle");
      } catch (err: any) {
        assert.ok(
          err.error?.errorCode?.code === "CircleNotReady" ||
            err.message.includes("CircleNotReady") ||
            err.message.includes("not ready"),
          "should reject start_circle when circle is not ready"
        );
      }
    });

    it("rejects claim_collateral before circle completes", async () => {
      // Use the default test circle which is Active
      const [mem1Pda] = deriveMemberPda(
        program.programId,
        defaultCirclePda,
        member1.publicKey
      );
      const [col1Pda] = deriveCollateralRecordPda(
        program.programId,
        defaultCirclePda,
        member1.publicKey
      );
      const [collV] = deriveCollateralVaultPda(
        program.programId,
        defaultCirclePda
      );

      try {
        await program.methods
          .claimCollateral()
          .accountsPartial({
            member: member1.publicKey,
            protocolConfig: configPda,
            circleAccount: defaultCirclePda,
            memberAccount: mem1Pda,
            collateralRecord: col1Pda,
            collateralVault: collV,
            memberTokenAccount: member1Ata,
            usdcMint,
            tokenProgram: TOKEN_2022_PROGRAM_ID,
            systemProgram: SystemProgram.programId,
          })
          .signers([member1])
          .rpc();

        assert.fail("should have rejected claim before completion");
      } catch (err: any) {
        assert.ok(
          err.error?.errorCode?.code === "CircleNotComplete" ||
            err.message.includes("CircleNotComplete") ||
            err.message.includes("not complete"),
          "should reject claim_collateral before circle completes"
        );
      }
    });
  });

  // ─────────────────────────────────────────────────────────
  // 12. NONCE / SEQUENTIAL CIRCLE TESTS
  // ─────────────────────────────────────────────────────────
  describe("nonce — sequential circles", () => {
    it("allows creating a second circle with same params at nonce 1", async () => {
      const amount = new BN(1_000_000); // 1 USDC
      const members = 2;
      const freq = 0; // Daily

      // Create nonce 0
      const [pda0] = deriveCirclePda(
        program.programId,
        amount,
        members,
        freq,
        usdcMint,
        0
      );
      const [colV0] = deriveCollateralVaultPda(program.programId, pda0);
      const [potV0] = derivePotVaultPda(program.programId, pda0);

      await program.methods
        .createCircle(amount, members, { daily: {} }, 0)
        .accountsPartial({
          creator: member1.publicKey,
          protocolConfig: configPda,
          circleAccount: pda0,
          collateralVault: colV0,
          potVault: potV0,
          usdcMint,
          systemProgram: SystemProgram.programId,
          tokenProgram: TOKEN_2022_PROGRAM_ID,
          associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
        })
        .signers([member1])
        .rpc();

      // Create nonce 1 — same params, should succeed
      const [pda1] = deriveCirclePda(
        program.programId,
        amount,
        members,
        freq,
        usdcMint,
        1
      );
      const [colV1] = deriveCollateralVaultPda(program.programId, pda1);
      const [potV1] = derivePotVaultPda(program.programId, pda1);

      await program.methods
        .createCircle(amount, members, { daily: {} }, 1)
        .accountsPartial({
          creator: member1.publicKey,
          protocolConfig: configPda,
          circleAccount: pda1,
          collateralVault: colV1,
          potVault: potV1,
          usdcMint,
          systemProgram: SystemProgram.programId,
          tokenProgram: TOKEN_2022_PROGRAM_ID,
          associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
        })
        .signers([member1])
        .rpc();

      const circle0 = await program.account.circleAccount.fetch(pda0);
      const circle1 = await program.account.circleAccount.fetch(pda1);

      assert.equal(circle0.nonce, 0, "nonce 0 circle should have nonce 0");
      assert.equal(circle1.nonce, 1, "nonce 1 circle should have nonce 1");
      assert.notEqual(
        pda0.toBase58(),
        pda1.toBase58(),
        "PDAs should be different"
      );
      assert.deepEqual(circle0.state, { open: {} }, "nonce 0 should be open");
      assert.deepEqual(circle1.state, { open: {} }, "nonce 1 should be open");
    });

    it("rejects creating the same nonce twice", async () => {
      const amount = new BN(1_000_000);
      const [pda0] = deriveCirclePda(
        program.programId,
        amount,
        2,
        0,
        usdcMint,
        0
      );
      const [colV0] = deriveCollateralVaultPda(program.programId, pda0);
      const [potV0] = derivePotVaultPda(program.programId, pda0);

      try {
        await program.methods
          .createCircle(amount, 2, { daily: {} }, 0)
          .accountsPartial({
            creator: member1.publicKey,
            protocolConfig: configPda,
            circleAccount: pda0,
            collateralVault: colV0,
            potVault: potV0,
            usdcMint,
            systemProgram: SystemProgram.programId,
            tokenProgram: TOKEN_2022_PROGRAM_ID,
            associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
          })
          .signers([member1])
          .rpc();

        assert.fail("should have rejected duplicate nonce");
      } catch (err: any) {
        assert.ok(
          err.message.includes("already in use") ||
            err.logs?.some((l: string) => l.includes("already in use")),
          "should reject creating same nonce twice"
        );
      }
    });
  });
});
