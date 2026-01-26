# Local Network Tests

Integration tests for the Doge Bridge that run on a real local Solana validator.

## Overview

These tests complement the `solana-program-test` based tests in `tests/integration/`. While the integration tests use a simulated environment with instant finality, these tests run against an actual local Solana validator, providing:

- Real program deployment and execution
- Actual transaction confirmation times
- Real RPC interactions
- Testing of the actual compiled `.so` binaries

## Prerequisites

1. **Solana CLI Tools** - Install from [Solana docs](https://docs.solana.com/cli/install-solana-cli-tools)
   ```bash
   sh -c "$(curl -sSfL https://release.solana.com/stable/install)"
   ```

2. **cargo-build-sbf** - For building Solana programs
   ```bash
   cargo install --locked cargo-build-sbf
   ```

3. Verify installation:
   ```bash
   make check-tools
   ```

## Quick Start

### Option 1: Full Setup (Recommended for first run)

```bash
# From repository root
make setup-and-test
```

This will:
1. Stop any running validator
2. Clean ledger data
3. Start a fresh validator
4. Build and deploy all programs
5. Run the local network tests

### Option 2: Manual Setup

```bash
# 1. Start the validator
make start-validator

# 2. Build and deploy programs
make deploy-programs

# 3. Run tests
make run-local-tests

# 4. When done, stop the validator
make stop-validator
```

### Option 3: Development Workflow

If the validator is already running:

```bash
make dev-test
```

This will rebuild, redeploy, and retest without restarting the validator.

## Running Individual Tests

```bash
cd tests/local-network-tests

# Run all tests
cargo test -- --nocapture

# Run a specific test
cargo test test_reorg_with_fast_forward -- --nocapture

# Run tests with verbose output
RUST_LOG=debug cargo test -- --nocapture
```

## Program IDs

The local network tests use program keypairs stored in `program-keys/`. These generate specific program IDs that differ from the hardcoded production IDs:

```bash
make show-program-ids
```

Current program IDs:
- **Doge Bridge**: `DBjo5tqf2uwt4sg9JznSk9SBbEvsLixknN58y3trwCxJ`
- **Pending Mint**: `PMUSqycT1j5JTLmHk8frGSCido2h9VG1pyh2MPEa33o`
- **TXO Buffer**: `TXWhjswto9q6hfaGPuAhDS79wAHKfbMJLVR178xYAaQ`
- **Generic Buffer**: `GBYLmevzPSBPWfWrJ1h9gNzHqUjDXETzHKL1AasLyKwC`
- **Manual Claim**: `MCdYbqiK3uj36tohbMjsh3Ssg8iRSJmSHToNxW8TWWE`

## Test Files

| File | Description |
|------|-------------|
| `tests/test_reorg.rs` | Tests blockchain reorg scenarios, fast-forward through empty blocks |

## Architecture

### Key Components

1. **LocalTestClient** (`src/local_test_client.rs`)
   - Comprehensive RPC client with retry logic
   - Transaction sending with confirmation polling
   - Buffer management (pending mints, TXO, generic)
   - Token operations

2. **LocalBridgeContext** (`src/local_bridge_context.rs`)
   - Test context setup
   - Program verification
   - Mint creation

3. **LocalBlockTransitionHelper** (`src/local_block_transition_helper.rs`)
   - Simulates block mining on local network
   - Handles reorg scenarios
   - Manages user accounts and deposits

### Differences from Integration Tests

| Aspect | Integration Tests | Local Network Tests |
|--------|------------------|---------------------|
| Client | `BanksClient` | `RpcClient` |
| Finality | Instant | Real confirmation |
| Programs | In-process | Deployed `.so` files |
| Speed | Fast | Slower (network latency) |
| Use Case | Unit-style tests | E2E validation |

## Troubleshooting

### "Failed to create LocalBridgeContext"

Make sure the validator is running and programs are deployed:
```bash
make start-validator
make deploy-programs
```

### "Program not deployed"

Redeploy the programs:
```bash
make deploy-programs
```

### Transaction timeout

The tests have a 60-second confirmation timeout. If you see timeouts:
1. Check validator is running: `solana cluster-version`
2. Check validator logs in `test-ledger/validator.log`
3. Try restarting: `make stop-validator && make start-validator`

### Airdrop failed

The local validator has unlimited SOL for airdrops. If airdrop fails:
1. Check validator health: `solana cluster-version`
2. Try manual airdrop: `solana airdrop 100 <PUBKEY> --url localhost`

## Adding New Tests

1. Create a new test file in `tests/local-network-tests/tests/`
2. Use `LocalBridgeContext::new()` to set up the test environment
3. Use `LocalBlockTransitionHelper` for block simulation
4. Run with `cargo test <test_name> -- --nocapture`

Example:
```rust
#[tokio::test]
async fn test_my_feature() {
    let ctx = LocalBridgeContext::new().await
        .expect("Failed to create context");

    // Initialize bridge
    ctx.client.initialize_bridge(&params).await.unwrap();

    // Create helper
    let mut helper = LocalBlockTransitionHelper::new_from_client(
        ctx.client.try_clone().unwrap()
    ).await.unwrap();

    // Add users and mine blocks
    let user = helper.add_user();
    helper.mine_and_process_block(vec![deposit]).await.unwrap();

    // Verify results
    assert_eq!(ctx.client.get_token_balance(&ata).await.unwrap(), expected);
}
```
