use crate::error::ManualClaimError;
use crate::instruction::ManualClaimInstruction;
use bytemuck::{Pod, Zeroable};
use psy_bridge_core::common_types::QHash256;
use psy_bridge_core::crypto::zk::{CompactBridgeZKProof, CompactBridgeZKVerifierKey};
use psy_bridge_core::error::{DogeBridgeError, QDogeResult};
use psy_doge_solana_core::generic_cpi::ManualDepositMainBridgeCPIHelper;
use psy_doge_solana_core::program_state::BridgeProgramStateWithDogeMint;
use psy_doge_solana_core::user_manual_deposit_manager::UserManualDepositManagerProgramState;
use solana_program::{
    account_info::{next_account_info, AccountInfo}, entrypoint::ProgramResult, instruction::{AccountMeta, Instruction}, program::invoke_signed, pubkey::Pubkey, rent::Rent, system_instruction, sysvar::Sysvar
};
pub const DOGE_BRIDGE_INSTRUCTION_PROCESS_MANUAL_DEPOSIT: u8 = 5;

pub const MC_MANUAL_CLAIM_TRANSACTION_DESCRIMINATOR: u8 = 0;
#[cfg(feature = "mock-zkp")]
use psy_bridge_core::crypto::zk::jtmb::FakeZKProof as ZKVerifier;
#[cfg(feature = "mock-zkp")]
const MANUAL_CLAIM_VK: CompactBridgeZKVerifierKey = [244, 238, 171, 78, 131, 171, 99, 200, 141, 114, 186, 29, 84, 79, 47, 233, 193, 81, 129, 115, 35, 74, 185, 6, 230, 88, 87, 77, 239, 104, 25, 48];

#[cfg(not(feature = "mock-zkp"))]
use psy_bridge_core::crypto::zk::sp1_groth16::SP1Groth16Verifier as ZKVerifier;
#[cfg(not(feature = "mock-zkp"))]
pub const MANUAL_CLAIM_VK: CompactBridgeZKVerifierKey = [0u8; 32]; // add updated sp1 vk for prod

#[derive(Pod, Zeroable, Clone, Copy, Debug)]
#[repr(C)]
pub struct ProcessManualDepositInstructionData {
    pub tx_hash: QHash256,
    pub recent_block_merkle_tree_root: QHash256,
    pub recent_auto_claim_txo_root: QHash256,
    pub combined_txo_index: u64,
    pub depositor_solana_public_key: [u8; 32],
    pub deposit_amount_sats: u64,
}

pub fn process_instruction(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    instruction_data: &[u8],
) -> ProgramResult {
    let instruction = bytemuck::try_from_bytes::<ManualClaimInstruction>(&instruction_data[8..])
        .map_err(|_| ManualClaimError::SerializationError)?;

            process_claim(
                program_id,
                accounts,
                &instruction.proof,
                instruction.recent_block_merkle_tree_root,
                instruction.recent_auto_claim_txo_root,
                instruction.new_manual_claim_txo_root,
                instruction.tx_hash,
                instruction.combined_txo_index,
                instruction.deposit_amount_sats,
            )
}

fn process_claim(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    proof: &CompactBridgeZKProof,
    recent_block_merkle_tree_root: [u8; 32],
    recent_auto_claim_txo_root: [u8; 32],
    new_manual_claim_txo_root: [u8; 32],
    tx_hash: [u8; 32],
    combined_txo_index: u64,
    deposit_amount_sats: u64,
) -> ProgramResult {
    let account_info_iter = &mut accounts.iter();
    let claim_state_pda = next_account_info(account_info_iter)?;
    let bridge_state_account = next_account_info(account_info_iter)?;
    let recipient_account = next_account_info(account_info_iter)?;
    let doge_mint = next_account_info(account_info_iter)?;
    let token_program = next_account_info(account_info_iter)?;
    let main_bridge_program = next_account_info(account_info_iter)?;
    let user = next_account_info(account_info_iter)?;
    let payer = next_account_info(account_info_iter)?;
    let system_program = next_account_info(account_info_iter)?;

    if !user.is_signer {
        return Err(solana_program::program_error::ProgramError::MissingRequiredSignature);
    }

    let (pda, bump) = Pubkey::find_program_address(&[b"manual-claim", user.key.as_ref()], program_id);
    if pda != *claim_state_pda.key {
        return Err(ManualClaimError::InvalidPDA.into());
    }

    if claim_state_pda.data_len() == 0 {
        let space = std::mem::size_of::<UserManualDepositManagerProgramState>();
        let rent = Rent::get()?.minimum_balance(space);
        invoke_signed(
            &system_instruction::create_account(
                payer.key,
                claim_state_pda.key,
                rent,
                space as u64,
                program_id,
            ),
            &[payer.clone(), claim_state_pda.clone(), system_program.clone()],
            &[&[b"manual-claim", user.key.as_ref(), &[bump]]],
        )?;
        
        let mut data = claim_state_pda.try_borrow_mut_data()?;
        let empty = UserManualDepositManagerProgramState::new_empty();
        let state_bytes = bytemuck::bytes_of(&empty);
        data[..state_bytes.len()].copy_from_slice(state_bytes);
    }else if claim_state_pda.owner != program_id {
        return Err(ManualClaimError::InvalidPDA.into());
    }

    let mut claim_data = claim_state_pda.try_borrow_mut_data()?;
    let state = bytemuck::try_from_bytes_mut::<UserManualDepositManagerProgramState>(&mut claim_data)
        .map_err(|_| ManualClaimError::SerializationError)?;

    let custodian_wallet_config_hash = {
        let bridge_data = bridge_state_account.try_borrow_data()?;
        
        let bridge_state = bytemuck::try_from_bytes::<BridgeProgramStateWithDogeMint>(&bridge_data)
            .map_err(|_| ManualClaimError::SerializationError)?;
        bridge_state.core_state.custodian_wallet_config.get_wallet_config_hash()
    };

    let helper = SolanaManualDepositHelper {
        bridge_program: main_bridge_program,
        bridge_state: bridge_state_account,
        recipient_account,
        doge_mint,
        token_program,
        claim_pda: claim_state_pda,
        claim_pda_seeds: &[b"manual-claim", user.key.as_ref(), &[bump]],
    };

    state.manual_claim_deposit::<ZKVerifier, SolanaManualDepositHelper>(
        proof,
        &MANUAL_CLAIM_VK,
        &helper,
        recent_block_merkle_tree_root,
        recent_auto_claim_txo_root,
        new_manual_claim_txo_root,
        custodian_wallet_config_hash,
        tx_hash,
        combined_txo_index,
        user.key.to_bytes(),
        deposit_amount_sats,
    ).map_err(|_| ManualClaimError::CoreError)?;

    Ok(())
}

struct SolanaManualDepositHelper<'a, 'b> {
    bridge_program: &'a AccountInfo<'b>,
    bridge_state: &'a AccountInfo<'b>,
    recipient_account: &'a AccountInfo<'b>,
    doge_mint: &'a AccountInfo<'b>,
    token_program: &'a AccountInfo<'b>,
    claim_pda: &'a AccountInfo<'b>,
    claim_pda_seeds: &'a [&'a [u8]],
}

impl<'a, 'b> ManualDepositMainBridgeCPIHelper for SolanaManualDepositHelper<'a, 'b> {
    fn derive_token_ata_from_signer(&self, _signer_public_key: [u8; 32]) -> [u8; 32] {
        self.recipient_account.key.to_bytes()
    }

    fn ensure_current_program_is_lowest_possible_pda_seed(&self, _signer_public_key: [u8; 32]) -> bool {
        true
    }

    fn process_manual_deposit(
        &self,
        recent_block_merkle_tree_root: [u8; 32],
        recent_auto_claim_txo_root: [u8; 32],
        tx_hash: [u8; 32],
        combined_txo_index: u64,
        depositor_solana_public_key: [u8; 32],
        deposit_amount_sats: u64,
    ) -> QDogeResult<()> {
        let ix_data = ProcessManualDepositInstructionData {
            tx_hash,
            recent_block_merkle_tree_root,
            recent_auto_claim_txo_root,
            combined_txo_index,
            depositor_solana_public_key: depositor_solana_public_key,
            deposit_amount_sats,
        };

        let mut data = Vec::with_capacity(std::mem::size_of::<ProcessManualDepositInstructionData>() + 8);
        data.push(DOGE_BRIDGE_INSTRUCTION_PROCESS_MANUAL_DEPOSIT);
        for _ in 0..7 {
            data.push(0u8);
        }

        data.extend_from_slice(bytemuck::bytes_of(&ix_data));
        let instruction = Instruction {
            program_id: *self.bridge_program.key,
            accounts: vec![
                AccountMeta::new(*self.bridge_state.key, false),
                AccountMeta::new(*self.recipient_account.key, false),
                AccountMeta::new(*self.doge_mint.key, false),
                AccountMeta::new_readonly(*self.token_program.key, false),
                AccountMeta::new_readonly(*self.claim_pda.key, true), // Signer
            ],
            data: data,
        };

        invoke_signed(
            &instruction,
            &[
                self.bridge_state.clone(),
                self.recipient_account.clone(),
                self.doge_mint.clone(),
                self.token_program.clone(),
                self.claim_pda.clone(),
            ],
            &[self.claim_pda_seeds]
        ).map_err(|_| DogeBridgeError::CpiManualDepositCallError)
    }
}
