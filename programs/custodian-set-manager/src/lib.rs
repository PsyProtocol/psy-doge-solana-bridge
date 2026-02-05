use bytemuck::{Pod, Zeroable};
use solana_program::{
    account_info::{next_account_info, AccountInfo},
    declare_id,
    entrypoint::ProgramResult,
    msg,
    program::invoke_signed,
    program_error::ProgramError,
    pubkey::Pubkey,
    rent::Rent,
    system_instruction,
    sysvar::Sysvar,
};

/// RemoteMultisigCustodianConfig - 232 bytes
/// Matches the struct in psy-bridge-core
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct RemoteMultisigCustodianConfig {
    pub signer_public_keys: [[u8; 32]; 7], // 224 bytes
    pub custodian_config_id: u32,          // 4 bytes
    pub signer_public_keys_y_parity: u32,  // 4 bytes
}

/// CustodianSetAccount - 264 bytes total
/// [0..32]: operator pubkey
/// [32..264]: RemoteMultisigCustodianConfig (232 bytes)
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct CustodianSetAccount {
    pub operator: [u8; 32],
    pub config: RemoteMultisigCustodianConfig,
}

const ACCOUNT_SIZE: usize = std::mem::size_of::<CustodianSetAccount>(); // 264 bytes
const CONFIG_SIZE: usize = std::mem::size_of::<RemoteMultisigCustodianConfig>(); // 232 bytes

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
    let (tag, rest) = instruction_data.split_first().unwrap();

    let account_info_iter = &mut accounts.iter();
    let contract_account = next_account_info(account_info_iter)?;

    match tag {
        // 0: Initialize(operator: [u8; 32], config: RemoteMultisigCustodianConfig)
        // instruction_data: [tag(1)] + [operator(32)] + [config(232)] = 265 bytes
        0 => {
            if rest.len() != ACCOUNT_SIZE {
                msg!("Expected {} bytes, got {}", ACCOUNT_SIZE, rest.len());
                return Err(ProgramError::InvalidInstructionData);
            }

            let operator: [u8; 32] = rest[0..32].try_into().unwrap();

            // Verify PDA address
            // Seeds: [b"custodian_set", operator]
            let (expected_pda, bump) =
                Pubkey::find_program_address(&[b"custodian_set", &operator], program_id);
            if contract_account.key != &expected_pda {
                msg!("Invalid PDA for custodian set.");
                return Err(ProgramError::InvalidSeeds);
            }

            // Allocate & assign if needed
            if contract_account.data_len() == 0 {
                let seeds: &[&[u8]] = &[b"custodian_set", &operator, &[bump]];
                let rent = Rent::get()?;
                let required_lamports = rent.minimum_balance(ACCOUNT_SIZE);

                if contract_account.lamports() < required_lamports {
                    return Err(ProgramError::InsufficientFunds);
                }

                invoke_signed(
                    &system_instruction::allocate(contract_account.key, ACCOUNT_SIZE as u64),
                    &[contract_account.clone()],
                    &[seeds],
                )?;
                invoke_signed(
                    &system_instruction::assign(contract_account.key, program_id),
                    &[contract_account.clone()],
                    &[seeds],
                )?;
            }

            if contract_account.owner != program_id {
                return Err(ProgramError::IncorrectProgramId);
            }

            let mut data = contract_account.try_borrow_mut_data()?;
            if data.len() < ACCOUNT_SIZE {
                return Err(ProgramError::AccountDataTooSmall);
            }

            // Copy the entire account data from instruction
            data[0..ACCOUNT_SIZE].copy_from_slice(&rest[0..ACCOUNT_SIZE]);

            msg!("Custodian set initialized. Operator set.");
        }

        // 1: UpdateCustodianConfig(config: RemoteMultisigCustodianConfig)
        // instruction_data: [tag(1)] + [config(232)] = 233 bytes
        // Only callable by operator
        1 => {
            if contract_account.owner != program_id {
                return Err(ProgramError::IncorrectProgramId);
            }
            if rest.len() != CONFIG_SIZE {
                msg!("Expected {} bytes, got {}", CONFIG_SIZE, rest.len());
                return Err(ProgramError::InvalidInstructionData);
            }

            let signer = next_account_info(account_info_iter)?;
            if !signer.is_signer {
                return Err(ProgramError::MissingRequiredSignature);
            }

            let mut data = contract_account.try_borrow_mut_data()?;
            if data.len() < ACCOUNT_SIZE {
                return Err(ProgramError::AccountDataTooSmall);
            }

            let account = bytemuck::from_bytes_mut::<CustodianSetAccount>(&mut data[0..ACCOUNT_SIZE]);

            // Verify signer is the operator
            if account.operator != signer.key.to_bytes() {
                msg!("Signer is not the operator.");
                return Err(ProgramError::IllegalOwner);
            }

            // Update the config
            let new_config = bytemuck::from_bytes::<RemoteMultisigCustodianConfig>(rest);
            account.config = *new_config;

            msg!("Custodian config updated.");
        }

        _ => return Err(ProgramError::InvalidInstructionData),
    }

    Ok(())
}

declare_id!("CMsT3coDPqpJc7FEhJwWLuJ11y6b9benPb82pHqpDUWt");
