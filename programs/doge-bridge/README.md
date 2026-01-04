# Doge Bridge (Core Program)

**The Central Hub and Authority of the pDoge Bridge.**

This is the primary Solana smart contract that orchestrates the bridge. It acts as the **Light Client Verifier** for the Dogecoin blockchain and the **Mint Authority** for the wrapped pDOGE token.

## Core Responsibilities

1.  **Light Client Verification:**
    *   Stores the current "Tip" of the Dogecoin blockchain (Block Headers, Merkle Roots).
    *   Verifies **Groth16 Zero-Knowledge Proofs** submitted by the operator. These proofs mathematically certify that a Dogecoin block is valid (PoW) and contains specific transactions.

2.  **State Transitions:**
    *   **Block Updates:** Advances the bridge state when a valid proof for the next Dogecoin block is submitted.
    *   **Reorg Handling:** Supports chain reorganization logic (up to 10 blocks deep) to maintain synchronization with Dogecoin's longest chain rule.

3.  **Asset Management:**
    *   **Minting:** Authorities the minting of pDOGE tokens to users after verifying deposits via the `pending-mint-buffer`.
    *   **Burning/Withdrawals:** Verifies user burn requests and emits Wormhole VAAs to authorize the release of real DOGE on the Dogecoin network.

## Interaction Model

This program rarely acts alone. It relies on the **Buffer Pattern**:
*   It **Locks** `pending-mint-buffer` and `txo-buffer` accounts to ensure data consistency during block verification.
*   It reads from `generic-buffer` accounts to verify large Dogecoin withdrawal transactions that cannot fit in a standard Solana transaction.

## Security

*   **Mint Authority:** This program holds the Mint Authority for the SPL Token. It never delegates this authority; it only executes mints based on verified ZK proofs.
*   **Data Availability:** It enforces that all data required to reconstruct the bridge state (who owns which UTXO) is posted to the auxiliary buffer programs before accepting a state transition.