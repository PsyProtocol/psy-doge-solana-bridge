# Generic Buffer

**General-purpose large data storage for the Bridge.**

While `pending-mint-buffer` and `txo-buffer` have specific schemas and locking logic, the `generic-buffer` is a simple "Byte Bucket".

## Primary Use Case: Withdrawals

When a user wants to withdraw funds, the operator constructs a **Dogecoin Transaction**. These transactions can be large (containing multiple inputs, scripts, and signatures), easily exceeding 1kb.

1.  **Write:** The client creates a `generic-buffer` account and writes the raw bytes of the unsigned/partially-signed Dogecoin transaction into it.
2.  **Process:** The client calls `process_withdrawal` on the main `doge-bridge`.
3.  **Read:** The main bridge program reads the raw bytes from the `generic-buffer` account, hashes them to calculate the `Sighash`, and verifies the ZK proof against this hash.

This decouples the data size of the Dogecoin protocol from the instruction size limits of the Solana protocol.