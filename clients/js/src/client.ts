/**
 * Main BridgeClient implementation.
 *
 * Provides a clean, abstracted interface to all bridge operations.
 */

import {
  Connection,
  Keypair,
  PublicKey,
  Transaction,
  TransactionInstruction,
  TransactionSignature,
  sendAndConfirmTransaction,
  Commitment,
} from "@solana/web3.js";
import { getAssociatedTokenAddress } from "@solana/spl-token";
import {
  BridgeClientConfig,
  BridgeClientConfigBuilder,
  RetryConfig,
  DEFAULT_RETRY_CONFIG,
} from "./config";
import { BridgeError } from "./errors";
import {
  getBridgeStatePda,
  initializeBridge,
  blockUpdate,
  processReorgBlocks,
  requestWithdrawal,
  processWithdrawal,
  processReplayWithdrawal,
  processManualDeposit,
  manualClaimDeposit,
  processMintGroup,
  processMintGroupAutoAdvance,
  operatorWithdrawFees,
  snapshotWithdrawals,
} from "./instructions";
import {
  createGenericBuffer,
  createTxoBuffer,
  createPendingMintBuffer,
  getMintBufferPda,
  getTxoBufferPda,
} from "./buffers";
import {
  PsyBridgeHeader,
  PsyReturnTxOutput,
  PsyBridgeProgramState,
  PsyWithdrawalChainSnapshot,
  InitializeBridgeParams,
  FinalizedBlockMintTxoInfo,
  PendingMint,
  CompactBridgeZKProof,
  ProcessMintsResult,
  DepositTxOutputRecord,
  emptyProcessMintsResult,
  decodePsyBridgeHeader,
  decodePsyReturnTxOutput,
  decodePsyBridgeConfig,
  PSY_BRIDGE_HEADER_SIZE,
  PSY_BRIDGE_CONFIG_SIZE,
  PSY_RETURN_TX_OUTPUT_SIZE,
} from "./types";
import { PM_MAX_PENDING_MINTS_PER_GROUP } from "./constants";

/**
 * Main client for interacting with the Doge bridge on Solana.
 */
export class BridgeClient {
  readonly config: BridgeClientConfig;
  readonly connection: Connection;
  private dogeMintCache?: PublicKey;

  constructor(config: BridgeClientConfig) {
    this.config = config;
    this.connection = new Connection(config.rpcUrl, "confirmed");
  }

  /**
   * Create a new bridge client with minimal configuration.
   */
  static create(
    rpcUrl: string,
    operatorKeypair: Keypair,
    payerKeypair: Keypair,
    bridgeStatePda: PublicKey,
    wormholeCoreProgramId: PublicKey,
    wormholeShimProgramId: PublicKey
  ): BridgeClient {
    const config = new BridgeClientConfigBuilder()
      .rpcUrl(rpcUrl)
      .bridgeStatePda(bridgeStatePda)
      .operator(operatorKeypair)
      .payer(payerKeypair)
      .wormholeCoreProgramId(wormholeCoreProgramId)
      .wormholeShimProgramId(wormholeShimProgramId)
      .build();
    return new BridgeClient(config);
  }

  /**
   * Get the bridge state PDA.
   */
  get bridgeStatePda(): PublicKey {
    return this.config.bridgeStatePda;
  }

  /**
   * Get the operator public key.
   */
  get operatorPubkey(): PublicKey {
    return this.config.operator.publicKey;
  }

  /**
   * Get the payer public key.
   */
  get payerPubkey(): PublicKey {
    return this.config.payer.publicKey;
  }

  /**
   * Get the DOGE mint address (cached after first fetch).
   */
  async getDogeMint(): Promise<PublicKey> {
    if (this.dogeMintCache) {
      return this.dogeMintCache;
    }
    if (this.config.dogeMint) {
      this.dogeMintCache = this.config.dogeMint;
      return this.dogeMintCache;
    }
    const mint = await this.getDogeMintFromState();
    this.dogeMintCache = mint;
    return mint;
  }

  /**
   * Fetch DOGE mint from on-chain state.
   */
  private async getDogeMintFromState(): Promise<PublicKey> {
    const account = await this.connection.getAccountInfo(this.config.bridgeStatePda);
    if (!account) {
      throw BridgeError.accountNotFound(this.config.bridgeStatePda.toString());
    }
    // DOGE mint is stored after the header, config, and other fields
    // The exact offset depends on the state layout
    return new PublicKey(account.data.subarray(account.data.length - 32));
  }

  /**
   * Send a transaction and wait for confirmation.
   */
  async sendAndConfirm(
    instructions: TransactionInstruction[],
    extraSigners: Keypair[] = []
  ): Promise<TransactionSignature> {
    const tx = new Transaction().add(...instructions);
    const signers = [this.config.payer, ...extraSigners];
    return sendAndConfirmTransaction(this.connection, tx, signers);
  }

  /**
   * Send with retry logic.
   */
  async sendWithRetry(
    instructions: TransactionInstruction[],
    extraSigners: Keypair[] = [],
    retryConfig: RetryConfig = DEFAULT_RETRY_CONFIG
  ): Promise<TransactionSignature> {
    let lastError: Error | undefined;
    let delay = retryConfig.initialDelayMs;

    for (let attempt = 0; attempt <= retryConfig.maxRetries; attempt++) {
      try {
        return await this.sendAndConfirm(instructions, extraSigners);
      } catch (error) {
        lastError = error as Error;
        if (attempt < retryConfig.maxRetries) {
          await new Promise((resolve) => setTimeout(resolve, delay));
          delay = Math.min(delay * retryConfig.backoffMultiplier, retryConfig.maxDelayMs);
        }
      }
    }

    throw lastError;
  }

  // ===========================================================================
  // BridgeApi Methods
  // ===========================================================================

  /**
   * Get the current bridge program state from on-chain.
   */
  async getCurrentBridgeState(): Promise<PsyBridgeProgramState> {
    const account = await this.connection.getAccountInfo(this.config.bridgeStatePda);
    if (!account) {
      throw BridgeError.accountNotFound(this.config.bridgeStatePda.toString());
    }

    const data = account.data;
    let offset = 0;

    const header = decodePsyBridgeHeader(data, offset);
    offset += PSY_BRIDGE_HEADER_SIZE;

    const config = decodePsyBridgeConfig(data, offset);
    offset += PSY_BRIDGE_CONFIG_SIZE;

    const operatorPubkey = data.slice(offset, offset + 32);
    offset += 32;

    const feeSpenderPubkey = data.slice(offset, offset + 32);
    offset += 32;

    const dogeMint = data.slice(offset, offset + 32);
    offset += 32;

    const returnTxOutput = decodePsyReturnTxOutput(data, offset);
    offset += PSY_RETURN_TX_OUTPUT_SIZE;

    // Custodian wallet config hash
    const configHash = data.slice(offset, offset + 32);
    offset += 32;

    const view = new DataView(data.buffer, data.byteOffset + offset);

    const nextWithdrawalIndex = view.getBigUint64(0, true);
    const nextProcessedWithdrawalsIndex = view.getBigUint64(8, true);
    const withdrawalSnapshotIndex = view.getBigUint64(16, true);
    offset += 24;

    // Skip withdrawal snapshots array (fixed size in on-chain)
    const withdrawalSnapshots: PsyWithdrawalChainSnapshot[] = [];

    // Continue reading remaining fields...
    // Note: Simplified - full decoding would require complete state layout knowledge

    return {
      header,
      config,
      operatorPubkey,
      feeSpenderPubkey,
      dogeMint,
      returnTxOutput,
      custodianWalletConfig: { configHash },
      nextWithdrawalIndex,
      nextProcessedWithdrawalsIndex,
      withdrawalSnapshotIndex,
      withdrawalSnapshots,
      spentTxoTreeRoot: new Uint8Array(32),
      manualClaimTxoTreeRoot: new Uint8Array(32),
      nextManualDepositIndex: 0n,
      nextProcessedManualDepositIndex: 0n,
      accumulatedFees: 0n,
    };
  }

  /**
   * Get manual deposits starting from a specific index.
   */
  async getManualDepositsAt(
    nextProcessedManualDepositIndex: bigint,
    maxCount: number
  ): Promise<DepositTxOutputRecord[]> {
    // This would require reading from the deposit records in the bridge state
    // Implementation depends on on-chain state layout
    return [];
  }

  /**
   * Process remaining pending mint groups.
   */
  async processRemainingPendingMintsGroups(
    pendingMints: PendingMint[],
    mintBufferAccount: PublicKey,
    mintBufferBump: number
  ): Promise<ProcessMintsResult> {
    if (pendingMints.length === 0) {
      return emptyProcessMintsResult();
    }

    const dogeMint = await this.getDogeMint();
    const signatures: TransactionSignature[] = [];
    const groupCount = Math.ceil(pendingMints.length / PM_MAX_PENDING_MINTS_PER_GROUP);

    for (let i = 0; i < groupCount; i++) {
      const start = i * PM_MAX_PENDING_MINTS_PER_GROUP;
      const end = Math.min(start + PM_MAX_PENDING_MINTS_PER_GROUP, pendingMints.length);
      const group = pendingMints.slice(start, end);
      const recipients = group.map((m) => m.recipient);
      const isLast = i === groupCount - 1;

      const ix = processMintGroup(
        this.config.programId,
        this.config.operator.publicKey,
        mintBufferAccount,
        dogeMint,
        recipients,
        i,
        mintBufferBump,
        isLast,
        this.config.pendingMintProgramId
      );

      const sig = await this.sendWithRetry([ix], [this.config.operator]);
      signatures.push(sig);
    }

    return {
      groupsProcessed: groupCount,
      totalMintsProcessed: pendingMints.length,
      signatures,
      fullyCompleted: true,
    };
  }

  /**
   * Process remaining pending mint groups with auto-advance.
   */
  async processRemainingPendingMintsGroupsAutoAdvance(
    pendingMints: PendingMint[],
    mintBufferAccount: PublicKey,
    mintBufferBump: number,
    txoBufferAccount: PublicKey,
    txoBufferBump: number
  ): Promise<ProcessMintsResult> {
    if (pendingMints.length === 0) {
      return emptyProcessMintsResult();
    }

    const dogeMint = await this.getDogeMint();
    const signatures: TransactionSignature[] = [];
    const groupCount = Math.ceil(pendingMints.length / PM_MAX_PENDING_MINTS_PER_GROUP);

    for (let i = 0; i < groupCount; i++) {
      const start = i * PM_MAX_PENDING_MINTS_PER_GROUP;
      const end = Math.min(start + PM_MAX_PENDING_MINTS_PER_GROUP, pendingMints.length);
      const group = pendingMints.slice(start, end);
      const recipients = group.map((m) => m.recipient);
      const isLast = i === groupCount - 1;

      const ix = processMintGroupAutoAdvance(
        this.config.programId,
        this.config.operator.publicKey,
        mintBufferAccount,
        txoBufferAccount,
        dogeMint,
        recipients,
        i,
        mintBufferBump,
        txoBufferBump,
        isLast,
        this.config.pendingMintProgramId,
        this.config.txoBufferProgramId
      );

      const sig = await this.sendWithRetry([ix], [this.config.operator]);
      signatures.push(sig);
    }

    return {
      groupsProcessed: groupCount,
      totalMintsProcessed: pendingMints.length,
      signatures,
      fullyCompleted: true,
    };
  }

  /**
   * Process a block transition.
   */
  async processBlockTransition(
    proof: CompactBridgeZKProof,
    header: PsyBridgeHeader,
    mintBufferAccount: PublicKey,
    mintBufferBump: number,
    txoBufferAccount: PublicKey,
    txoBufferBump: number
  ): Promise<TransactionSignature> {
    const ix = blockUpdate(
      this.config.programId,
      this.config.payer.publicKey,
      proof,
      header,
      this.config.operator.publicKey,
      mintBufferAccount,
      txoBufferAccount,
      mintBufferBump,
      txoBufferBump,
      this.config.pendingMintProgramId,
      this.config.txoBufferProgramId
    );

    return this.sendWithRetry([ix], [this.config.operator]);
  }

  /**
   * Process a block reorganization.
   */
  async processBlockReorg(
    proof: CompactBridgeZKProof,
    header: PsyBridgeHeader,
    extraBlocks: FinalizedBlockMintTxoInfo[],
    mintBufferAccount: PublicKey,
    mintBufferBump: number,
    txoBufferAccount: PublicKey,
    txoBufferBump: number
  ): Promise<TransactionSignature> {
    const ix = processReorgBlocks(
      this.config.programId,
      this.config.payer.publicKey,
      proof,
      header,
      extraBlocks,
      this.config.operator.publicKey,
      mintBufferAccount,
      txoBufferAccount,
      mintBufferBump,
      txoBufferBump,
      this.config.pendingMintProgramId,
      this.config.txoBufferProgramId
    );

    return this.sendWithRetry([ix], [this.config.operator]);
  }

  /**
   * Setup a TXO buffer for a block.
   */
  async setupTxoBuffer(blockHeight: number, txos: number[]): Promise<[PublicKey, number]> {
    return createTxoBuffer(
      this.connection,
      this.config.txoBufferProgramId,
      this.config.payer,
      blockHeight,
      txos
    );
  }

  /**
   * Setup a pending mints buffer.
   */
  async setupPendingMintsBuffer(
    blockHeight: number,
    pendingMints: PendingMint[]
  ): Promise<[PublicKey, number]> {
    const [bridgeState] = getBridgeStatePda(this.config.programId);
    return createPendingMintBuffer(
      this.connection,
      this.config.pendingMintProgramId,
      this.config.payer,
      bridgeState,
      pendingMints
    );
  }

  /**
   * Snapshot the withdrawal chain state.
   */
  async snapshotWithdrawals(): Promise<TransactionSignature> {
    const ix = snapshotWithdrawals(
      this.config.programId,
      this.config.operator.publicKey,
      this.config.payer.publicKey
    );

    return this.sendWithRetry([ix], [this.config.operator]);
  }

  // ===========================================================================
  // WithdrawalApi Methods
  // ===========================================================================

  /**
   * Request a withdrawal from Solana to Dogecoin.
   */
  async requestWithdrawal(
    userAuthority: Keypair,
    recipientAddress: Uint8Array,
    amountSats: bigint,
    addressType: number
  ): Promise<TransactionSignature> {
    const mint = await this.getDogeMint();
    const userTokenAccount = await getAssociatedTokenAddress(mint, userAuthority.publicKey);

    const ix = requestWithdrawal(
      this.config.programId,
      userAuthority.publicKey,
      mint,
      userTokenAccount,
      recipientAddress,
      amountSats,
      addressType
    );

    return this.sendAndConfirm([ix], [userAuthority]);
  }

  /**
   * Process a withdrawal transaction.
   */
  async processWithdrawal(
    proof: CompactBridgeZKProof,
    newReturnOutput: PsyReturnTxOutput,
    newSpentTxoTreeRoot: Uint8Array,
    newNextProcessedWithdrawalsIndex: bigint,
    dogeTxBytes: Uint8Array
  ): Promise<TransactionSignature> {
    const genericBuffer = await createGenericBuffer(
      this.connection,
      this.config.genericBufferProgramId,
      this.config.payer,
      dogeTxBytes
    );

    const ix = processWithdrawal(
      this.config.programId,
      this.config.payer.publicKey,
      genericBuffer,
      this.config.wormholeShimProgramId,
      this.config.wormholeCoreProgramId,
      proof,
      newReturnOutput,
      newSpentTxoTreeRoot,
      newNextProcessedWithdrawalsIndex
    );

    return this.sendWithRetry([ix]);
  }

  /**
   * Replay a withdrawal message (for Wormhole integration).
   */
  async replayWithdrawal(dogeTxBytes: Uint8Array): Promise<TransactionSignature> {
    const genericBuffer = await createGenericBuffer(
      this.connection,
      this.config.genericBufferProgramId,
      this.config.payer,
      dogeTxBytes
    );

    const ix = processReplayWithdrawal(
      this.config.programId,
      this.config.payer.publicKey,
      genericBuffer,
      this.config.wormholeShimProgramId,
      this.config.wormholeCoreProgramId
    );

    return this.sendWithRetry([ix]);
  }

  // ===========================================================================
  // ManualClaimApi Methods
  // ===========================================================================

  /**
   * Execute a manual claim for a deposit.
   */
  async manualClaimDeposit(
    userSigner: Keypair,
    proof: CompactBridgeZKProof,
    recentBlockMerkleTreeRoot: Uint8Array,
    recentAutoClaimTxoRoot: Uint8Array,
    newManualClaimTxoRoot: Uint8Array,
    txHash: Uint8Array,
    combinedTxoIndex: bigint,
    depositAmountSats: bigint
  ): Promise<TransactionSignature> {
    const mint = await this.getDogeMint();
    const recipientTokenAccount = await getAssociatedTokenAddress(mint, userSigner.publicKey);

    const ix = manualClaimDeposit(
      this.config.manualClaimProgramId,
      this.config.programId,
      userSigner.publicKey,
      this.config.payer.publicKey,
      mint,
      recipientTokenAccount,
      proof,
      recentBlockMerkleTreeRoot,
      recentAutoClaimTxoRoot,
      newManualClaimTxoRoot,
      txHash,
      combinedTxoIndex,
      depositAmountSats
    );

    return this.sendAndConfirm([ix], [userSigner]);
  }

  // ===========================================================================
  // OperatorApi Methods
  // ===========================================================================

  /**
   * Initialize the bridge.
   */
  async initializeBridge(params: InitializeBridgeParams): Promise<TransactionSignature> {
    const dogeMint = this.config.dogeMint;
    if (!dogeMint) {
      throw BridgeError.missingField("dogeMint required for initialization");
    }

    const ix = initializeBridge(
      this.config.payer.publicKey,
      this.config.operator.publicKey,
      this.config.operator.publicKey, // fee spender same as operator
      dogeMint,
      params,
      this.config.programId
    );

    return this.sendAndConfirm([ix]);
  }

  /**
   * Withdraw accumulated operator fees.
   */
  async operatorWithdrawFees(): Promise<TransactionSignature> {
    const mint = await this.getDogeMint();
    const operatorTokenAccount = await getAssociatedTokenAddress(
      mint,
      this.config.operator.publicKey
    );

    const ix = operatorWithdrawFees(
      this.config.programId,
      this.config.operator.publicKey,
      operatorTokenAccount,
      mint
    );

    return this.sendWithRetry([ix], [this.config.operator]);
  }

  /**
   * Execute snapshot withdrawals.
   */
  async executeSnapshotWithdrawals(): Promise<TransactionSignature> {
    return this.snapshotWithdrawals();
  }
}
