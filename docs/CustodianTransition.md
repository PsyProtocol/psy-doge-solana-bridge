# Custodian Set Transition

This document describes the custodian set transition process for the Doge Bridge. This feature allows the bridge to safely transition from one custodian configuration to another while ensuring all UTXOs are properly consolidated.

## Overview

The custodian set transition enables the bridge to migrate from one multisig custodian configuration (e.g., 5-of-7) to a new configuration. This is necessary when:
- Adding or removing custodians from the set
- Rotating keys for security purposes
- Upgrading the custodian infrastructure

## State Machine

The transition follows a state machine with four states:

```
┌────────────────┐     notify_custodian_config_update     ┌─────────────────┐
│                │ ─────────────────────────────────────> │                 │
│      NONE      │                                        │     PENDING     │
│                │ <───────────────────────────────────── │  (grace period) │
└────────────────┘       cancel_custodian_transition      └─────────────────┘
                                                                   │
                                                                   │ pause_deposits_for_transition
                                                                   │ (after 2 hours)
                                                                   ▼
                         ┌─────────────────┐     process_custodian_transition     ┌─────────────────┐
                         │                 │ <─────────────────────────────────── │                 │
                         │    COMPLETED    │                                      │ DEPOSITS_PAUSED │
                         │   (new config)  │                                      │ (consolidation) │
                         └─────────────────┘                                      └─────────────────┘
```

### States

1. **NONE**: No transition in progress. Normal bridge operation.
2. **PENDING**: Transition has been notified. Grace period is active (2 hours). Deposits are still accepted.
3. **DEPOSITS_PAUSED**: Grace period has elapsed. Deposits are blocked. UTXOs are being consolidated.
4. **COMPLETED**: Transition complete. New custodian configuration is active.

## Grace Period

The 2-hour grace period serves several purposes:

1. **User Safety**: Users with pending deposits have time to complete their deposits before the transition
2. **Reorg Protection**: Provides buffer for any blockchain reorganizations
3. **Operational Flexibility**: Allows the operator to cancel if issues are discovered

During the grace period:
- Deposits are still accepted and processed normally
- The bridge continues normal operation
- The operator can cancel the transition if needed

## Instructions

### 1. notify_custodian_config_update

Initiates a custodian transition by recording the new configuration hash.

**Accounts:**
- `bridge_state` (writable): The bridge state PDA
- `custodian_set_manager_account` (readonly): Account containing the new custodian config
- `operator` (signer): The bridge operator

**Parameters:**
- `expected_new_custodian_config_hash`: The expected hash of the new custodian configuration

**Effects:**
- Records the transition start timestamp
- Stores the incoming custodian config hash
- Starts the 2-hour grace period

### 2. pause_deposits_for_transition

Pauses deposits after the grace period has elapsed.

**Accounts:**
- `bridge_state` (writable): The bridge state PDA
- `operator` (signer): The bridge operator

**Requirements:**
- A transition must be pending
- The 2-hour grace period must have elapsed
- Deposits must not already be paused

**Effects:**
- Sets `deposits_paused_mode` to `PAUSED`
- Blocks new auto-claimed deposits
- Blocks new manual deposit claims

### 3. process_custodian_transition

Completes the transition by providing a ZK proof that the return TXO has been transferred to the new custodian.

**Accounts:**
- `bridge_state` (writable): The bridge state PDA
- `generic_buffer_account` (readonly): Contains the transition transaction
- Wormhole accounts for VAA emission

**Parameters:**
- `proof`: ZK proof verifying the transfer from old to new custodian
- `new_return_output`: The new return output controlled by the new custodian

**Requirements:**
- All deposits must be consolidated (verified by `total_spent_deposit_utxo_count >= consolidation_target`)
- Deposits must be paused

**Effects:**
- Verifies the ZK proof (transfer from old to new custodian)
- Updates the custodian config hash to the new value
- Updates the return output
- Resets transition state
- Sets deposits back to active mode
- Emits a Wormhole VAA to signal completion

### 4. cancel_custodian_transition

Cancels a pending transition (emergency use).

**Accounts:**
- `bridge_state` (writable): The bridge state PDA
- `operator` (signer): The bridge operator

**Requirements:**
- A transition must be pending

**Effects:**
- Clears the transition timestamp
- Clears the incoming config hash
- Sets deposits back to active mode (if paused)

## Consolidation Process

The transition uses a clean separation of concerns:

1. **Consolidation (done via regular withdrawals):** Before the transition can complete, all deposit UTXOs must be spent through normal withdrawal processing. This increments `total_spent_deposit_utxo_count`.

2. **Transition (ZKP verified):** The transition ZKP only verifies the final transfer - spending the return TXO from the old custodian to create a new return TXO under the new custodian's control.

The consolidation target is calculated dynamically to be reorg-safe:

```
target = auto_claimed_deposits_next_index + manual_deposits_tree.next_index
```

The program verifies consolidation is complete before allowing the transition:
- Checks `total_spent_deposit_utxo_count >= target`

The ZK proof verifies the transfer transaction:
- Input: old return TXO (controlled by old custodian)
- Output: new return TXO (controlled by new custodian)
- Public inputs: hash(old_return_output, new_return_output, old_custodian_hash, new_custodian_hash)

## CLI Commands

### Query Transition Status

```bash
doge-bridge-cli transition-status --rpc-url <RPC_URL>
```

Displays:
- Current custodian config hash
- Incoming custodian config hash (if transitioning)
- Transition start timestamp
- Deposits paused mode
- Consolidation progress (if applicable)

### Notify Custodian Update

```bash
doge-bridge-cli notify-custodian-update \
  --rpc-url <RPC_URL> \
  --operator-keypair <KEYPAIR_PATH> \
  --custodian-account <PUBKEY> \
  --config-hash <HEX_HASH>
```

### Pause Deposits for Transition

```bash
doge-bridge-cli pause-for-transition \
  --rpc-url <RPC_URL> \
  --operator-keypair <KEYPAIR_PATH>
```

### Cancel Transition

```bash
doge-bridge-cli cancel-transition \
  --rpc-url <RPC_URL> \
  --operator-keypair <KEYPAIR_PATH>
```

## Error Codes

| Code | Name | Description |
|------|------|-------------|
| 960 | NoCustodianTransitionPending | No transition is pending |
| 961 | CustodianTransitionGracePeriodNotElapsed | 2-hour grace period not elapsed |
| 962 | DepositsAlreadyPausedForTransition | Deposits already paused |
| 963 | CustodianTransitionInProgress | Cannot perform action during transition |
| 964 | DepositsNotPausedForTransition | Deposits not paused |
| 965 | InvalidCustodianTransitionProof | ZK proof verification failed |
| 966 | IncompleteConsolidation | Not all UTXOs have been consolidated |
| 967 | DepositsBlockedDuringTransition | Deposits blocked during consolidation |
| 968 | InvalidConsolidationTarget | Invalid consolidation target |

## Security Considerations

1. **Operator Authority**: Only the bridge operator can initiate, pause, and cancel transitions
2. **Grace Period**: 2-hour grace period protects users with pending deposits
3. **ZK Proof Verification**: Consolidation must be cryptographically verified
4. **Reorg Safety**: Consolidation target is calculated to handle reorgs safely
5. **Wormhole Integration**: Transition completion is signaled via Wormhole VAA

## Testing

Run custodian transition tests:

```bash
# Integration tests
make test-custodian-transition

# Local validator tests
make run-custodian-tests
```
