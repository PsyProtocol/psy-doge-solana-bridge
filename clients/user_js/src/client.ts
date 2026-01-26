/**
 * Main UserClient implementation.
 *
 * Provides a simple interface for end-users to interact with the Doge bridge.
 */

import {
  Connection,
  Keypair,
  PublicKey,
  Transaction,
  TransactionInstruction,
  TransactionSignature,
  sendAndConfirmTransaction,
} from "@solana/web3.js";
import {
  getAssociatedTokenAddress,
  createAssociatedTokenAccountInstruction,
  createTransferInstruction,
  createSetAuthorityInstruction,
  AuthorityType,
  getAccount,
  TOKEN_PROGRAM_ID,
} from "@solana/spl-token";
import { UserClientConfig, UserClientConfigBuilder, DEFAULT_BRIDGE_PROGRAM_ID } from "./config";
import { UserClientError } from "./errors";
import { requestWithdrawal as requestWithdrawalInstruction } from "./instructions";

/**
 * Client for end-users to interact with the Doge bridge on Solana.
 *
 * This client provides simple operations for:
 * - Creating token accounts for DOGE tokens
 * - Transferring DOGE tokens
 * - Requesting withdrawals to Dogecoin
 * - Managing token account authorities
 *
 * @example
 * ```typescript
 * const client = UserClient.create("https://api.mainnet-beta.solana.com");
 *
 * // Create a token account for DOGE
 * const [signature, tokenAccount] = await client.createTokenAccount(userKeypair);
 * console.log("Token account created:", tokenAccount.toString());
 *
 * // Request a withdrawal to Dogecoin
 * const sig = await client.requestWithdrawal(
 *   userKeypair,
 *   dogeAddressBytes,
 *   100000000n, // 1 DOGE
 *   0 // P2PKH
 * );
 * ```
 */
export class UserClient {
  readonly config: UserClientConfig;
  readonly connection: Connection;
  private dogeMintCache?: PublicKey;

  constructor(config: UserClientConfig) {
    this.config = config;
    this.connection = new Connection(config.rpcUrl, "confirmed");
  }

  /**
   * Create a new user client with just an RPC URL.
   * Uses default program IDs for mainnet.
   */
  static create(rpcUrl: string): UserClient {
    const config = new UserClientConfigBuilder()
      .rpcUrl(rpcUrl)
      .build();
    return new UserClient(config);
  }

  /**
   * Create a new user client with a custom program ID.
   */
  static withProgramId(rpcUrl: string, programId: PublicKey): UserClient {
    const config = new UserClientConfigBuilder()
      .rpcUrl(rpcUrl)
      .programId(programId)
      .build();
    return new UserClient(config);
  }

  /**
   * Get the bridge program ID.
   */
  get programId(): PublicKey {
    return this.config.programId;
  }

  /**
   * Get the bridge state PDA.
   */
  get bridgeStatePda(): PublicKey {
    return this.config.bridgeStatePda;
  }

  /**
   * Get the DOGE mint address (cached after first fetch).
   */
  async getDogeMint(): Promise<PublicKey> {
    if (this.dogeMintCache) {
      return this.dogeMintCache;
    }

    const account = await this.connection.getAccountInfo(this.config.bridgeStatePda);
    if (!account) {
      throw UserClientError.accountNotFound(this.config.bridgeStatePda.toString());
    }

    // DOGE mint is at a specific offset in the bridge state
    // Based on the Rust code, it's after header + config + operator + fee_spender
    // For simplicity, we read from a known offset or from the end
    const mintOffset = account.data.length - 32;
    const mintBytes = account.data.slice(mintOffset, mintOffset + 32);
    const mint = new PublicKey(mintBytes);

    this.dogeMintCache = mint;
    return mint;
  }

  /**
   * Get the associated token account address for a user.
   */
  async getTokenAccountAddress(owner: PublicKey): Promise<PublicKey> {
    const dogeMint = await this.getDogeMint();
    return getAssociatedTokenAddress(dogeMint, owner);
  }

  /**
   * Check if a token account exists for the given owner.
   */
  async tokenAccountExists(owner: PublicKey): Promise<boolean> {
    const tokenAccount = await this.getTokenAccountAddress(owner);
    const account = await this.connection.getAccountInfo(tokenAccount);
    return account !== null;
  }

  /**
   * Get the token balance for a user (in satoshis).
   */
  async getBalance(owner: PublicKey): Promise<bigint> {
    const tokenAccountAddress = await this.getTokenAccountAddress(owner);

    try {
      const tokenAccount = await getAccount(this.connection, tokenAccountAddress);
      return tokenAccount.amount;
    } catch (error) {
      throw UserClientError.accountNotFound(tokenAccountAddress.toString());
    }
  }

  // ===========================================================================
  // Core User Operations
  // ===========================================================================

  /**
   * Create an associated token account for the DOGE mint.
   *
   * This creates a token account owned by the user that can hold DOGE tokens.
   * The payer pays for the account creation rent.
   *
   * @param payer - The keypair that will pay for account creation
   * @param owner - The public key that will own the token account (optional, defaults to payer)
   * @returns The transaction signature and the created token account address.
   */
  async createTokenAccount(
    payer: Keypair,
    owner?: PublicKey
  ): Promise<[TransactionSignature, PublicKey]> {
    const ownerPubkey = owner ?? payer.publicKey;
    const dogeMint = await this.getDogeMint();
    const tokenAccount = await getAssociatedTokenAddress(dogeMint, ownerPubkey);

    // Check if account already exists
    if (await this.tokenAccountExists(ownerPubkey)) {
      throw UserClientError.tokenAccountExists(tokenAccount.toString());
    }

    const ix = createAssociatedTokenAccountInstruction(
      payer.publicKey,
      tokenAccount,
      ownerPubkey,
      dogeMint
    );

    const signature = await this.sendAndConfirm([ix], payer);
    return [signature, tokenAccount];
  }

  /**
   * Transfer DOGE tokens to another Solana address.
   *
   * @param sender - The keypair of the sender (must own the tokens)
   * @param recipient - The recipient's public key
   * @param amountSats - Amount to transfer in satoshis
   * @returns The transaction signature.
   */
  async transfer(
    sender: Keypair,
    recipient: PublicKey,
    amountSats: bigint
  ): Promise<TransactionSignature> {
    const dogeMint = await this.getDogeMint();
    const senderTokenAccount = await getAssociatedTokenAddress(dogeMint, sender.publicKey);
    const recipientTokenAccount = await getAssociatedTokenAddress(dogeMint, recipient);

    // Check sender has enough balance
    const balance = await this.getBalance(sender.publicKey);
    if (balance < amountSats) {
      throw UserClientError.insufficientBalance(amountSats, balance);
    }

    const instructions: TransactionInstruction[] = [];

    // Create recipient token account if it doesn't exist
    const recipientAccountInfo = await this.connection.getAccountInfo(recipientTokenAccount);
    if (!recipientAccountInfo) {
      instructions.push(
        createAssociatedTokenAccountInstruction(
          sender.publicKey,
          recipientTokenAccount,
          recipient,
          dogeMint
        )
      );
    }

    // Transfer tokens
    instructions.push(
      createTransferInstruction(
        senderTokenAccount,
        recipientTokenAccount,
        sender.publicKey,
        amountSats
      )
    );

    return this.sendAndConfirm(instructions, sender);
  }

  /**
   * Request a withdrawal from Solana to a Dogecoin address.
   *
   * This burns the DOGE tokens on Solana and queues a withdrawal request
   * that will be processed by the bridge operator to send DOGE on the
   * Dogecoin network.
   *
   * @param user - The keypair of the user requesting the withdrawal
   * @param recipientAddress - 20-byte Dogecoin address (P2PKH hash160)
   * @param amountSats - Amount to withdraw in satoshis
   * @param addressType - Address type (0 for P2PKH)
   * @returns The transaction signature.
   */
  async requestWithdrawal(
    user: Keypair,
    recipientAddress: Uint8Array,
    amountSats: bigint,
    addressType: number = 0
  ): Promise<TransactionSignature> {
    if (recipientAddress.length !== 20) {
      throw UserClientError.invalidInput("Recipient address must be 20 bytes");
    }

    const dogeMint = await this.getDogeMint();
    const userTokenAccount = await getAssociatedTokenAddress(dogeMint, user.publicKey);

    // Check balance
    const balance = await this.getBalance(user.publicKey);
    if (balance < amountSats) {
      throw UserClientError.insufficientBalance(amountSats, balance);
    }

    const ix = requestWithdrawalInstruction(
      this.config.programId,
      user.publicKey,
      dogeMint,
      userTokenAccount,
      recipientAddress,
      amountSats,
      addressType
    );

    return this.sendAndConfirm([ix], user);
  }

  /**
   * Set the close authority of a token account to null.
   *
   * This prevents the token account from being closed, which can be useful
   * for security purposes or to ensure the account persists.
   *
   * @param owner - The keypair of the token account owner
   * @param tokenAccount - Optional specific token account (defaults to the owner's ATA)
   * @returns The transaction signature.
   */
  async setCloseAuthorityToNull(
    owner: Keypair,
    tokenAccount?: PublicKey
  ): Promise<TransactionSignature> {
    const dogeMint = await this.getDogeMint();
    const account = tokenAccount ?? await getAssociatedTokenAddress(dogeMint, owner.publicKey);

    const ix = createSetAuthorityInstruction(
      account,
      owner.publicKey,
      AuthorityType.CloseAccount,
      null // Set to null
    );

    return this.sendAndConfirm([ix], owner);
  }

  // ===========================================================================
  // Internal Helpers
  // ===========================================================================

  /**
   * Send a transaction and wait for confirmation.
   */
  private async sendAndConfirm(
    instructions: TransactionInstruction[],
    payer: Keypair,
    extraSigners: Keypair[] = []
  ): Promise<TransactionSignature> {
    const tx = new Transaction().add(...instructions);
    const signers = [payer, ...extraSigners];
    return sendAndConfirmTransaction(this.connection, tx, signers);
  }
}
