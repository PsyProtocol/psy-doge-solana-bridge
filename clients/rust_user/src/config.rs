//! Configuration for the user client.

use solana_sdk::pubkey::Pubkey;

/// Default bridge program ID
pub const DEFAULT_BRIDGE_PROGRAM_ID: &str = "DBjo5tqf2uwt4sg9JznSk9SBbEvsLixknN58y3trwCxJ";

/// Configuration for the UserClient.
#[derive(Clone)]
pub struct UserClientConfig {
    /// Solana RPC URL
    pub rpc_url: String,
    /// Bridge program ID
    pub program_id: Pubkey,
    /// Bridge state PDA (derived from program_id if not provided)
    pub bridge_state_pda: Pubkey,
}

/// Builder for UserClientConfig.
pub struct UserClientConfigBuilder {
    rpc_url: Option<String>,
    program_id: Option<Pubkey>,
    bridge_state_pda: Option<Pubkey>,
}

impl UserClientConfigBuilder {
    /// Create a new config builder.
    pub fn new() -> Self {
        Self {
            rpc_url: None,
            program_id: None,
            bridge_state_pda: None,
        }
    }

    /// Set the RPC URL.
    pub fn rpc_url(mut self, url: impl Into<String>) -> Self {
        self.rpc_url = Some(url.into());
        self
    }

    /// Set the bridge program ID.
    pub fn program_id(mut self, id: Pubkey) -> Self {
        self.program_id = Some(id);
        self
    }

    /// Set the bridge state PDA.
    pub fn bridge_state_pda(mut self, pda: Pubkey) -> Self {
        self.bridge_state_pda = Some(pda);
        self
    }

    /// Build the configuration.
    pub fn build(self) -> Result<UserClientConfig, String> {
        let rpc_url = self.rpc_url.ok_or("RPC URL is required")?;

        let program_id = self.program_id.unwrap_or_else(|| {
            Pubkey::from_str_const(DEFAULT_BRIDGE_PROGRAM_ID)
        });

        let bridge_state_pda = self.bridge_state_pda.unwrap_or_else(|| {
            Pubkey::find_program_address(&[b"bridge_state"], &program_id).0
        });

        Ok(UserClientConfig {
            rpc_url,
            program_id,
            bridge_state_pda,
        })
    }
}

impl Default for UserClientConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}
