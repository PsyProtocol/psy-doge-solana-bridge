import { PublicKey, TransactionInstruction, SystemProgram, AccountMeta } from "@solana/web3.js";
import { TOKEN_PROGRAM_ID } from "@solana/spl-token";
import {
  INITIALIZE_BRIDGE_INSTRUCTION_DATA_SIZE,
  encodeInitializeBridgeInstructionData,
  InitializeBridgeInstructionData,
  PsyBridgeHeader,
  encodePsyBridgeHeader,
  PSY_BRIDGE_HEADER_SIZE,
  FinalizedBlockMintTxoInfo,
  FINALIZED_BLOCK_MINT_TXO_INFO_SIZE,
  encodeFinalizedBlockMintTxoInfo,
  PsyReturnTxOutput,
  encodePsyReturnTxOutput,
  PSY_RETURN_TX_OUTPUT_SIZE,
  MANUAL_CLAIM_INSTRUCTION_DATA_SIZE,
  encodeManualClaimInstructionData
} from "./layout";

const DOGE_BRIDGE_INSTRUCTION_INITIALIZE = 0;
const DOGE_BRIDGE_INSTRUCTION_BLOCK_UPDATE = 1;
const DOGE_BRIDGE_INSTRUCTION_REQUEST_WITHDRAWAL = 2;
const DOGE_BRIDGE_INSTRUCTION_PROCESS_WITHDRAWAL = 3;
const DOGE_BRIDGE_INSTRUCTION_OPERATOR_WITHDRAW_FEES = 4;
const DOGE_BRIDGE_INSTRUCTION_PROCESS_MANUAL_DEPOSIT = 5;
const DOGE_BRIDGE_INSTRUCTION_PROCESS_MINT_GROUP = 7;
const DOGE_BRIDGE_INSTRUCTION_PROCESS_REORG_BLOCKS = 8;
const DOGE_BRIDGE_INSTRUCTION_PROCESS_MINT_GROUP_AUTO_ADVANCE = 9;

const MANUAL_CLAIM_INSTRUCTION_CLAIM = 0;

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

export function createInitializeInstruction(
  programId: PublicKey,
  payer: PublicKey,
  operator: PublicKey,
  feeSpender: PublicKey,
  dogeMint: PublicKey,
  params: Omit<InitializeBridgeInstructionData, "operator_pubkey" | "fee_spender_pubkey" | "doge_mint">
): TransactionInstruction {
  const [bridgeState] = PublicKey.findProgramAddressSync(
    [new TextEncoder().encode("bridge_state")],
    programId
  );

  const fields: InitializeBridgeInstructionData = {
    operator_pubkey: operator,
    fee_spender_pubkey: feeSpender,
    doge_mint: dogeMint,
    bridge_header: params.bridge_header,
    start_return_txo_output: params.start_return_txo_output,
    config_params: params.config_params,
  };

  const instructionData = new Uint8Array(INITIALIZE_BRIDGE_INSTRUCTION_DATA_SIZE);
  encodeInitializeBridgeInstructionData(fields, instructionData);

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

export function createBlockUpdateInstruction(
  programId: PublicKey,
  operator: PublicKey,
  payer: PublicKey,
  proof: Uint8Array,
  header: PsyBridgeHeader,
  mintBuffer: PublicKey,
  txoBuffer: PublicKey,
  mintBufferBump: number,
  txoBufferBump: number,
  pendingMintPid: PublicKey,
  txoBufferPid: PublicKey,
): TransactionInstruction {
  const [bridgeState] = PublicKey.findProgramAddressSync(
    [new TextEncoder().encode("bridge_state")],
    programId
  );

  // CompactBridgeZKProof (256) + PsyBridgeHeader
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

export function createProcessReorgBlocksInstruction(
  programId: PublicKey,
  operator: PublicKey,
  payer: PublicKey,
  proof: Uint8Array,
  header: PsyBridgeHeader,
  extraBlocks: FinalizedBlockMintTxoInfo[],
  mintBuffer: PublicKey,
  txoBuffer: PublicKey,
  mintBufferBump: number,
  txoBufferBump: number,
  pendingMintPid: PublicKey,
  txoBufferPid: PublicKey,
): TransactionInstruction {
  const [bridgeState] = PublicKey.findProgramAddressSync(
    [new TextEncoder().encode("bridge_state")],
    programId
  );

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

export function createRequestWithdrawalInstruction(
  programId: PublicKey,
  userAuthority: PublicKey,
  mint: PublicKey,
  userTokenAccount: PublicKey,
  recipientAddress: Uint8Array, // [u8; 20]
  amountSats: bigint,
  addressType: number
): TransactionInstruction {
  const [bridgeState] = PublicKey.findProgramAddressSync(
    [new TextEncoder().encode("bridge_state")],
    programId
  );

  // Layout:
  // recipient_address: 20
  // address_type: 4
  // amount_sats: 8
  // arg_recipient: 20
  // arg_type: 4
  const dataSize = 20 + 4 + 8 + 20 + 4;
  const instructionData = new Uint8Array(dataSize);
  const view = new DataView(instructionData.buffer);

  instructionData.set(recipientAddress, 0);
  view.setUint32(20, addressType, true);
  view.setBigUint64(24, amountSats, true);
  instructionData.set(recipientAddress, 32);
  view.setUint32(52, addressType, true);

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

export function createProcessWithdrawalInstruction(
  programId: PublicKey,
  genericBufferAccount: PublicKey,
  proof: Uint8Array,
  newReturnOutput: PsyReturnTxOutput,
  newSpentTxoTreeRoot: Uint8Array,
  newNextProcessedWithdrawalsIndex: bigint
): TransactionInstruction {
  const [bridgeState] = PublicKey.findProgramAddressSync(
    [new TextEncoder().encode("bridge_state")],
    programId
  );

  // Layout:
  // Proof (256)
  // ReturnOutput (48)
  // SpentRoot (32)
  // NextIndex (8)
  const dataSize = 256 + PSY_RETURN_TX_OUTPUT_SIZE + 32 + 8;
  const instructionData = new Uint8Array(dataSize);
  
  let offset = 0;
  instructionData.set(proof, offset); offset += 256;
  offset += encodePsyReturnTxOutput(newReturnOutput, instructionData, offset);
  instructionData.set(newSpentTxoTreeRoot, offset); offset += 32;
  new DataView(instructionData.buffer, instructionData.byteOffset + offset, 8).setBigUint64(0, newNextProcessedWithdrawalsIndex, true);

  const header = createInstructionHeader(DOGE_BRIDGE_INSTRUCTION_PROCESS_WITHDRAWAL);
  const data = concatBytes(header, instructionData);

  return new TransactionInstruction({
    keys: [
      { pubkey: bridgeState, isSigner: false, isWritable: true },
      { pubkey: genericBufferAccount, isSigner: false, isWritable: false },
    ],
    programId,
    data: Buffer.from(data),
  });
}

export function createProcessManualDepositInstruction(
  programId: PublicKey,
  manualClaimPid: PublicKey,
  recipient: PublicKey,
  mint: PublicKey,
  txHash: Uint8Array,
  recentBlockMerkleTreeRoot: Uint8Array,
  recentAutoClaimTxoRoot: Uint8Array,
  combinedTxoIndex: bigint,
  depositorPublicKey: PublicKey,
  depositAmountSats: bigint
): TransactionInstruction {
  const [bridgeState] = PublicKey.findProgramAddressSync([new TextEncoder().encode("bridge_state")], programId);
  const [manualClaimSigner] = PublicKey.findProgramAddressSync([new TextEncoder().encode("manual-claim"), depositorPublicKey.toBuffer()], manualClaimPid);

  // Layout:
  // tx_hash (32)
  // recent_block_root (32)
  // recent_auto_root (32)
  // combined_index (8)
  // depositor_key (32)
  // amount (8)
  const dataSize = 32 + 32 + 32 + 8 + 32 + 8;
  const instructionData = new Uint8Array(dataSize);
  const view = new DataView(instructionData.buffer);
  
  let offset = 0;
  instructionData.set(txHash, offset); offset += 32;
  instructionData.set(recentBlockMerkleTreeRoot, offset); offset += 32;
  instructionData.set(recentAutoClaimTxoRoot, offset); offset += 32;
  view.setBigUint64(offset, combinedTxoIndex, true); offset += 8;
  instructionData.set(depositorPublicKey.toBuffer(), offset); offset += 32;
  view.setBigUint64(offset, depositAmountSats, true);

  const header = createInstructionHeader(DOGE_BRIDGE_INSTRUCTION_PROCESS_MANUAL_DEPOSIT);
  const data = concatBytes(header, instructionData);

  return new TransactionInstruction({
    keys: [
      { pubkey: bridgeState, isSigner: false, isWritable: true },
      { pubkey: recipient, isSigner: false, isWritable: true },
      { pubkey: mint, isSigner: false, isWritable: true },
      { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
      { pubkey: manualClaimSigner, isSigner: true, isWritable: false },
    ],
    programId,
    data: Buffer.from(data),
  });
}

export function createProcessMintGroupInstruction(
  programId: PublicKey,
  operator: PublicKey,
  dogeMint: PublicKey,
  mintBuffer: PublicKey,
  pendingMintPid: PublicKey,
  recipientAtas: PublicKey[],
  groupIndex: number,
  mintBufferBump: number,
  shouldUnlock: boolean,
): TransactionInstruction {
  const [bridgeState] = PublicKey.findProgramAddressSync([new TextEncoder().encode("bridge_state")], programId);

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

  recipientAtas.forEach(r => keys.push({ pubkey: r, isSigner: false, isWritable: true }));

  return new TransactionInstruction({ keys, programId, data: Buffer.from(data) });
}

export function createProcessMintGroupAutoAdvanceInstruction(
  programId: PublicKey,
  operator: PublicKey,
  dogeMint: PublicKey,
  mintBuffer: PublicKey,
  txoBuffer: PublicKey,
  pendingMintPid: PublicKey,
  txoBufferPid: PublicKey,
  recipientAtas: PublicKey[],
  groupIndex: number,
  mintBufferBump: number,
  txoBufferBump: number,
  shouldUnlock: boolean,
): TransactionInstruction {
  const [bridgeState] = PublicKey.findProgramAddressSync([new TextEncoder().encode("bridge_state")], programId);

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

  recipientAtas.forEach(r => keys.push({ pubkey: r, isSigner: false, isWritable: true }));

  return new TransactionInstruction({ keys, programId, data: Buffer.from(data) });
}

export function createOperatorWithdrawFeesInstruction(
  programId: PublicKey,
  operator: PublicKey,
  operatorTokenAccount: PublicKey,
  dogeMint: PublicKey,
): TransactionInstruction {
  const [bridgeState] = PublicKey.findProgramAddressSync(
    [new TextEncoder().encode("bridge_state")],
    programId
  );

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

export function createManualClaimInstruction(
  manualClaimProgramId: PublicKey,
  bridgeProgramId: PublicKey,
  payer: PublicKey,
  userSigner: PublicKey,
  mint: PublicKey,
  recipientTokenAccount: PublicKey,
  proof: Uint8Array,
  recentBlockMerkleTreeRoot: Uint8Array,
  recentAutoClaimTxoRoot: Uint8Array,
  newManualClaimTxoRoot: Uint8Array,
  txHash: Uint8Array,
  combinedTxoIndex: bigint,
  depositAmountSats: bigint
): TransactionInstruction {
  const [bridgeState] = PublicKey.findProgramAddressSync([new TextEncoder().encode("bridge_state")], bridgeProgramId);
  const [claimPda] = PublicKey.findProgramAddressSync(
    [new TextEncoder().encode("manual-claim"), userSigner.toBuffer()],
    manualClaimProgramId
  );

  const instructionData = new Uint8Array(MANUAL_CLAIM_INSTRUCTION_DATA_SIZE);
  encodeManualClaimInstructionData({
    proof,
    recent_block_merkle_tree_root: recentBlockMerkleTreeRoot,
    recent_auto_claim_txo_root: recentAutoClaimTxoRoot,
    new_manual_claim_txo_root: newManualClaimTxoRoot,
    tx_hash: txHash,
    combined_txo_index: combinedTxoIndex,
    deposit_amount_sats: depositAmountSats
  }, instructionData);

  const header = createInstructionHeader(MANUAL_CLAIM_INSTRUCTION_CLAIM);
  const data = concatBytes(header, instructionData);

  return new TransactionInstruction({
    keys: [
      { pubkey: claimPda, isSigner: false, isWritable: true },
      { pubkey: bridgeState, isSigner: false, isWritable: true },
      { pubkey: recipientTokenAccount, isSigner: false, isWritable: true },
      { pubkey: mint, isSigner: false, isWritable: true },
      { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
      { pubkey: bridgeProgramId, isSigner: false, isWritable: false },
      { pubkey: userSigner, isSigner: true, isWritable: false },
      { pubkey: payer, isSigner: true, isWritable: true },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
    ],
    programId: manualClaimProgramId,
    data: Buffer.from(data),
  });
}