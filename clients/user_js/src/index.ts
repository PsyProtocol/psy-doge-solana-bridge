/**
 * Doge Bridge User Client
 *
 * A TypeScript client for end-users to interact with the Doge bridge on Solana.
 *
 * Features:
 * - Create associated token accounts for DOGE tokens
 * - Transfer DOGE tokens between Solana accounts
 * - Request withdrawals to Dogecoin addresses
 * - Set close authority to null for token accounts
 *
 * @example
 * ```typescript
 * import { UserClient } from "@psy/doge-bridge-user-client";
 *
 * const client = UserClient.create("https://api.mainnet-beta.solana.com");
 *
 * // Create a token account for DOGE
 * const [signature, tokenAccount] = await client.createTokenAccount(userKeypair);
 * console.log("Token account created:", tokenAccount.toString());
 *
 * // Get balance
 * const balance = await client.getBalance(userKeypair.publicKey);
 * console.log("Balance:", balance, "satoshis");
 *
 * // Transfer tokens
 * await client.transfer(senderKeypair, recipientPubkey, 100000000n);
 *
 * // Request withdrawal to Dogecoin
 * await client.requestWithdrawal(
 *   userKeypair,
 *   dogeAddressBytes, // 20-byte hash160
 *   100000000n,       // 1 DOGE
 *   0                 // P2PKH address type
 * );
 * ```
 */

// Client
export { UserClient } from "./client";

// Configuration
export {
  UserClientConfig,
  UserClientConfigBuilder,
  DEFAULT_BRIDGE_PROGRAM_ID,
  BRIDGE_STATE_SEED,
  getBridgeStatePda,
} from "./config";

// Errors
export { UserClientError } from "./errors";

// Instructions
export { requestWithdrawal, getBridgeStatePda as getBridgeStatePdaFromInstructions } from "./instructions";

// Manual Claim Client
export {
  ManualClaimClient,
  ManualClaimClientConfig,
  ManualClaimClientConfigBuilder,
  ManualClaimInstruction,
  ParsedManualClaim,
  DEFAULT_MANUAL_CLAIM_PROGRAM_ID,
  MANUAL_CLAIM_SEED,
  MC_MANUAL_CLAIM_TRANSACTION_DISCRIMINATOR,
  MANUAL_CLAIM_INSTRUCTION_SIZE,
  createEmptyManualClaimInstruction,
} from "./manual_claim_client";
