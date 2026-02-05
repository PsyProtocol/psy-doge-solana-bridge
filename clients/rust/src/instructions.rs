use psy_doge_solana_core::instructions::manual_claim::{MC_MANUAL_CLAIM_TRANSACTION_DESCRIMINATOR, ManualClaimInstruction};
use psy_bridge_core::{common_types::QHash256, crypto::zk::CompactBridgeZKProof, header::PsyBridgeHeader};
use psy_doge_solana_core::program_state::{FinalizedBlockMintTxoInfo, PsyReturnTxOutput, PsyWithdrawalRequest};
use psy_doge_solana_core::instructions::doge_bridge::{BlockUpdateFixedData, DOGE_BRIDGE_INSTRUCTION_BLOCK_UPDATE, DOGE_BRIDGE_INSTRUCTION_INITIALIZE, DOGE_BRIDGE_INSTRUCTION_OPERATOR_WITHDRAW_FEES, DOGE_BRIDGE_INSTRUCTION_PROCESS_MANUAL_DEPOSIT, DOGE_BRIDGE_INSTRUCTION_PROCESS_MINT_GROUP, DOGE_BRIDGE_INSTRUCTION_PROCESS_MINT_GROUP_AUTO_ADVANCE, DOGE_BRIDGE_INSTRUCTION_PROCESS_REORG_BLOCKS, DOGE_BRIDGE_INSTRUCTION_PROCESS_WITHDRAWAL, DOGE_BRIDGE_INSTRUCTION_REPLAY_WITHDRAWAL, DOGE_BRIDGE_INSTRUCTION_REQUEST_WITHDRAWAL, DOGE_BRIDGE_INSTRUCTION_SNAPSHOT_WITHDRAWALS, InitializeBridgeInstructionData, InitializeBridgeParams, ProcessManualDepositInstructionData, ProcessReorgBlocksFixedData, ProcessWithdrawalInstructionData, RequestWithdrawalInstructionData};
use solana_sdk::sysvar::clock;
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    system_program,
};

use crate::constants::{DOGE_BRIDGE_PROGRAM_ID, PENDING_MINT_BUFFER_BUILDER_PROGRAM_ID, TXO_BUFFER_BUILDER_PROGRAM_ID};

pub fn gen_aligned_instruction(instruction_discriminator: u8, data_struct_bytes: &[u8]) -> Vec<u8> {
    let mut data = vec![instruction_discriminator; 8];
    data.extend_from_slice(data_struct_bytes);
    data
}
pub fn gen_aligned_instruction_with_bumps(
    instruction_discriminator: u8,
    data_struct_bytes: &[u8],
    bumps: &[u8],
) -> Vec<u8> {
    if bumps.len() >= 7 {
        panic!("Bumps length exceeds maximum allowed size of 7");
    }

    let mut data = vec![instruction_discriminator; 8 - bumps.len()];
    data.extend_from_slice(bumps);
    data.extend_from_slice(data_struct_bytes);
    data
}

pub fn initialize_bridge(
    payer: Pubkey,
    operator_pubkey: Pubkey,
    fee_spender_pubkey: Pubkey,
    doge_mint: Pubkey,
    initialize_bridge_params: &InitializeBridgeParams,
) -> Instruction {
    let (bridge_state, _) = Pubkey::find_program_address(&[b"bridge_state"], &DOGE_BRIDGE_PROGRAM_ID);

    let data_struct = InitializeBridgeInstructionData {
        operator_pubkey: operator_pubkey.to_bytes(),
        fee_spender_pubkey: fee_spender_pubkey.to_bytes(),
        doge_mint: doge_mint.to_bytes(),
        bridge_header: initialize_bridge_params.bridge_header,
        start_return_txo_output: initialize_bridge_params.start_return_txo_output,
        config_params: initialize_bridge_params.config_params,
        custodian_wallet_config_hash: initialize_bridge_params.custodian_wallet_config_hash,
    };
    let data = gen_aligned_instruction(
        DOGE_BRIDGE_INSTRUCTION_INITIALIZE,
        bytemuck::bytes_of(&data_struct),
    );
    
    Instruction {
        program_id: DOGE_BRIDGE_PROGRAM_ID,
        accounts: vec![
            AccountMeta::new(bridge_state, false),
            AccountMeta::new(payer, true),
            AccountMeta::new_readonly(system_program::id(), false),
        ],
        data,
    }
}

pub fn block_update(
    program_id: Pubkey,
    payer: Pubkey,
    proof: CompactBridgeZKProof,
    header: PsyBridgeHeader,
    operator: Pubkey,
    mint_buffer: Pubkey,
    txo_buffer: Pubkey,
    mint_buffer_bump: u8,
    txo_buffer_bump: u8,
) -> Instruction {
    let (bridge_state, _) = Pubkey::find_program_address(&[b"bridge_state"], &program_id);

    let fixed = BlockUpdateFixedData { proof, header };
    let data = gen_aligned_instruction_with_bumps(
        DOGE_BRIDGE_INSTRUCTION_BLOCK_UPDATE,
        bytemuck::bytes_of(&fixed),
        &[mint_buffer_bump, txo_buffer_bump]
    );

    Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(bridge_state, false),
            AccountMeta::new(mint_buffer, false),
            AccountMeta::new(txo_buffer, false),
            AccountMeta::new_readonly(operator, true),
            AccountMeta::new(payer, true),
            // Program Accounts for CPI
            AccountMeta::new_readonly(PENDING_MINT_BUFFER_BUILDER_PROGRAM_ID, false),
            AccountMeta::new_readonly(TXO_BUFFER_BUILDER_PROGRAM_ID, false),
        ],
        data,
    }
}

pub fn process_reorg_blocks(
    program_id: Pubkey,
    payer: Pubkey,
    proof: CompactBridgeZKProof,
    header: PsyBridgeHeader,
    extra_blocks: Vec<FinalizedBlockMintTxoInfo>,
    operator: Pubkey,
    mint_buffer: Pubkey,
    txo_buffer: Pubkey,
    mint_buffer_bump: u8,
    txo_buffer_bump: u8,
) -> Instruction {
    let (bridge_state, _) = Pubkey::find_program_address(&[b"bridge_state"], &program_id);

    let fixed = ProcessReorgBlocksFixedData { proof, header };
    let mut data = gen_aligned_instruction_with_bumps(
        DOGE_BRIDGE_INSTRUCTION_PROCESS_REORG_BLOCKS,
        bytemuck::bytes_of(&fixed),
        &[mint_buffer_bump, txo_buffer_bump]
    );
    for block in extra_blocks {
        data.extend_from_slice(bytemuck::bytes_of(&block));
    }

    Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(bridge_state, false),
            AccountMeta::new(mint_buffer, false),
            AccountMeta::new(txo_buffer, false),
            AccountMeta::new_readonly(operator, true),
            AccountMeta::new(payer, true),
            AccountMeta::new_readonly(PENDING_MINT_BUFFER_BUILDER_PROGRAM_ID, false),
            AccountMeta::new_readonly(TXO_BUFFER_BUILDER_PROGRAM_ID, false),
        ],
        data,
    }
}

pub fn request_withdrawal(
    program_id: Pubkey,
    user_authority: Pubkey,
    mint: Pubkey,
    user_token_account: Pubkey,
    recipient_address: [u8; 20],
    amount_sats: u64,
    address_type: u32,
) -> Instruction {
    let (bridge_state, _) = Pubkey::find_program_address(&[b"bridge_state"], &program_id);
    let request = PsyWithdrawalRequest::new(recipient_address, amount_sats, address_type);
    
    let data_struct = RequestWithdrawalInstructionData {
        request,
    };
    
    let data = gen_aligned_instruction(
        DOGE_BRIDGE_INSTRUCTION_REQUEST_WITHDRAWAL,
        bytemuck::bytes_of(&data_struct),
    );

    Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(bridge_state, false),
            AccountMeta::new(user_token_account, false),
            AccountMeta::new(mint, false),
            AccountMeta::new_readonly(user_authority, true),
            AccountMeta::new_readonly(spl_token::id(), false),
        ],
        data,
    }
}

pub fn process_withdrawal(
    program_id: Pubkey,
    payer: Pubkey,
    generic_buffer_account: Pubkey,
    wormhole_shim_program_id: Pubkey,
    wormhole_core_program_id: Pubkey,
    proof: CompactBridgeZKProof,
    new_return_output: PsyReturnTxOutput,
    new_spent_txo_tree_root: QHash256,
    new_next_processed_withdrawals_index: u64,
    new_total_spent_deposit_utxo_count: u64,
) -> Instruction {
    let (bridge_state, _) = Pubkey::find_program_address(&[b"bridge_state"], &program_id);

    // --- Derive Wormhole Accounts ---
    // Bridge Config
    let (bridge_config, _) = Pubkey::find_program_address(&[b"Bridge"], &wormhole_core_program_id);
    
    // Fee Collector
    let (fee_collector, _) = Pubkey::find_program_address(&[b"fee_collector"], &wormhole_core_program_id);
    
    // Emitter (The Bridge State PDA acts as the emitter)
    let emitter = bridge_state;

    // Sequence (Core Bridge PDA based on emitter)
    let (sequence, _) = Pubkey::find_program_address(&[b"Sequence", emitter.as_ref()], &wormhole_core_program_id);

    // Message (Shim PDA based on emitter)
    let (message, _) = Pubkey::find_program_address(&[emitter.as_ref()], &wormhole_shim_program_id);

    // Event Authority (Shim PDA)
    let (event_authority, _) = Pubkey::find_program_address(&[b"__event_authority"], &wormhole_shim_program_id);

    let data_struct = ProcessWithdrawalInstructionData {
        proof,
        new_return_output,
        new_spent_txo_tree_root,
        new_next_processed_withdrawals_index,
        new_total_spent_deposit_utxo_count,
    };

    let data = gen_aligned_instruction(
        DOGE_BRIDGE_INSTRUCTION_PROCESS_WITHDRAWAL,
        bytemuck::bytes_of(&data_struct),
    );

    Instruction {
        program_id,
        accounts: vec![
            // Standard Accounts
            AccountMeta::new(bridge_state, false), // 0: Bridge State / Emitter
            AccountMeta::new_readonly(generic_buffer_account, false), // 1: Generic Buffer (TX Data)
            
            // Wormhole Shim Accounts
            AccountMeta::new_readonly(wormhole_shim_program_id, false), // 2
            AccountMeta::new(bridge_config, false), // 3
            AccountMeta::new(message, false), // 4
            // Emitter is index 0 (bridge_state)
            AccountMeta::new(sequence, false), // 5
            AccountMeta::new(payer, true), // 6: Payer
            AccountMeta::new(fee_collector, false), // 7
            AccountMeta::new_readonly(clock::id(), false), // 8
            AccountMeta::new_readonly(system_program::id(), false), // 9
            AccountMeta::new_readonly(wormhole_core_program_id, false), // 10
            AccountMeta::new_readonly(event_authority, false), // 11
        ],
        data,
    }
}
pub fn process_replay_withdrawal(
    program_id: Pubkey,
    payer: Pubkey,
    generic_buffer_account: Pubkey,
    wormhole_shim_program_id: Pubkey,
    wormhole_core_program_id: Pubkey,
) -> Instruction {
    let (bridge_state, _) = Pubkey::find_program_address(&[b"bridge_state"], &program_id);

    // --- Derive Wormhole Accounts ---
    // Bridge Config
    let (bridge_config, _) = Pubkey::find_program_address(&[b"Bridge"], &wormhole_core_program_id);
    
    // Fee Collector
    let (fee_collector, _) = Pubkey::find_program_address(&[b"fee_collector"], &wormhole_core_program_id);
    
    // Emitter (The Bridge State PDA acts as the emitter)
    let emitter = bridge_state;

    // Sequence (Core Bridge PDA based on emitter)
    let (sequence, _) = Pubkey::find_program_address(&[b"Sequence", emitter.as_ref()], &wormhole_core_program_id);

    // Message (Shim PDA based on emitter)
    let (message, _) = Pubkey::find_program_address(&[emitter.as_ref()], &wormhole_shim_program_id);

    // Event Authority (Shim PDA)
    let (event_authority, _) = Pubkey::find_program_address(&[b"__event_authority"], &wormhole_shim_program_id);


    let data = gen_aligned_instruction(
        DOGE_BRIDGE_INSTRUCTION_REPLAY_WITHDRAWAL,
        &[],
    );

    Instruction {
        program_id,
        accounts: vec![
            // Standard Accounts
            AccountMeta::new(bridge_state, false), // 0: Bridge State / Emitter
            AccountMeta::new_readonly(generic_buffer_account, false), // 1: Generic Buffer (TX Data)
            
            // Wormhole Shim Accounts
            AccountMeta::new_readonly(wormhole_shim_program_id, false), // 2
            AccountMeta::new(bridge_config, false), // 3
            AccountMeta::new(message, false), // 4
            // Emitter is index 0 (bridge_state)
            AccountMeta::new(sequence, false), // 5
            AccountMeta::new(payer, true), // 6: Payer
            AccountMeta::new(fee_collector, false), // 7
            AccountMeta::new_readonly(clock::id(), false), // 8
            AccountMeta::new_readonly(system_program::id(), false), // 9
            AccountMeta::new_readonly(wormhole_core_program_id, false), // 10
            AccountMeta::new_readonly(event_authority, false), // 11
        ],
        data,
    }
}
pub fn process_manual_deposit(
    program_id: Pubkey,
    manual_claim_program_id: Pubkey,
    mint: Pubkey,
    recipient: Pubkey,
    tx_hash: QHash256,
    recent_block_merkle_tree_root: QHash256,
    recent_auto_claim_txo_root: QHash256,
    combined_txo_index: u64,
    depositor_solana_public_key: [u8; 32],
    deposit_amount_sats: u64,
) -> Instruction {
    let (bridge_state, _) = Pubkey::find_program_address(&[b"bridge_state"], &program_id);

    let data_struct = ProcessManualDepositInstructionData {
        tx_hash,
        recent_block_merkle_tree_root,
        recent_auto_claim_txo_root,
        combined_txo_index,
        depositor_solana_public_key,
        deposit_amount_sats,
    };

    let data = gen_aligned_instruction(
        DOGE_BRIDGE_INSTRUCTION_PROCESS_MANUAL_DEPOSIT,
        bytemuck::bytes_of(&data_struct),
    );
    Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(bridge_state, false),
            AccountMeta::new(recipient, false),
            AccountMeta::new(mint, false),
            AccountMeta::new_readonly(spl_token::id(), false),
            AccountMeta::new_readonly(manual_claim_program_id, true),
        ],
        data,
    }
}

pub fn manual_claim_deposit_instruction(
    program_id: Pubkey,
    bridge_program_id: Pubkey,
    user_signer: Pubkey,
    payer: Pubkey,
    mint: Pubkey,
    recipient: Pubkey,
    proof: CompactBridgeZKProof,
    recent_block_merkle_tree_root: QHash256,
    recent_auto_claim_txo_root: QHash256,
    new_manual_claim_txo_root: QHash256,
    tx_hash: QHash256,
    combined_txo_index: u64,
    deposit_amount_sats: u64,
) -> Instruction {
    let (claim_pda, _) = Pubkey::find_program_address(&[b"manual-claim", user_signer.as_ref()], &program_id);
    let (bridge_state, _) = Pubkey::find_program_address(&[b"bridge_state"], &bridge_program_id);

    let ix_data = ManualClaimInstruction {
        proof,
        recent_block_merkle_tree_root,
        recent_auto_claim_txo_root,
        new_manual_claim_txo_root,
        tx_hash,
        combined_txo_index,
        deposit_amount_sats,
    };

    let data = gen_aligned_instruction(
        MC_MANUAL_CLAIM_TRANSACTION_DESCRIMINATOR,
        bytemuck::bytes_of(&ix_data),
    );

    Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(claim_pda, false),
            AccountMeta::new_readonly(bridge_state, false),
            AccountMeta::new(recipient, false),
            AccountMeta::new(mint, false),
            AccountMeta::new_readonly(spl_token::id(), false),
            AccountMeta::new_readonly(bridge_program_id, false),
            AccountMeta::new_readonly(user_signer, true),
            AccountMeta::new(payer, true),
            AccountMeta::new_readonly(system_program::id(), false),
        ],
        data,
    }
}

pub fn generic_buffer_init(program_id: Pubkey, account: Pubkey, payer: Pubkey, target_size: u32) -> Instruction {
    let mut data = vec![0u8];
    data.extend_from_slice(&target_size.to_le_bytes());
    Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(account, false),
            AccountMeta::new(payer, true),
            AccountMeta::new_readonly(system_program::id(), false),
        ],
        data,
    }
}

pub fn generic_buffer_write(program_id: Pubkey, account: Pubkey, payer: Pubkey, offset: u32, bytes: &[u8]) -> Instruction {
    let mut data = vec![2u8];
    data.extend_from_slice(&offset.to_le_bytes());
    data.extend_from_slice(bytes);
    Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(account, false),
            AccountMeta::new(payer, true),
            AccountMeta::new_readonly(system_program::id(), false),
        ],
        data,
    }
}

pub fn pending_mint_setup(program_id: Pubkey, account: Pubkey, locker: Pubkey, writer: Pubkey) -> Instruction {
    let mut data = vec![0u8];
    data.extend_from_slice(locker.as_ref());
    data.extend_from_slice(writer.as_ref());
    Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(account, false),
            AccountMeta::new_readonly(system_program::id(), false), // System Program for allocate/assign CPI
        ],
        data,
    }
}

pub fn pending_mint_reinit(program_id: Pubkey, account: Pubkey, payer: Pubkey, total_mints: u16) -> Instruction {
    let mut data = vec![1u8];
    data.extend_from_slice(&total_mints.to_le_bytes());
    Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(account, false),
            AccountMeta::new(payer, true),
            AccountMeta::new_readonly(system_program::id(), false),
        ],
        data,
    }
}

pub fn process_mint_group(
    program_id: Pubkey,
    operator: Pubkey,
    mint_buffer: Pubkey,
    doge_mint: Pubkey,
    recipients: Vec<Pubkey>,
    group_index: u16,
    mint_buffer_bump: u8,
    should_unlock: bool,
) -> Instruction {
    let (bridge_state, _) = Pubkey::find_program_address(&[b"bridge_state"], &program_id);
    
    let mut data = Vec::with_capacity(4);
    data.extend_from_slice(&group_index.to_le_bytes());
    data.push(mint_buffer_bump);
    data.push(if should_unlock { 1 } else { 0 });


    let data = gen_aligned_instruction(
        DOGE_BRIDGE_INSTRUCTION_PROCESS_MINT_GROUP,
        &data,
    );

    let mut accounts = vec![
        AccountMeta::new(bridge_state, false),
        AccountMeta::new(mint_buffer, false),
        AccountMeta::new_readonly(operator, true),
        AccountMeta::new(doge_mint, false),
        AccountMeta::new(operator, true), // Payer
        AccountMeta::new_readonly(PENDING_MINT_BUFFER_BUILDER_PROGRAM_ID, false), // Program Account
        AccountMeta::new_readonly(spl_token::id(), false), // Token Program
    ];

    for r in recipients {
        accounts.push(AccountMeta::new(r, false));
    }

    Instruction {
        program_id,
        accounts,
        data,
    }
}

pub fn process_mint_group_auto_advance(
    program_id: Pubkey,
    operator: Pubkey,
    mint_buffer: Pubkey,
    txo_buffer: Pubkey,
    doge_mint: Pubkey,
    recipients: Vec<Pubkey>,
    group_index: u16,
    mint_buffer_bump: u8,
    txo_buffer_bump: u8,
    should_unlock: bool,
) -> Instruction {
    let (bridge_state, _) = Pubkey::find_program_address(&[b"bridge_state"], &program_id);
    
    let mut data_payload = Vec::new();
    data_payload.extend_from_slice(&group_index.to_le_bytes());
    data_payload.push(mint_buffer_bump); 
    data_payload.push(if should_unlock { 1 } else { 0 });

    let data = gen_aligned_instruction_with_bumps(
        DOGE_BRIDGE_INSTRUCTION_PROCESS_MINT_GROUP_AUTO_ADVANCE,
        &data_payload,
        &[mint_buffer_bump, txo_buffer_bump]
    );

    let mut accounts = vec![
        AccountMeta::new(bridge_state, false),
        AccountMeta::new(mint_buffer, false),
        AccountMeta::new(txo_buffer, false),
        AccountMeta::new_readonly(operator, true),
        AccountMeta::new(doge_mint, false),
        AccountMeta::new(operator, true), // Payer
        AccountMeta::new_readonly(PENDING_MINT_BUFFER_BUILDER_PROGRAM_ID, false),
        AccountMeta::new_readonly(TXO_BUFFER_BUILDER_PROGRAM_ID, false),
        AccountMeta::new_readonly(spl_token::id(), false),
    ];

    for r in recipients {
        accounts.push(AccountMeta::new(r, false));
    }

    Instruction {
        program_id,
        accounts,
        data,
    }
}

pub fn pending_mint_insert(program_id: Pubkey, account: Pubkey, payer: Pubkey, group_idx: u16, mint_data: &[u8]) -> Instruction {
    let mut data = vec![3u8];
    data.extend_from_slice(&group_idx.to_le_bytes());
    data.extend_from_slice(mint_data);
    Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(account, false),
            AccountMeta::new(payer, true),
            AccountMeta::new_readonly(system_program::id(), false),
        ],
        data,
    }
}

pub fn txo_buffer_init(program_id: Pubkey, account: Pubkey, writer: Pubkey) -> Instruction {
    let mut data = vec![0u8];
    data.extend_from_slice(writer.as_ref());
    Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(account, false),
            AccountMeta::new_readonly(system_program::id(), false), // System Program for allocate/assign CPI
        ],
        data,
    }
}

pub fn txo_buffer_set_len(
    program_id: Pubkey, 
    account: Pubkey, 
    payer: Pubkey, 
    writer: Pubkey, 
    new_len: u32,
    resize: bool,
    batch_id: u32,
    height: u32,
    finalize: bool
) -> Instruction {
    let mut data = vec![1u8];
    data.extend_from_slice(&new_len.to_le_bytes());
    data.push(if resize { 1 } else { 0 });
    data.extend_from_slice(&batch_id.to_le_bytes());
    data.extend_from_slice(&height.to_le_bytes());
    data.push(if finalize { 1 } else { 0 });
    
    Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(account, false),
            AccountMeta::new(payer, true),
            AccountMeta::new_readonly(system_program::id(), false),
            AccountMeta::new_readonly(writer, true),
        ],
        data,
    }
}

pub fn txo_buffer_write(
    program_id: Pubkey,
    account: Pubkey,
    writer: Pubkey,
    batch_id: u32,
    offset: u32,
    bytes: &[u8]
) -> Instruction {
    let mut data = vec![2u8];
    data.extend_from_slice(&batch_id.to_le_bytes());
    data.extend_from_slice(&offset.to_le_bytes());
    data.extend_from_slice(bytes);
    Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(account, false),
            AccountMeta::new(writer, true),
        ],
        data,
    }
}

pub fn operator_withdraw_fees(
    program_id: Pubkey,
    operator: Pubkey,
    operator_token_account: Pubkey,
    doge_mint: Pubkey,
) -> Instruction {
    let (bridge_state, _) = Pubkey::find_program_address(&[b"bridge_state"], &program_id);

    let data = gen_aligned_instruction(DOGE_BRIDGE_INSTRUCTION_OPERATOR_WITHDRAW_FEES, &[]);

    Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(bridge_state, false),
            AccountMeta::new(operator_token_account, false),
            AccountMeta::new(doge_mint, false),
            AccountMeta::new(operator, true),
            AccountMeta::new_readonly(spl_token::id(), false),
        ],
        data,
    }
}

pub fn snapshot_withdrawals(
    program_id: Pubkey,
    operator: Pubkey,
    payer: Pubkey,
) -> Instruction {
    let (bridge_state, _) = Pubkey::find_program_address(&[b"bridge_state"], &program_id);

    let data = gen_aligned_instruction(DOGE_BRIDGE_INSTRUCTION_SNAPSHOT_WITHDRAWALS, &[]);

    Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(bridge_state, false),
            AccountMeta::new_readonly(operator, true),
            AccountMeta::new(payer, true),
        ],
        data,
    }
}