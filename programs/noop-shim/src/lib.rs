use solana_program::{
    account_info::{next_account_info, AccountInfo},
    declare_id,
    entrypoint::ProgramResult,
    msg,
    program_error::ProgramError,
    pubkey::Pubkey,
};

declare_id!("FwDChsHWLwbhTiYQ4Sum5mjVWswECi9cmrA11GUFUuxi");

/// The doge-bridge program ID (used to derive the expected bridge state PDA)
pub const DOGE_BRIDGE_PROGRAM_ID: Pubkey =
    Pubkey::from_str_const("DBjo5tqf2uwt4sg9JznSk9SBbEvsLixknN58y3trwCxJ");

#[cfg(not(feature = "no-entrypoint"))]
solana_program::entrypoint!(process_instruction);

/// Noop shim that mimics the wormhole shim interface for testing.
///
/// Expected accounts (matching wormhole shim post_message):
/// 0. `[writable]` bridge_config
/// 1. `[writable]` message
/// 2. `[signer]` emitter - must be the bridge state PDA
/// 3. `[writable]` sequence
/// 4. `[writable, signer]` payer
/// 5. `[writable]` fee_collector
/// 6. `[]` clock
/// 7. `[]` system_program
/// 8. `[]` core_bridge_program
/// 9. `[]` event_authority
pub fn process_instruction(
    _program_id: &Pubkey,
    accounts: &[AccountInfo],
    instruction_data: &[u8],
) -> ProgramResult {
    let account_info_iter = &mut accounts.iter();

    // Skip bridge_config (index 0)
    let _bridge_config = next_account_info(account_info_iter)?;

    // Skip message (index 1)
    let _message = next_account_info(account_info_iter)?;

    // Emitter (index 2) - must be a signer and must be the bridge state PDA
    let emitter = next_account_info(account_info_iter)?;

    // Skip sequence (index 3)
    let _sequence = next_account_info(account_info_iter)?;

    // Payer (index 4) - must be a signer
    let payer = next_account_info(account_info_iter)?;

    // Verify emitter is a signer
    if !emitter.is_signer {
        msg!("NoopShim: emitter must be a signer");
        return Err(ProgramError::MissingRequiredSignature);
    }

    // Verify payer is a signer
    if !payer.is_signer {
        msg!("NoopShim: payer must be a signer");
        return Err(ProgramError::MissingRequiredSignature);
    }

    // Verify emitter is the bridge state PDA
    let (expected_bridge_pda, _bump) =
        Pubkey::find_program_address(&[b"bridge_state"], &DOGE_BRIDGE_PROGRAM_ID);

    if emitter.key != &expected_bridge_pda {
        msg!(
            "NoopShim: invalid emitter. Expected bridge PDA {}, got {}",
            expected_bridge_pda,
            emitter.key
        );
        return Err(ProgramError::InvalidAccountData);
    }

    msg!(
        "NoopShim: received {} bytes from bridge PDA {}, payer {}",
        instruction_data.len(),
        emitter.key,
        payer.key
    );

    Ok(())
}
