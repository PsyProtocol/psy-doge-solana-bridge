import { PublicKey } from "@solana/web3.js";

export interface PsyBridgeStateCommitment {
  block_hash: Uint8Array; // 32
  block_merkle_tree_root: Uint8Array; // 32
  pending_mints_finalized_hash: Uint8Array; // 32
  txo_output_list_finalized_hash: Uint8Array; // 32
  auto_claimed_txo_tree_root: Uint8Array; // 32
  auto_claimed_deposits_tree_root: Uint8Array; // 32
  auto_claimed_deposits_next_index: number; // u32
  block_height: number; // u32
}

export interface PsyBridgeHeader {
  tip_state: PsyBridgeStateCommitment;
  finalized_state: PsyBridgeStateCommitment;
  bridge_state_hash: Uint8Array; // 32
  last_rollback_at_secs: number; // u32
  paused_until_secs: number; // u32
  total_finalized_fees_collected_chain_history: bigint; // u64
}

export interface PsyReturnTxOutput {
  sighash: Uint8Array; // 32
  output_index: bigint; // u64
  amount_sats: bigint; // u64
}

export interface PsyBridgeConfig {
  deposit_fee_rate_numerator: bigint; // u64
  deposit_fee_rate_denominator: bigint; // u64
  withdrawal_fee_rate_numerator: bigint; // u64
  withdrawal_fee_rate_denominator: bigint; // u64
  deposit_flat_fee_sats: bigint; // u64
  withdrawal_flat_fee_sats: bigint; // u64
}

export interface InitializeBridgeInstructionData {
  operator_pubkey: PublicKey;
  fee_spender_pubkey: PublicKey;
  doge_mint: PublicKey;
  bridge_header: PsyBridgeHeader;
  start_return_txo_output: PsyReturnTxOutput;
  config_params: PsyBridgeConfig;
}

export interface FinalizedBlockMintTxoInfo {
  pending_mints_finalized_hash: Uint8Array; // 32
  txo_output_list_finalized_hash: Uint8Array; // 32
}

export interface PendingMint {
  recipient: PublicKey;
  amount: bigint;
}

export interface ManualClaimInstructionData {
  proof: Uint8Array; // 256
  recent_block_merkle_tree_root: Uint8Array; // 32
  recent_auto_claim_txo_root: Uint8Array; // 32
  new_manual_claim_txo_root: Uint8Array; // 32
  tx_hash: Uint8Array; // 32
  combined_txo_index: bigint; // u64
  deposit_amount_sats: bigint; // u64
}

// --- Sizes ---
export const PSY_BRIDGE_STATE_COMMITMENT_SIZE = 200;
export const PSY_BRIDGE_HEADER_SIZE = 448;
export const PSY_RETURN_TX_OUTPUT_SIZE = 48;
export const PSY_BRIDGE_CONFIG_SIZE = 48;
export const INITIALIZE_BRIDGE_INSTRUCTION_DATA_SIZE = 32 * 3 + PSY_BRIDGE_HEADER_SIZE + PSY_RETURN_TX_OUTPUT_SIZE + PSY_BRIDGE_CONFIG_SIZE;
export const FINALIZED_BLOCK_MINT_TXO_INFO_SIZE = 64;
export const PENDING_MINT_SIZE = 40;
export const MANUAL_CLAIM_INSTRUCTION_DATA_SIZE = 256 + 32 * 4 + 16;

// --- Encoders ---

export function encodePsyBridgeStateCommitment(state: PsyBridgeStateCommitment, buffer: Uint8Array, offset: number = 0): number {
  let currentOffset = offset;
  buffer.set(state.block_hash, currentOffset); currentOffset += 32;
  buffer.set(state.block_merkle_tree_root, currentOffset); currentOffset += 32;
  buffer.set(state.pending_mints_finalized_hash, currentOffset); currentOffset += 32;
  buffer.set(state.txo_output_list_finalized_hash, currentOffset); currentOffset += 32;
  buffer.set(state.auto_claimed_txo_tree_root, currentOffset); currentOffset += 32;
  buffer.set(state.auto_claimed_deposits_tree_root, currentOffset); currentOffset += 32;
  
  const view = new DataView(buffer.buffer, buffer.byteOffset + currentOffset, 8);
  view.setUint32(0, state.auto_claimed_deposits_next_index, true);
  view.setUint32(4, state.block_height, true);
  currentOffset += 8;

  return currentOffset - offset;
}

export function encodePsyBridgeHeader(header: PsyBridgeHeader, buffer: Uint8Array, offset: number = 0): number {
  let currentOffset = offset;
  currentOffset += encodePsyBridgeStateCommitment(header.tip_state, buffer, currentOffset);
  currentOffset += encodePsyBridgeStateCommitment(header.finalized_state, buffer, currentOffset);
  
  buffer.set(header.bridge_state_hash, currentOffset); currentOffset += 32;
  
  const view = new DataView(buffer.buffer, buffer.byteOffset + currentOffset, 16);
  view.setUint32(0, header.last_rollback_at_secs, true);
  view.setUint32(4, header.paused_until_secs, true);
  view.setBigUint64(8, header.total_finalized_fees_collected_chain_history, true);
  currentOffset += 16;

  return currentOffset - offset;
}

export function encodePsyReturnTxOutput(output: PsyReturnTxOutput, buffer: Uint8Array, offset: number = 0): number {
  let currentOffset = offset;
  buffer.set(output.sighash, currentOffset); currentOffset += 32;
  
  const view = new DataView(buffer.buffer, buffer.byteOffset + currentOffset, 16);
  view.setBigUint64(0, output.output_index, true);
  view.setBigUint64(8, output.amount_sats, true);
  currentOffset += 16;

  return currentOffset - offset;
}

export function encodePsyBridgeConfig(config: PsyBridgeConfig, buffer: Uint8Array, offset: number = 0): number {
  const view = new DataView(buffer.buffer, buffer.byteOffset + offset, 48);
  view.setBigUint64(0, config.deposit_fee_rate_numerator, true);
  view.setBigUint64(8, config.deposit_fee_rate_denominator, true);
  view.setBigUint64(16, config.withdrawal_fee_rate_numerator, true);
  view.setBigUint64(24, config.withdrawal_fee_rate_denominator, true);
  view.setBigUint64(32, config.deposit_flat_fee_sats, true);
  view.setBigUint64(40, config.withdrawal_flat_fee_sats, true);
  return 48;
}

export function encodeInitializeBridgeInstructionData(data: InitializeBridgeInstructionData, buffer: Uint8Array, offset: number = 0): number {
  let currentOffset = offset;
  buffer.set(data.operator_pubkey.toBuffer(), currentOffset); currentOffset += 32;
  buffer.set(data.fee_spender_pubkey.toBuffer(), currentOffset); currentOffset += 32;
  buffer.set(data.doge_mint.toBuffer(), currentOffset); currentOffset += 32;
  
  currentOffset += encodePsyBridgeHeader(data.bridge_header, buffer, currentOffset);
  currentOffset += encodePsyReturnTxOutput(data.start_return_txo_output, buffer, currentOffset);
  currentOffset += encodePsyBridgeConfig(data.config_params, buffer, currentOffset);
  
  return currentOffset - offset;
}

export function encodeFinalizedBlockMintTxoInfo(info: FinalizedBlockMintTxoInfo, buffer: Uint8Array, offset: number = 0): number {
  let currentOffset = offset;
  buffer.set(info.pending_mints_finalized_hash, currentOffset); currentOffset += 32;
  buffer.set(info.txo_output_list_finalized_hash, currentOffset); currentOffset += 32;
  return 64;
}

export function encodePendingMint(mint: PendingMint, buffer: Uint8Array, offset: number = 0): number {
  let currentOffset = offset;
  buffer.set(mint.recipient.toBuffer(), currentOffset); currentOffset += 32;
  const view = new DataView(buffer.buffer, buffer.byteOffset + currentOffset, 8);
  view.setBigUint64(0, mint.amount, true);
  return 40;
}

export function encodeManualClaimInstructionData(data: ManualClaimInstructionData, buffer: Uint8Array, offset: number = 0): number {
  let currentOffset = offset;
  buffer.set(data.proof, currentOffset); currentOffset += 256;
  buffer.set(data.recent_block_merkle_tree_root, currentOffset); currentOffset += 32;
  buffer.set(data.recent_auto_claim_txo_root, currentOffset); currentOffset += 32;
  buffer.set(data.new_manual_claim_txo_root, currentOffset); currentOffset += 32;
  buffer.set(data.tx_hash, currentOffset); currentOffset += 32;
  
  const view = new DataView(buffer.buffer, buffer.byteOffset + currentOffset, 16);
  view.setBigUint64(0, data.combined_txo_index, true);
  view.setBigUint64(8, data.deposit_amount_sats, true);
  currentOffset += 16;

  return currentOffset - offset;
}