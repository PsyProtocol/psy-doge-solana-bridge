pub mod error;
pub mod instruction;
pub mod processor;
#[cfg(not(feature = "no-entrypoint"))]
use solana_program::{
    account_info::AccountInfo, entrypoint::ProgramResult, pubkey::Pubkey
};

#[cfg(not(feature = "no-entrypoint"))]
solana_program::entrypoint!(process_instruction);

#[cfg(not(feature = "no-entrypoint"))]
fn process_instruction(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    instruction_data: &[u8],
) -> ProgramResult {
    processor::process_instruction(program_id, accounts, instruction_data)
}

solana_program::declare_id!("DogeBridgeManua1C1a1m1111111111111111111111"); 
