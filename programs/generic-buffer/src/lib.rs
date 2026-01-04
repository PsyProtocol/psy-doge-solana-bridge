/*
since most of the bridge instruction functions will have instructions that make the total transaction size larger than the 1232 byte limit, we will need to have another generic buffer builder program that is used for building call data which is only accessed atomically (ie. no need for locking), and which does not need to be tagged with a specific block number (which txo buffer does):
*/
use bytemuck::{Pod, Zeroable};
use solana_program::{
    account_info::{AccountInfo, next_account_info}, declare_id, entrypoint::ProgramResult, msg, program::invoke, program_error::ProgramError, pubkey::Pubkey, rent::Rent, system_instruction, sysvar::Sysvar
};

// ============================================================================
// Constants & Structs
// ============================================================================

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct BufferBuilderHeader {
    pub authorized_writer: [u8; 32],
}

const HEADER_SIZE: usize = std::mem::size_of::<BufferBuilderHeader>();

const MAX_PERMITTED_DATA_INCREASE: usize = 10_240;

// ============================================================================
// Helpers
// ============================================================================

pub fn realloc_account<'a>(
    account: &AccountInfo<'a>,
    payer: &AccountInfo<'a>,
    system_program: &AccountInfo<'a>,
    target_size: usize,
) -> ProgramResult {
    let current_size = account.data_len();
    let rent = Rent::get()?;
    let current_minimum_balance = rent.minimum_balance(current_size);
    let target_minimum_balance = rent.minimum_balance(target_size);

    let lamports_diff = if target_size > current_size {
        (target_minimum_balance - current_minimum_balance) as u64
    } else {
        (current_minimum_balance - target_minimum_balance) as u64
    };

    if target_size > current_size {
        invoke(
            &system_instruction::transfer(payer.key, account.key, lamports_diff),
            &[payer.clone(), account.clone(), system_program.clone()],
        )?;
    } else if target_size < current_size {
        **account.try_borrow_mut_lamports()? -= lamports_diff;
        **payer.try_borrow_mut_lamports()? += lamports_diff;
    }

    account.realloc(target_size, false)?;
    Ok(())
}

// ============================================================================
// Entrypoint
// ============================================================================

#[cfg(not(feature = "no-entrypoint"))]
solana_program::entrypoint!(process_instruction);

pub fn process_instruction(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    instruction_data: &[u8],
) -> ProgramResult {
    if instruction_data.is_empty() {
        return Err(ProgramError::InvalidInstructionData);
    }

    let accounts_iter = &mut accounts.iter();
    let storage_account = next_account_info(accounts_iter)?;

    if storage_account.owner != program_id {
        return Err(ProgramError::IncorrectProgramId);
    }

    let tag = instruction_data[0];
    let rest = &instruction_data[1..];

    let payer = next_account_info(accounts_iter)?;
    let system_program = next_account_info(accounts_iter)?;

    if !payer.is_signer {
        return Err(ProgramError::MissingRequiredSignature);
    }

    match tag {
        // --------------------------------------------------------------------
        // 0: Init(target_data_size: u32)
        // --------------------------------------------------------------------
        0 => {
            if rest.len() != 4 {
                return Err(ProgramError::InvalidInstructionData);
            }
            let target_data_size = u32::from_le_bytes(rest.try_into().unwrap()) as usize;
            let target_total_size = HEADER_SIZE + target_data_size;

            let mut data = storage_account.try_borrow_mut_data()?;
            if data.len() < HEADER_SIZE {
                return Err(ProgramError::AccountDataTooSmall);
            }

            let header = bytemuck::from_bytes_mut::<BufferBuilderHeader>(&mut data[0..HEADER_SIZE]);
            if header.authorized_writer != [0u8; 32] {
                return Err(ProgramError::AccountAlreadyInitialized);
            }
            header.authorized_writer = payer.key.to_bytes();

            drop(data);

            let current_size = storage_account.data_len();
            let achievable_size = if target_total_size > current_size {
                let increase = target_total_size - current_size;
                current_size + increase.min(MAX_PERMITTED_DATA_INCREASE)
            } else {
                target_total_size
            };

            realloc_account(storage_account, payer, system_program, achievable_size)?;
            msg!("Initialized and resized to {}", achievable_size);
        }

        // --------------------------------------------------------------------
        // 1: Resize(target_data_size: u32)
        // --------------------------------------------------------------------
        1 => {
            if rest.len() != 4 {
                return Err(ProgramError::InvalidInstructionData);
            }
            let target_data_size = u32::from_le_bytes(rest.try_into().unwrap()) as usize;
            let target_total_size = HEADER_SIZE + target_data_size;

            let data = storage_account.try_borrow_data()?;
            if data.len() < HEADER_SIZE {
                return Err(ProgramError::AccountDataTooSmall);
            }
            let header = bytemuck::from_bytes::<BufferBuilderHeader>(&data[0..HEADER_SIZE]);
            if header.authorized_writer != payer.key.to_bytes() {
                return Err(ProgramError::IllegalOwner);
            }
            drop(data);

            let current_size = storage_account.data_len();
            let achievable_size = if target_total_size > current_size {
                let increase = target_total_size - current_size;
                current_size + increase.min(MAX_PERMITTED_DATA_INCREASE)
            } else {
                target_total_size
            };

            realloc_account(storage_account, payer, system_program, achievable_size)?;
            msg!("Resized to {}", achievable_size);
        }

        // --------------------------------------------------------------------
        // 2: WriteData(offset: u32, data: &[u8])
        // --------------------------------------------------------------------
        2 => {
            if rest.len() < 4 {
                return Err(ProgramError::InvalidInstructionData);
            }
            let offset = u32::from_le_bytes(rest[0..4].try_into().unwrap()) as usize;
            let write_data = &rest[4..];

            let data = storage_account.try_borrow_data()?;
            if data.len() < HEADER_SIZE {
                return Err(ProgramError::AccountDataTooSmall);
            }
            let header = bytemuck::from_bytes::<BufferBuilderHeader>(&data[0..HEADER_SIZE]);
            if header.authorized_writer != payer.key.to_bytes() {
                return Err(ProgramError::IllegalOwner);
            }
            drop(data);

            let current_total_size = storage_account.data_len();
            let required_total_size = HEADER_SIZE + offset + write_data.len();

            if required_total_size > current_total_size {
                let increase = required_total_size - current_total_size;
                let achievable_total_size = current_total_size + increase.min(MAX_PERMITTED_DATA_INCREASE);
                realloc_account(storage_account, payer, system_program, achievable_total_size)?;
            }

            let mut data = storage_account.try_borrow_mut_data()?;
            let current_total_size_now = data.len();
            if HEADER_SIZE + offset + write_data.len() > current_total_size_now {
                return Err(ProgramError::AccountDataTooSmall);
            }

            let dest_start = HEADER_SIZE + offset;
            let dest_end = dest_start + write_data.len();
            data[dest_start..dest_end].copy_from_slice(write_data);

            msg!("Wrote {} bytes at offset {}", write_data.len(), offset);
        }

        _ => return Err(ProgramError::InvalidInstructionData),
    }

    Ok(())
}

declare_id!("BufferBui1dr1111111111111111111111111111111"); 

/*

const HEADER_SIZE = 32;

const IX_INIT = 0;
const IX_RESIZE = 1;
const IX_WRITE = 2;

function u8ArrayToHex(buffer) {
  return Array.from(buffer)
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("");
}

function decodeHeader(buffer) {
  if (buffer.length < HEADER_SIZE) return null;
  return {
    authorizedWriter: new web3.PublicKey(buffer.subarray(0, 32)),
  };
}

describe("Generic Buffer Builder", function () {
  this.timeout(60000);

  let bufferKp;
  let writerKp;

  before(async () => {
    bufferKp = new web3.Keypair();
    writerKp = pg.wallet.keypair;
    console.log("Buffer:", bufferKp.publicKey.toString());
  });

  it("1. Create and Init with target 1000", async () => {
    const targetDataSize = 1000;
    const initSize = HEADER_SIZE;

    const lamports = await pg.connection.getMinimumBalanceForRentExemption(initSize);

    const createIx = web3.SystemProgram.createAccount({
      fromPubkey: pg.wallet.publicKey,
      newAccountPubkey: bufferKp.publicKey,
      lamports,
      space: initSize,
      programId: pg.PROGRAM_ID,
    });

    const initData = Buffer.alloc(5);
    initData.writeUInt8(IX_INIT, 0);
    initData.writeUInt32LE(targetDataSize, 1);

    const initIx = new web3.TransactionInstruction({
      keys: [
        { pubkey: bufferKp.publicKey, isSigner: false, isWritable: true },
        { pubkey: writerKp.publicKey, isSigner: true, isWritable: true },
        { pubkey: web3.SystemProgram.programId, isSigner: false, isWritable: false },
      ],
      programId: pg.PROGRAM_ID,
      data: initData,
    });

    await web3.sendAndConfirmTransaction(
      pg.connection,
      new web3.Transaction().add(createIx, initIx),
      [pg.wallet.keypair, bufferKp]
    );

    const acc = await pg.connection.getAccountInfo(bufferKp.publicKey);
    assert.equal(acc.data.length, HEADER_SIZE + targetDataSize);

    const header = decodeHeader(acc.data);
    assert(header.authorizedWriter.equals(writerKp.publicKey));
  });

  it("2. Resize to larger (20000, partial)", async () => {
    const targetDataSize = 20000;

    const resizeData = Buffer.alloc(5);
    resizeData.writeUInt8(IX_RESIZE, 0);
    resizeData.writeUInt32LE(targetDataSize, 1);

    const resizeIx = new web3.TransactionInstruction({
      keys: [
        { pubkey: bufferKp.publicKey, isSigner: false, isWritable: true },
        { pubkey: writerKp.publicKey, isSigner: true, isWritable: true },
        { pubkey: web3.SystemProgram.programId, isSigner: false, isWritable: false },
      ],
      programId: pg.PROGRAM_ID,
      data: resizeData,
    });

    await web3.sendAndConfirmTransaction(
      pg.connection,
      new web3.Transaction().add(resizeIx),
      [writerKp]
    );

    const acc = await pg.connection.getAccountInfo(bufferKp.publicKey);
    const expectedSize = HEADER_SIZE + 1000 + 10240;
    assert.equal(acc.data.length, expectedSize);
  });

  it("3. Resize again to reach 20000", async () => {
    const targetDataSize = 20000;

    const resizeData = Buffer.alloc(5);
    resizeData.writeUInt8(IX_RESIZE, 0);
    resizeData.writeUInt32LE(targetDataSize, 1);

    const resizeIx = new web3.TransactionInstruction({
      keys: [
        { pubkey: bufferKp.publicKey, isSigner: false, isWritable: true },
        { pubkey: writerKp.publicKey, isSigner: true, isWritable: true },
        { pubkey: web3.SystemProgram.programId, isSigner: false, isWritable: false },
      ],
      programId: pg.PROGRAM_ID,
      data: resizeData,
    });

    await web3.sendAndConfirmTransaction(
      pg.connection,
      new web3.Transaction().add(resizeIx),
      [writerKp]
    );

    const acc = await pg.connection.getAccountInfo(bufferKp.publicKey);
    assert.equal(acc.data.length, HEADER_SIZE + 20000);
  });

  it("4. Write data at offset 0", async () => {
    const offset = 0;
    const payload = Buffer.from("Hello, Solana!");

    const writeData = Buffer.alloc(5 + payload.length);
    writeData.writeUInt8(IX_WRITE, 0);
    writeData.writeUInt32LE(offset, 1);
    payload.copy(writeData, 5);

    const writeIx = new web3.TransactionInstruction({
      keys: [
        { pubkey: bufferKp.publicKey, isSigner: false, isWritable: true },
        { pubkey: writerKp.publicKey, isSigner: true, isWritable: true },
        { pubkey: web3.SystemProgram.programId, isSigner: false, isWritable: false },
      ],
      programId: pg.PROGRAM_ID,
      data: writeData,
    });

    await web3.sendAndConfirmTransaction(
      pg.connection,
      new web3.Transaction().add(writeIx),
      [writerKp]
    );

    const acc = await pg.connection.getAccountInfo(bufferKp.publicKey);
    const written = acc.data.subarray(HEADER_SIZE, HEADER_SIZE + payload.length);
    assert.deepEqual(Array.from(written), Array.from(payload));
  });

  it("5. Write at large offset (auto resize)", async () => {
    const offset = 25000;
    const payload = Buffer.from("Beyond current size");

    const writeData = Buffer.alloc(5 + payload.length);
    writeData.writeUInt8(IX_WRITE, 0);
    writeData.writeUInt32LE(offset, 1);
    payload.copy(writeData, 5);

    const writeIx = new web3.TransactionInstruction({
      keys: [
        { pubkey: bufferKp.publicKey, isSigner: false, isWritable: true },
        { pubkey: writerKp.publicKey, isSigner: true, isWritable: true },
        { pubkey: web3.SystemProgram.programId, isSigner: false, isWritable: false },
      ],
      programId: pg.PROGRAM_ID,
      data: writeData,
    });

    await web3.sendAndConfirmTransaction(
      pg.connection,
      new web3.Transaction().add(writeIx),
      [writerKp]
    );

    const acc = await pg.connection.getAccountInfo(bufferKp.publicKey);
    const expectedSize = HEADER_SIZE + offset + payload.length;
    assert.equal(acc.data.length, expectedSize);

    const written = acc.data.subarray(HEADER_SIZE + offset, HEADER_SIZE + offset + payload.length);
    assert.deepEqual(Array.from(written), Array.from(payload));
  });

  it("6. Shrink down to 100", async () => {
    const targetDataSize = 100;

    const resizeData = Buffer.alloc(5);
    resizeData.writeUInt8(IX_RESIZE, 0);
    resizeData.writeUInt32LE(targetDataSize, 1);

    const resizeIx = new web3.TransactionInstruction({
      keys: [
        { pubkey: bufferKp.publicKey, isSigner: false, isWritable: true },
        { pubkey: writerKp.publicKey, isSigner: true, isWritable: true },
        { pubkey: web3.SystemProgram.programId, isSigner: false, isWritable: false },
      ],
      programId: pg.PROGRAM_ID,
      data: resizeData,
    });

    await web3.sendAndConfirmTransaction(
      pg.connection,
      new web3.Transaction().add(resizeIx),
      [writerKp]
    );

    const acc = await pg.connection.getAccountInfo(bufferKp.publicKey);
    assert.equal(acc.data.length, HEADER_SIZE + 100);
  });
});

*/
