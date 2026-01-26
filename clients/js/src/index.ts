/**
 * Doge Bridge Client
 *
 * A TypeScript client for interacting with the Doge bridge on Solana.
 *
 * @example
 * ```typescript
 * import { BridgeClient, BridgeClientConfigBuilder } from "@psy/doge-bridge-client";
 *
 * const client = BridgeClient.create(
 *   "https://api.mainnet-beta.solana.com",
 *   operatorKeypair,
 *   payerKeypair,
 *   bridgeStatePda,
 *   wormholeCoreProgramId,
 *   wormholeShimProgramId
 * );
 *
 * const state = await client.getCurrentBridgeState();
 * console.log("Block height:", state.header.finalizedState.blockHeight);
 * ```
 */

// Client
export { BridgeClient } from "./client";

// Configuration
export {
  BridgeClientConfig,
  BridgeClientConfigBuilder,
  RateLimitConfig,
  RetryConfig,
  ParallelismConfig,
  DEFAULT_RATE_LIMIT_CONFIG,
  DEFAULT_RETRY_CONFIG,
  DEFAULT_PARALLELISM_CONFIG,
} from "./config";

// Types
export {
  // Core types
  PsyBridgeStateCommitment,
  PsyBridgeHeader,
  PsyReturnTxOutput,
  PsyBridgeConfig,
  BridgeCustodianWalletConfig,
  PsyWithdrawalRequest,
  PsyWithdrawalChainSnapshot,
  PsyBridgeProgramState,
  // Instruction parameters
  InitializeBridgeParams,
  FinalizedBlockMintTxoInfo,
  PendingMint,
  CompactBridgeZKProof,
  ManualClaimInstructionData,
  // Result types
  DepositTxOutputRecord,
  ProcessMintsResult,
  BridgeStateInfo,
  WithdrawalRequestInfo,
  TransactionResult,
  // Size constants
  PSY_BRIDGE_STATE_COMMITMENT_SIZE,
  PSY_BRIDGE_HEADER_SIZE,
  PSY_RETURN_TX_OUTPUT_SIZE,
  PSY_BRIDGE_CONFIG_SIZE,
  CUSTODIAN_WALLET_CONFIG_SIZE,
  FINALIZED_BLOCK_MINT_TXO_INFO_SIZE,
  PENDING_MINT_SIZE,
  COMPACT_ZK_PROOF_SIZE,
  MANUAL_CLAIM_INSTRUCTION_DATA_SIZE,
  // Encoders
  encodePsyBridgeStateCommitment,
  encodePsyBridgeHeader,
  encodePsyReturnTxOutput,
  encodePsyBridgeConfig,
  encodeCustodianWalletConfig,
  encodeFinalizedBlockMintTxoInfo,
  encodePendingMint,
  encodeManualClaimInstructionData,
  // Decoders
  decodePsyBridgeStateCommitment,
  decodePsyBridgeHeader,
  decodePsyReturnTxOutput,
  decodePsyBridgeConfig,
  // Helpers
  emptyProcessMintsResult,
} from "./types";

// Constants
export {
  BRIDGE_STATE_SEED,
  MANUAL_CLAIM_SEED,
  MINT_BUFFER_SEED,
  TXO_BUFFER_SEED,
  DOGE_BRIDGE_PROGRAM_ID,
  MANUAL_CLAIM_PROGRAM_ID,
  PENDING_MINT_BUFFER_BUILDER_PROGRAM_ID,
  GENERIC_BUFFER_BUILDER_PROGRAM_ID,
  TXO_BUFFER_BUILDER_PROGRAM_ID,
  CHUNK_SIZE,
  PM_MAX_PENDING_MINTS_PER_GROUP,
} from "./constants";

// Instructions
export {
  getBridgeStatePda,
  getManualClaimPda,
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
  // Buffer instructions
  genericBufferInit,
  genericBufferWrite,
  pendingMintSetup,
  pendingMintReinit,
  pendingMintInsert,
  txoBufferInit,
  txoBufferSetLen,
  txoBufferWrite,
} from "./instructions";

// Buffers
export {
  getMintBufferPda,
  getTxoBufferPda,
  createGenericBuffer,
  createTxoBuffer,
  createPendingMintBuffer,
} from "./buffers";

// Errors
export { BridgeError, ErrorCategory } from "./errors";
