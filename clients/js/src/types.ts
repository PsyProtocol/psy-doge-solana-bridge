/**
 * Type definitions for the Doge Bridge client.
 */

import { PublicKey, TransactionSignature } from "@solana/web3.js";

// =============================================================================
// Core Bridge Types
// =============================================================================

export interface PsyBridgeStateCommitment {
  blockHash: Uint8Array;
  blockMerkleTreeRoot: Uint8Array;
  pendingMintsFinalizedHash: Uint8Array;
  txoOutputListFinalizedHash: Uint8Array;
  autoClaimedTxoTreeRoot: Uint8Array;
  autoClaimedDepositsTreeRoot: Uint8Array;
  autoClaimedDepositsNextIndex: number;
  blockHeight: number;
}

export interface PsyBridgeHeader {
  tipState: PsyBridgeStateCommitment;
  finalizedState: PsyBridgeStateCommitment;
  bridgeStateHash: Uint8Array;
  lastRollbackAtSecs: number;
  pausedUntilSecs: number;
  totalFinalizedFeesCollectedChainHistory: bigint;
}

export interface PsyReturnTxOutput {
  sighash: Uint8Array;
  outputIndex: bigint;
  amountSats: bigint;
}

export interface PsyBridgeConfig {
  depositFeeRateNumerator: bigint;
  depositFeeRateDenominator: bigint;
  withdrawalFeeRateNumerator: bigint;
  withdrawalFeeRateDenominator: bigint;
  depositFlatFeeSats: bigint;
  withdrawalFlatFeeSats: bigint;
}

export interface BridgeCustodianWalletConfig {
  configHash: Uint8Array; // 32 bytes
}

export interface PsyWithdrawalRequest {
  recipientAddress: Uint8Array;
  amountSats: bigint;
  addressType: number;
}

export interface PsyWithdrawalChainSnapshot {
  nextWithdrawalIndex: bigint;
  withdrawalsMerkleRoot: Uint8Array;
}

export interface PsyBridgeProgramState {
  header: PsyBridgeHeader;
  config: PsyBridgeConfig;
  operatorPubkey: Uint8Array;
  feeSpenderPubkey: Uint8Array;
  dogeMint: Uint8Array;
  returnTxOutput: PsyReturnTxOutput;
  custodianWalletConfig: BridgeCustodianWalletConfig;
  nextWithdrawalIndex: bigint;
  nextProcessedWithdrawalsIndex: bigint;
  withdrawalSnapshotIndex: bigint;
  withdrawalSnapshots: PsyWithdrawalChainSnapshot[];
  spentTxoTreeRoot: Uint8Array;
  manualClaimTxoTreeRoot: Uint8Array;
  nextManualDepositIndex: bigint;
  nextProcessedManualDepositIndex: bigint;
  accumulatedFees: bigint;
}

export interface InitializeBridgeParams {
  bridgeHeader: PsyBridgeHeader;
  startReturnTxoOutput: PsyReturnTxOutput;
  configParams: PsyBridgeConfig;
  custodianWalletConfig: BridgeCustodianWalletConfig;
}

export interface FinalizedBlockMintTxoInfo {
  pendingMintsFinalizedHash: Uint8Array;
  txoOutputListFinalizedHash: Uint8Array;
}

export interface PendingMint {
  recipient: PublicKey;
  amount: bigint;
}

export type CompactBridgeZKProof = Uint8Array;

export interface DepositTxOutputRecord {
  txHash: Uint8Array;
  combinedTxoIndex: bigint;
  recipientPubkey: Uint8Array;
  amountSats: bigint;
  blockHeight: number;
}

export interface ProcessMintsResult {
  groupsProcessed: number;
  totalMintsProcessed: number;
  signatures: TransactionSignature[];
  fullyCompleted: boolean;
}

export interface BridgeStateInfo {
  state: PsyBridgeProgramState;
  dogeMint: PublicKey;
  bridgeStatePda: PublicKey;
}

export interface WithdrawalRequestInfo {
  request: PsyWithdrawalRequest;
  index: bigint;
  userPubkey: Uint8Array;
}

export interface TransactionResult {
  signature: TransactionSignature;
  slot: number;
}

// =============================================================================
// Size Constants
// =============================================================================

export const PSY_BRIDGE_STATE_COMMITMENT_SIZE = 200;
export const PSY_BRIDGE_HEADER_SIZE = 448;
export const PSY_RETURN_TX_OUTPUT_SIZE = 48;
export const PSY_BRIDGE_CONFIG_SIZE = 48;
export const CUSTODIAN_WALLET_CONFIG_SIZE = 32;
export const FINALIZED_BLOCK_MINT_TXO_INFO_SIZE = 64;
export const PENDING_MINT_SIZE = 40;
export const COMPACT_ZK_PROOF_SIZE = 256;
export const MANUAL_CLAIM_INSTRUCTION_DATA_SIZE = 256 + 32 * 4 + 16;

// =============================================================================
// Encoders
// =============================================================================

export function encodePsyBridgeStateCommitment(
  state: PsyBridgeStateCommitment,
  buffer: Uint8Array,
  offset: number = 0
): number {
  let pos = offset;
  buffer.set(state.blockHash, pos); pos += 32;
  buffer.set(state.blockMerkleTreeRoot, pos); pos += 32;
  buffer.set(state.pendingMintsFinalizedHash, pos); pos += 32;
  buffer.set(state.txoOutputListFinalizedHash, pos); pos += 32;
  buffer.set(state.autoClaimedTxoTreeRoot, pos); pos += 32;
  buffer.set(state.autoClaimedDepositsTreeRoot, pos); pos += 32;
  const view = new DataView(buffer.buffer, buffer.byteOffset + pos, 8);
  view.setUint32(0, state.autoClaimedDepositsNextIndex, true);
  view.setUint32(4, state.blockHeight, true);
  pos += 8;
  return pos - offset;
}

export function encodePsyBridgeHeader(
  header: PsyBridgeHeader,
  buffer: Uint8Array,
  offset: number = 0
): number {
  let pos = offset;
  pos += encodePsyBridgeStateCommitment(header.tipState, buffer, pos);
  pos += encodePsyBridgeStateCommitment(header.finalizedState, buffer, pos);
  buffer.set(header.bridgeStateHash, pos); pos += 32;
  const view = new DataView(buffer.buffer, buffer.byteOffset + pos, 16);
  view.setUint32(0, header.lastRollbackAtSecs, true);
  view.setUint32(4, header.pausedUntilSecs, true);
  view.setBigUint64(8, header.totalFinalizedFeesCollectedChainHistory, true);
  pos += 16;
  return pos - offset;
}

export function encodePsyReturnTxOutput(
  output: PsyReturnTxOutput,
  buffer: Uint8Array,
  offset: number = 0
): number {
  let pos = offset;
  buffer.set(output.sighash, pos); pos += 32;
  const view = new DataView(buffer.buffer, buffer.byteOffset + pos, 16);
  view.setBigUint64(0, output.outputIndex, true);
  view.setBigUint64(8, output.amountSats, true);
  pos += 16;
  return pos - offset;
}

export function encodePsyBridgeConfig(
  config: PsyBridgeConfig,
  buffer: Uint8Array,
  offset: number = 0
): number {
  const view = new DataView(buffer.buffer, buffer.byteOffset + offset, 48);
  view.setBigUint64(0, config.depositFeeRateNumerator, true);
  view.setBigUint64(8, config.depositFeeRateDenominator, true);
  view.setBigUint64(16, config.withdrawalFeeRateNumerator, true);
  view.setBigUint64(24, config.withdrawalFeeRateDenominator, true);
  view.setBigUint64(32, config.depositFlatFeeSats, true);
  view.setBigUint64(40, config.withdrawalFlatFeeSats, true);
  return 48;
}

export function encodeCustodianWalletConfig(
  config: BridgeCustodianWalletConfig,
  buffer: Uint8Array,
  offset: number = 0
): number {
  buffer.set(config.configHash, offset);
  return 32;
}

export function encodeFinalizedBlockMintTxoInfo(
  info: FinalizedBlockMintTxoInfo,
  buffer: Uint8Array,
  offset: number = 0
): number {
  buffer.set(info.pendingMintsFinalizedHash, offset);
  buffer.set(info.txoOutputListFinalizedHash, offset + 32);
  return 64;
}

export function encodePendingMint(
  mint: PendingMint,
  buffer: Uint8Array,
  offset: number = 0
): number {
  buffer.set(mint.recipient.toBuffer(), offset);
  const view = new DataView(buffer.buffer, buffer.byteOffset + offset + 32, 8);
  view.setBigUint64(0, mint.amount, true);
  return 40;
}

export interface ManualClaimInstructionData {
  proof: Uint8Array;
  recentBlockMerkleTreeRoot: Uint8Array;
  recentAutoClaimTxoRoot: Uint8Array;
  newManualClaimTxoRoot: Uint8Array;
  txHash: Uint8Array;
  combinedTxoIndex: bigint;
  depositAmountSats: bigint;
}

export function encodeManualClaimInstructionData(
  data: ManualClaimInstructionData,
  buffer: Uint8Array,
  offset: number = 0
): number {
  let pos = offset;
  buffer.set(data.proof, pos); pos += 256;
  buffer.set(data.recentBlockMerkleTreeRoot, pos); pos += 32;
  buffer.set(data.recentAutoClaimTxoRoot, pos); pos += 32;
  buffer.set(data.newManualClaimTxoRoot, pos); pos += 32;
  buffer.set(data.txHash, pos); pos += 32;
  const view = new DataView(buffer.buffer, buffer.byteOffset + pos, 16);
  view.setBigUint64(0, data.combinedTxoIndex, true);
  view.setBigUint64(8, data.depositAmountSats, true);
  pos += 16;
  return pos - offset;
}

// =============================================================================
// Decoders
// =============================================================================

export function decodePsyBridgeStateCommitment(
  buffer: Uint8Array,
  offset: number = 0
): PsyBridgeStateCommitment {
  let pos = offset;
  const blockHash = buffer.slice(pos, pos + 32); pos += 32;
  const blockMerkleTreeRoot = buffer.slice(pos, pos + 32); pos += 32;
  const pendingMintsFinalizedHash = buffer.slice(pos, pos + 32); pos += 32;
  const txoOutputListFinalizedHash = buffer.slice(pos, pos + 32); pos += 32;
  const autoClaimedTxoTreeRoot = buffer.slice(pos, pos + 32); pos += 32;
  const autoClaimedDepositsTreeRoot = buffer.slice(pos, pos + 32); pos += 32;
  const view = new DataView(buffer.buffer, buffer.byteOffset + pos, 8);
  return {
    blockHash,
    blockMerkleTreeRoot,
    pendingMintsFinalizedHash,
    txoOutputListFinalizedHash,
    autoClaimedTxoTreeRoot,
    autoClaimedDepositsTreeRoot,
    autoClaimedDepositsNextIndex: view.getUint32(0, true),
    blockHeight: view.getUint32(4, true),
  };
}

export function decodePsyBridgeHeader(
  buffer: Uint8Array,
  offset: number = 0
): PsyBridgeHeader {
  let pos = offset;
  const tipState = decodePsyBridgeStateCommitment(buffer, pos);
  pos += PSY_BRIDGE_STATE_COMMITMENT_SIZE;
  const finalizedState = decodePsyBridgeStateCommitment(buffer, pos);
  pos += PSY_BRIDGE_STATE_COMMITMENT_SIZE;
  const bridgeStateHash = buffer.slice(pos, pos + 32); pos += 32;
  const view = new DataView(buffer.buffer, buffer.byteOffset + pos, 16);
  return {
    tipState,
    finalizedState,
    bridgeStateHash,
    lastRollbackAtSecs: view.getUint32(0, true),
    pausedUntilSecs: view.getUint32(4, true),
    totalFinalizedFeesCollectedChainHistory: view.getBigUint64(8, true),
  };
}

export function decodePsyReturnTxOutput(
  buffer: Uint8Array,
  offset: number = 0
): PsyReturnTxOutput {
  const sighash = buffer.slice(offset, offset + 32);
  const view = new DataView(buffer.buffer, buffer.byteOffset + offset + 32, 16);
  return {
    sighash,
    outputIndex: view.getBigUint64(0, true),
    amountSats: view.getBigUint64(8, true),
  };
}

export function decodePsyBridgeConfig(
  buffer: Uint8Array,
  offset: number = 0
): PsyBridgeConfig {
  const view = new DataView(buffer.buffer, buffer.byteOffset + offset, 48);
  return {
    depositFeeRateNumerator: view.getBigUint64(0, true),
    depositFeeRateDenominator: view.getBigUint64(8, true),
    withdrawalFeeRateNumerator: view.getBigUint64(16, true),
    withdrawalFeeRateDenominator: view.getBigUint64(24, true),
    depositFlatFeeSats: view.getBigUint64(32, true),
    withdrawalFlatFeeSats: view.getBigUint64(40, true),
  };
}

export function emptyProcessMintsResult(): ProcessMintsResult {
  return {
    groupsProcessed: 0,
    totalMintsProcessed: 0,
    signatures: [],
    fullyCompleted: true,
  };
}
