/**
 * Error types for the bridge client.
 */

/**
 * Error category for metrics and logging.
 */
export enum ErrorCategory {
  Network = "network",
  Transaction = "transaction",
  Program = "program",
  Buffer = "buffer",
  Config = "config",
  Cryptographic = "cryptographic",
  Validation = "validation",
  Account = "account",
  Internal = "internal",
}

/**
 * Main error type for bridge client operations.
 */
export class BridgeError extends Error {
  readonly category: ErrorCategory;
  readonly retryable: boolean;
  readonly retryHintMs?: number;

  constructor(
    message: string,
    category: ErrorCategory,
    retryable: boolean = false,
    retryHintMs?: number
  ) {
    super(message);
    this.name = "BridgeError";
    this.category = category;
    this.retryable = retryable;
    this.retryHintMs = retryHintMs;
  }

  static rpc(message: string): BridgeError {
    return new BridgeError(`RPC error: ${message}`, ErrorCategory.Network, true, 1000);
  }

  static rateLimited(retryAfterMs: number): BridgeError {
    return new BridgeError(
      `Rate limited, retry after ${retryAfterMs}ms`,
      ErrorCategory.Network,
      true,
      retryAfterMs
    );
  }

  static connectionTimeout(): BridgeError {
    return new BridgeError("Connection timeout", ErrorCategory.Network, true, 1000);
  }

  static simulationFailed(message: string, logs: string[] = []): BridgeError {
    const logStr = logs.length > 0 ? `\nLogs: ${logs.join("\n")}` : "";
    return new BridgeError(
      `Simulation failed: ${message}${logStr}`,
      ErrorCategory.Transaction,
      false
    );
  }

  static confirmationTimeout(timeoutMs: number): BridgeError {
    return new BridgeError(
      `Confirmation timeout after ${timeoutMs}ms`,
      ErrorCategory.Transaction,
      true,
      2000
    );
  }

  static transactionRejected(reason: string): BridgeError {
    return new BridgeError(
      `Transaction rejected: ${reason}`,
      ErrorCategory.Transaction,
      false
    );
  }

  static alreadyProcessed(): BridgeError {
    return new BridgeError(
      "Transaction already processed",
      ErrorCategory.Transaction,
      false
    );
  }

  static invalidBridgeState(message: string): BridgeError {
    return new BridgeError(
      `Invalid bridge state: ${message}`,
      ErrorCategory.Program,
      false
    );
  }

  static pendingMintsNotReady(reason: string): BridgeError {
    return new BridgeError(
      `Pending mints not ready: ${reason}`,
      ErrorCategory.Program,
      false
    );
  }

  static blockHeightMismatch(expected: number, actual: number): BridgeError {
    return new BridgeError(
      `Block height mismatch: expected ${expected}, got ${actual}`,
      ErrorCategory.Program,
      false
    );
  }

  static bufferCreationFailed(message: string): BridgeError {
    return new BridgeError(
      `Buffer creation failed: ${message}`,
      ErrorCategory.Buffer,
      false
    );
  }

  static bufferTooLarge(size: number, maxSize: number): BridgeError {
    return new BridgeError(
      `Buffer too large: ${size} bytes exceeds max ${maxSize}`,
      ErrorCategory.Buffer,
      false
    );
  }

  static bufferNotFound(address: string): BridgeError {
    return new BridgeError(
      `Buffer not found: ${address}`,
      ErrorCategory.Buffer,
      false
    );
  }

  static invalidConfig(message: string): BridgeError {
    return new BridgeError(
      `Invalid configuration: ${message}`,
      ErrorCategory.Config,
      false
    );
  }

  static missingField(field: string): BridgeError {
    return new BridgeError(
      `Missing required field: ${field}`,
      ErrorCategory.Config,
      false
    );
  }

  static invalidProof(): BridgeError {
    return new BridgeError("Invalid ZK proof", ErrorCategory.Cryptographic, false);
  }

  static hashMismatch(expected: string, actual: string): BridgeError {
    return new BridgeError(
      `Hash mismatch: expected ${expected}, got ${actual}`,
      ErrorCategory.Cryptographic,
      false
    );
  }

  static invalidInput(message: string): BridgeError {
    return new BridgeError(
      `Invalid input: ${message}`,
      ErrorCategory.Validation,
      false
    );
  }

  static serializationError(message: string): BridgeError {
    return new BridgeError(
      `Serialization error: ${message}`,
      ErrorCategory.Validation,
      false
    );
  }

  static signerError(): BridgeError {
    return new BridgeError("Signer error", ErrorCategory.Validation, false);
  }

  static accountNotFound(address: string): BridgeError {
    return new BridgeError(
      `Account not found: ${address}`,
      ErrorCategory.Account,
      false
    );
  }

  static insufficientBalance(required: bigint, available: bigint): BridgeError {
    return new BridgeError(
      `Insufficient balance: need ${required}, have ${available}`,
      ErrorCategory.Account,
      false
    );
  }

  static internal(message: string): BridgeError {
    return new BridgeError(message, ErrorCategory.Internal, false);
  }
}

export type BridgeResult<T> = T;
