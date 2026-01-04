use crate::cpi_impl::*;
use crate::error::BridgeError;
use crate::instruction::ReorgBlockUpdateReader;
use crate::program_pub_keys::{
    GENERIC_BUFFER_BUILDER_PROGRAM_ID, MANUAL_CLAIM_PROGRAM_ID,
    PENDING_MINT_BUFFER_BUILDER_PROGRAM_ID, TXO_BUFFER_BUILDER_PROGRAM_ID,
};
use crate::state::BridgeState;
use bytemuck::from_bytes;
use psy_bridge_core::common_types::QHash256;
use psy_bridge_core::crypto::hash::merkle::fixed_append_tree::FixedMerkleAppendTreePartialMerkleProof;
use psy_bridge_core::crypto::hash::sha256::btc_hash256_bytes;
use psy_bridge_core::crypto::zk::{CompactBridgeZKProof, CompactBridgeZKVerifierKey};
use psy_bridge_core::error::DogeBridgeError;
use psy_bridge_core::header::PsyBridgeHeader;
use psy_doge_solana_core::data_accounts::pending_mint::{
    PendingMint, PM_DA_PENDING_MINT_SIZE, PM_MAX_PENDING_MINTS_PER_GROUP_U16,
};
use psy_doge_solana_core::generic_cpi::{
    AutoClaimMintBufferAddressHelper, LockAutoClaimMintBufferCPIHelper, MintCPIHelper,
    UnlockAutoClaimMintBufferCPIHelper,
};
use psy_doge_solana_core::instructions::doge_bridge::{
    BlockUpdateFixedData, InitializeBridgeInstructionData, ProcessManualDepositInstructionData,
    ProcessWithdrawalInstructionData, RequestWithdrawalInstructionData,
    DOGE_BRIDGE_INSTRUCTION_BLOCK_UPDATE, DOGE_BRIDGE_INSTRUCTION_INITIALIZE,
    DOGE_BRIDGE_INSTRUCTION_OPERATOR_WITHDRAW_FEES, DOGE_BRIDGE_INSTRUCTION_PROCESS_MANUAL_DEPOSIT,
    DOGE_BRIDGE_INSTRUCTION_PROCESS_MINT_GROUP, DOGE_BRIDGE_INSTRUCTION_PROCESS_WITHDRAWAL,
    DOGE_BRIDGE_INSTRUCTION_REPLAY_WITHDRAWAL, DOGE_BRIDGE_INSTRUCTION_REQUEST_WITHDRAWAL,
};
use psy_doge_solana_core::instructions::doge_bridge::{
    DOGE_BRIDGE_INSTRUCTION_PROCESS_MINT_GROUP_AUTO_ADVANCE,
    DOGE_BRIDGE_INSTRUCTION_PROCESS_REORG_BLOCKS,
};
use psy_doge_solana_core::program_state::{FinalizedBlockMintTxoInfo, PsyReturnTxOutput};
use solana_program::instruction::{AccountMeta, Instruction};
use solana_program::program_error::ProgramError;
use solana_program::{
    account_info::{next_account_info, AccountInfo},
    entrypoint::ProgramResult,
    msg,
    program::invoke_signed,
    pubkey::Pubkey,
    rent::Rent,
    system_instruction,
    sysvar::{clock::Clock, Sysvar},
};

#[cfg(feature = "mock-zkp")]
use psy_bridge_core::crypto::zk::jtmb::FakeZKProof as ZKVerifier;

#[cfg(feature = "mock-zkp")]
const SINGLE_BLOCK_UPDATE_VK: CompactBridgeZKVerifierKey = [
    224, 71, 200, 255, 235, 105, 228, 248, 83, 103, 107, 62, 251, 29, 89, 254, 141, 66, 191, 70,
    35, 77, 2, 119, 194, 250, 202, 121, 98, 143, 174, 36,
];
#[cfg(feature = "mock-zkp")]
const BLOCK_REORG_VK: CompactBridgeZKVerifierKey = [
    228, 86, 252, 55, 160, 173, 2, 14, 26, 224, 246, 209, 47, 74, 197, 29, 210, 177, 146, 100, 219,
    86, 83, 85, 50, 6, 37, 144, 62, 51, 225, 225,
];
#[cfg(feature = "mock-zkp")]
const WITHDRAWAL_VK: CompactBridgeZKVerifierKey = [
    8, 226, 175, 15, 239, 184, 85, 227, 153, 188, 3, 12, 129, 135, 7, 228, 244, 252, 32, 220, 134,
    243, 114, 51, 151, 15, 18, 170, 135, 135, 20, 16,
];

#[cfg(not(feature = "mock-zkp"))]
use psy_bridge_core::crypto::zk::sp1_groth16::SP1Groth16Verifier as ZKVerifier;
#[cfg(not(feature = "mock-zkp"))]
pub const SINGLE_BLOCK_UPDATE_VK: CompactBridgeZKVerifierKey = [0u8; 32];
#[cfg(not(feature = "mock-zkp"))]
pub const BLOCK_REORG_VK: CompactBridgeZKVerifierKey = [0u8; 32];
#[cfg(not(feature = "mock-zkp"))]
pub const WITHDRAWAL_VK: CompactBridgeZKVerifierKey = [0u8; 32];

pub fn process_instruction(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    instruction_data: &[u8],
) -> ProgramResult {
    if instruction_data.is_empty() {
        return Err(BridgeError::SerializationError.into());
    }

    let discriminator = instruction_data[0];
    let data = &instruction_data[8..];
    match discriminator {
        DOGE_BRIDGE_INSTRUCTION_INITIALIZE => {
            if data.len() != std::mem::size_of::<InitializeBridgeInstructionData>() {
                return Err(BridgeError::SerializationError.into());
            }
            let params: &InitializeBridgeInstructionData = from_bytes(data);
            process_initialize(program_id, accounts, params)
        }
        DOGE_BRIDGE_INSTRUCTION_BLOCK_UPDATE => {
            if data.len() != std::mem::size_of::<BlockUpdateFixedData>() {
                return Err(BridgeError::SerializationError.into());
            }
            let fixed: &BlockUpdateFixedData = from_bytes(data);

            let auto_claim_mint_buffer_bump = instruction_data[6];
            let auto_claim_txo_buffer_bump = instruction_data[7];

            process_block_update(
                program_id,
                accounts,
                &fixed.proof,
                &fixed.header,
                auto_claim_mint_buffer_bump,
                auto_claim_txo_buffer_bump,
            )
        }
        DOGE_BRIDGE_INSTRUCTION_PROCESS_REORG_BLOCKS => {
            let reader =
                ReorgBlockUpdateReader::new(data).ok_or(BridgeError::SerializationError)?;
            let auto_claim_mint_buffer_bump = instruction_data[6];
            let auto_claim_txo_buffer_bump = instruction_data[7];

            process_reorg_blocks(
                program_id,
                accounts,
                reader.proof,
                reader.header,
                &reader.extra_finalized_blocks,
                auto_claim_mint_buffer_bump,
                auto_claim_txo_buffer_bump,
            )
        }
        DOGE_BRIDGE_INSTRUCTION_REQUEST_WITHDRAWAL => {
            if data.len() != std::mem::size_of::<RequestWithdrawalInstructionData>() {
                return Err(BridgeError::SerializationError.into());
            }
            let params: &RequestWithdrawalInstructionData = from_bytes(data);
            process_request_withdrawal(
                program_id,
                accounts,
                params.request,
                params.recipient_address,
                params.address_type,
            )
        }
        DOGE_BRIDGE_INSTRUCTION_PROCESS_WITHDRAWAL => {
            if data.len() != std::mem::size_of::<ProcessWithdrawalInstructionData>() {
                return Err(BridgeError::SerializationError.into());
            }
            let params: &ProcessWithdrawalInstructionData = from_bytes(data);
            process_process_withdrawal(
                program_id,
                accounts,
                &params.proof,
                params.new_return_output,
                params.new_spent_txo_tree_root,
                params.new_next_processed_withdrawals_index,
            )
        }
        DOGE_BRIDGE_INSTRUCTION_OPERATOR_WITHDRAW_FEES => {
            process_operator_withdraw_fees(program_id, accounts)
        }
        DOGE_BRIDGE_INSTRUCTION_PROCESS_MANUAL_DEPOSIT => {
            if data.len() != std::mem::size_of::<ProcessManualDepositInstructionData>() {
                return Err(BridgeError::SerializationError.into());
            }
            let params: &ProcessManualDepositInstructionData = from_bytes(data);
            process_process_manual_deposit(
                program_id,
                accounts,
                params.tx_hash,
                params.recent_block_merkle_tree_root,
                params.recent_auto_claim_txo_root,
                params.combined_txo_index,
                params.deposit_amount_sats,
                params.depositor_solana_public_key,
            )
        }
        DOGE_BRIDGE_INSTRUCTION_REPLAY_WITHDRAWAL => {
            process_replay_withdrawal(program_id, accounts)
        }
        DOGE_BRIDGE_INSTRUCTION_PROCESS_MINT_GROUP => {
            if data.len() != 4 {
                return Err(BridgeError::SerializationError.into());
            }
            let group_index = u16::from_le_bytes(data[0..2].try_into().unwrap());
            let mint_buffer_pda_bump = data[2];
            let should_unlock = data[3] != 0;
            process_process_mint_group(
                program_id,
                accounts,
                group_index,
                mint_buffer_pda_bump,
                should_unlock,
            )
        }
        DOGE_BRIDGE_INSTRUCTION_PROCESS_MINT_GROUP_AUTO_ADVANCE => {
            if data.len() != 4 {
                return Err(BridgeError::SerializationError.into());
            }
            let group_index = u16::from_le_bytes(data[0..2].try_into().unwrap());
            let mint_buffer_pda_bump = data[2];
            let txo_buffer_pda_bump = instruction_data[7];
            let should_unlock = data[3] != 0;

            process_process_mint_group_auto_advance(
                program_id,
                accounts,
                group_index,
                mint_buffer_pda_bump,
                txo_buffer_pda_bump,
                should_unlock,
            )
        }
        _ => Err(BridgeError::SerializationError.into()),
    }
}

fn process_initialize(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    initialize_instruction: &InitializeBridgeInstructionData,
) -> ProgramResult {
    let account_info_iter = &mut accounts.iter();
    let bridge_state_account = next_account_info(account_info_iter)?;
    let payer = next_account_info(account_info_iter)?;
    let system_program = next_account_info(account_info_iter)?;
    let _rent_sysvar = next_account_info(account_info_iter).ok();

    let (pda, bump) = Pubkey::find_program_address(&[b"bridge_state"], program_id);
    if pda != *bridge_state_account.key {
        return Err(BridgeError::InvalidPDA.into());
    }

    if bridge_state_account.data_len() == 0 {
        let space = BridgeState::SIZE;
        let rent = Rent::get()?.minimum_balance(space);

        invoke_signed(
            &system_instruction::create_account(
                payer.key,
                bridge_state_account.key,
                rent,
                space as u64,
                program_id,
            ),
            &[
                payer.clone(),
                bridge_state_account.clone(),
                system_program.clone(),
            ],
            &[&[b"bridge_state", &[bump]]],
        )?;
    }
    let mut data = bridge_state_account.try_borrow_mut_data()?;

    let bridge_state = bytemuck::try_from_bytes_mut::<BridgeState>(&mut data)
        .map_err(|_| BridgeError::SerializationError)?;
    bridge_state.doge_mint = initialize_instruction.doge_mint;
    bridge_state.core_state.initialize(initialize_instruction);

    Ok(())
}

fn process_block_update(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    proof: &CompactBridgeZKProof,
    new_header: &PsyBridgeHeader,
    auto_claim_mint_buffer_bump: u8,
    auto_claim_txo_buffer_bump: u8,
) -> ProgramResult {
    // Standard single block update - expects no extra blocks
    let extra_finalized_blocks = &[];

    run_block_update_common(
        program_id,
        accounts,
        proof,
        new_header,
        extra_finalized_blocks,
        auto_claim_mint_buffer_bump,
        auto_claim_txo_buffer_bump,
        false, // is_reorg
    )
}

fn process_reorg_blocks(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    proof: &CompactBridgeZKProof,
    new_header: &PsyBridgeHeader,
    extra_finalized_blocks: &[&FinalizedBlockMintTxoInfo],
    auto_claim_mint_buffer_bump: u8,
    auto_claim_txo_buffer_bump: u8,
) -> ProgramResult {
    // Process reorg update
    run_block_update_common(
        program_id,
        accounts,
        proof,
        new_header,
        extra_finalized_blocks,
        auto_claim_mint_buffer_bump,
        auto_claim_txo_buffer_bump,
        true, // is_reorg
    )
}

fn run_block_update_common(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    proof: &CompactBridgeZKProof,
    new_header: &PsyBridgeHeader,
    extra_finalized_blocks: &[&FinalizedBlockMintTxoInfo],
    auto_claim_mint_buffer_bump: u8,
    auto_claim_txo_buffer_bump: u8,
    is_reorg: bool,
) -> ProgramResult {
    let account_info_iter = &mut accounts.iter();
    let bridge_state_account = next_account_info(account_info_iter)?;
    let auto_claim_mint_buffer = next_account_info(account_info_iter)?;
    let auto_claim_txo_buffer = next_account_info(account_info_iter)?;
    let operator = next_account_info(account_info_iter)?;
    let _payer = next_account_info(account_info_iter)?;

    // Consume Program accounts
    let mint_buffer_program_account = next_account_info(account_info_iter)?;
    let _txo_buffer_program_account = next_account_info(account_info_iter)?;

    if !operator.is_signer {
        return Err(solana_program::program_error::ProgramError::MissingRequiredSignature);
    }

    // Verify Mint Buffer PDA
    {
        let expected = Pubkey::create_program_address(
            &[
                b"mint_buffer",
                operator.key.as_ref(),
                &[auto_claim_mint_buffer_bump],
            ],
            &PENDING_MINT_BUFFER_BUILDER_PROGRAM_ID,
        )
        .map_err(|_| ProgramError::InvalidSeeds)?;
        if auto_claim_mint_buffer.key != &expected {
            return Err(DogeBridgeError::InvalidMintBufferPDA.into());
        }
    }

    // Verify TXO Buffer PDA
    {
        let expected = Pubkey::create_program_address(
            &[
                b"txo_buffer",
                operator.key.as_ref(),
                &[auto_claim_txo_buffer_bump],
            ],
            &TXO_BUFFER_BUILDER_PROGRAM_ID,
        )
        .map_err(|_| ProgramError::InvalidSeeds)?;
        if auto_claim_txo_buffer.key != &expected {
            return Err(DogeBridgeError::InvalidTxoBufferPDA.into());
        }
    }

    let (_bridge_pda, bump) = Pubkey::find_program_address(&[b"bridge_state"], program_id);
    let seeds = &[b"bridge_state", &[bump][..]];

    let mint_buffer_locker_account_pubkey = {
        let mint_buffer_locker = SolanaMintBufferLocker {
            buffer_program_key: &PENDING_MINT_BUFFER_BUILDER_PROGRAM_ID,
            is_valid_pda: true,
            buffer_account: auto_claim_mint_buffer,
            buffer_program_account: mint_buffer_program_account,
            authority_info: bridge_state_account,
            authority_seeds: seeds,
        };
        // Verify owner
        if auto_claim_mint_buffer.owner != &PENDING_MINT_BUFFER_BUILDER_PROGRAM_ID {
            return Err(DogeBridgeError::InvalidMintBufferPdaProgram.into());
        }

        mint_buffer_locker.lock_buffer()?;
        mint_buffer_locker.get_mint_buffer_program_address()
    };

    let mut data = bridge_state_account.try_borrow_mut_data()?;
    let bridge_state = bytemuck::try_from_bytes_mut::<BridgeState>(&mut data)
        .map_err(|_| BridgeError::SerializationError)?;

    let self_pubkey_bytes = bridge_state_account.key.to_bytes();

    if auto_claim_txo_buffer.owner != &TXO_BUFFER_BUILDER_PROGRAM_ID {
        return Err(ProgramError::IllegalOwner);
    }

    let mint_buffer_data = auto_claim_mint_buffer.try_borrow_data()?;
    let txo_buffer_data = auto_claim_txo_buffer.try_borrow_data()?;

    let old_index = bridge_state
        .core_state
        .bridge_header
        .finalized_state
        .auto_claimed_deposits_next_index;

    if is_reorg {
        bridge_state
            .core_state
            .run_block_transition_reorg::<ZKVerifier>(
                proof,
                &BLOCK_REORG_VK,
                new_header,
                extra_finalized_blocks,
                &self_pubkey_bytes,
                mint_buffer_locker_account_pubkey,
                &txo_buffer_data,
                &mint_buffer_data,
            )?;
    } else {
        let res = bridge_state
            .core_state
            .run_standard_single_block_transition::<ZKVerifier>(
                proof,
                &SINGLE_BLOCK_UPDATE_VK,
                new_header,
                &self_pubkey_bytes,
                mint_buffer_locker_account_pubkey,
                &txo_buffer_data,
                &mint_buffer_data,
            );
        if res.is_err() {
            msg!("Error during single block transition: {:?}", res.err());
            return Err(res.err().unwrap().into());
        }
    }

    let new_index = bridge_state
        .core_state
        .bridge_header
        .finalized_state
        .auto_claimed_deposits_next_index;

    // Drop borrows
    let _ = bridge_state;
    drop(data);
    drop(mint_buffer_data);
    drop(txo_buffer_data);

    if old_index == new_index {
        // Unlock if no new mints
        let mint_buffer_locker = SolanaMintBufferLocker {
            buffer_program_key: &PENDING_MINT_BUFFER_BUILDER_PROGRAM_ID,
            is_valid_pda: true,
            buffer_account: auto_claim_mint_buffer,
            buffer_program_account: mint_buffer_program_account,
            authority_info: bridge_state_account,
            authority_seeds: seeds,
        };
        mint_buffer_locker.unlock_buffer(&PENDING_MINT_BUFFER_BUILDER_PROGRAM_ID.to_bytes())?;
    }

    Ok(())
}

fn process_process_mint_group(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    mint_group_index: u16,
    _mint_buffer_pda_bump: u8,
    should_unlock: bool,
) -> ProgramResult {
    let account_info_iter = &mut accounts.iter();

    let bridge_state_account = next_account_info(account_info_iter)?;
    let auto_claim_mint_buffer = next_account_info(account_info_iter)?;
    let operator = next_account_info(account_info_iter)?;
    let doge_mint = next_account_info(account_info_iter)?;
    let _payer = next_account_info(account_info_iter)?;
    let mint_buffer_program_account = next_account_info(account_info_iter)?;
    let token_program = next_account_info(account_info_iter)?;

    if !operator.is_signer {
        return Err(solana_program::program_error::ProgramError::MissingRequiredSignature);
    }
    let (_bridge_pda, bump) = Pubkey::find_program_address(&[b"bridge_state"], program_id);
    let seeds = &[b"bridge_state", &[bump][..]];

    let mut data = bridge_state_account.try_borrow_mut_data()?;
    let bridge_state = bytemuck::try_from_bytes_mut::<BridgeState>(&mut data)
        .map_err(|_| BridgeError::SerializationError)?;

    if auto_claim_mint_buffer.key.to_bytes()
        != bridge_state
            .core_state
            .pending_mint_txos
            .current_pending_mints_tracker
            .last_finalized_auto_claim_mints_storage_account
    {
        return Err(DogeBridgeError::InvalidAccountKey.into());
    }

    if doge_mint.key.to_bytes() != bridge_state.doge_mint {
        return Err(BridgeError::InvalidAccountInput.into());
    }

    if operator.key.to_bytes() != bridge_state.core_state.access_control.operator_pubkey {
        return Err(solana_program::program_error::ProgramError::MissingRequiredSignature);
    }

    let (can_unlock, mints_count, start_offset) = bridge_state
        .core_state
        .run_auto_mint_group_precheck(mint_group_index, &auto_claim_mint_buffer.key.to_bytes())?;

    if can_unlock != should_unlock {
        if should_unlock {
            return Err(DogeBridgeError::AttemptedUnlockPendingMintBuffer.into());
        } else {
            return Err(DogeBridgeError::FailedUnlockPendingMintBuffer.into());
        }
    }

    let _ = bridge_state;
    drop(data);

    let recipients_slice = &accounts[7..];
    if mints_count as usize != recipients_slice.len() {
        return Err(BridgeError::InvalidAccountInput.into());
    }

    let minter = SolanaMinter {
        mint: doge_mint,
        authority_info: bridge_state_account,
        authority_seeds: seeds,
        recipient_map: recipients_slice,
        token_program,
    };

    let auto_claim_mint_buffer_data = auto_claim_mint_buffer.try_borrow_data()?;

    for p in 0..mints_count {
        let offset = start_offset + p as usize * PM_DA_PENDING_MINT_SIZE;
        let pending_mint: &PendingMint = bytemuck::from_bytes(
            &auto_claim_mint_buffer_data[offset..(offset + PM_DA_PENDING_MINT_SIZE)],
        );
        minter.mint_to(p as usize, &pending_mint.recipient, pending_mint.amount)?;
    }

    drop(auto_claim_mint_buffer_data);

    if should_unlock {
        let mint_buffer_locker = SolanaMintBufferLocker {
            buffer_program_key: &PENDING_MINT_BUFFER_BUILDER_PROGRAM_ID,
            is_valid_pda: true,
            buffer_account: auto_claim_mint_buffer,
            buffer_program_account: mint_buffer_program_account,
            authority_info: bridge_state_account,
            authority_seeds: seeds,
        };
        mint_buffer_locker.unlock_buffer(&PENDING_MINT_BUFFER_BUILDER_PROGRAM_ID.to_bytes())?;
    }

    Ok(())
}

fn process_process_mint_group_auto_advance(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    mint_group_index: u16,
    _mint_buffer_pda_bump: u8,
    txo_buffer_pda_bump: u8,
    should_unlock: bool,
) -> ProgramResult {
    let account_info_iter = &mut accounts.iter();

    let bridge_state_account = next_account_info(account_info_iter)?;
    let auto_claim_mint_buffer = next_account_info(account_info_iter)?;
    let auto_claim_txo_buffer = next_account_info(account_info_iter)?;
    let operator = next_account_info(account_info_iter)?;
    let doge_mint = next_account_info(account_info_iter)?;
    let _payer = next_account_info(account_info_iter)?;
    let mint_buffer_program_account = next_account_info(account_info_iter)?;
    let _txo_buffer_program_account = next_account_info(account_info_iter)?;
    let token_program = next_account_info(account_info_iter)?;

    if !operator.is_signer {
        return Err(solana_program::program_error::ProgramError::MissingRequiredSignature);
    }
    let (_bridge_pda, bump) = Pubkey::find_program_address(&[b"bridge_state"], program_id);
    let seeds = &[b"bridge_state", &[bump][..]];

    let mint_buffer_locker = SolanaMintBufferLocker {
        buffer_program_key: &PENDING_MINT_BUFFER_BUILDER_PROGRAM_ID,
        is_valid_pda: true,
        buffer_account: auto_claim_mint_buffer,
        buffer_program_account: mint_buffer_program_account,
        authority_info: bridge_state_account,
        authority_seeds: seeds,
    };

    let mut data = bridge_state_account.try_borrow_mut_data()?;
    let bridge_state = bytemuck::try_from_bytes_mut::<BridgeState>(&mut data)
        .map_err(|_| BridgeError::SerializationError)?;

    let advance_with_jit = bridge_state
        .core_state
        .pending_mint_txos
        .current_pending_mints_tracker
        .is_empty()
        && !bridge_state.core_state.pending_mint_txos.is_empty();
    // JIT: Advance State if currently empty but has backlog
    if advance_with_jit {
        // Verify TXO Owner
        if auto_claim_txo_buffer.owner != &TXO_BUFFER_BUILDER_PROGRAM_ID {
            return Err(ProgramError::IllegalOwner);
        }

        // Verify TXO PDA
        let expected_txo = Pubkey::create_program_address(
            &[b"txo_buffer", operator.key.as_ref(), &[txo_buffer_pda_bump]],
            &TXO_BUFFER_BUILDER_PROGRAM_ID,
        )
        .map_err(|_| ProgramError::InvalidSeeds)?;

        if auto_claim_txo_buffer.key != &expected_txo {
            return Err(DogeBridgeError::InvalidTxoBufferPDA.into());
        }

        let txo_data = auto_claim_txo_buffer.try_borrow_data()?;
        let mint_data = auto_claim_mint_buffer.try_borrow_data()?;

        bridge_state.core_state.run_setup_next_pending_buffer(
            &bridge_state_account.key.to_bytes(),
            mint_buffer_locker.get_mint_buffer_program_address(),
            &txo_data,
            &mint_data,
        )?;

        drop(txo_data);
        drop(mint_data);
    }

    // Now validate active buffer matches input
    if auto_claim_mint_buffer.key.to_bytes()
        != bridge_state
            .core_state
            .pending_mint_txos
            .current_pending_mints_tracker
            .last_finalized_auto_claim_mints_storage_account
    {
        return Err(DogeBridgeError::InvalidAccountKey.into());
    }

    let (can_unlock, mints_count, start_offset) = bridge_state
        .core_state
        .run_auto_mint_group_precheck(mint_group_index, &auto_claim_mint_buffer.key.to_bytes())?;

    if can_unlock != should_unlock {
        if should_unlock {
            return Err(DogeBridgeError::AttemptedUnlockPendingMintBuffer.into());
        } else {
            return Err(DogeBridgeError::FailedUnlockPendingMintBuffer.into());
        }
    }

    let _ = bridge_state;
    drop(data);

    let recipients_slice = &accounts[9..]; // Offset by 9 fixed accounts
    if mints_count as usize != recipients_slice.len() {
        return Err(BridgeError::InvalidAccountInput.into());
    }

    let minter = SolanaMinter {
        mint: doge_mint,
        authority_info: bridge_state_account,
        authority_seeds: seeds,
        recipient_map: recipients_slice,
        token_program,
    };

    if advance_with_jit {
        mint_buffer_locker.lock_buffer()?;
    }
    let auto_claim_mint_buffer_data = auto_claim_mint_buffer.try_borrow_data()?;

    for p in 0..mints_count {
        let offset = start_offset + p as usize * PM_DA_PENDING_MINT_SIZE;
        let pending_mint: &PendingMint = bytemuck::from_bytes(
            &auto_claim_mint_buffer_data[offset..(offset + PM_DA_PENDING_MINT_SIZE)],
        );
        minter.mint_to(p as usize, &pending_mint.recipient, pending_mint.amount)?;
    }

    drop(auto_claim_mint_buffer_data);

    if should_unlock {
        if advance_with_jit && mints_count > PM_MAX_PENDING_MINTS_PER_GROUP_U16 {
            return Err(DogeBridgeError::CannotUnlockAfterAutoAdvance.into());
        }
        // Note: mint_buffer_locker created earlier borrows authority_info (bridge_state)
        // We dropped `data` borrow but `bridge_state_account` is still valid.
        mint_buffer_locker.unlock_buffer(&PENDING_MINT_BUFFER_BUILDER_PROGRAM_ID.to_bytes())?;
    }

    Ok(())
}

fn process_request_withdrawal(
    _program_id: &Pubkey,
    accounts: &[AccountInfo],
    request: psy_doge_solana_core::program_state::PsyWithdrawalRequest,
    recipient_address: [u8; 20],
    address_type: u32,
) -> ProgramResult {
    let account_info_iter = &mut accounts.iter();
    let bridge_state_account = next_account_info(account_info_iter)?;
    let user_token_account = next_account_info(account_info_iter)?;
    let doge_mint = next_account_info(account_info_iter)?;
    let user_authority = next_account_info(account_info_iter)?;
    let token_program = next_account_info(account_info_iter)?;

    let mut data = bridge_state_account.try_borrow_mut_data()?;
    let bridge_state = bytemuck::try_from_bytes_mut::<BridgeState>(&mut data)
        .map_err(|_| BridgeError::SerializationError)?;

    let burner = SolanaBurner {
        mint: doge_mint,
        user_token_account,
        authority: user_authority,
        token_program,
    };

    bridge_state.core_state.request_withdrawal(
        &burner,
        &user_authority.key.to_bytes(),
        &request,
        recipient_address,
        address_type,
        request.amount_sats,
    )?;

    Ok(())
}

// Wormhole VAA Discriminator: sha256("global:post_message")[:8]
const WORMHOLE_VAA_DISCRIMINATOR: [u8; 8] = [214, 50, 100, 209, 38, 34, 7, 76];
/// Sends a VAA via the Wormhole Shim program using a CPI call.
/// This function manually constructs the Anchor instruction and invokes it.
fn send_wormhole_vaa<'a>(
    withdrawal_nonce: u32,
    shim_program: &AccountInfo<'a>,
    bridge_config: &AccountInfo<'a>,
    message: &AccountInfo<'a>,
    emitter: &AccountInfo<'a>,
    sequence: &AccountInfo<'a>,
    payer: &AccountInfo<'a>,
    fee_collector: &AccountInfo<'a>,
    clock: &AccountInfo<'a>,
    system_program: &AccountInfo<'a>,
    core_bridge_program: &AccountInfo<'a>,
    event_authority: &AccountInfo<'a>,
    emitter_seeds: &[&[u8]],
    sighash: &[u8],
    transaction_buffer: &[u8],
) -> ProgramResult {
    let payload_len = sighash.len() + transaction_buffer.len();
    let consistency_level: u8 = 1; // 1 = Finalized

    let mut ix_data = Vec::with_capacity(8 + 4 + 1 + 4 + payload_len);
    ix_data.extend_from_slice(&WORMHOLE_VAA_DISCRIMINATOR);
    ix_data.extend_from_slice(&withdrawal_nonce.to_le_bytes());
    ix_data.push(consistency_level);
    ix_data.extend_from_slice(&(payload_len as u32).to_le_bytes());
    ix_data.extend_from_slice(sighash);
    ix_data.extend_from_slice(transaction_buffer);

    let accounts = vec![
        AccountMeta::new(*bridge_config.key, false),
        AccountMeta::new(*message.key, false),
        AccountMeta::new_readonly(*emitter.key, true),
        AccountMeta::new(*sequence.key, false),
        AccountMeta::new(*payer.key, true),
        AccountMeta::new(*fee_collector.key, false),
        AccountMeta::new_readonly(*clock.key, false),
        AccountMeta::new_readonly(*system_program.key, false),
        AccountMeta::new_readonly(*core_bridge_program.key, false),
        AccountMeta::new_readonly(*event_authority.key, false),
    ];

    let instruction = Instruction {
        program_id: *shim_program.key,
        accounts,
        data: ix_data,
    };

    #[cfg(feature = "wormhole")]
    invoke_signed(
        &instruction,
        &[
            bridge_config.clone(),
            message.clone(),
            emitter.clone(),
            sequence.clone(),
            payer.clone(),
            fee_collector.clone(),
            clock.clone(),
            system_program.clone(),
            core_bridge_program.clone(),
            event_authority.clone(),
        ],
        &[emitter_seeds],
    )?;
    msg!("Wormhole VAA sent via CPI: {}, seeds: {:?}", instruction.data.len(), emitter_seeds);

    Ok(())
}
fn process_process_withdrawal(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    proof: &CompactBridgeZKProof,
    new_return_output: PsyReturnTxOutput,
    new_spent_txo_tree_root: psy_bridge_core::common_types::QHash256,
    new_next_processed_withdrawals_index: u64,
) -> ProgramResult {
    let account_info_iter = &mut accounts.iter();

    // Core Accounts
    let bridge_state_account = next_account_info(account_info_iter)?;
    let doge_tx_buffer = next_account_info(account_info_iter)?;

    // Wormhole Shim Accounts (Must match client order)
    let shim_program_id = next_account_info(account_info_iter)?;
    let bridge_config = next_account_info(account_info_iter)?;
    let message = next_account_info(account_info_iter)?;
    let sequence = next_account_info(account_info_iter)?;
    let payer = next_account_info(account_info_iter)?;
    let fee_collector = next_account_info(account_info_iter)?;
    let clock = next_account_info(account_info_iter)?;
    let system_program = next_account_info(account_info_iter)?;
    let core_bridge_program = next_account_info(account_info_iter)?;
    let event_authority = next_account_info(account_info_iter)?;

    // Verify Bridge State PDA
    let (bridge_pda, bump) = Pubkey::find_program_address(&[b"bridge_state"], program_id);
    if bridge_state_account.key != &bridge_pda {
        return Err(BridgeError::InvalidPDA.into());
    }
    // We need the seeds to sign as the Emitter
    let seeds = &[b"bridge_state", &[bump][..]];

    // Verify Generic Buffer and Read Large Data
    if doge_tx_buffer.owner != &GENERIC_BUFFER_BUILDER_PROGRAM_ID {
        return Err(solana_program::program_error::ProgramError::IllegalOwner);
    }

    if &bridge_pda != bridge_state_account.key {
        return Err(BridgeError::InvalidPDA.into());
    }

    let mut data = bridge_state_account.try_borrow_mut_data()?;
    let bridge_state = bytemuck::try_from_bytes_mut::<BridgeState>(&mut data)
        .map_err(|_| BridgeError::SerializationError)?;

    // make sure to check that all the accounts are correct and owned by the right programs

    let dogecoin_tx = doge_tx_buffer.try_borrow_data()?;
    if dogecoin_tx.len() < 32 {
        return Err(BridgeError::InvalidAccountInput.into());
    }
    let tx_data = &dogecoin_tx[32..];

    let sighash = bridge_state
        .core_state
        .run_process_bridge_withdrawal::<ZKVerifier>(
            proof,
            &WITHDRAWAL_VK,
            tx_data,
            new_return_output,
            new_spent_txo_tree_root,
            new_next_processed_withdrawals_index,
        )?;
    msg!("requesting_sighash: {:?}", sighash);

    let nonce = (bridge_state.core_state.next_processed_withdrawals_index & 0xFFFFFFFF) as u32;
    
    drop(data);
    send_wormhole_vaa(
        nonce,
        shim_program_id,
        bridge_config,
        message,
        bridge_state_account,
        sequence,
        payer,
        fee_collector,
        clock,
        system_program,
        core_bridge_program,
        event_authority,
        seeds,
        &sighash,
        &dogecoin_tx,
    )?;

    Ok(())
}

fn process_operator_withdraw_fees(program_id: &Pubkey, accounts: &[AccountInfo]) -> ProgramResult {
    let account_info_iter = &mut accounts.iter();
    let bridge_state_account = next_account_info(account_info_iter)?;
    let operator_token_account = next_account_info(account_info_iter)?;
    let doge_mint = next_account_info(account_info_iter)?;
    let operator = next_account_info(account_info_iter)?;
    let token_program = next_account_info(account_info_iter)?;

    if !operator.is_signer {
        return Err(solana_program::program_error::ProgramError::MissingRequiredSignature);
    }

    let (_bridge_pda, bump) = Pubkey::find_program_address(&[b"bridge_state"], program_id);
    let seeds = &[b"bridge_state", &[bump][..]];

    let recipient_map = &[operator_token_account.clone()];

    let minter = SolanaMinter {
        mint: doge_mint,
        authority_info: bridge_state_account,
        authority_seeds: seeds,
        recipient_map,
        token_program,
    };

    let fees_to_withdraw = {
        let mut data = bridge_state_account.try_borrow_mut_data()?;
        let bridge_state = bytemuck::try_from_bytes_mut::<BridgeState>(&mut data)
            .map_err(|_| BridgeError::SerializationError)?;

        if doge_mint.key.to_bytes() != bridge_state.doge_mint {
            return Err(BridgeError::InvalidAccountInput.into());
        }

        if operator.key.to_bytes() != bridge_state.core_state.access_control.operator_pubkey {
            return Err(solana_program::program_error::ProgramError::MissingRequiredSignature);
        }

        bridge_state
            .core_state
            .run_bridge_operator_withdraw_fees_precheck()?
    };

    minter.mint_to(0, &operator_token_account.key.to_bytes(), fees_to_withdraw)?;

    Ok(())
}

fn process_process_manual_deposit(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    tx_hash: QHash256,
    recent_block_merkle_tree_root: QHash256,
    recent_auto_claim_txo_root: QHash256,
    combined_txo_index: u64,
    deposit_amount_sats: u64,
    depositor_solana_public_key: [u8; 32],
) -> ProgramResult {
    let account_info_iter = &mut accounts.iter();
    let bridge_state_account = next_account_info(account_info_iter)?;
    let recipient_account = next_account_info(account_info_iter)?;
    let doge_mint = next_account_info(account_info_iter)?;
    let token_program = next_account_info(account_info_iter)?;
    let manual_claim_program_signer = next_account_info(account_info_iter)?;

    if !manual_claim_program_signer.is_signer {
        return Err(solana_program::program_error::ProgramError::MissingRequiredSignature);
    }
    verify_lowest_pda(
        b"manual-claim",
        manual_claim_program_signer.key,
        &MANUAL_CLAIM_PROGRAM_ID,
        &Pubkey::new_from_array(depositor_solana_public_key),
    )?;

    let mint_amount = {
        let mut data = bridge_state_account.try_borrow_mut_data()?;
        let bridge_state = bytemuck::try_from_bytes_mut::<BridgeState>(&mut data)
            .map_err(|_| BridgeError::SerializationError)?;

        bridge_state.core_state.process_manual_claimed_deposit(
            tx_hash,
            recent_block_merkle_tree_root,
            recent_auto_claim_txo_root,
            combined_txo_index,
            &depositor_solana_public_key,
            deposit_amount_sats,
        )?
    };

    let (_bridge_pda, bump) = Pubkey::find_program_address(&[b"bridge_state"], program_id);
    let seeds = &[b"bridge_state", &[bump][..]];

    let recipient_map = &[recipient_account.clone()];

    let minter = SolanaMinter {
        mint: doge_mint,
        authority_info: bridge_state_account,
        authority_seeds: seeds,
        recipient_map,
        token_program,
    };

    minter.mint_to(0, &depositor_solana_public_key, mint_amount)?;

    Ok(())
}

fn process_replay_withdrawal(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
) -> ProgramResult {
    let account_info_iter = &mut accounts.iter();

    // Core Accounts
    let bridge_state_account = next_account_info(account_info_iter)?;
    let doge_tx_buffer = next_account_info(account_info_iter)?;

    // Wormhole Shim Accounts (Must match client order)
    let shim_program_id = next_account_info(account_info_iter)?;
    let bridge_config = next_account_info(account_info_iter)?;
    let message = next_account_info(account_info_iter)?;
    let sequence = next_account_info(account_info_iter)?;
    let payer = next_account_info(account_info_iter)?;
    let fee_collector = next_account_info(account_info_iter)?;
    let clock = next_account_info(account_info_iter)?;
    let system_program = next_account_info(account_info_iter)?;
    let core_bridge_program = next_account_info(account_info_iter)?;
    let event_authority = next_account_info(account_info_iter)?;

    // Verify Bridge State PDA
    let (bridge_pda, bump) = Pubkey::find_program_address(&[b"bridge_state"], program_id);
    if bridge_state_account.key != &bridge_pda {
        return Err(BridgeError::InvalidPDA.into());
    }
    // We need the seeds to sign as the Emitter
    let seeds = &[b"bridge_state", &[bump][..]];

    // Verify Generic Buffer and Read Large Data
    if doge_tx_buffer.owner != &GENERIC_BUFFER_BUILDER_PROGRAM_ID {
        return Err(solana_program::program_error::ProgramError::IllegalOwner);
    }

    if &bridge_pda != bridge_state_account.key {
        return Err(BridgeError::InvalidPDA.into());
    }

    let mut data = bridge_state_account.try_borrow_mut_data()?;
    let bridge_state = bytemuck::try_from_bytes_mut::<BridgeState>(&mut data)
        .map_err(|_| BridgeError::SerializationError)?;

    // make sure to check that all the accounts are correct and owned by the right programs

    let proof_and_dogecoin_tx = doge_tx_buffer.try_borrow_data()?;
    if proof_and_dogecoin_tx.len() < 32 + std::mem::size_of::<FixedMerkleAppendTreePartialMerkleProof>() + 10 {
        return Err(BridgeError::InvalidAccountInput.into());
    }
    let proof: &FixedMerkleAppendTreePartialMerkleProof = bytemuck::from_bytes(&proof_and_dogecoin_tx[32..32 + std::mem::size_of::<FixedMerkleAppendTreePartialMerkleProof>()]);
    let tx_data = &proof_and_dogecoin_tx[32 + std::mem::size_of::<FixedMerkleAppendTreePartialMerkleProof>()..];

    let sighash = btc_hash256_bytes(&tx_data);
    let mut current_timestamp = (Clock::get()?.unix_timestamp & 0xFFFFFFFFi64) as u32;

    if bridge_state.core_state.last_processed_withdrawals_at_ms == current_timestamp as u64 {
        current_timestamp = current_timestamp.wrapping_add(1);
    }

    if proof.value != sighash {
        msg!("Provided transaction data does not match proof value");
        return Err(BridgeError::InvalidAccountInput.into());
    }

    



    if bridge_state
        .core_state
        .process_replay_withdrawal_proof(proof, current_timestamp)
    {
        let nonce = (bridge_state.core_state.next_processed_withdrawals_index & 0xFFFFFFFF) as u32;
        
        msg!("requesting_sighash: {:?}", sighash);    
        drop(data);
        send_wormhole_vaa(
            nonce,
            shim_program_id,
            bridge_config,
            message,
            bridge_state_account,
            sequence,
            payer,
            fee_collector,
            clock,
            system_program,
            core_bridge_program,
            event_authority,
            seeds,
            &sighash,
            &tx_data,
        )?;
        

    } else {
        msg!("Replay withdrawal failed validation or too soon");
        return Err(BridgeError::CoreError.into());
    }

    Ok(())
}
