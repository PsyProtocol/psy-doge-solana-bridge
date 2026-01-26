/**
 * Configuration for the user client.
 */

import { PublicKey } from "@solana/web3.js";

/** Default bridge program ID */
export const DEFAULT_BRIDGE_PROGRAM_ID = new PublicKey("DBjo5tqf2uwt4sg9JznSk9SBbEvsLixknN58y3trwCxJ");

/** Bridge state seed */
export const BRIDGE_STATE_SEED = "bridge_state";

/**
 * Get the bridge state PDA for a program ID.
 */
export function getBridgeStatePda(programId: PublicKey): [PublicKey, number] {
  return PublicKey.findProgramAddressSync(
    [new TextEncoder().encode(BRIDGE_STATE_SEED)],
    programId
  );
}

/**
 * Configuration for the UserClient.
 */
export interface UserClientConfig {
  /** Solana RPC URL */
  rpcUrl: string;
  /** Bridge program ID */
  programId: PublicKey;
  /** Bridge state PDA (derived from programId if not provided) */
  bridgeStatePda: PublicKey;
}

/**
 * Builder for UserClientConfig.
 */
export class UserClientConfigBuilder {
  private _rpcUrl?: string;
  private _programId?: PublicKey;
  private _bridgeStatePda?: PublicKey;

  /**
   * Set the RPC URL.
   */
  rpcUrl(url: string): this {
    this._rpcUrl = url;
    return this;
  }

  /**
   * Set the bridge program ID.
   */
  programId(id: PublicKey): this {
    this._programId = id;
    return this;
  }

  /**
   * Set the bridge state PDA.
   */
  bridgeStatePda(pda: PublicKey): this {
    this._bridgeStatePda = pda;
    return this;
  }

  /**
   * Build the configuration.
   */
  build(): UserClientConfig {
    if (!this._rpcUrl) {
      throw new Error("RPC URL is required");
    }

    const programId = this._programId ?? DEFAULT_BRIDGE_PROGRAM_ID;
    const [derivedPda] = getBridgeStatePda(programId);
    const bridgeStatePda = this._bridgeStatePda ?? derivedPda;

    return {
      rpcUrl: this._rpcUrl,
      programId,
      bridgeStatePda,
    };
  }
}
