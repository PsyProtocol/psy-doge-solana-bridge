/**
 * Error types for the user client.
 */

/**
 * Errors that can occur when using the user client.
 */
export class UserClientError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "UserClientError";
  }

  static rpc(message: string): UserClientError {
    return new UserClientError(`RPC error: ${message}`);
  }

  static invalidInput(message: string): UserClientError {
    return new UserClientError(`Invalid input: ${message}`);
  }

  static accountNotFound(address: string): UserClientError {
    return new UserClientError(`Account not found: ${address}`);
  }

  static invalidConfig(message: string): UserClientError {
    return new UserClientError(`Configuration error: ${message}`);
  }

  static transactionFailed(message: string): UserClientError {
    return new UserClientError(`Transaction failed: ${message}`);
  }

  static tokenAccountExists(address: string): UserClientError {
    return new UserClientError(`Token account already exists: ${address}`);
  }

  static insufficientBalance(required: bigint, available: bigint): UserClientError {
    return new UserClientError(`Insufficient balance: required ${required}, available ${available}`);
  }
}
