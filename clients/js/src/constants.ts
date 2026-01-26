/**
 * Program IDs and constants for the Doge Bridge.
 */

import { PublicKey } from "@solana/web3.js";

export const BRIDGE_STATE_SEED = "bridge_state";
export const MANUAL_CLAIM_SEED = "manual-claim";
export const MINT_BUFFER_SEED = "mint_buffer";
export const TXO_BUFFER_SEED = "txo_buffer";

export const DOGE_BRIDGE_PROGRAM_ID = new PublicKey("DBjo5tqf2uwt4sg9JznSk9SBbEvsLixknN58y3trwCxJ");
export const MANUAL_CLAIM_PROGRAM_ID = new PublicKey("MCdYbqiK3uj36tohbMjsh3Ssg8iRSJmSHToNxW8TWWE");
export const PENDING_MINT_BUFFER_BUILDER_PROGRAM_ID = new PublicKey("PMUSqycT1j5JTLmHk8frGSCido2h9VG1pyh2MPEa33o");
export const GENERIC_BUFFER_BUILDER_PROGRAM_ID = new PublicKey("GBYLmevzPSBPWfWrJ1h9gNzHqUjDXETzHKL1AasLyKwC");
export const TXO_BUFFER_BUILDER_PROGRAM_ID = new PublicKey("TXWhjswto9q6hfaGPuAhDS79wAHKfbMJLVR178xYAaQ");

// Instruction discriminators
export const DOGE_BRIDGE_INSTRUCTION_INITIALIZE = 0;
export const DOGE_BRIDGE_INSTRUCTION_BLOCK_UPDATE = 1;
export const DOGE_BRIDGE_INSTRUCTION_REQUEST_WITHDRAWAL = 2;
export const DOGE_BRIDGE_INSTRUCTION_PROCESS_WITHDRAWAL = 3;
export const DOGE_BRIDGE_INSTRUCTION_OPERATOR_WITHDRAW_FEES = 4;
export const DOGE_BRIDGE_INSTRUCTION_PROCESS_MANUAL_DEPOSIT = 5;
export const DOGE_BRIDGE_INSTRUCTION_REPLAY_WITHDRAWAL = 6;
export const DOGE_BRIDGE_INSTRUCTION_PROCESS_MINT_GROUP = 7;
export const DOGE_BRIDGE_INSTRUCTION_PROCESS_REORG_BLOCKS = 8;
export const DOGE_BRIDGE_INSTRUCTION_PROCESS_MINT_GROUP_AUTO_ADVANCE = 9;
export const DOGE_BRIDGE_INSTRUCTION_SNAPSHOT_WITHDRAWALS = 10;

export const MC_MANUAL_CLAIM_TRANSACTION_DISCRIMINATOR = 0;

// Buffer constants
export const CHUNK_SIZE = 900;
export const PM_MAX_PENDING_MINTS_PER_GROUP = 24;
