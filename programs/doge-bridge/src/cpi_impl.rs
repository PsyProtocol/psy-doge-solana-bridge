use psy_bridge_core::error::{DogeBridgeError, QDogeResult};
use psy_doge_solana_core::generic_cpi::{
    AutoClaimMintBufferAddressHelper, BurnCPIHelper, LockAutoClaimMintBufferCPIHelper,
    MintCPIHelper, SendDogecoinSignatureRequestCPIHelper, UnlockAutoClaimMintBufferCPIHelper,
};
use solana_program::{
    account_info::AccountInfo,
    instruction::{AccountMeta, Instruction},
    msg,
    program::{invoke, invoke_signed},
    program_error::ProgramError,
    pubkey::Pubkey,
};

use spl_token::ID as TOKEN_PROGRAM_ID;
pub fn verify_pda(
    base_seed: &[u8],
    provided_account: &Pubkey,
    program_id: &Pubkey,
    user_key: &Pubkey,
    bump: u8,
) -> Result<(), ProgramError> {
    // Re-derive the address using the expected seeds
    let expected_pda =
        Pubkey::create_program_address(&[base_seed, user_key.as_ref(), &[bump]], program_id)
            .map_err(|_| ProgramError::InvalidSeeds)?;

    // Compare
    if provided_account != &expected_pda {
        return Err(ProgramError::InvalidSeeds);
    }

    Ok(())
}
pub fn verify_lowest_pda(
    base_seed: &[u8],
    provided_account: &Pubkey,
    program_id: &Pubkey,
    user_key: &Pubkey,
) -> Result<(), ProgramError> {
    let (lowest_pda, _lowest_bump) =
        Pubkey::find_program_address(&[base_seed, user_key.as_ref()], program_id);
    if provided_account != &lowest_pda {
        return Err(ProgramError::InvalidSeeds);
    }
    Ok(())
}
/// Helper for minting tokens via CPI to SPL Token
pub struct SolanaMinter<'a, 'b> {
    pub mint: &'a AccountInfo<'b>,
    pub authority_info: &'a AccountInfo<'b>,
    pub authority_seeds: &'a [&'a [u8]],
    pub recipient_map: &'a [AccountInfo<'b>],
    pub token_program: &'a AccountInfo<'b>,
}

impl<'a, 'b> MintCPIHelper for SolanaMinter<'a, 'b> {
    fn mint_to(
        &self,
        index_in_mint_group: usize,
        account: &[u8; 32],
        amount: u64,
    ) -> QDogeResult<()> {
        let recipient_pubkey = Pubkey::new_from_array(*account);
        // Find the account info in the slice that matches the recipient pubkey
        if index_in_mint_group >= self.recipient_map.len() {
            return Err(DogeBridgeError::AccountMismatch);
        } else if self.recipient_map[index_in_mint_group].key != &recipient_pubkey {
            return Err(DogeBridgeError::AccountMismatch);
        }
        let recipient_info = &self.recipient_map[index_in_mint_group];

        let ix = spl_token::instruction::mint_to(
            &TOKEN_PROGRAM_ID,
            self.mint.key,
            recipient_info.key,
            self.authority_info.key,
            &[],
            amount,
        )
        .map_err(|_| DogeBridgeError::SerializationError)?;

        let res = invoke_signed(
            &ix,
            &[
                self.mint.clone(),
                recipient_info.clone(),
                self.authority_info.clone(),
                self.token_program.clone(),
            ],
            &[self.authority_seeds],
        );
        if res.is_err() {
            msg!("Mint CPI failed for recipient: {:?}, {:?}", recipient_info.key, res.err().unwrap());
            return Err(DogeBridgeError::CpiTokenMintToCallError);
        }
        Ok(())
        
    }
}

/// Helper for burning tokens via CPI to SPL Token
pub struct SolanaBurner<'a, 'b> {
    pub mint: &'a AccountInfo<'b>,
    pub user_token_account: &'a AccountInfo<'b>,
    pub authority: &'a AccountInfo<'b>,
    pub token_program: &'a AccountInfo<'b>,
}

impl<'a, 'b> BurnCPIHelper for SolanaBurner<'a, 'b> {
    fn burn_from(&self, _account: &[u8; 32], amount: u64) -> QDogeResult<()> {
        let ix = spl_token::instruction::burn(
            &TOKEN_PROGRAM_ID,
            self.user_token_account.key, // source
            self.mint.key,               // mint
            self.authority.key,          // authority
            &[],
            amount,
        )
        .map_err(|_| DogeBridgeError::SerializationError)?;

        invoke(
            &ix,
            &[
                self.user_token_account.clone(),
                self.mint.clone(),
                self.authority.clone(),
                self.token_program.clone(),
            ],
        )
        .map_err(|_| DogeBridgeError::CpiTokenBurnCallError)
    }
}

/// Helper for interacting with the Pending Mint Buffer Program
pub struct SolanaMintBufferLocker<'a, 'b> {
    pub buffer_program_key: &'a Pubkey,
    pub is_valid_pda: bool,
    pub buffer_account: &'a AccountInfo<'b>,
    pub buffer_program_account: &'a AccountInfo<'b>,
    pub authority_info: &'a AccountInfo<'b>,
    pub authority_seeds: &'a [&'a [u8]],
}

impl<'a, 'b> AutoClaimMintBufferAddressHelper for SolanaMintBufferLocker<'a, 'b> {
    fn get_mint_buffer_program_address(&self) -> [u8; 32] {
        self.buffer_account.key.to_bytes()
    }

    fn is_pda_of_correct_auto_claim_deposit_mint_buffer_program(&self) -> bool {
        self.is_valid_pda
    }
}
impl<'a, 'b> LockAutoClaimMintBufferCPIHelper for SolanaMintBufferLocker<'a, 'b> {
    fn lock_buffer(&self) -> QDogeResult<()> {
        // Instruction discriminator for 'Lock' is 4 in the buffer builder program
        let data = vec![4u8];
        let ix = Instruction {
            program_id: *self.buffer_program_key,
            accounts: vec![
                AccountMeta::new(*self.buffer_account.key, false),
                AccountMeta::new_readonly(*self.authority_info.key, true),
            ],
            data,
        };

        invoke_signed(
            &ix,
            &[
                self.buffer_program_account.clone(),
                self.buffer_account.clone(), 
                self.authority_info.clone()
            ],
            &[self.authority_seeds],
        )
        .map_err(|err| {
            msg!("Error locking mint buffer: {:?}", err);
            DogeBridgeError::CpiLockCallError
        })
    }
}

impl<'a, 'b> UnlockAutoClaimMintBufferCPIHelper for SolanaMintBufferLocker<'a, 'b> {
    fn unlock_buffer(&self, _mint_buffer_program_address: &[u8; 32]) -> QDogeResult<()> {
        // Instruction discriminator for 'Unlock' is 5
        let data = vec![5u8];
        let ix = Instruction {
            program_id: *self.buffer_program_key,
            accounts: vec![
                AccountMeta::new(*self.buffer_account.key, false),
                AccountMeta::new_readonly(*self.authority_info.key, true),
            ],
            data,
        };

        invoke_signed(
            &ix,
            &[
                self.buffer_program_account.clone(),
                self.buffer_account.clone(), 
                self.authority_info.clone()
            ],
            &[self.authority_seeds],
        )
        .map_err(|_| DogeBridgeError::CpiUnlockCallError)
    }
}

pub struct SolanaSigRequester;

impl SendDogecoinSignatureRequestCPIHelper for SolanaSigRequester {
    fn send_signature_request_for_tx(
        &self,
        sighash: &[u8],
        _transaction_buffer: &[u8],
    ) -> QDogeResult<()> {
        // Log the request for off-chain indexing
        msg!("Requesting Signature for Sighash: {:?}", sighash);
        //msg!("Requesting Signature for tx: {:?}", transaction_buffer);
        Ok(())
    }
}