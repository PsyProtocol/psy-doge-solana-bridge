# Pending Mint Buffer

**High-throughput storage for user deposits.**

The `pending-mint-buffer` program implements a specialized storage pattern designed to bypass Solana's transaction size limitations (~1232 bytes).

## The Problem
When a Dogecoin block contains hundreds of deposits, the list of `{ Recipient_Solana_Address, Amount }` pairs is too large to pass to the main bridge program in a single instruction along with the ZK proof.

## The Solution
This program acts as a **Temporary Staging Area**.

1.  **Upload:** The operator uploads the list of pending mints in chunks (e.g., 24 mints per chunk) into a buffer account owned by this program.
2.  **Lock:** During a `Block Update`, the main `doge-bridge` program invokes this program to **LOCK** the buffer.
    *   *Invariant:* Once locked, the data cannot be modified by the operator.
3.  **Verify & Mint:** The `doge-bridge` verifies that the SHA256 hash of the locked buffer matches the `public_inputs` of the ZK proof. It then iterates over the buffer to mint pDOGE tokens.
4.  **Recycle:** Once all mints are processed, the buffer is unlocked and can be reused for the next block.

## Key Features
*   **Zero-Copy Deserialization:** Uses `bytemuck` to map raw byte data directly to `PendingMint` structs for maximum compute efficiency (CU).
*   **Batching:** Optimized for processing groups of 24 mints at a time.