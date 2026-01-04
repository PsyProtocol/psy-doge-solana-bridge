import {
  Connection,
  Keypair,
  PublicKey,
  SystemProgram,
  Transaction,
  TransactionInstruction,
  sendAndConfirmTransaction
} from "@solana/web3.js";
import { encodePendingMint, PENDING_MINT_SIZE, PendingMint } from "./layout";

const CHUNK_SIZE = 900;
const PM_MAX_PENDING_MINTS_PER_GROUP = 24;

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
    programId
  });

  const initData = new Uint8Array(5);
  new DataView(initData.buffer).setUint8(0, 0);
  new DataView(initData.buffer).setUint32(1, dataContent.length, true);

  const initIx = new TransactionInstruction({
    keys: [
      { pubkey: bufferKp.publicKey, isSigner: false, isWritable: true },
      { pubkey: payer.publicKey, isSigner: true, isWritable: true },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false }
    ],
    programId,
    data: Buffer.from(initData)
  });

  await sendAndConfirmTransaction(connection, new Transaction().add(createIx, initIx), [payer, bufferKp]);

  for (let i = 0; i < dataContent.length; i += CHUNK_SIZE) {
    const end = Math.min(i + CHUNK_SIZE, dataContent.length);
    const chunk = dataContent.subarray(i, end);
    
    const writeData = new Uint8Array(5 + chunk.length);
    const view = new DataView(writeData.buffer);
    view.setUint8(0, 2);
    view.setUint32(1, i, true);
    writeData.set(chunk, 5);

    const writeIx = new TransactionInstruction({
      keys: [
        { pubkey: bufferKp.publicKey, isSigner: false, isWritable: true },
        { pubkey: payer.publicKey, isSigner: true, isWritable: true },
        { pubkey: SystemProgram.programId, isSigner: false, isWritable: false }
      ],
      programId,
      data: Buffer.from(writeData)
    });
    await sendAndConfirmTransaction(connection, new Transaction().add(writeIx), [payer]);
  }
  return bufferKp.publicKey;
}

export async function createTxoBuffer(
  connection: Connection,
  programId: PublicKey,
  payer: Keypair,
  dogeBlockHeight: number,
  txoIndicesU32: number[]
): Promise<PublicKey> {
  const [bufferPda] = PublicKey.findProgramAddressSync([new TextEncoder().encode("txo_buffer"), payer.publicKey.toBuffer()], programId);
  
  const accountInfo = await connection.getAccountInfo(bufferPda);
  let batchId = 0;

  if (!accountInfo) {
    const space = 48;
    const rent = await connection.getMinimumBalanceForRentExemption(space);
    const transferIx = SystemProgram.transfer({ fromPubkey: payer.publicKey, toPubkey: bufferPda, lamports: rent });
    
    const initData = new Uint8Array(33);
    initData[0] = 0;
    initData.set(payer.publicKey.toBuffer(), 1);
    
    const initIx = new TransactionInstruction({
      keys: [{ pubkey: bufferPda, isSigner: false, isWritable: true }],
      programId,
      data: Buffer.from(initData)
    });
    
    await sendAndConfirmTransaction(connection, new Transaction().add(transferIx, initIx), [payer]);
  } else {
    batchId = 1 + new DataView(accountInfo.data.buffer, accountInfo.data.byteOffset).getUint32(40, true);
  }
  
  const txoBytes = new Uint8Array(txoIndicesU32.length * 4);
  const txoView = new DataView(txoBytes.buffer);
  for(let i=0; i<txoIndicesU32.length; i++) {
    txoView.setUint32(i*4, txoIndicesU32[i], true);
  }

  const setLenData = new Uint8Array(15);
  const setLenView = new DataView(setLenData.buffer);
  setLenView.setUint8(0, 1);
  setLenView.setUint32(1, txoBytes.length, true);
  setLenView.setUint8(5, 1); // resize
  setLenView.setUint32(6, batchId, true);
  setLenView.setUint32(10, dogeBlockHeight, true);
  setLenView.setUint8(14, 0); // finalize=false

  const setLenIx = new TransactionInstruction({
    keys: [
      { pubkey: bufferPda, isSigner: false, isWritable: true },
      { pubkey: payer.publicKey, isSigner: true, isWritable: true },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
      { pubkey: payer.publicKey, isSigner: true, isWritable: false },
    ],
    programId,
    data: Buffer.from(setLenData)
  });
  await sendAndConfirmTransaction(connection, new Transaction().add(setLenIx), [payer]);

  for (let i = 0; i < txoBytes.length; i += CHUNK_SIZE) {
    const end = Math.min(i + CHUNK_SIZE, txoBytes.length);
    const chunk = txoBytes.subarray(i, end);
    
    const writeData = new Uint8Array(9 + chunk.length);
    const writeView = new DataView(writeData.buffer);
    writeView.setUint8(0, 2);
    writeView.setUint32(1, batchId, true);
    writeView.setUint32(5, i, true);
    writeData.set(chunk, 9);

    const writeIx = new TransactionInstruction({
      keys: [
        { pubkey: bufferPda, isSigner: false, isWritable: true },
        { pubkey: payer.publicKey, isSigner: true, isWritable: true },
      ],
      programId,
      data: Buffer.from(writeData)
    });
    await sendAndConfirmTransaction(connection, new Transaction().add(writeIx), [payer]);
  }

  const finalizeData = new Uint8Array(15);
  const finalizeView = new DataView(finalizeData.buffer);
  finalizeView.setUint8(0, 1);
  finalizeView.setUint32(1, txoBytes.length, true);
  finalizeView.setUint8(5, 0); // resize=false
  finalizeView.setUint32(6, batchId, true);
  finalizeView.setUint32(10, dogeBlockHeight, true);
  finalizeView.setUint8(14, 1); // finalize=true

  const finalizeIx = new TransactionInstruction({
    keys: [
      { pubkey: bufferPda, isSigner: false, isWritable: true },
      { pubkey: payer.publicKey, isSigner: true, isWritable: true },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
      { pubkey: payer.publicKey, isSigner: true, isWritable: false },
    ],
    programId,
    data: Buffer.from(finalizeData)
  });
  await sendAndConfirmTransaction(connection, new Transaction().add(finalizeIx), [payer]);

  return bufferPda;
}

export async function createPendingMintBuffer(
  connection: Connection,
  programId: PublicKey,
  payer: Keypair,
  locker: PublicKey,
  mints: PendingMint[]
): Promise<PublicKey> {
  const [bufferPda] = PublicKey.findProgramAddressSync([new TextEncoder().encode("mint_buffer"), payer.publicKey.toBuffer()], programId);
  
  const accountInfo = await connection.getAccountInfo(bufferPda);

  if (!accountInfo) {
    const space = 72;
    const rent = await connection.getMinimumBalanceForRentExemption(space);
    const transferIx = SystemProgram.transfer({ fromPubkey: payer.publicKey, toPubkey: bufferPda, lamports: rent });

    const setupData = new Uint8Array(65);
    setupData[0] = 0;
    setupData.set(locker.toBuffer(), 1);
    setupData.set(payer.publicKey.toBuffer(), 33);
    
    const setupIx = new TransactionInstruction({
      keys: [{ pubkey: bufferPda, isSigner: false, isWritable: true }],
      programId,
      data: Buffer.from(setupData)
    });

    await sendAndConfirmTransaction(connection, new Transaction().add(transferIx, setupIx), [payer]);
  }

  const reinitData = new Uint8Array(3);
  const reinitView = new DataView(reinitData.buffer);
  reinitView.setUint8(0, 1);
  reinitView.setUint16(1, mints.length, true);
  
  const reinitIx = new TransactionInstruction({ 
    keys: [
      { pubkey: bufferPda, isSigner: false, isWritable: true }, 
      { pubkey: payer.publicKey, isSigner: true, isWritable: true }, 
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false }
    ], 
    programId, 
    data: Buffer.from(reinitData) 
  });
  await sendAndConfirmTransaction(connection, new Transaction().add(reinitIx), [payer]);

  const groupCount = Math.ceil(mints.length / PM_MAX_PENDING_MINTS_PER_GROUP);
  for(let i=0; i<groupCount; i++) {
    const start = i * PM_MAX_PENDING_MINTS_PER_GROUP;
    const end = Math.min(start + PM_MAX_PENDING_MINTS_PER_GROUP, mints.length);
    const group = mints.slice(start, end);

    const mintData = new Uint8Array(group.length * PENDING_MINT_SIZE);
    let offset = 0;
    for(const m of group) {
      encodePendingMint(m, mintData, offset);
      offset += PENDING_MINT_SIZE;
    }

    const insertData = new Uint8Array(3 + mintData.length);
    const insertView = new DataView(insertData.buffer);
    insertView.setUint8(0, 3);
    insertView.setUint16(1, i, true);
    insertData.set(mintData, 3);

    const insertIx = new TransactionInstruction({ 
      keys: [
        { pubkey: bufferPda, isSigner: false, isWritable: true }, 
        { pubkey: payer.publicKey, isSigner: true, isWritable: true }, 
        { pubkey: SystemProgram.programId, isSigner: false, isWritable: false }
      ], 
      programId, 
      data: Buffer.from(insertData) 
    });
    await sendAndConfirmTransaction(connection, new Transaction().add(insertIx), [payer]);
  }

  return bufferPda;
}