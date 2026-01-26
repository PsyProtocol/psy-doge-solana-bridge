/**
 * Manual Claim Client
 *
 * A client for interacting with the manual-claim program, allowing users to:
 * - Create/derive their manual claim PDA account
 * - Submit manual claims with ZK proofs
 * - Scan transaction history for previous claims
 */

import {
  Connection,
  Keypair,
  PublicKey,
  Transaction,
  TransactionInstruction,
  TransactionSignature,
  sendAndConfirmTransaction,
  SystemProgram,
  ConfirmedSignatureInfo,
  ParsedTransactionWithMeta,
} from "@solana/web3.js";
import {
  getAssociatedTokenAddress,
  TOKEN_PROGRAM_ID,
} from "@solana/spl-token";
import { UserClientError } from "./errors";
import { DEFAULT_BRIDGE_PROGRAM_ID, BRIDGE_STATE_SEED } from "./config";

/** Default manual-claim program ID */
export const DEFAULT_MANUAL_CLAIM_PROGRAM_ID = new PublicKey("MCdYbqiK3uj36tohbMjsh3Ssg8iRSJmSHToNxW8TWWE");

/** Manual claim PDA seed */
export const MANUAL_CLAIM_SEED = "manual-claim";

/** Manual claim instruction discriminator */
export const MC_MANUAL_CLAIM_TRANSACTION_DISCRIMINATOR = 0;

/**
 * ManualClaimInstruction data structure
 *
 * Layout:
 * - proof: 256 bytes (CompactBridgeZKProof)
 * - recent_block_merkle_tree_root: 32 bytes
 * - recent_auto_claim_txo_root: 32 bytes
 * - new_manual_claim_txo_root: 32 bytes
 * - tx_hash: 32 bytes
 * - combined_txo_index: 8 bytes (u64)
 * - deposit_amount_sats: 8 bytes (u64)
 *
 * Total: 400 bytes
 */
export interface ManualClaimInstruction {
  /** ZK proof (256 bytes) */
  proof: Uint8Array;
  /** Recent block merkle tree root (32 bytes) */
  recentBlockMerkleTreeRoot: Uint8Array;
  /** Recent auto claim TXO root (32 bytes) */
  recentAutoClaimTxoRoot: Uint8Array;
  /** New manual claim TXO root (32 bytes) */
  newManualClaimTxoRoot: Uint8Array;
  /** Transaction hash (32 bytes) */
  txHash: Uint8Array;
  /** Combined TXO index */
  combinedTxoIndex: bigint;
  /** Deposit amount in satoshis */
  depositAmountSats: bigint;
}

/** Size of ManualClaimInstruction in bytes */
export const MANUAL_CLAIM_INSTRUCTION_SIZE = 256 + 32 + 32 + 32 + 32 + 8 + 8; // 400 bytes

/**
 * A parsed manual claim from transaction history
 */
export interface ParsedManualClaim {
  /** Transaction signature */
  signature: string;
  /** Slot the transaction was confirmed in */
  slot: number;
  /** Block time (Unix timestamp) if available */
  blockTime: number | null;
  /** The parsed instruction data */
  instruction: ManualClaimInstruction;
}

/**
 * Configuration for the ManualClaimClient
 */
export interface ManualClaimClientConfig {
  /** Solana RPC URL */
  rpcUrl: string;
  /** Manual-claim program ID */
  manualClaimProgramId: PublicKey;
  /** Main bridge program ID */
  bridgeProgramId: PublicKey;
  /** Bridge state PDA */
  bridgeStatePda: PublicKey;
}

/**
 * Builder for ManualClaimClientConfig
 */
export class ManualClaimClientConfigBuilder {
  private _rpcUrl?: string;
  private _manualClaimProgramId?: PublicKey;
  private _bridgeProgramId?: PublicKey;
  private _bridgeStatePda?: PublicKey;

  /**
   * Set the RPC URL
   */
  rpcUrl(url: string): this {
    this._rpcUrl = url;
    return this;
  }

  /**
   * Set the manual-claim program ID
   */
  manualClaimProgramId(id: PublicKey): this {
    this._manualClaimProgramId = id;
    return this;
  }

  /**
   * Set the main bridge program ID
   */
  bridgeProgramId(id: PublicKey): this {
    this._bridgeProgramId = id;
    return this;
  }

  /**
   * Set the bridge state PDA
   */
  bridgeStatePda(pda: PublicKey): this {
    this._bridgeStatePda = pda;
    return this;
  }

  /**
   * Build the configuration
   */
  build(): ManualClaimClientConfig {
    if (!this._rpcUrl) {
      throw new Error("RPC URL is required");
    }

    const manualClaimProgramId = this._manualClaimProgramId ?? DEFAULT_MANUAL_CLAIM_PROGRAM_ID;
    const bridgeProgramId = this._bridgeProgramId ?? DEFAULT_BRIDGE_PROGRAM_ID;

    const [derivedBridgeStatePda] = PublicKey.findProgramAddressSync(
      [new TextEncoder().encode(BRIDGE_STATE_SEED)],
      bridgeProgramId
    );
    const bridgeStatePda = this._bridgeStatePda ?? derivedBridgeStatePda;

    return {
      rpcUrl: this._rpcUrl,
      manualClaimProgramId,
      bridgeProgramId,
      bridgeStatePda,
    };
  }
}

/**
 * Client for manual claim operations
 */
export class ManualClaimClient {
  readonly config: ManualClaimClientConfig;
  readonly connection: Connection;
  private dogeMintCache?: PublicKey;

  constructor(config: ManualClaimClientConfig) {
    this.config = config;
    this.connection = new Connection(config.rpcUrl, "confirmed");
  }

  /**
   * Create a new manual claim client with just an RPC URL
   */
  static create(rpcUrl: string): ManualClaimClient {
    const config = new ManualClaimClientConfigBuilder()
      .rpcUrl(rpcUrl)
      .build();
    return new ManualClaimClient(config);
  }

  /**
   * Create a new manual claim client with custom configuration
   */
  static withConfig(config: ManualClaimClientConfig): ManualClaimClient {
    return new ManualClaimClient(config);
  }

  /**
   * Get the manual-claim program ID
   */
  get manualClaimProgramId(): PublicKey {
    return this.config.manualClaimProgramId;
  }

  /**
   * Get the bridge program ID
   */
  get bridgeProgramId(): PublicKey {
    return this.config.bridgeProgramId;
  }

  /**
   * Get the bridge state PDA
   */
  get bridgeStatePda(): PublicKey {
    return this.config.bridgeStatePda;
  }

  /**
   * Get the DOGE mint address (cached after first fetch)
   */
  async getDogeMint(): Promise<PublicKey> {
    if (this.dogeMintCache) {
      return this.dogeMintCache;
    }

    const account = await this.connection.getAccountInfo(this.config.bridgeStatePda);
    if (!account) {
      throw UserClientError.accountNotFound(this.config.bridgeStatePda.toString());
    }

    // DOGE mint is at the end of the bridge state
    const mintOffset = account.data.length - 32;
    const mintBytes = account.data.slice(mintOffset, mintOffset + 32);
    const mint = new PublicKey(mintBytes);

    this.dogeMintCache = mint;
    return mint;
  }

  // ===========================================================================
  // PDA Derivation
  // ===========================================================================

  /**
   * Derive the manual claim PDA for a user
   *
   * The PDA is derived using seeds: ["manual-claim", user_pubkey]
   */
  deriveClaimPda(user: PublicKey): [PublicKey, number] {
    return PublicKey.findProgramAddressSync(
      [new TextEncoder().encode(MANUAL_CLAIM_SEED), user.toBytes()],
      this.config.manualClaimProgramId
    );
  }

  /**
   * Get the manual claim PDA address for a user (without bump)
   */
  getClaimPdaAddress(user: PublicKey): PublicKey {
    return this.deriveClaimPda(user)[0];
  }

  /**
   * Check if a user's manual claim PDA account exists
   */
  async claimAccountExists(user: PublicKey): Promise<boolean> {
    const [pda] = this.deriveClaimPda(user);
    const account = await this.connection.getAccountInfo(pda);
    return account !== null;
  }

  /**
   * Get the manual claim state for a user (if it exists)
   *
   * Returns the raw account data as Uint8Array (32 bytes: manual_claimed_txo_tree_root)
   */
  async getClaimState(user: PublicKey): Promise<Uint8Array | null> {
    const [pda] = this.deriveClaimPda(user);
    const account = await this.connection.getAccountInfo(pda);

    if (!account) {
      return null;
    }

    return account.data;
  }

  // ===========================================================================
  // Claim Operations
  // ===========================================================================

  /**
   * Submit a manual claim
   *
   * This creates the claim PDA if it doesn't exist and processes the manual claim.
   *
   * @param user - The user's keypair (signer)
   * @param payer - The payer's keypair (pays for PDA creation if needed)
   * @param instructionData - The manual claim instruction data
   * @returns The transaction signature
   */
  async submitManualClaim(
    user: Keypair,
    payer: Keypair,
    instructionData: ManualClaimInstruction
  ): Promise<TransactionSignature> {
    const dogeMint = await this.getDogeMint();
    const [claimPda] = this.deriveClaimPda(user.publicKey);
    const recipientAta = await getAssociatedTokenAddress(dogeMint, user.publicKey);

    const ix = this.buildManualClaimInstruction(
      user.publicKey,
      payer.publicKey,
      claimPda,
      recipientAta,
      dogeMint,
      instructionData
    );

    return this.sendAndConfirm([ix], payer, [user]);
  }

  /**
   * Build a manual claim instruction
   */
  buildManualClaimInstruction(
    user: PublicKey,
    payer: PublicKey,
    claimPda: PublicKey,
    recipientAta: PublicKey,
    dogeMint: PublicKey,
    instructionData: ManualClaimInstruction
  ): TransactionInstruction {
    const data = this.serializeManualClaimInstruction(instructionData);

    return new TransactionInstruction({
      keys: [
        { pubkey: claimPda, isSigner: false, isWritable: true },                    // claim_state_pda
        { pubkey: this.config.bridgeStatePda, isSigner: false, isWritable: false }, // bridge_state_account
        { pubkey: recipientAta, isSigner: false, isWritable: true },                // recipient_account
        { pubkey: dogeMint, isSigner: false, isWritable: true },                    // doge_mint
        { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },           // token_program
        { pubkey: this.config.bridgeProgramId, isSigner: false, isWritable: false },// main_bridge_program
        { pubkey: user, isSigner: true, isWritable: false },                        // user (signer)
        { pubkey: payer, isSigner: true, isWritable: true },                        // payer (signer, writable)
        { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },    // system_program
      ],
      programId: this.config.manualClaimProgramId,
      data: Buffer.from(data),
    });
  }

  /**
   * Serialize ManualClaimInstruction to bytes
   */
  private serializeManualClaimInstruction(instruction: ManualClaimInstruction): Uint8Array {
    // Validate sizes
    if (instruction.proof.length !== 256) {
      throw UserClientError.invalidInput("Proof must be 256 bytes");
    }
    if (instruction.recentBlockMerkleTreeRoot.length !== 32) {
      throw UserClientError.invalidInput("Recent block merkle tree root must be 32 bytes");
    }
    if (instruction.recentAutoClaimTxoRoot.length !== 32) {
      throw UserClientError.invalidInput("Recent auto claim TXO root must be 32 bytes");
    }
    if (instruction.newManualClaimTxoRoot.length !== 32) {
      throw UserClientError.invalidInput("New manual claim TXO root must be 32 bytes");
    }
    if (instruction.txHash.length !== 32) {
      throw UserClientError.invalidInput("TX hash must be 32 bytes");
    }

    // Aligned instruction: 8 bytes discriminator + instruction data
    const totalSize = 8 + MANUAL_CLAIM_INSTRUCTION_SIZE;
    const data = new Uint8Array(totalSize);
    const view = new DataView(data.buffer);

    // Fill discriminator (first 8 bytes)
    data.fill(MC_MANUAL_CLAIM_TRANSACTION_DISCRIMINATOR, 0, 8);

    let offset = 8;

    // proof (256 bytes)
    data.set(instruction.proof, offset);
    offset += 256;

    // recent_block_merkle_tree_root (32 bytes)
    data.set(instruction.recentBlockMerkleTreeRoot, offset);
    offset += 32;

    // recent_auto_claim_txo_root (32 bytes)
    data.set(instruction.recentAutoClaimTxoRoot, offset);
    offset += 32;

    // new_manual_claim_txo_root (32 bytes)
    data.set(instruction.newManualClaimTxoRoot, offset);
    offset += 32;

    // tx_hash (32 bytes)
    data.set(instruction.txHash, offset);
    offset += 32;

    // combined_txo_index (u64, little-endian)
    view.setBigUint64(offset, instruction.combinedTxoIndex, true);
    offset += 8;

    // deposit_amount_sats (u64, little-endian)
    view.setBigUint64(offset, instruction.depositAmountSats, true);

    return data;
  }

  /**
   * Deserialize ManualClaimInstruction from bytes
   */
  private deserializeManualClaimInstruction(data: Uint8Array): ManualClaimInstruction | null {
    // Check minimum size: 8 bytes discriminator + instruction data
    if (data.length < 8 + MANUAL_CLAIM_INSTRUCTION_SIZE) {
      return null;
    }

    // Check discriminator (first byte should match, rest are padding)
    if (data[0] !== MC_MANUAL_CLAIM_TRANSACTION_DISCRIMINATOR) {
      return null;
    }

    const view = new DataView(data.buffer, data.byteOffset);
    let offset = 8;

    const proof = data.slice(offset, offset + 256);
    offset += 256;

    const recentBlockMerkleTreeRoot = data.slice(offset, offset + 32);
    offset += 32;

    const recentAutoClaimTxoRoot = data.slice(offset, offset + 32);
    offset += 32;

    const newManualClaimTxoRoot = data.slice(offset, offset + 32);
    offset += 32;

    const txHash = data.slice(offset, offset + 32);
    offset += 32;

    const combinedTxoIndex = view.getBigUint64(offset, true);
    offset += 8;

    const depositAmountSats = view.getBigUint64(offset, true);

    return {
      proof: new Uint8Array(proof),
      recentBlockMerkleTreeRoot: new Uint8Array(recentBlockMerkleTreeRoot),
      recentAutoClaimTxoRoot: new Uint8Array(recentAutoClaimTxoRoot),
      newManualClaimTxoRoot: new Uint8Array(newManualClaimTxoRoot),
      txHash: new Uint8Array(txHash),
      combinedTxoIndex,
      depositAmountSats,
    };
  }

  // ===========================================================================
  // History Scanning
  // ===========================================================================

  /**
   * Get all manual claims for a user by scanning transaction history
   *
   * This fetches signatures for the user's manual claim PDA and parses the
   * ManualClaimInstruction from each transaction.
   *
   * @param user - The user's public key
   * @param before - Optional signature to start scanning before (for pagination)
   * @param limit - Maximum number of claims to return (default: 100)
   * @returns A list of parsed manual claims
   */
  async getClaimHistory(
    user: PublicKey,
    before?: string,
    limit: number = 100
  ): Promise<ParsedManualClaim[]> {
    const [claimPda] = this.deriveClaimPda(user);

    // Fetch signatures for the PDA
    const signatures = await this.connection.getSignaturesForAddress(
      claimPda,
      {
        before,
        limit,
      },
      "confirmed"
    );

    const claims: ParsedManualClaim[] = [];

    for (const sigInfo of signatures) {
      // Skip failed transactions
      if (sigInfo.err) {
        continue;
      }

      try {
        const instruction = await this.fetchAndParseClaimTransaction(sigInfo.signature);
        if (instruction) {
          claims.push({
            signature: sigInfo.signature,
            slot: sigInfo.slot,
            blockTime: sigInfo.blockTime ?? null,
            instruction,
          });
        }
      } catch {
        // Skip transactions we can't parse
        continue;
      }
    }

    return claims;
  }

  /**
   * Fetch and parse a manual claim instruction from a transaction
   */
  private async fetchAndParseClaimTransaction(
    signature: string
  ): Promise<ManualClaimInstruction | null> {
    const tx = await this.connection.getTransaction(signature, {
      commitment: "confirmed",
      maxSupportedTransactionVersion: 0,
    });

    if (!tx || !tx.transaction) {
      return null;
    }

    const message = tx.transaction.message;
    const accountKeys = message.staticAccountKeys;

    // Handle both legacy and versioned transactions
    const instructions = message.compiledInstructions;

    for (const ix of instructions) {
      const programIdIndex = ix.programIdIndex;
      if (programIdIndex >= accountKeys.length) {
        continue;
      }

      const programId = accountKeys[programIdIndex];

      if (programId.equals(this.config.manualClaimProgramId)) {
        const data = ix.data;
        const instruction = this.deserializeManualClaimInstruction(data);
        if (instruction) {
          return instruction;
        }
      }
    }

    return null;
  }

  /**
   * Get all claim history without pagination (fetches all pages)
   *
   * This repeatedly calls getClaimHistory until all claims are fetched.
   * Use with caution for users with many claims.
   *
   * @param user - The user's public key
   * @param batchSize - Number of claims to fetch per request (default: 100)
   * @returns A list of all parsed manual claims
   */
  async getAllClaimHistory(
    user: PublicKey,
    batchSize: number = 100
  ): Promise<ParsedManualClaim[]> {
    const allClaims: ParsedManualClaim[] = [];
    let before: string | undefined = undefined;

    while (true) {
      const claims = await this.getClaimHistory(user, before, batchSize);

      if (claims.length === 0) {
        break;
      }

      // Set the "before" cursor to the last signature for next page
      before = claims[claims.length - 1].signature;

      allClaims.push(...claims);

      // If we got fewer than batchSize, we've reached the end
      if (claims.length < batchSize) {
        break;
      }
    }

    return allClaims;
  }

  // ===========================================================================
  // Internal Helpers
  // ===========================================================================

  /**
   * Send a transaction and wait for confirmation
   */
  private async sendAndConfirm(
    instructions: TransactionInstruction[],
    payer: Keypair,
    extraSigners: Keypair[] = []
  ): Promise<TransactionSignature> {
    const tx = new Transaction().add(...instructions);
    const signers = [payer, ...extraSigners];
    return sendAndConfirmTransaction(this.connection, tx, signers);
  }
}

/**
 * Helper to create an empty ManualClaimInstruction
 */
export function createEmptyManualClaimInstruction(): ManualClaimInstruction {
  return {
    proof: new Uint8Array(256),
    recentBlockMerkleTreeRoot: new Uint8Array(32),
    recentAutoClaimTxoRoot: new Uint8Array(32),
    newManualClaimTxoRoot: new Uint8Array(32),
    txHash: new Uint8Array(32),
    combinedTxoIndex: 0n,
    depositAmountSats: 0n,
  };
}
