/**
 * Configuration types for the BridgeClient.
 */

import { Keypair, PublicKey } from "@solana/web3.js";
import {
  DOGE_BRIDGE_PROGRAM_ID,
  GENERIC_BUFFER_BUILDER_PROGRAM_ID,
  MANUAL_CLAIM_PROGRAM_ID,
  PENDING_MINT_BUFFER_BUILDER_PROGRAM_ID,
  TXO_BUFFER_BUILDER_PROGRAM_ID,
} from "./constants";

/**
 * Rate limiting configuration for RPC requests.
 */
export interface RateLimitConfig {
  /** Maximum requests per second */
  maxRps: number;
  /** Burst capacity for token bucket */
  burstSize: number;
  /** Whether to queue requests when rate limited */
  queueOnLimit: boolean;
  /** Maximum queue depth before rejecting */
  maxQueueDepth: number;
}

export const DEFAULT_RATE_LIMIT_CONFIG: RateLimitConfig = {
  maxRps: 10,
  burstSize: 20,
  queueOnLimit: true,
  maxQueueDepth: 100,
};

/**
 * Retry configuration for failed operations.
 */
export interface RetryConfig {
  /** Maximum number of retry attempts */
  maxRetries: number;
  /** Initial delay between retries in milliseconds */
  initialDelayMs: number;
  /** Maximum delay between retries in milliseconds */
  maxDelayMs: number;
  /** Multiplier for exponential backoff */
  backoffMultiplier: number;
  /** Transaction confirmation timeout in milliseconds */
  confirmationTimeoutMs: number;
}

export const DEFAULT_RETRY_CONFIG: RetryConfig = {
  maxRetries: 10,
  initialDelayMs: 500,
  maxDelayMs: 30_000,
  backoffMultiplier: 2.0,
  confirmationTimeoutMs: 60_000,
};

/**
 * Parallelism configuration for buffer operations.
 */
export interface ParallelismConfig {
  /** Maximum concurrent write operations */
  maxConcurrentWrites: number;
  /** Maximum concurrent resize operations */
  maxConcurrentResizes: number;
  /** Batch size for group insertions */
  groupBatchSize: number;
}

export const DEFAULT_PARALLELISM_CONFIG: ParallelismConfig = {
  maxConcurrentWrites: 4,
  maxConcurrentResizes: 2,
  groupBatchSize: 4,
};

/**
 * Main configuration for the BridgeClient.
 */
export interface BridgeClientConfig {
  /** Solana RPC URL */
  rpcUrl: string;
  /** Bridge state PDA */
  bridgeStatePda: PublicKey;
  /** Operator keypair (signs operator-only transactions) */
  operator: Keypair;
  /** Payer keypair (pays for transaction fees) */
  payer: Keypair;
  /** DOGE mint address (can be fetched from state if undefined) */
  dogeMint?: PublicKey;
  /** Rate limiting configuration */
  rateLimit: RateLimitConfig;
  /** Retry configuration */
  retry: RetryConfig;
  /** Parallelism configuration */
  parallelism: ParallelismConfig;
  /** Doge bridge program ID */
  programId: PublicKey;
  /** Manual claim program ID */
  manualClaimProgramId: PublicKey;
  /** Pending mint buffer program ID */
  pendingMintProgramId: PublicKey;
  /** TXO buffer program ID */
  txoBufferProgramId: PublicKey;
  /** Generic buffer program ID */
  genericBufferProgramId: PublicKey;
  /** Wormhole core program ID */
  wormholeCoreProgramId: PublicKey;
  /** Wormhole shim program ID */
  wormholeShimProgramId: PublicKey;
}

/**
 * Builder for BridgeClientConfig.
 */
export class BridgeClientConfigBuilder {
  private _rpcUrl?: string;
  private _bridgeStatePda?: PublicKey;
  private _operator?: Keypair;
  private _payer?: Keypair;
  private _dogeMint?: PublicKey;
  private _rateLimit?: RateLimitConfig;
  private _retry?: RetryConfig;
  private _parallelism?: ParallelismConfig;
  private _programId?: PublicKey;
  private _manualClaimProgramId?: PublicKey;
  private _pendingMintProgramId?: PublicKey;
  private _txoBufferProgramId?: PublicKey;
  private _genericBufferProgramId?: PublicKey;
  private _wormholeCoreProgramId?: PublicKey;
  private _wormholeShimProgramId?: PublicKey;

  rpcUrl(url: string): this {
    this._rpcUrl = url;
    return this;
  }

  bridgeStatePda(pda: PublicKey): this {
    this._bridgeStatePda = pda;
    return this;
  }

  operator(keypair: Keypair): this {
    this._operator = keypair;
    return this;
  }

  payer(keypair: Keypair): this {
    this._payer = keypair;
    return this;
  }

  operatorAndPayer(keypair: Keypair): this {
    this._operator = keypair;
    this._payer = keypair;
    return this;
  }

  dogeMint(mint: PublicKey): this {
    this._dogeMint = mint;
    return this;
  }

  rateLimit(config: RateLimitConfig): this {
    this._rateLimit = config;
    return this;
  }

  retry(config: RetryConfig): this {
    this._retry = config;
    return this;
  }

  parallelism(config: ParallelismConfig): this {
    this._parallelism = config;
    return this;
  }

  programId(id: PublicKey): this {
    this._programId = id;
    return this;
  }

  manualClaimProgramId(id: PublicKey): this {
    this._manualClaimProgramId = id;
    return this;
  }

  pendingMintProgramId(id: PublicKey): this {
    this._pendingMintProgramId = id;
    return this;
  }

  txoBufferProgramId(id: PublicKey): this {
    this._txoBufferProgramId = id;
    return this;
  }

  genericBufferProgramId(id: PublicKey): this {
    this._genericBufferProgramId = id;
    return this;
  }

  wormholeCoreProgramId(id: PublicKey): this {
    this._wormholeCoreProgramId = id;
    return this;
  }

  wormholeShimProgramId(id: PublicKey): this {
    this._wormholeShimProgramId = id;
    return this;
  }

  build(): BridgeClientConfig {
    if (!this._rpcUrl) {
      throw new Error("Missing required field: rpcUrl");
    }
    if (!this._bridgeStatePda) {
      throw new Error("Missing required field: bridgeStatePda");
    }
    if (!this._operator) {
      throw new Error("Missing required field: operator");
    }
    if (!this._payer) {
      throw new Error("Missing required field: payer");
    }
    if (!this._wormholeCoreProgramId) {
      throw new Error("Missing required field: wormholeCoreProgramId");
    }
    if (!this._wormholeShimProgramId) {
      throw new Error("Missing required field: wormholeShimProgramId");
    }

    return {
      rpcUrl: this._rpcUrl,
      bridgeStatePda: this._bridgeStatePda,
      operator: this._operator,
      payer: this._payer,
      dogeMint: this._dogeMint,
      rateLimit: this._rateLimit ?? DEFAULT_RATE_LIMIT_CONFIG,
      retry: this._retry ?? DEFAULT_RETRY_CONFIG,
      parallelism: this._parallelism ?? DEFAULT_PARALLELISM_CONFIG,
      programId: this._programId ?? DOGE_BRIDGE_PROGRAM_ID,
      manualClaimProgramId: this._manualClaimProgramId ?? MANUAL_CLAIM_PROGRAM_ID,
      pendingMintProgramId: this._pendingMintProgramId ?? PENDING_MINT_BUFFER_BUILDER_PROGRAM_ID,
      txoBufferProgramId: this._txoBufferProgramId ?? TXO_BUFFER_BUILDER_PROGRAM_ID,
      genericBufferProgramId: this._genericBufferProgramId ?? GENERIC_BUFFER_BUILDER_PROGRAM_ID,
      wormholeCoreProgramId: this._wormholeCoreProgramId,
      wormholeShimProgramId: this._wormholeShimProgramId,
    };
  }
}
