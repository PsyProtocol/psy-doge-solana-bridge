# Manual Claim

**Censorship Resistance & Disaster Recovery Module.**

The core pDoge bridge is designed for an "Auto-Claiming" UX, where the operator handles the claiming process for users. However, relying solely on an operator introduces a liveness assumption. The `manual-claim` program removes this assumption.

## The Security Guarantee
**"If the bridge holds your funds, you can claim them, even if the operator is dead or malicious."**

## How it works

If a user's deposit is ignored by the operator (not included in the `pending-mint-buffer`):

1.  **User Proving:** The user generates their own ZK proof using the open-source prover.
2.  **Proof Content:** The proof demonstrates:
    *   "My transaction `TxHash` exists in Dogecoin Block `N`."
    *   "Block `N` is finalized in the bridge state."
    *   "This transaction has NOT been claimed in the `Auto-Claim` tree."
    *   "This transaction has NOT been claimed in my `Manual-Claim` tree."
3.  **Submission:** The user submits this proof directly to the `manual-claim` program.
4.  **Execution:** The program verifies the proof against the roots stored in the main `doge-bridge` and issues a CPI call to mint the pDOGE tokens to the user.

This ensures the bridge remains **Permissionless** and **Trust-Minimized**.