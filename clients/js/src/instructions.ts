/**
 * Instruction builders for the Doge Bridge program.
 */

import { PublicKey, TransactionInstruction, SystemProgram, AccountMeta, SYSVAR_CLOCK_PUBKEY } from "@solana/web3.js";
import { TOKEN_PROGRAM_ID } from "@solana/spl-token";
import {
  DOGE_BRIDGE_PROGRAM_ID,
  PENDING_MINT_BUFFER_BUILDER_PROGRAM_ID,
  TXO_BUFFER_BUILDER_PROGRAM_ID,
  DOGE_BRIDGE_INSTRUCTION_INITIALIZE,
  DOGE_BRIDGE_INSTRUCTION_BLOCK_UPDATE,
  DOGE_BRIDGE_INSTRUCTION_REQUEST_WITHDRAWAL,
  DOGE_BRIDGE_INSTRUCTION_PROCESS_WITHDRAWAL,
  DOGE_BRIDGE_INSTRUCTION_OPERATOR_WITHDRAW_FEES,
  DOGE_BRIDGE_INSTRUCTION_PROCESS_MANUAL_DEPOSIT,
  DOGE_BRIDGE_INSTRUCTION_REPLAY_WITHDRAWAL,
  DOGE_BRIDGE_INSTRUCTION_PROCESS_MINT_GROUP,
  DOGE_BRIDGE_INSTRUCTION_PROCESS_REORG_BLOCKS,
  DOGE_BRIDGE_INSTRUCTION_PROCESS_MINT_GROUP_AUTO_ADVANCE,
  DOGE_BRIDGE_INSTRUCTION_SNAPSHOT_WITHDRAWALS,
  MC_MANUAL_CLAIM_TRANSACTION_DISCRIMINATOR,
  BRIDGE_STATE_SEED,
  MANUAL_CLAIM_SEED,
} from "./constants";
import {
  PsyBridgeHeader,
  PsyReturnTxOutput,
  PsyBridgeConfig,
  BridgeCustodianWalletConfig,
  FinalizedBlockMintTxoInfo,
  InitializeBridgeParams,
  CompactBridgeZKProof,
  PSY_BRIDGE_HEADER_SIZE,
  FINALIZED_BLOCK_MINT_TXO_INFO_SIZE,
  PSY_RETURN_TX_OUTPUT_SIZE,
  MANUAL_CLAIM_INSTRUCTION_DATA_SIZE,
  encodePsyBridgeHeader,
  encodePsyReturnTxOutput,
  encodePsyBridgeConfig,
  encodeCustodianWalletConfig,
  encodeFinalizedBlockMintTxoInfo,
  encodeManualClaimInstructionData,
} from "./types";

// =============================================================================
// Helpers
// =============================================================================

function concatBytes(...arrays: Uint8Array[]): Uint8Array {
  const totalLength = arrays.reduce((acc, curr) => acc + curr.length, 0);
  const result = new Uint8Array(totalLength);
  let offset = 0;
  for (const arr of arrays) {
    result.set(arr, offset);
    offset += arr.length;
  }
  return result;
}

function createInstructionHeader(instruction: number, bumps?: number[]): Uint8Array {
  const header = new Uint8Array(8);
  const bumpCount = bumps ? bumps.length : 0;
  if (bumpCount >= 8) {
    throw new Error("Bumps length cannot be greater than 7");
  }
  const padLen = 8 - bumpCount;
  header.fill(instruction, 0, padLen);
  if (bumps) {
    for (let i = 0; i < bumps.length; i++) {
      header[padLen + i] = bumps[i];
    }
  }
  return header;
}

export function getBridgeStatePda(programId: PublicKey = DOGE_BRIDGE_PROGRAM_ID): [PublicKey, number] {
  return PublicKey.findProgramAddressSync(
    [new TextEncoder().encode(BRIDGE_STATE_SEED)],
    programId
  );
}

export function getManualClaimPda(
  userPubkey: PublicKey,
  manualClaimProgramId: PublicKey
): [PublicKey, number] {
  return PublicKey.findProgramAddressSync(
    [new TextEncoder().encode(MANUAL_CLAIM_SEED), userPubkey.toBuffer()],
    manualClaimProgramId
  );
}

// =============================================================================
// Bridge Instructions
// =============================================================================

export function initializeBridge(
  payer: PublicKey,
  operatorPubkey: PublicKey,
  feeSpenderPubkey: PublicKey,
  dogeMint: PublicKey,
  params: InitializeBridgeParams,
  programId: PublicKey = DOGE_BRIDGE_PROGRAM_ID
): TransactionInstruction {
  const [bridgeState] = getBridgeStatePda(programId);

  const dataSize = 32 * 3 + PSY_BRIDGE_HEADER_SIZE + PSY_RETURN_TX_OUTPUT_SIZE + 48 + 32;
  const instructionData = new Uint8Array(dataSize);
  let offset = 0;

  instructionData.set(operatorPubkey.toBuffer(), offset); offset += 32;
  instructionData.set(feeSpenderPubkey.toBuffer(), offset); offset += 32;
  instructionData.set(dogeMint.toBuffer(), offset); offset += 32;
  offset += encodePsyBridgeHeader(params.bridgeHeader, instructionData, offset);
  offset += encodePsyReturnTxOutput(params.startReturnTxoOutput, instructionData, offset);
  offset += encodePsyBridgeConfig(params.configParams, instructionData, offset);
  encodeCustodianWalletConfig(params.custodianWalletConfig, instructionData, offset);

  const header = createInstructionHeader(DOGE_BRIDGE_INSTRUCTION_INITIALIZE);
  const data = concatBytes(header, instructionData);

  return new TransactionInstruction({
    keys: [
      { pubkey: bridgeState, isSigner: false, isWritable: true },
      { pubkey: payer, isSigner: true, isWritable: true },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
    ],
    programId,
    data: Buffer.from(data),
  });
}

export function blockUpdate(
  programId: PublicKey,
  payer: PublicKey,
  proof: CompactBridgeZKProof,
  header: PsyBridgeHeader,
  operator: PublicKey,
  mintBuffer: PublicKey,
  txoBuffer: PublicKey,
  mintBufferBump: number,
  txoBufferBump: number,
  pendingMintPid: PublicKey = PENDING_MINT_BUFFER_BUILDER_PROGRAM_ID,
  txoBufferPid: PublicKey = TXO_BUFFER_BUILDER_PROGRAM_ID
): TransactionInstruction {
  const [bridgeState] = getBridgeStatePda(programId);

  const dataSize = 256 + PSY_BRIDGE_HEADER_SIZE;
  const instructionData = new Uint8Array(dataSize);
  instructionData.set(proof, 0);
  encodePsyBridgeHeader(header, instructionData, 256);

  const headerBuffer = createInstructionHeader(DOGE_BRIDGE_INSTRUCTION_BLOCK_UPDATE, [mintBufferBump, txoBufferBump]);
  const data = concatBytes(headerBuffer, instructionData);

  return new TransactionInstruction({
    keys: [
      { pubkey: bridgeState, isSigner: false, isWritable: true },
      { pubkey: mintBuffer, isSigner: false, isWritable: true },
      { pubkey: txoBuffer, isSigner: false, isWritable: true },
      { pubkey: operator, isSigner: true, isWritable: false },
      { pubkey: payer, isSigner: true, isWritable: true },
      { pubkey: pendingMintPid, isSigner: false, isWritable: false },
      { pubkey: txoBufferPid, isSigner: false, isWritable: false },
    ],
    programId,
    data: Buffer.from(data),
  });
}

export function processReorgBlocks(
  programId: PublicKey,
  payer: PublicKey,
  proof: CompactBridgeZKProof,
  header: PsyBridgeHeader,
  extraBlocks: FinalizedBlockMintTxoInfo[],
  operator: PublicKey,
  mintBuffer: PublicKey,
  txoBuffer: PublicKey,
  mintBufferBump: number,
  txoBufferBump: number,
  pendingMintPid: PublicKey = PENDING_MINT_BUFFER_BUILDER_PROGRAM_ID,
  txoBufferPid: PublicKey = TXO_BUFFER_BUILDER_PROGRAM_ID
): TransactionInstruction {
  const [bridgeState] = getBridgeStatePda(programId);

  const fixedSize = 256 + PSY_BRIDGE_HEADER_SIZE;
  const extraBlocksSize = extraBlocks.length * FINALIZED_BLOCK_MINT_TXO_INFO_SIZE;
  const instructionData = new Uint8Array(fixedSize + extraBlocksSize);

  instructionData.set(proof, 0);
  encodePsyBridgeHeader(header, instructionData, 256);

  let offset = fixedSize;
  for (const block of extraBlocks) {
    encodeFinalizedBlockMintTxoInfo(block, instructionData, offset);
    offset += FINALIZED_BLOCK_MINT_TXO_INFO_SIZE;
  }

  const headerBuffer = createInstructionHeader(DOGE_BRIDGE_INSTRUCTION_PROCESS_REORG_BLOCKS, [mintBufferBump, txoBufferBump]);
  const data = concatBytes(headerBuffer, instructionData);

  return new TransactionInstruction({
    keys: [
      { pubkey: bridgeState, isSigner: false, isWritable: true },
      { pubkey: mintBuffer, isSigner: false, isWritable: true },
      { pubkey: txoBuffer, isSigner: false, isWritable: true },
      { pubkey: operator, isSigner: true, isWritable: false },
      { pubkey: payer, isSigner: true, isWritable: true },
      { pubkey: pendingMintPid, isSigner: false, isWritable: false },
      { pubkey: txoBufferPid, isSigner: false, isWritable: false },
    ],
    programId,
    data: Buffer.from(data),
  });
}

export function requestWithdrawal(
  programId: PublicKey,
  userAuthority: PublicKey,
  mint: PublicKey,
  userTokenAccount: PublicKey,
  recipientAddress: Uint8Array,
  amountSats: bigint,
  addressType: number
): TransactionInstruction {
  const [bridgeState] = getBridgeStatePda(programId);

  // RequestWithdrawalInstructionData layout:
  // PsyWithdrawalRequest (32 bytes):
  //   - amount_sats: u64 (8 bytes) at offset 0
  //   - address_type: u32 (4 bytes) at offset 8
  //   - recipient_address: [u8; 20] at offset 12
  const dataSize = 32;
  const instructionData = new Uint8Array(dataSize);
  const view = new DataView(instructionData.buffer);

  // PsyWithdrawalRequest
  view.setBigUint64(0, amountSats, true);
  view.setUint32(8, addressType, true);
  instructionData.set(recipientAddress, 12);

  const header = createInstructionHeader(DOGE_BRIDGE_INSTRUCTION_REQUEST_WITHDRAWAL);
  const data = concatBytes(header, instructionData);

  return new TransactionInstruction({
    keys: [
      { pubkey: bridgeState, isSigner: false, isWritable: true },
      { pubkey: userTokenAccount, isSigner: false, isWritable: true },
      { pubkey: mint, isSigner: false, isWritable: true },
      { pubkey: userAuthority, isSigner: true, isWritable: false },
      { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
    ],
    programId,
    data: Buffer.from(data),
  });
}

export function processWithdrawal(
  programId: PublicKey,
  payer: PublicKey,
  genericBufferAccount: PublicKey,
  wormholeShimProgramId: PublicKey,
  wormholeCoreProgramId: PublicKey,
  proof: CompactBridgeZKProof,
  newReturnOutput: PsyReturnTxOutput,
  newSpentTxoTreeRoot: Uint8Array,
  newNextProcessedWithdrawalsIndex: bigint
): TransactionInstruction {
  const [bridgeState] = getBridgeStatePda(programId);

  // Derive Wormhole accounts
  const [bridgeConfig] = PublicKey.findProgramAddressSync([Buffer.from("Bridge")], wormholeCoreProgramId);
  const [feeCollector] = PublicKey.findProgramAddressSync([Buffer.from("fee_collector")], wormholeCoreProgramId);
  const emitter = bridgeState;
  const [sequence] = PublicKey.findProgramAddressSync([Buffer.from("Sequence"), emitter.toBuffer()], wormholeCoreProgramId);
  const [message] = PublicKey.findProgramAddressSync([emitter.toBuffer()], wormholeShimProgramId);
  const [eventAuthority] = PublicKey.findProgramAddressSync([Buffer.from("__event_authority")], wormholeShimProgramId);

  const dataSize = 256 + PSY_RETURN_TX_OUTPUT_SIZE + 32 + 8;
  const instructionData = new Uint8Array(dataSize);

  let offset = 0;
  instructionData.set(proof, offset); offset += 256;
  offset += encodePsyReturnTxOutput(newReturnOutput, instructionData, offset);
  instructionData.set(newSpentTxoTreeRoot, offset); offset += 32;
  new DataView(instructionData.buffer, offset, 8).setBigUint64(0, newNextProcessedWithdrawalsIndex, true);

  const header = createInstructionHeader(DOGE_BRIDGE_INSTRUCTION_PROCESS_WITHDRAWAL);
  const data = concatBytes(header, instructionData);

  return new TransactionInstruction({
    keys: [
      { pubkey: bridgeState, isSigner: false, isWritable: true },
      { pubkey: genericBufferAccount, isSigner: false, isWritable: false },
      { pubkey: wormholeShimProgramId, isSigner: false, isWritable: false },
      { pubkey: bridgeConfig, isSigner: false, isWritable: true },
      { pubkey: message, isSigner: false, isWritable: true },
      { pubkey: sequence, isSigner: false, isWritable: true },
      { pubkey: payer, isSigner: true, isWritable: true },
      { pubkey: feeCollector, isSigner: false, isWritable: true },
      { pubkey: SYSVAR_CLOCK_PUBKEY, isSigner: false, isWritable: false },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
      { pubkey: wormholeCoreProgramId, isSigner: false, isWritable: false },
      { pubkey: eventAuthority, isSigner: false, isWritable: false },
    ],
    programId,
    data: Buffer.from(data),
  });
}

export function processReplayWithdrawal(
  programId: PublicKey,
  payer: PublicKey,
  genericBufferAccount: PublicKey,
  wormholeShimProgramId: PublicKey,
  wormholeCoreProgramId: PublicKey
): TransactionInstruction {
  const [bridgeState] = getBridgeStatePda(programId);

  // Derive Wormhole accounts
  const [bridgeConfig] = PublicKey.findProgramAddressSync([Buffer.from("Bridge")], wormholeCoreProgramId);
  const [feeCollector] = PublicKey.findProgramAddressSync([Buffer.from("fee_collector")], wormholeCoreProgramId);
  const emitter = bridgeState;
  const [sequence] = PublicKey.findProgramAddressSync([Buffer.from("Sequence"), emitter.toBuffer()], wormholeCoreProgramId);
  const [message] = PublicKey.findProgramAddressSync([emitter.toBuffer()], wormholeShimProgramId);
  const [eventAuthority] = PublicKey.findProgramAddressSync([Buffer.from("__event_authority")], wormholeShimProgramId);

  const header = createInstructionHeader(DOGE_BRIDGE_INSTRUCTION_REPLAY_WITHDRAWAL);

  return new TransactionInstruction({
    keys: [
      { pubkey: bridgeState, isSigner: false, isWritable: true },
      { pubkey: genericBufferAccount, isSigner: false, isWritable: false },
      { pubkey: wormholeShimProgramId, isSigner: false, isWritable: false },
      { pubkey: bridgeConfig, isSigner: false, isWritable: true },
      { pubkey: message, isSigner: false, isWritable: true },
      { pubkey: sequence, isSigner: false, isWritable: true },
      { pubkey: payer, isSigner: true, isWritable: true },
      { pubkey: feeCollector, isSigner: false, isWritable: true },
      { pubkey: SYSVAR_CLOCK_PUBKEY, isSigner: false, isWritable: false },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
      { pubkey: wormholeCoreProgramId, isSigner: false, isWritable: false },
      { pubkey: eventAuthority, isSigner: false, isWritable: false },
    ],
    programId,
    data: Buffer.from(header),
  });
}

export function processManualDeposit(
  programId: PublicKey,
  manualClaimProgramId: PublicKey,
  mint: PublicKey,
  recipient: PublicKey,
  txHash: Uint8Array,
  recentBlockMerkleTreeRoot: Uint8Array,
  recentAutoClaimTxoRoot: Uint8Array,
  combinedTxoIndex: bigint,
  depositorSolanaPublicKey: Uint8Array,
  depositAmountSats: bigint
): TransactionInstruction {
  const [bridgeState] = getBridgeStatePda(programId);

  const dataSize = 32 + 32 + 32 + 8 + 32 + 8;
  const instructionData = new Uint8Array(dataSize);
  const view = new DataView(instructionData.buffer);

  let offset = 0;
  instructionData.set(txHash, offset); offset += 32;
  instructionData.set(recentBlockMerkleTreeRoot, offset); offset += 32;
  instructionData.set(recentAutoClaimTxoRoot, offset); offset += 32;
  view.setBigUint64(offset, combinedTxoIndex, true); offset += 8;
  instructionData.set(depositorSolanaPublicKey, offset); offset += 32;
  view.setBigUint64(offset, depositAmountSats, true);

  const header = createInstructionHeader(DOGE_BRIDGE_INSTRUCTION_PROCESS_MANUAL_DEPOSIT);
  const data = concatBytes(header, instructionData);

  return new TransactionInstruction({
    keys: [
      { pubkey: bridgeState, isSigner: false, isWritable: true },
      { pubkey: recipient, isSigner: false, isWritable: true },
      { pubkey: mint, isSigner: false, isWritable: true },
      { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
      { pubkey: manualClaimProgramId, isSigner: true, isWritable: false },
    ],
    programId,
    data: Buffer.from(data),
  });
}

export function manualClaimDeposit(
  programId: PublicKey,
  bridgeProgramId: PublicKey,
  userSigner: PublicKey,
  payer: PublicKey,
  mint: PublicKey,
  recipient: PublicKey,
  proof: CompactBridgeZKProof,
  recentBlockMerkleTreeRoot: Uint8Array,
  recentAutoClaimTxoRoot: Uint8Array,
  newManualClaimTxoRoot: Uint8Array,
  txHash: Uint8Array,
  combinedTxoIndex: bigint,
  depositAmountSats: bigint
): TransactionInstruction {
  const [claimPda] = getManualClaimPda(userSigner, programId);
  const [bridgeState] = getBridgeStatePda(bridgeProgramId);

  const instructionData = new Uint8Array(MANUAL_CLAIM_INSTRUCTION_DATA_SIZE);
  encodeManualClaimInstructionData({
    proof,
    recentBlockMerkleTreeRoot,
    recentAutoClaimTxoRoot,
    newManualClaimTxoRoot,
    txHash,
    combinedTxoIndex,
    depositAmountSats,
  }, instructionData);

  const header = createInstructionHeader(MC_MANUAL_CLAIM_TRANSACTION_DISCRIMINATOR);
  const data = concatBytes(header, instructionData);

  return new TransactionInstruction({
    keys: [
      { pubkey: claimPda, isSigner: false, isWritable: true },
      { pubkey: bridgeState, isSigner: false, isWritable: false },
      { pubkey: recipient, isSigner: false, isWritable: true },
      { pubkey: mint, isSigner: false, isWritable: true },
      { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
      { pubkey: bridgeProgramId, isSigner: false, isWritable: false },
      { pubkey: userSigner, isSigner: true, isWritable: false },
      { pubkey: payer, isSigner: true, isWritable: true },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
    ],
    programId,
    data: Buffer.from(data),
  });
}

export function processMintGroup(
  programId: PublicKey,
  operator: PublicKey,
  mintBuffer: PublicKey,
  dogeMint: PublicKey,
  recipients: PublicKey[],
  groupIndex: number,
  mintBufferBump: number,
  shouldUnlock: boolean,
  pendingMintPid: PublicKey = PENDING_MINT_BUFFER_BUILDER_PROGRAM_ID
): TransactionInstruction {
  const [bridgeState] = getBridgeStatePda(programId);

  const payload = new Uint8Array(4);
  const view = new DataView(payload.buffer);
  view.setUint16(0, groupIndex, true);
  view.setUint8(2, mintBufferBump);
  view.setUint8(3, shouldUnlock ? 1 : 0);

  const header = createInstructionHeader(DOGE_BRIDGE_INSTRUCTION_PROCESS_MINT_GROUP);
  const data = concatBytes(header, payload);

  const keys: AccountMeta[] = [
    { pubkey: bridgeState, isSigner: false, isWritable: true },
    { pubkey: mintBuffer, isSigner: false, isWritable: true },
    { pubkey: operator, isSigner: true, isWritable: false },
    { pubkey: dogeMint, isSigner: false, isWritable: true },
    { pubkey: operator, isSigner: true, isWritable: true },
    { pubkey: pendingMintPid, isSigner: false, isWritable: false },
    { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
  ];

  for (const r of recipients) {
    keys.push({ pubkey: r, isSigner: false, isWritable: true });
  }

  return new TransactionInstruction({ keys, programId, data: Buffer.from(data) });
}

export function processMintGroupAutoAdvance(
  programId: PublicKey,
  operator: PublicKey,
  mintBuffer: PublicKey,
  txoBuffer: PublicKey,
  dogeMint: PublicKey,
  recipients: PublicKey[],
  groupIndex: number,
  mintBufferBump: number,
  txoBufferBump: number,
  shouldUnlock: boolean,
  pendingMintPid: PublicKey = PENDING_MINT_BUFFER_BUILDER_PROGRAM_ID,
  txoBufferPid: PublicKey = TXO_BUFFER_BUILDER_PROGRAM_ID
): TransactionInstruction {
  const [bridgeState] = getBridgeStatePda(programId);

  const payload = new Uint8Array(4);
  const view = new DataView(payload.buffer);
  view.setUint16(0, groupIndex, true);
  view.setUint8(2, mintBufferBump);
  view.setUint8(3, shouldUnlock ? 1 : 0);

  const header = createInstructionHeader(DOGE_BRIDGE_INSTRUCTION_PROCESS_MINT_GROUP_AUTO_ADVANCE, [mintBufferBump, txoBufferBump]);
  const data = concatBytes(header, payload);

  const keys: AccountMeta[] = [
    { pubkey: bridgeState, isSigner: false, isWritable: true },
    { pubkey: mintBuffer, isSigner: false, isWritable: true },
    { pubkey: txoBuffer, isSigner: false, isWritable: true },
    { pubkey: operator, isSigner: true, isWritable: false },
    { pubkey: dogeMint, isSigner: false, isWritable: true },
    { pubkey: operator, isSigner: true, isWritable: true },
    { pubkey: pendingMintPid, isSigner: false, isWritable: false },
    { pubkey: txoBufferPid, isSigner: false, isWritable: false },
    { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
  ];

  for (const r of recipients) {
    keys.push({ pubkey: r, isSigner: false, isWritable: true });
  }

  return new TransactionInstruction({ keys, programId, data: Buffer.from(data) });
}

export function operatorWithdrawFees(
  programId: PublicKey,
  operator: PublicKey,
  operatorTokenAccount: PublicKey,
  dogeMint: PublicKey
): TransactionInstruction {
  const [bridgeState] = getBridgeStatePda(programId);

  const header = createInstructionHeader(DOGE_BRIDGE_INSTRUCTION_OPERATOR_WITHDRAW_FEES);

  return new TransactionInstruction({
    keys: [
      { pubkey: bridgeState, isSigner: false, isWritable: true },
      { pubkey: operatorTokenAccount, isSigner: false, isWritable: true },
      { pubkey: dogeMint, isSigner: false, isWritable: true },
      { pubkey: operator, isSigner: true, isWritable: true },
      { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
    ],
    programId,
    data: Buffer.from(header),
  });
}

export function snapshotWithdrawals(
  programId: PublicKey,
  operator: PublicKey,
  payer: PublicKey
): TransactionInstruction {
  const [bridgeState] = getBridgeStatePda(programId);

  const header = createInstructionHeader(DOGE_BRIDGE_INSTRUCTION_SNAPSHOT_WITHDRAWALS);

  return new TransactionInstruction({
    keys: [
      { pubkey: bridgeState, isSigner: false, isWritable: true },
      { pubkey: operator, isSigner: true, isWritable: false },
      { pubkey: payer, isSigner: true, isWritable: true },
    ],
    programId,
    data: Buffer.from(header),
  });
}

// =============================================================================
// Buffer Instructions
// =============================================================================

export function genericBufferInit(
  programId: PublicKey,
  account: PublicKey,
  payer: PublicKey,
  targetSize: number
): TransactionInstruction {
  const data = new Uint8Array(5);
  data[0] = 0;
  new DataView(data.buffer).setUint32(1, targetSize, true);

  return new TransactionInstruction({
    keys: [
      { pubkey: account, isSigner: false, isWritable: true },
      { pubkey: payer, isSigner: true, isWritable: true },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
    ],
    programId,
    data: Buffer.from(data),
  });
}

export function genericBufferWrite(
  programId: PublicKey,
  account: PublicKey,
  payer: PublicKey,
  offset: number,
  bytes: Uint8Array
): TransactionInstruction {
  const data = new Uint8Array(5 + bytes.length);
  data[0] = 2;
  new DataView(data.buffer).setUint32(1, offset, true);
  data.set(bytes, 5);

  return new TransactionInstruction({
    keys: [
      { pubkey: account, isSigner: false, isWritable: true },
      { pubkey: payer, isSigner: true, isWritable: true },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
    ],
    programId,
    data: Buffer.from(data),
  });
}

export function pendingMintSetup(
  programId: PublicKey,
  account: PublicKey,
  locker: PublicKey,
  writer: PublicKey
): TransactionInstruction {
  const data = new Uint8Array(65);
  data[0] = 0;
  data.set(locker.toBuffer(), 1);
  data.set(writer.toBuffer(), 33);

  return new TransactionInstruction({
    keys: [
      { pubkey: account, isSigner: false, isWritable: true },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
    ],
    programId,
    data: Buffer.from(data),
  });
}

export function pendingMintReinit(
  programId: PublicKey,
  account: PublicKey,
  payer: PublicKey,
  totalMints: number
): TransactionInstruction {
  const data = new Uint8Array(3);
  data[0] = 1;
  new DataView(data.buffer).setUint16(1, totalMints, true);

  return new TransactionInstruction({
    keys: [
      { pubkey: account, isSigner: false, isWritable: true },
      { pubkey: payer, isSigner: true, isWritable: true },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
    ],
    programId,
    data: Buffer.from(data),
  });
}

export function pendingMintInsert(
  programId: PublicKey,
  account: PublicKey,
  payer: PublicKey,
  groupIdx: number,
  mintData: Uint8Array
): TransactionInstruction {
  const data = new Uint8Array(3 + mintData.length);
  data[0] = 3;
  new DataView(data.buffer).setUint16(1, groupIdx, true);
  data.set(mintData, 3);

  return new TransactionInstruction({
    keys: [
      { pubkey: account, isSigner: false, isWritable: true },
      { pubkey: payer, isSigner: true, isWritable: true },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
    ],
    programId,
    data: Buffer.from(data),
  });
}

export function txoBufferInit(
  programId: PublicKey,
  account: PublicKey,
  writer: PublicKey
): TransactionInstruction {
  const data = new Uint8Array(33);
  data[0] = 0;
  data.set(writer.toBuffer(), 1);

  return new TransactionInstruction({
    keys: [
      { pubkey: account, isSigner: false, isWritable: true },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
    ],
    programId,
    data: Buffer.from(data),
  });
}

export function txoBufferSetLen(
  programId: PublicKey,
  account: PublicKey,
  payer: PublicKey,
  writer: PublicKey,
  newLen: number,
  resize: boolean,
  batchId: number,
  height: number,
  finalize: boolean
): TransactionInstruction {
  const data = new Uint8Array(15);
  const view = new DataView(data.buffer);
  data[0] = 1;
  view.setUint32(1, newLen, true);
  data[5] = resize ? 1 : 0;
  view.setUint32(6, batchId, true);
  view.setUint32(10, height, true);
  data[14] = finalize ? 1 : 0;

  return new TransactionInstruction({
    keys: [
      { pubkey: account, isSigner: false, isWritable: true },
      { pubkey: payer, isSigner: true, isWritable: true },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
      { pubkey: writer, isSigner: true, isWritable: false },
    ],
    programId,
    data: Buffer.from(data),
  });
}

export function txoBufferWrite(
  programId: PublicKey,
  account: PublicKey,
  writer: PublicKey,
  batchId: number,
  offset: number,
  bytes: Uint8Array
): TransactionInstruction {
  const data = new Uint8Array(9 + bytes.length);
  const view = new DataView(data.buffer);
  data[0] = 2;
  view.setUint32(1, batchId, true);
  view.setUint32(5, offset, true);
  data.set(bytes, 9);

  return new TransactionInstruction({
    keys: [
      { pubkey: account, isSigner: false, isWritable: true },
      { pubkey: writer, isSigner: true, isWritable: true },
    ],
    programId,
    data: Buffer.from(data),
  });
}
