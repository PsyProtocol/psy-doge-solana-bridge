/**
 * Instruction builders for user operations.
 */

import { PublicKey, TransactionInstruction } from "@solana/web3.js";
import { TOKEN_PROGRAM_ID } from "@solana/spl-token";
import { BRIDGE_STATE_SEED } from "./config";

const DOGE_BRIDGE_INSTRUCTION_REQUEST_WITHDRAWAL = 2;

/**
 * Generate aligned instruction data with the discriminator.
 */
function genAlignedInstruction(instructionDiscriminator: number, dataStructBytes: Uint8Array): Uint8Array {
  const data = new Uint8Array(8 + dataStructBytes.length);
  data.fill(instructionDiscriminator, 0, 8);
  data.set(dataStructBytes, 8);
  return data;
}

/**
 * Get the bridge state PDA.
 */
export function getBridgeStatePda(programId: PublicKey): [PublicKey, number] {
  return PublicKey.findProgramAddressSync(
    [new TextEncoder().encode(BRIDGE_STATE_SEED)],
    programId
  );
}

/**
 * Build a request_withdrawal instruction.
 *
 * @param programId - The bridge program ID
 * @param userAuthority - The user's public key (signer)
 * @param mint - The DOGE mint address
 * @param userTokenAccount - The user's token account
 * @param recipientAddress - 20-byte Dogecoin address (P2PKH hash160)
 * @param amountSats - Amount to withdraw in satoshis
 * @param addressType - Address type (0 for P2PKH)
 */
export function requestWithdrawal(
  programId: PublicKey,
  userAuthority: PublicKey,
  mint: PublicKey,
  userTokenAccount: PublicKey,
  recipientAddress: Uint8Array,
  amountSats: bigint,
  addressType: number
): TransactionInstruction {
  const [bridgeState] = getBridgeStatePda(programId);

  // RequestWithdrawalInstructionData layout:
  // PsyWithdrawalRequest (32 bytes):
  //   - amount_sats: u64 (8 bytes) at offset 0
  //   - address_type: u32 (4 bytes) at offset 8
  //   - recipient_address: [u8; 20] at offset 12
  const dataSize = 32;
  const instructionData = new Uint8Array(dataSize);
  const view = new DataView(instructionData.buffer);

  // PsyWithdrawalRequest
  view.setBigUint64(0, amountSats, true);
  view.setUint32(8, addressType, true);
  instructionData.set(recipientAddress, 12);

  const data = genAlignedInstruction(DOGE_BRIDGE_INSTRUCTION_REQUEST_WITHDRAWAL, instructionData);

  return new TransactionInstruction({
    keys: [
      { pubkey: bridgeState, isSigner: false, isWritable: true },
      { pubkey: userTokenAccount, isSigner: false, isWritable: true },
      { pubkey: mint, isSigner: false, isWritable: true },
      { pubkey: userAuthority, isSigner: true, isWritable: false },
      { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
    ],
    programId,
    data: Buffer.from(data),
  });
}
