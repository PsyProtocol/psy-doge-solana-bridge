import { Connection, Keypair, PublicKey, sendAndConfirmTransaction, SystemProgram, Transaction } from "@solana/web3.js";
import { createGenericBuffer, createPendingMintBuffer, createTxoBuffer } from "./buffer_utils";
import { getAssociatedTokenAddress } from "@solana/spl-token";
import { 
  createBlockUpdateInstruction, 
  createInitializeInstruction, 
  createRequestWithdrawalInstruction, 
  createProcessWithdrawalInstruction, 
  createProcessManualDepositInstruction,
  createProcessReorgBlocksInstruction,
  createProcessMintGroupInstruction,
  createProcessMintGroupAutoAdvanceInstruction,
  createOperatorWithdrawFeesInstruction,
  createManualClaimInstruction
} from "./instructions";
import { FinalizedBlockMintTxoInfo, InitializeBridgeInstructionData, PendingMint, PsyBridgeHeader, PsyReturnTxOutput } from "./layout";

export class DogeBridgeClient {
  constructor(
    public connection: Connection,
    public payer: Keypair,
    public programId: PublicKey,
    public manualClaimPid: PublicKey,
    public pendingMintPid: PublicKey,
    public txoBufferPid: PublicKey,
    public genericBufferPid: PublicKey
  ) {}

  async initialize(
    feeSpender: PublicKey, 
    dogeMint: PublicKey, 
    params: Omit<InitializeBridgeInstructionData, "operator_pubkey" | "fee_spender_pubkey" | "doge_mint">
  ) {
    const ix = createInitializeInstruction(
      this.programId,
      this.payer.publicKey,
      this.payer.publicKey,
      feeSpender,
      dogeMint,
      params
    );
    const [bridgeStatePda] = PublicKey.findProgramAddressSync([new TextEncoder().encode("bridge_state")], this.programId);
    const space = 15152;
    const rent = await this.connection.getMinimumBalanceForRentExemption(space);

    const createIx = SystemProgram.createAccount({
      fromPubkey: this.payer.publicKey,
      newAccountPubkey: bridgeStatePda,
      lamports: rent,
      space,
      programId: this.programId
    });

    await sendAndConfirmTransaction(this.connection, new Transaction().add(createIx, ix), [this.payer]);
  }

  async submitBlockUpdate(
    proof: Uint8Array, 
    header: PsyBridgeHeader, 
    extraBlocks: FinalizedBlockMintTxoInfo[], 
    newPendingMints: PendingMint[],
    newTxoIndices: number[]
  ) {
    const [bridgeStatePda] = PublicKey.findProgramAddressSync([new TextEncoder().encode("bridge_state")], this.programId);
    
    const mintBufferPda = await createPendingMintBuffer(
      this.connection,
      this.pendingMintPid,
      this.payer,
      bridgeStatePda,
      newPendingMints
    );

    const txoBufferPda = await createTxoBuffer(
      this.connection, 
      this.txoBufferPid, 
      this.payer,
      header.finalized_state.block_height, 
      newTxoIndices
    );

    const [, mintBufferBump] = PublicKey.findProgramAddressSync([new TextEncoder().encode("mint_buffer"), this.payer.publicKey.toBuffer()], this.pendingMintPid);
    const [, txoBufferBump] = PublicKey.findProgramAddressSync([new TextEncoder().encode("txo_buffer"), this.payer.publicKey.toBuffer()], this.txoBufferPid);
    
    let ix;
    if (extraBlocks.length === 0) {
      ix = createBlockUpdateInstruction(
        this.programId, 
        this.payer.publicKey,
        this.payer.publicKey,
        proof, 
        header, 
        mintBufferPda,
        txoBufferPda,
        mintBufferBump,
        txoBufferBump,
        this.pendingMintPid,
        this.txoBufferPid
      );
    } else {
      ix = createProcessReorgBlocksInstruction(
        this.programId,
        this.payer.publicKey,
        this.payer.publicKey,
        proof,
        header,
        extraBlocks,
        mintBufferPda,
        txoBufferPda,
        mintBufferBump,
        txoBufferBump,
        this.pendingMintPid,
        this.txoBufferPid
      );
    }
    
    await sendAndConfirmTransaction(this.connection, new Transaction().add(ix), [this.payer]);
  }

  async requestWithdrawal(user: Keypair, mint: PublicKey, recipientAddress: Uint8Array, amountSats: bigint, addressType: number) {
    const userTokenAccount = await getAssociatedTokenAddress(mint, user.publicKey);
    const ix = createRequestWithdrawalInstruction(
      this.programId,
      user.publicKey,
      mint,
      userTokenAccount,
      recipientAddress,
      amountSats,
      addressType
    );
    const tx = new Transaction().add(ix);
    await sendAndConfirmTransaction(this.connection, tx, [this.payer, user]);
  }

  async processWithdrawal(proof: Uint8Array, newReturnOutput: PsyReturnTxOutput, newSpentTxoRoot: Uint8Array, newNextIndex: bigint, dogeTx: Uint8Array) {
    const genericBuffer = await createGenericBuffer(
      this.connection,
      this.genericBufferPid,
      this.payer,
      dogeTx
    );

    const ix = createProcessWithdrawalInstruction(
      this.programId,
      genericBuffer,
      proof,
      newReturnOutput,
      newSpentTxoRoot,
      newNextIndex
    );
    await sendAndConfirmTransaction(this.connection, new Transaction().add(ix), [this.payer]);
  }

  async processManualDeposit(
    mint: PublicKey,
    recipientAta: PublicKey,
    depositorSolanaKey: PublicKey,
    txHash: Uint8Array,
    recentBlockMerkleTreeRoot: Uint8Array,
    recentAutoClaimTxoRoot: Uint8Array,
    combinedTxoIndex: bigint,
    depositAmountSats: bigint
  ) {
    const ix = createProcessManualDepositInstruction(
      this.programId,
      this.manualClaimPid,
      recipientAta,
      mint,
      txHash,
      recentBlockMerkleTreeRoot,
      recentAutoClaimTxoRoot,
      combinedTxoIndex,
      depositorSolanaKey,
      depositAmountSats
    );
    return ix;
  }

  async processMintGroup(
    dogeMint: PublicKey,
    mintBuffer: PublicKey,
    mintsInGroup: PendingMint[],
    groupIndex: number,
    shouldUnlock: boolean
  ) {
    const recipientAtas = mintsInGroup.map(m => m.recipient);
    const [, mintBufferBump] = PublicKey.findProgramAddressSync([new TextEncoder().encode("mint_buffer"), this.payer.publicKey.toBuffer()], this.pendingMintPid);

    const ix = createProcessMintGroupInstruction(
      this.programId,
      this.payer.publicKey,
      dogeMint,
      mintBuffer,
      this.pendingMintPid,
      recipientAtas,
      groupIndex,
      mintBufferBump,
      shouldUnlock
    );
    await sendAndConfirmTransaction(this.connection, new Transaction().add(ix), [this.payer]);
  }
  
  async processMintGroupAutoAdvance(
    dogeMint: PublicKey,
    mintBuffer: PublicKey,
    txoBuffer: PublicKey,
    mintsInGroup: PendingMint[],
    groupIndex: number,
    shouldUnlock: boolean,
  ) {
    const recipientAtas = mintsInGroup.map(m => m.recipient);
    const [, mintBufferBump] = PublicKey.findProgramAddressSync([new TextEncoder().encode("mint_buffer"), this.payer.publicKey.toBuffer()], this.pendingMintPid);
    const [, txoBufferBump] = PublicKey.findProgramAddressSync([new TextEncoder().encode("txo_buffer"), this.payer.publicKey.toBuffer()], this.txoBufferPid);

    const ix = createProcessMintGroupAutoAdvanceInstruction(
      this.programId,
      this.payer.publicKey,
      dogeMint,
      mintBuffer,
      txoBuffer,
      this.pendingMintPid,
      this.txoBufferPid,
      recipientAtas,
      groupIndex,
      mintBufferBump,
      txoBufferBump,
      shouldUnlock
    );
    await sendAndConfirmTransaction(this.connection, new Transaction().add(ix), [this.payer]);
  }

  async withdrawOperatorFees(operatorTokenAccount: PublicKey) {
    const mint = await this.getDogeMintFromState();
    const ix = createOperatorWithdrawFeesInstruction(
      this.programId,
      this.payer.publicKey,
      operatorTokenAccount,
      mint
    );
    await sendAndConfirmTransaction(this.connection, new Transaction().add(ix), [this.payer]);
  }

  private async getDogeMintFromState(): Promise<PublicKey> {
    const [bridgeState] = PublicKey.findProgramAddressSync([new TextEncoder().encode("bridge_state")], this.programId);
    const account = await this.connection.getAccountInfo(bridgeState);
    if (!account) throw new Error("Bridge state not found");
    return new PublicKey(account.data.subarray(account.data.length - 32));
  }

  async executeManualClaim(
    userSigner: Keypair,
    recipientTokenAccount: PublicKey,
    proof: Uint8Array,
    recentBlockMerkleTreeRoot: Uint8Array,
    recentAutoClaimTxoRoot: Uint8Array,
    newManualClaimTxoRoot: Uint8Array,
    txHash: Uint8Array,
    combinedTxoIndex: bigint,
    depositAmountSats: bigint
  ) {
    const mint = await this.getDogeMintFromState();
    
    const ix = createManualClaimInstruction(
      this.manualClaimPid,
      this.programId,
      this.payer.publicKey,
      userSigner.publicKey,
      mint,
      recipientTokenAccount,
      proof,
      recentBlockMerkleTreeRoot,
      recentAutoClaimTxoRoot,
      newManualClaimTxoRoot,
      txHash,
      combinedTxoIndex,
      depositAmountSats
    );

    const tx = new Transaction().add(ix);
    await sendAndConfirmTransaction(this.connection, tx, [this.payer, userSigner]);
  }
}