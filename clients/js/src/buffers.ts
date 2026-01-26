/**
 * Buffer management utilities for the Doge Bridge.
 */

import {
  Connection,
  Keypair,
  PublicKey,
  SystemProgram,
  Transaction,
  sendAndConfirmTransaction,
} from "@solana/web3.js";
import {
  genericBufferInit,
  genericBufferWrite,
  pendingMintSetup,
  pendingMintReinit,
  pendingMintInsert,
  txoBufferInit,
  txoBufferSetLen,
  txoBufferWrite,
} from "./instructions";
import {
  CHUNK_SIZE,
  PM_MAX_PENDING_MINTS_PER_GROUP,
  MINT_BUFFER_SEED,
  TXO_BUFFER_SEED,
} from "./constants";
import { PendingMint, PENDING_MINT_SIZE, encodePendingMint } from "./types";

/**
 * Get the mint buffer PDA for a writer.
 */
export function getMintBufferPda(
  writer: PublicKey,
  programId: PublicKey
): [PublicKey, number] {
  return PublicKey.findProgramAddressSync(
    [new TextEncoder().encode(MINT_BUFFER_SEED), writer.toBuffer()],
    programId
  );
}

/**
 * Get the TXO buffer PDA for a writer.
 */
export function getTxoBufferPda(
  writer: PublicKey,
  programId: PublicKey
): [PublicKey, number] {
  return PublicKey.findProgramAddressSync(
    [new TextEncoder().encode(TXO_BUFFER_SEED), writer.toBuffer()],
    programId
  );
}

/**
 * Create and populate a generic buffer with data.
 */
export async function createGenericBuffer(
  connection: Connection,
  programId: PublicKey,
  payer: Keypair,
  dataContent: Uint8Array
): Promise<PublicKey> {
  const bufferKp = Keypair.generate();
  const space = 32;
  const rent = await connection.getMinimumBalanceForRentExemption(space);

  const createIx = SystemProgram.createAccount({
    fromPubkey: payer.publicKey,
    newAccountPubkey: bufferKp.publicKey,
    lamports: rent,
    space,
    programId,
  });

  const initIx = genericBufferInit(programId, bufferKp.publicKey, payer.publicKey, dataContent.length);

  await sendAndConfirmTransaction(
    connection,
    new Transaction().add(createIx, initIx),
    [payer, bufferKp]
  );

  for (let i = 0; i < dataContent.length; i += CHUNK_SIZE) {
    const end = Math.min(i + CHUNK_SIZE, dataContent.length);
    const chunk = dataContent.subarray(i, end);
    const writeIx = genericBufferWrite(programId, bufferKp.publicKey, payer.publicKey, i, chunk);
    await sendAndConfirmTransaction(connection, new Transaction().add(writeIx), [payer]);
  }

  return bufferKp.publicKey;
}

/**
 * Create and populate a TXO buffer with indices.
 */
export async function createTxoBuffer(
  connection: Connection,
  programId: PublicKey,
  payer: Keypair,
  dogeBlockHeight: number,
  txoIndicesU32: number[]
): Promise<[PublicKey, number]> {
  const [bufferPda, bump] = getTxoBufferPda(payer.publicKey, programId);

  const accountInfo = await connection.getAccountInfo(bufferPda);
  let batchId = 0;

  if (!accountInfo) {
    const space = 48;
    const rent = await connection.getMinimumBalanceForRentExemption(space);
    const transferIx = SystemProgram.transfer({
      fromPubkey: payer.publicKey,
      toPubkey: bufferPda,
      lamports: rent,
    });
    const initIx = txoBufferInit(programId, bufferPda, payer.publicKey);
    await sendAndConfirmTransaction(connection, new Transaction().add(transferIx, initIx), [payer]);
  } else {
    batchId = 1 + new DataView(accountInfo.data.buffer, accountInfo.data.byteOffset).getUint32(40, true);
  }

  const txoBytes = new Uint8Array(txoIndicesU32.length * 4);
  const txoView = new DataView(txoBytes.buffer);
  for (let i = 0; i < txoIndicesU32.length; i++) {
    txoView.setUint32(i * 4, txoIndicesU32[i], true);
  }

  const setLenIx = txoBufferSetLen(
    programId,
    bufferPda,
    payer.publicKey,
    payer.publicKey,
    txoBytes.length,
    true,
    batchId,
    dogeBlockHeight,
    false
  );
  await sendAndConfirmTransaction(connection, new Transaction().add(setLenIx), [payer]);

  for (let i = 0; i < txoBytes.length; i += CHUNK_SIZE) {
    const end = Math.min(i + CHUNK_SIZE, txoBytes.length);
    const chunk = txoBytes.subarray(i, end);
    const writeIx = txoBufferWrite(programId, bufferPda, payer.publicKey, batchId, i, chunk);
    await sendAndConfirmTransaction(connection, new Transaction().add(writeIx), [payer]);
  }

  const finalizeIx = txoBufferSetLen(
    programId,
    bufferPda,
    payer.publicKey,
    payer.publicKey,
    txoBytes.length,
    false,
    batchId,
    dogeBlockHeight,
    true
  );
  await sendAndConfirmTransaction(connection, new Transaction().add(finalizeIx), [payer]);

  return [bufferPda, bump];
}

/**
 * Create and populate a pending mint buffer.
 */
export async function createPendingMintBuffer(
  connection: Connection,
  programId: PublicKey,
  payer: Keypair,
  locker: PublicKey,
  mints: PendingMint[]
): Promise<[PublicKey, number]> {
  const [bufferPda, bump] = getMintBufferPda(payer.publicKey, programId);

  const accountInfo = await connection.getAccountInfo(bufferPda);

  if (!accountInfo) {
    const space = 72;
    const rent = await connection.getMinimumBalanceForRentExemption(space);
    const transferIx = SystemProgram.transfer({
      fromPubkey: payer.publicKey,
      toPubkey: bufferPda,
      lamports: rent,
    });
    const setupIx = pendingMintSetup(programId, bufferPda, locker, payer.publicKey);
    await sendAndConfirmTransaction(connection, new Transaction().add(transferIx, setupIx), [payer]);
  }

  const reinitIx = pendingMintReinit(programId, bufferPda, payer.publicKey, mints.length);
  await sendAndConfirmTransaction(connection, new Transaction().add(reinitIx), [payer]);

  const groupCount = Math.ceil(mints.length / PM_MAX_PENDING_MINTS_PER_GROUP);
  for (let i = 0; i < groupCount; i++) {
    const start = i * PM_MAX_PENDING_MINTS_PER_GROUP;
    const end = Math.min(start + PM_MAX_PENDING_MINTS_PER_GROUP, mints.length);
    const group = mints.slice(start, end);

    const mintData = new Uint8Array(group.length * PENDING_MINT_SIZE);
    let offset = 0;
    for (const m of group) {
      encodePendingMint(m, mintData, offset);
      offset += PENDING_MINT_SIZE;
    }

    const insertIx = pendingMintInsert(programId, bufferPda, payer.publicKey, i, mintData);
    await sendAndConfirmTransaction(connection, new Transaction().add(insertIx), [payer]);
  }

  return [bufferPda, bump];
}
