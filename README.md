# pDoge Bridge
<b>NOTE: This is an unaudited, pre-release reference implmentation of pDoge. Do not use in production.</b>

## 1. Overview

<div align="center">
<img src="./static/pdoge_slide.png" width="90%" />
</div>

pDoge is a **maximally-secure, bi-directional bridge** connecting Dogecoin (PoW) and Solana (PoS), utilizing **Zero-Knowledge Proofs (ZKPs)** to cryptographically verify the state of the Dogecoin blockchain directly on Solana.

Unlike standard federation bridges that rely entirely on a committee of signers for both deposits and withdrawals, pDoge creates a **ZK Light Client** on Solana. This allows the Solana smart contract to verify Dogecoin Proof-of-Work (AuxPoW) and transaction inclusion mathematically, removing the need to trust an intermediary for deposits.

### The Security Ceiling
Due to Dogecoin's lack of Taproot (SegWit v1) and limited scripting capabilities, trustless verification of external chains *on* Dogecoin is technically impossible at the consensus level. Therefore, pDoge implements the **theoretical maximum security** possible for a Dogecoin bridge today:
*   **Inbound (Doge $\to$ Sol):** **Trustless.** Verified via ZK Proofs checking PoW and Merkle roots.
*   **Outbound (Sol $\to$ Doge):** **Federated.** Secured by the Wormhole Guardian set, as Dogecoin cannot natively verify Solana state proofs.

---

## 2. Core Architecture

The system utilizes a **Hub-and-Spoke** model where the "Hub" is a suite of Solana programs acting as the source of truth.

### 2.1 The "Proof of Math" (Inbound)
The bridge does not rely on oracles to tell it that a deposit happened. It verifies the raw data itself using a zkVM.

1.  **Off-Chain Proving:** An operator runs a Rust-based light client inside the zkVM. This program inputs Dogecoin block headers and transaction data. It computes the heavy SHA256 and Scrypt hashes required to validate Dogecoin's Auxiliary Proof of Work (AuxPoW).
2.  **On-Chain Verification:** The `doge-bridge` program on Solana verifies the resulting Groth16 proof.
3.  **State Update:** If the proof is valid, the bridge updates its internal "Tip" (block height and state root) and accepts the new deposits.

### 2.2 The Buffer Pattern (Data Availability)
A primary engineering challenge on Solana is the transaction size limit (~1232 bytes). It is impossible to submit a ZK proof, a block header, and a list of 1,000 user deposits in a single transaction.

To solve this, pDoge implements a novel **Buffer Pattern**:

1.  **Upload:** The operator uploads data (UTXO indices, pending mints, raw transactions) into dedicated buffer accounts (`txo-buffer`, `pending-mint-buffer`, `generic-buffer`) via multiple transactions.
2.  **Lock & Hash:** The main `doge-bridge` program "locks" these buffers so they cannot be modified. It calculates a SHA256 hash of their contents.
3.  **Verify:** The ZK proof includes these data hashes as **Public Inputs**. The Solana program checks: `Hash(OnChain_Buffer) == Proof_Public_Input`.
4.  **Execute:** Once verified, the data is guaranteed to match the valid Dogecoin block. The bridge then iterates over the buffer to mint tokens or update the UTXO set.

**Security Implication:** This prevents Data Withholding Attacks. The bridge state cannot advance unless the data describing *who* owns the new tokens is publicly posted to Solana's ledger.

---

## 3. Component Breakdown

### A. Programs (On-Chain Logic)

| Program | Function | Key Mechanism |
| :--- | :--- | :--- |
| **`doge-bridge`** | The Brain. Stores block headers, merkle roots, and configuration. | Verifies Groth16 proofs; Authorities minting/burning. |
| **`pending-mint-buffer`** | High-throughput storage for user deposits. | Stores `[Recipient, Amount]` pairs. Decouples proving from minting. |
| **`txo-buffer`** | State tracking for Dogecoin UTXOs. | Stores compressed indices of UTXOs owned by the bridge. |
| **`manual-claim`** | Censorship Resistance module. | Allows users to force-claim a deposit if the operator goes offline. |
| **`generic-buffer`** | Raw byte storage. | Used to upload large Dogecoin withdrawal transactions for verification. |

### B. Libraries (Shared Logic)

*   **`psy-bridge-core`**: Contains the cryptographic primitives. This includes the `FixedMerkleAppendTree` (used to track history without unlimited state growth) and hashing implementations (SHA256, Ripemd160) used by both the off-chain prover and on-chain verifier.
*   **`psy-doge-solana-core`**: Defines the shared state structs (`PsyBridgeProgramState`, `PsyBridgeHeader`). This ensures the off-chain zkVM prover and the on-chain Solana program agree *exactly* on the memory layout of the data being verified.

---

## 4. Asset Flow & User Experience

### Deposit (The "Automagical" Flow)
1.  **Address Derivation:** The user connects their Solana wallet. The client derives a unique **P2SH** (Pay-To-Script-Hash) Dogecoin address based on their Solana Public Key.
2.  **Send:** User sends DOGE to this P2SH address.
3.  **Detection:** The Bridge Operator detects the transaction.
4.  **Proof Generation:** The Operator generates a ZK proof validating the block and the transaction's inclusion.
5.  **Execution:** The Operator submits the proof and the `pending-mint-buffer` to Solana. The bridge mints `pDOGE` directly to the user's Associated Token Account.
    *   *Result:* The user does not need to submit a "claim" transaction or pay SOL gas fees. The asset just appears.

### Withdrawal (The Federated Exit)
1.  **Burn:** The user calls `BurnAndWithdraw` on Solana, specifying a Dogecoin address.
2.  **Verification:** The `doge-bridge` verifies the burn and adds the request to the `requested_withdrawals_tree`.
3.  **Construction:** The operator constructs a Dogecoin transaction spending the bridge's UTXOs to the user.
4.  **VAA Emission:** The Solana program verifies the proposed Dogecoin transaction matches the user's request and emits a **Wormhole VAA**.
5.  **Signing:** The Wormhole Guardian network observes the VAA and signs the Dogecoin transaction, releasing the funds.

---

## 5. Security & Trust Assumptions

1.  **Dogecoin Consensus (Critical):** The bridge relies on the honesty of the Dogecoin miners. A 51% attack on Dogecoin could revert a deposit block. The bridge implements a configurable confirmation delay to mitigate reorg risks.
2.  **Math (Critical):** The system relies on the soundness of the Groth16 proving system and the zkVM implementation.
3.  **Wormhole Federation (Outbound Only):** Withdrawals require 2/3rds of the Wormhole Guardians to be honest.
4.  **Operator Liveness (Non-Critical):** If the Operator goes offline or attempts to censor transactions, the **`manual-claim`** program allows any user to submit their own ZK proof to claim their funds, ensuring the bridge remains permissionless.


## Testing
```bash
./build.sh
cargo test --package doge-bridge-integration-tests --test bridge_flow -- test_bridge_extended_flow --exact --nocapture
cargo test --package doge-bridge-integration-tests --test test_reorg -- test_reorg_with_fast_forward --exact --nocapture
```

## License
Copyright 2025 Zero Knowledge Labs Limited, Carter Jack Feldman
<https://psy.xyz>


Permission is hereby granted, free of charge, to any person obtaining a copy of this software and associated documentation files (the “Software”), to deal in the Software without restriction, including without limitation the rights to use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of the Software, and to permit persons to whom the Software is furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED “AS IS”, WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
