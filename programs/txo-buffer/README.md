# TXO Buffer

**Compressed storage for Dogecoin UTXO ownership.**

This program provides temporary storage for the **UTXO Set Updates**. To securely construct withdrawal transactions, the bridge must know exactly which Unspent Transaction Outputs (UTXOs) on the Dogecoin network are owned by the bridge.

## Purpose

Similar to the `pending-mint-buffer`, the list of UTXO indices (consumed or created in a block) can be large. This program allows the operator to upload a compressed bit-vector or list of indices representing the state changes for a specific Dogecoin block.

## Workflow

1.  **Initialization:** Operator initializes a buffer for a specific Dogecoin Block Height.
2.  **Upload:** Operator writes the UTXO indices.
3.  **Finalization:** The operator marks the buffer as `finalized`.
4.  **Verification:** The main `doge-bridge` program checks that `Hash(TXO_Buffer) == Proof.new_utxo_root`.
5.  **Availability:** This ensures that the bridge state is fully reconstructible from Solana on-chain data. If the off-chain database is lost, the entire UTXO set can be rebuilt by replaying the history of `txo-buffer` transactions.