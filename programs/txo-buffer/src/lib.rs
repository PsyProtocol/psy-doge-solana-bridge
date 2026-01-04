use bytemuck::{Pod, Zeroable};
use solana_program::{
    account_info::{AccountInfo, next_account_info}, declare_id, entrypoint::ProgramResult, msg, program::invoke, program::invoke_signed, program_error::ProgramError, pubkey::Pubkey, rent::Rent, system_instruction, sysvar::Sysvar
};

// ... (Structs unchanged) ...
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct PendingMintsTxoBufferHeader {
    pub authorized_writer: [u8; 32],
    pub init_status: u16,
    pub finalized_status: u16,
    pub doge_block_height: u32,
    pub batch_id: u32,
    pub data_size: u32,
}
const HEADER_SIZE: usize = std::mem::size_of::<PendingMintsTxoBufferHeader>();

// ... (Helpers unchanged) ...
pub fn realloc_account<'a>(
    account: &AccountInfo<'a>,
    payer: &AccountInfo<'a>,
    system_program: &AccountInfo<'a>,
    new_size: usize,
) -> ProgramResult {
    let rent = Rent::get()?;
    let current_len = account.data_len();
    let old_rent = rent.minimum_balance(current_len);
    let new_rent = rent.minimum_balance(new_size);

    if new_rent > old_rent {
        let diff = new_rent - old_rent;
        invoke(
            &system_instruction::transfer(payer.key, account.key, diff),
            &[payer.clone(), account.clone(), system_program.clone()],
        )?;
    } else if old_rent > new_rent {
        let diff = old_rent - new_rent;
        let mut from_lamports = account.try_borrow_mut_lamports()?;
        let mut to_lamports = payer.try_borrow_mut_lamports()?;
        **from_lamports -= diff;
        **to_lamports += diff;
    }
    account.realloc(new_size, false)?;
    Ok(())
}

fn handle_batch_transition(header: &mut PendingMintsTxoBufferHeader, input_batch_id: u32) -> ProgramResult {
    if input_batch_id == header.batch_id {
        if header.finalized_status == 1 { return Err(ProgramError::AccountAlreadyInitialized); }
    } else if input_batch_id == header.batch_id + 1 {
        header.batch_id = input_batch_id;
        header.finalized_status = 0;
    } else {
        return Err(ProgramError::InvalidArgument);
    }
    Ok(())
}

#[cfg(not(feature = "no-entrypoint"))]
solana_program::entrypoint!(process_instruction);

pub fn process_instruction(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    instruction_data: &[u8],
) -> ProgramResult {
    if instruction_data.is_empty() { return Err(ProgramError::InvalidInstructionData); }
    let (tag, rest) = instruction_data.split_first().unwrap();

    let account_info_iter = &mut accounts.iter();
    let contract_account = next_account_info(account_info_iter)?;

    match tag {
        // 0: Initialize(authorized_writer: [32])
        0 => {
            if rest.len() != 32 { return Err(ProgramError::InvalidInstructionData); }
            let auth_key: [u8; 32] = rest[0..32].try_into().unwrap();

            // 1. Verify PDA Address
            // Seeds: [b"txo_buffer", writer_key]
            let (expected_pda, bump) = Pubkey::find_program_address(
                &[b"txo_buffer", &auth_key], 
                program_id
            );
            if contract_account.key != &expected_pda {
                msg!("Invalid PDA for TXO Buffer.");
                return Err(ProgramError::InvalidSeeds);
            }

            // 2. Allocate & Assign if needed
            if contract_account.data_len() == 0 {
                let seeds: &[&[u8]] = &[b"txo_buffer", &auth_key, &[bump]];
                let rent = Rent::get()?;
                let required_lamports = rent.minimum_balance(HEADER_SIZE);
                
                if contract_account.lamports() < required_lamports {
                    return Err(ProgramError::InsufficientFunds);
                }

                invoke_signed(
                    &system_instruction::allocate(contract_account.key, HEADER_SIZE as u64),
                    &[contract_account.clone()],
                    &[seeds],
                )?;
                invoke_signed(
                    &system_instruction::assign(contract_account.key, program_id),
                    &[contract_account.clone()],
                    &[seeds],
                )?;
            }

            if contract_account.owner != program_id { return Err(ProgramError::IncorrectProgramId); }

            let mut data = contract_account.try_borrow_mut_data()?;
            if data.len() < HEADER_SIZE { return Err(ProgramError::AccountDataTooSmall); }

            let header = bytemuck::from_bytes_mut::<PendingMintsTxoBufferHeader>(&mut data[0..HEADER_SIZE]);
            header.authorized_writer = auth_key;
            header.init_status = 1;
            // Height is set in set_len

            msg!("Initialized. Writer set.");
        }

        // 1: SetDataLength
        1 => {
            if contract_account.owner != program_id { return Err(ProgramError::IncorrectProgramId); }
            if rest.len() != 14 { return Err(ProgramError::InvalidInstructionData); }

            let new_length = u32::from_le_bytes(rest[0..4].try_into().unwrap());
            let resize = rest[4] != 0;
            let input_batch_id = u32::from_le_bytes(rest[5..9].try_into().unwrap());
            let input_doge_height = u32::from_le_bytes(rest[9..13].try_into().unwrap());
            let finalize = rest[13] != 0;

            let signer = next_account_info(account_info_iter)?;
            let system_program = next_account_info(account_info_iter)?;
            if !signer.is_signer { return Err(ProgramError::MissingRequiredSignature); }

            {
                let mut data = contract_account.try_borrow_mut_data()?;
                let header = bytemuck::from_bytes_mut::<PendingMintsTxoBufferHeader>(&mut data[0..HEADER_SIZE]);

                if header.authorized_writer != signer.key.to_bytes() { return Err(ProgramError::IllegalOwner); }
                
                // FIX: Check if this is a new batch BEFORE calling handle_batch_transition (which updates the state).
                // If input_batch_id is exactly 1 greater than current, it is a new batch/recycle.
                let is_new_batch = input_batch_id == header.batch_id + 1;

                handle_batch_transition(header, input_batch_id)?;

                // Height check
                // If it's a new batch, we ignore the old height (0 check irrelevant). 
                // We simply overwrite it.
                if !is_new_batch && header.doge_block_height != 0 && header.doge_block_height != input_doge_height {
                    return Err(ProgramError::InvalidInstructionData);
                }
                header.doge_block_height = input_doge_height;

                if finalize { header.finalized_status = 1; }
            }

            if resize {
                let required_physical = HEADER_SIZE + (new_length as usize);
                realloc_account(contract_account, signer, system_program, required_physical)?;
            } else if contract_account.data_len() < HEADER_SIZE + new_length as usize {
                return Err(ProgramError::AccountDataTooSmall);
            }

            {
                let mut data = contract_account.try_borrow_mut_data()?;
                let header = bytemuck::from_bytes_mut::<PendingMintsTxoBufferHeader>(&mut data[0..HEADER_SIZE]);
                header.data_size = new_length;
            }
        }

        // 2: WriteData
        2 => {
            if contract_account.owner != program_id { return Err(ProgramError::IncorrectProgramId); }
            if rest.len() < 8 { return Err(ProgramError::InvalidInstructionData); }
            let input_batch_id = u32::from_le_bytes(rest[0..4].try_into().unwrap());
            let offset = u32::from_le_bytes(rest[4..8].try_into().unwrap());
            let raw_data = &rest[8..];

            let signer = next_account_info(account_info_iter)?;
            if !signer.is_signer { return Err(ProgramError::MissingRequiredSignature); }

            let mut data = contract_account.try_borrow_mut_data()?;
            let (header_bytes, body_bytes) = data.split_at_mut(HEADER_SIZE);
            let header = bytemuck::from_bytes_mut::<PendingMintsTxoBufferHeader>(header_bytes);

            if header.authorized_writer != signer.key.to_bytes() { return Err(ProgramError::IllegalOwner); }
            handle_batch_transition(header, input_batch_id)?;

            let write_end = offset as usize + raw_data.len();
            if write_end > body_bytes.len() { return Err(ProgramError::AccountDataTooSmall); }
            body_bytes[offset as usize..write_end].copy_from_slice(raw_data);

            if write_end > header.data_size as usize { header.data_size = write_end as u32; }
        }

        _ => return Err(ProgramError::InvalidInstructionData),
    }

    Ok(())
}

declare_id!("TxoBuffer1111111111111111111111111111111111"); 
