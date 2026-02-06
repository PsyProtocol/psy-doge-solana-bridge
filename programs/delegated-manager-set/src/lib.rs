//! Mock `delegated-manager-set` — deploy at `wdmsTJP6YnsfeQjPuuEzGCrHmZvTmNy8VkxMCK8JkBX`.
//!
//! Single instruction: `set_manager_set { chain_id, index, data }`.
//! Creates both PDAs with the exact same layout/derivation as the real Anchor
//! program so your consuming program doesn't need to change between test and prod.
//!
//! Accounts (in order):
//!   0. payer              [signer, writable]
//!   1. manager_set_index  [writable]  PDA: ["manager_set_index", chain_id(BE)]
//!   2. manager_set        [writable]  PDA: ["manager_set", chain_id(BE), index(BE)]
//!   3. system_program

use borsh::{BorshDeserialize, BorshSerialize};
use delegated_manager_set_types::{
    ManagerSet, ManagerSetIndex, MANAGER_SET_DISC, MANAGER_SET_INDEX_DISC,
};
use solana_program::{
    account_info::{next_account_info, AccountInfo},
    entrypoint::ProgramResult,
    msg,
    program::invoke_signed,
    program_error::ProgramError,
    pubkey::Pubkey,
    rent::Rent,
    system_instruction,
    sysvar::Sysvar,
};

solana_program::declare_id!("wdmsTJP6YnsfeQjPuuEzGCrHmZvTmNy8VkxMCK8JkBX");

#[cfg(not(feature = "no-entrypoint"))]
solana_program::entrypoint!(process_instruction);

#[derive(BorshSerialize, BorshDeserialize)]
struct SetManagerSetArgs {
    chain_id: u16,
    index: u32,
    data: Vec<u8>,
}

pub fn process_instruction(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    instruction_data: &[u8],
) -> ProgramResult {
    let args = SetManagerSetArgs::try_from_slice(instruction_data)
        .map_err(|_| ProgramError::InvalidInstructionData)?;

    let iter = &mut accounts.iter();
    let payer = next_account_info(iter)?;
    let msi_ai = next_account_info(iter)?;
    let ms_ai = next_account_info(iter)?;
    let _system = next_account_info(iter)?;

    let chain_be = args.chain_id.to_be_bytes();
    let index_be = args.index.to_be_bytes();

    // --- ManagerSetIndex PDA ---
    let (msi_pda, msi_bump) =
        Pubkey::find_program_address(&[ManagerSetIndex::SEED, &chain_be], program_id);
    assert_eq!(&msi_pda, msi_ai.key, "manager_set_index PDA mismatch");

    if msi_ai.data_len() == 0 {
        create_pda(
            payer,
            msi_ai,
            ManagerSetIndex::SIZE,
            program_id,
            &[ManagerSetIndex::SEED, &chain_be, &[msi_bump]],
        )?;
    }

    {
        let val = ManagerSetIndex {
            manager_chain_id: args.chain_id,
            current_index: args.index,
        };
        let mut buf = msi_ai.try_borrow_mut_data()?;
        buf[..8].copy_from_slice(&MANAGER_SET_INDEX_DISC);
        let enc = borsh::to_vec(&val).unwrap();
        buf[8..8 + enc.len()].copy_from_slice(&enc);
    }

    // --- ManagerSet PDA ---
    let (ms_pda, ms_bump) =
        Pubkey::find_program_address(&[ManagerSet::SEED, &chain_be, &index_be], program_id);
    assert_eq!(&ms_pda, ms_ai.key, "manager_set PDA mismatch");

    let space = ManagerSet::size(args.data.len());
    if ms_ai.data_len() == 0 {
        create_pda(
            payer,
            ms_ai,
            space,
            program_id,
            &[ManagerSet::SEED, &chain_be, &index_be, &[ms_bump]],
        )?;
    }

    {
        let val = ManagerSet {
            manager_chain_id: args.chain_id,
            index: args.index,
            manager_set: args.data,
        };
        let mut buf = ms_ai.try_borrow_mut_data()?;
        buf[..8].copy_from_slice(&MANAGER_SET_DISC);
        let enc = borsh::to_vec(&val).unwrap();
        buf[8..8 + enc.len()].copy_from_slice(&enc);
    }

    msg!("mock: set chain={} index={}", args.chain_id, args.index);
    Ok(())
}

fn create_pda<'a>(
    payer: &AccountInfo<'a>,
    account: &AccountInfo<'a>,
    space: usize,
    owner: &Pubkey,
    seeds: &[&[u8]],
) -> ProgramResult {
    let rent = Rent::get()?;
    invoke_signed(
        &system_instruction::create_account(
            payer.key,
            account.key,
            rent.minimum_balance(space),
            space as u64,
            owner,
        ),
        &[payer.clone(), account.clone()],
        &[seeds],
    )
}
