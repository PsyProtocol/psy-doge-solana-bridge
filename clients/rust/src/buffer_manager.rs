use solana_sdk::{
    instruction::Instruction, pubkey::Pubkey, signature::{Keypair, Signer}, system_instruction, transaction::Transaction
};
use solana_client::nonblocking::rpc_client::RpcClient;
use crate::{errors::ClientError, instructions};
use std::sync::Arc;
use psy_doge_solana_core::data_accounts::pending_mint::{PM_MAX_PENDING_MINTS_PER_GROUP, PendingMint, PM_DA_PENDING_MINT_SIZE as PENDING_MINT_SIZE};

const CHUNK_SIZE: usize = 900;

pub struct BufferManager {
    client: Arc<RpcClient>,
    payer: Keypair,
}

impl BufferManager {
    pub fn new(client: Arc<RpcClient>, payer: Keypair) -> Self {
        Self { client, payer }
    }

    pub async fn send_tx(&self, ixs: &[Instruction], extra_signers: &[&Keypair]) -> Result<(), ClientError> {
        let recent_blockhash = self.client.get_latest_blockhash().await?;
        let mut signers = vec![&self.payer];
        signers.extend_from_slice(extra_signers);
        
        let tx = Transaction::new_signed_with_payer(
            ixs,
            Some(&self.payer.pubkey()),
            &signers,
            recent_blockhash,
        );
        
        self.client.send_and_confirm_transaction(&tx).await
            .map(|_| ())
            .map_err(ClientError::from)
    }

    pub async fn create_generic_buffer(&self, program_id: Pubkey, data: &[u8]) -> Result<Pubkey, ClientError> {
        let buffer_account = Keypair::new();
        let buffer_pubkey = buffer_account.pubkey();
        let target_size = data.len() as u32;

        let space = 32;
        let rent = self.client.get_minimum_balance_for_rent_exemption(space).await?;
        let create_ix = system_instruction::create_account(
            &self.payer.pubkey(),
            &buffer_pubkey,
            rent,
            space as u64,
            &program_id,
        );
        
        let init_ix = instructions::generic_buffer_init(program_id, buffer_pubkey, self.payer.pubkey(), target_size);
        
        self.send_tx(&[create_ix, init_ix], &[&buffer_account]).await?;

        for (i, chunk) in data.chunks(CHUNK_SIZE).enumerate() {
            let offset = (i * CHUNK_SIZE) as u32;
            let write_ix = instructions::generic_buffer_write(
                program_id, 
                buffer_pubkey, 
                self.payer.pubkey(), 
                offset, 
                chunk
            );
            self.send_tx(&[write_ix], &[]).await?;
        }

        Ok(buffer_pubkey)
    }

    pub async fn create_pending_mint_buffer(
        &self, 
        program_id: Pubkey, 
        locker: Pubkey, 
        mints: &[PendingMint]
    ) -> Result<(Pubkey, u8), ClientError> {
        let payer_pubkey = self.payer.pubkey().to_bytes();
        
        let seeds: &[&[u8]] = &[
            b"mint_buffer",
            &payer_pubkey,
        ];
        let (buffer_pubkey, bump) = Pubkey::find_program_address(seeds, &program_id);

        let account_info = self.client.get_account_with_commitment(&buffer_pubkey, self.client.commitment()).await?.value;
        let exists = account_info.is_some();

        if !exists {
            let space = 72;
            let rent = self.client.get_minimum_balance_for_rent_exemption(space).await?;
            let transfer_ix = system_instruction::transfer(&self.payer.pubkey(), &buffer_pubkey, rent);
            let setup_ix = instructions::pending_mint_setup(program_id, buffer_pubkey, locker, self.payer.pubkey());
            self.send_tx(&[transfer_ix, setup_ix], &[]).await?;
        }

        let total_mints = mints.len() as u16;
        let reinit_ix = instructions::pending_mint_reinit(program_id, buffer_pubkey, self.payer.pubkey(), total_mints);
        self.send_tx(&[reinit_ix], &[]).await?;

        let groups_count = (mints.len() + PM_MAX_PENDING_MINTS_PER_GROUP - 1) / PM_MAX_PENDING_MINTS_PER_GROUP;
        
        for group_idx in 0..groups_count {
            let start = group_idx * PM_MAX_PENDING_MINTS_PER_GROUP;
            let end = std::cmp::min(start + PM_MAX_PENDING_MINTS_PER_GROUP, mints.len());
            let group_mints = &mints[start..end];
            
            let mut mint_data = Vec::with_capacity(group_mints.len() * PENDING_MINT_SIZE);
            for m in group_mints {
                mint_data.extend_from_slice(bytemuck::bytes_of(m));
            }

            let insert_ix = instructions::pending_mint_insert(
                program_id,
                buffer_pubkey,
                self.payer.pubkey(),
                group_idx as u16,
                &mint_data
            );
            self.send_tx(&[insert_ix], &[]).await?;
        }

        Ok((buffer_pubkey, bump))
    }

    pub async fn create_txo_buffer(
        &self,
        program_id: Pubkey,
        doge_block_height: u32,
        txo_indices_u32: &[u32],
    ) -> Result<(Pubkey, u8), ClientError> {
        let payer_pubkey = self.payer.pubkey().to_bytes();
        let seeds: &[&[u8]] = &[
            b"txo_buffer",
            &payer_pubkey,
        ];
        let (buffer_pubkey, bump) = Pubkey::find_program_address(seeds, &program_id);
        
        let txo_bytes: Vec<u8> = txo_indices_u32.iter().flat_map(|x| x.to_le_bytes()).collect();
        let total_len = txo_bytes.len() as u32;
        
        let account_info = self.client.get_account_with_commitment(&buffer_pubkey, self.client.commitment()).await?.value;
        let exists = account_info.is_some();

        let batch_id = if !exists {
            let space = 48;
            let rent = self.client.get_minimum_balance_for_rent_exemption(space).await?;
            let transfer_ix = system_instruction::transfer(&self.payer.pubkey(), &buffer_pubkey, rent);
            let init_ix = instructions::txo_buffer_init(program_id, buffer_pubkey, self.payer.pubkey());
            self.send_tx(&[transfer_ix, init_ix], &[]).await?;
            0
        }else{
            1 + u32::from_le_bytes(account_info.unwrap().data[40..44].try_into().unwrap())
        };

        let set_len_ix = instructions::txo_buffer_set_len(
            program_id, buffer_pubkey, self.payer.pubkey(), self.payer.pubkey(),
            total_len, true, batch_id, doge_block_height, false
        );
        self.send_tx(&[set_len_ix], &[]).await?;

        for (i, chunk) in txo_bytes.chunks(CHUNK_SIZE).enumerate() {
            let offset = (i * CHUNK_SIZE) as u32;
            let write_ix = instructions::txo_buffer_write(
                program_id, buffer_pubkey, self.payer.pubkey(), batch_id, offset, chunk
            );
            self.send_tx(&[write_ix], &[]).await?;
        }

        let finalize_ix = instructions::txo_buffer_set_len(
            program_id, buffer_pubkey, self.payer.pubkey(), self.payer.pubkey(),
            total_len, false, batch_id, doge_block_height, true
        );
        self.send_tx(&[finalize_ix], &[]).await?;

        Ok((buffer_pubkey, bump))
    }
}
