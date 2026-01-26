//! Instruction builders for user operations.

use psy_doge_solana_core::{
    instructions::doge_bridge::{RequestWithdrawalInstructionData, DOGE_BRIDGE_INSTRUCTION_REQUEST_WITHDRAWAL},
    program_state::PsyWithdrawalRequest,
};
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
};

/// Generate aligned instruction data with the discriminator.
fn gen_aligned_instruction(instruction_discriminator: u8, data_struct_bytes: &[u8]) -> Vec<u8> {
    let mut data = vec![instruction_discriminator; 8];
    data.extend_from_slice(data_struct_bytes);
    data
}

/// Build a request_withdrawal instruction.
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
