pub mod cpi_impl;
pub mod error;
pub mod instruction;
pub mod processor;
pub mod state;
pub mod program_pub_keys;

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

solana_program::declare_id!("DBjo5tqf2uwt4sg9JznSk9SBbEvsLixknN58y3trwCxJ"); 
