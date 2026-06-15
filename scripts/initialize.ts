import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { RoundsProtocol } from "../target/types/rounds_protocol";
import { PublicKey, Keypair } from "@solana/web3.js";
import {
  createMint,
  getAssociatedTokenAddressSync,
  TOKEN_2022_PROGRAM_ID,
  ASSOCIATED_TOKEN_PROGRAM_ID,
} from "@solana/spl-token";
import * as fs from "fs";

const MINT_KEYPAIR_PATH = "scripts/usdc-mint-keypair.json";

async function main() {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.RoundsProtocol as Program<RoundsProtocol>;
  const admin = (provider.wallet as anchor.Wallet).payer;

  console.log("Admin:      ", admin.publicKey.toBase58());
  console.log("Program ID: ", program.programId.toBase58());

  // ── Step 1: Create or load USDC mint ──────────────────
  let usdcMint: PublicKey;

  if (fs.existsSync(MINT_KEYPAIR_PATH)) {
    const mintKeypairData = JSON.parse(
      fs.readFileSync(MINT_KEYPAIR_PATH, "utf-8")
    );
    const mintKeypair = Keypair.fromSecretKey(Uint8Array.from(mintKeypairData));
    usdcMint = mintKeypair.publicKey;
    console.log("USDC Mint (existing): ", usdcMint.toBase58());
  } else {
    console.log("Creating new Token 2022 USDC mint...");
    const mintKeypair = Keypair.generate();

    usdcMint = await createMint(
      provider.connection,
      admin,
      admin.publicKey,
      null,
      6,
      mintKeypair,
      undefined,
      TOKEN_2022_PROGRAM_ID
    );

    fs.writeFileSync(
      MINT_KEYPAIR_PATH,
      JSON.stringify(Array.from(mintKeypair.secretKey))
    );

    console.log("USDC Mint (new): ", usdcMint.toBase58());
    console.log("Mint keypair saved to:", MINT_KEYPAIR_PATH);
  }

  // ── Step 2: Derive PDAs ────────────────────────────────
  const [configPda] = PublicKey.findProgramAddressSync(
    [Buffer.from("config")],
    program.programId
  );

  const treasuryVaultPda = getAssociatedTokenAddressSync(
    usdcMint,
    configPda,
    true,
    TOKEN_2022_PROGRAM_ID,
    ASSOCIATED_TOKEN_PROGRAM_ID
  );

  console.log("\nConfig PDA:     ", configPda.toBase58());
  console.log("Treasury Vault: ", treasuryVaultPda.toBase58());

  // ── Step 3: Check if already initialised ──────────────
  try {
    const existing = await program.account.protocolConfig.fetch(configPda);
    console.log("\nProtocol already initialised.");
    console.log("Admin:     ", existing.admin.toBase58());
    console.log("Fee BPS:   ", existing.protocolFeeBps);
    console.log("Is Paused: ", existing.isPaused);
    console.log("\n─────────────────────────────────────────────");
    console.log("YOUR FRONTEND ENV VARIABLES:");
    console.log("─────────────────────────────────────────────");
    console.log(
      `NEXT_PUBLIC_PROGRAM_ID=7BBvnkQ4AKMFU6EfWvScSqi69eu9TjLoDzpmzG8ZeFhN`
    );
    console.log(`NEXT_PUBLIC_CONFIG_PDA=${configPda.toBase58()}`);
    console.log(`NEXT_PUBLIC_TREASURY_VAULT=${treasuryVaultPda.toBase58()}`);
    console.log(`NEXT_PUBLIC_USDC_MINT=${usdcMint.toBase58()}`);
    console.log(`NEXT_PUBLIC_CLUSTER=devnet`);
    return;
  } catch {
    console.log("\nConfig not found — initialising now...");
  }

  // ── Step 4: Initialize protocol ────────────────────────
  const tx = await program.methods
    .initializeProtocol(50)
    .accounts({
      admin: admin.publicKey,
      usdcMint: usdcMint,
      tokenProgram: TOKEN_2022_PROGRAM_ID,
    })
    .rpc();

  console.log("\n✓ Protocol initialised successfully.");
  console.log("Transaction:    ", tx);
  console.log("\n─────────────────────────────────────────────");
  console.log("YOUR FRONTEND ENV VARIABLES:");
  console.log("─────────────────────────────────────────────");
  console.log(
    `NEXT_PUBLIC_PROGRAM_ID=7BBvnkQ4AKMFU6EfWvScSqi69eu9TjLoDzpmzG8ZeFhN`
  );
  console.log(`NEXT_PUBLIC_CONFIG_PDA=${configPda.toBase58()}`);
  console.log(`NEXT_PUBLIC_TREASURY_VAULT=${treasuryVaultPda.toBase58()}`);
  console.log(`NEXT_PUBLIC_USDC_MINT=${usdcMint.toBase58()}`);
  console.log(`NEXT_PUBLIC_CLUSTER=devnet`);
  console.log("\nSave these into your frontend .env.local file.");
}

main().catch(console.error);
