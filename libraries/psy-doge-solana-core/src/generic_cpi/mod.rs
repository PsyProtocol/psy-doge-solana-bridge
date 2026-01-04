use psy_bridge_core::{common_types::QHash256, error::QDogeResult};
/*
"self" for the CPI traits should refer to pointers to the program accounts needed for the CPI calls.
The accounts needed will vary depending on the CPI call.
For example:
pub struct ExampleAccounts<'a> {
    pub signer: &'a Pubkey,
    pub token_mint: &'a Pubkey,
    pub payer_token_account: &'a Pubkey,
    pub token_program: &'a Pubkey,
    pub system_program: &'a Pubkey,
}
*/

// call when we mint tokens to users after a deposit
pub trait MintCPIHelper {
    fn mint_to(&self, index_in_mint_group: usize, account: &[u8; 32], amount: u64) -> QDogeResult<()>;
}

// Call when we burn tokens from users for withdrawals
pub trait BurnCPIHelper {
    fn burn_from(&self, account: &[u8; 32], amount: u64) -> QDogeResult<()>;
}


// Sends a wormhole VAA signature request 
pub trait SendDogecoinSignatureRequestCPIHelper {
    fn send_signature_request_for_tx(&self, sighash: &[u8], transaction_buffer: &[u8]) -> QDogeResult<()>;
}


// Called during block transition to ensure no modifications are made to the mint buffer while we are minting
pub trait AutoClaimMintBufferAddressHelper {
    fn get_mint_buffer_program_address(&self) -> [u8; 32];
    // gets whether the address is a PDA is of the correct program
    fn is_pda_of_correct_auto_claim_deposit_mint_buffer_program(&self) -> bool;
}
// Called during block transition to ensure no modifications are made to the mint buffer while we are minting
pub trait LockAutoClaimMintBufferCPIHelper: AutoClaimMintBufferAddressHelper {
    fn lock_buffer(&self) -> QDogeResult<()>;
}

// Called during block transition to ensure no modifications are made to the mint buffer while we are minting
pub trait UnlockAutoClaimMintBufferCPIHelper {
    // gets whether the address is a PDA is of the correct program
    fn unlock_buffer(&self, mint_buffer_program_address: &[u8; 32]) -> QDogeResult<()>;
}

pub trait ManualDepositMainBridgeCPIHelper {
    fn derive_token_ata_from_signer(&self, signer_public_key: [u8; 32]) -> [u8; 32];
    // we need to ensure we are the LOWEST possible PDA seed to ensure a manual claimer cannot create multiple pda accounts to double claim deposits
    fn ensure_current_program_is_lowest_possible_pda_seed(&self, signer_public_key: [u8; 32]) -> bool;
    fn process_manual_deposit(
        &self,
        recent_block_merkle_tree_root: QHash256,
        recent_auto_claim_txo_root: QHash256,
        tx_hash: QHash256,
        combined_txo_index: u64,
        depositor_solana_public_key: [u8; 32],
        deposit_amount_sats: u64,
    ) -> QDogeResult<()>;
}

// Call when we process a withdrawal request
pub trait LogRequestWithdrawalCPIHelper {
    fn log_request_withdrawal(
        &self,
        withdrawal_id: u64,
        withdrawal_leaf: QHash256,
    ) -> QDogeResult<()>;
}
// Call when we process a withdrawal request
pub trait LogManualDepositCPIHelper {
    fn log_manual_deposit(
        &self,
        tx_hash: QHash256,
        combined_txo_index: u64,
        depositor_public_key: &[u8; 32],
        deposit_amount_sats: u64,
    ) -> QDogeResult<()>;
}

