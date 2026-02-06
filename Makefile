# Doge Bridge Makefile
# Manages local Solana validator, program deployment, and testing

.PHONY: help check-tools start-validator stop-validator build-programs deploy-programs \
        run-local-tests run-history-tests run-client-tests clean-ledger setup-and-test dev-test show-program-ids \
        build-cli init-bridge create-users setup-user-atas setup-bridge setup-bridge-clean setup-dev-env \
        setup-dev-env-fast setup-metaplex-programs test-custodian-transition run-custodian-tests

# Metaplex program paths for local validator
METAPLEX_LOCAL_DIR := $(HOME)/.local/share/metaplex-local-validator
MPL_TOKEN_METADATA_SO := $(METAPLEX_LOCAL_DIR)/mpl-token-metadata.so
MPL_BUBBLEGUM_SO := $(METAPLEX_LOCAL_DIR)/mpl-bubblegum.so
MPL_CORE_SO := $(METAPLEX_LOCAL_DIR)/mpl-core.so

# Metaplex program IDs
MPL_TOKEN_METADATA_ID := metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s
MPL_BUBBLEGUM_ID := BGUMAp9Gq7iTEuizy4pqaxsTyUCBK68MDfK752saRPUY
MPL_CORE_ID := CoREENxT6tW1HoK8ypY1SxRMZTcVPm7R94rH4PZNhX7d

# Disable proxies for localhost connections
export no_proxy := localhost,127.0.0.1
export NO_PROXY := localhost,127.0.0.1

# Default target
help:
	@echo "Doge Bridge Makefile"
	@echo ""
	@echo "Usage: make <target>"
	@echo ""
	@echo "Validator Management:"
	@echo "  start-validator   - Start local Solana test validator"
	@echo "  stop-validator    - Stop local Solana test validator"
	@echo "  clean-ledger      - Remove local validator ledger data"
	@echo ""
	@echo "Building & Deployment:"
	@echo "  build-programs    - Build all Solana programs (BPF)"
	@echo "                      Use SHIM=1 for noopshim, SHIM=2 for wormhole: make build-programs SHIM=1"
	@echo "  build-cli         - Build the doge-bridge-cli tool"
	@echo "  deploy-programs   - Deploy programs to local validator"
	@echo "                      Use SHIM=1 for noopshim, SHIM=2 for wormhole: make deploy-programs SHIM=1"
	@echo "  show-program-ids  - Display program IDs from keypairs"
	@echo ""
	@echo "Bridge Setup:"
	@echo "  init-bridge          - Initialize bridge from ./bridge-config/doge_config.json"
	@echo "  create-users         - Create 3 test user accounts with DOGE ATAs"
	@echo "  setup-user-atas      - Create ATAs for existing users and set close authority to null"
	@echo "  setup-bridge         - Full bridge setup: init-bridge + create-users"
	@echo "  setup-bridge-clean   - Clean restart: stop validator, reset ledger, deploy, init bridge"
	@echo "  setup-dev-env        - Restart validator, deploy, init bridge, setup existing user ATAs"
	@echo "                         Use M=1 for Metaplex programs: make setup-dev-env M=1"
	@echo "  setup-dev-env-fast   - Same as setup-dev-env but loads programs via validator args (faster)"
	@echo "                         Use M=1 for Metaplex programs: make setup-dev-env-fast M=1"
	@echo "  setup-metaplex-programs - Download Metaplex programs from mainnet for local testing"
	@echo ""
	@echo "Testing:"
	@echo "  run-local-tests   - Run all local network integration tests"
	@echo "  run-history-tests - Run history sync tests (requires validator)"
	@echo "  run-client-tests  - Run bridge client tests (requires validator)"
	@echo "  run-integration   - Run solana-program-test integration tests"
	@echo "  test-custodian-transition - Run custodian transition integration tests"
	@echo "  run-custodian-tests       - Run custodian transition tests on local validator"
	@echo ""
	@echo "Workflows:"
	@echo "  setup-and-test    - Full setup: start validator, deploy, run tests"
	@echo "  dev-test          - Quick dev cycle: build, deploy, test (validator must be running)"
	@echo ""
	@echo "Utilities:"
	@echo "  check-tools       - Verify Solana CLI tools are installed"

# Program keypair paths
PROGRAM_KEYS_DIR := tests/local-network-tests/program-keys
DOGE_BRIDGE_KEY := $(PROGRAM_KEYS_DIR)/doge-bridge.json
PENDING_MINT_KEY := $(PROGRAM_KEYS_DIR)/pending-mint.json
TXO_BUFFER_KEY := $(PROGRAM_KEYS_DIR)/txo-buffer.json
GENERIC_BUFFER_KEY := $(PROGRAM_KEYS_DIR)/generic-buffer.json
MANUAL_CLAIM_KEY := $(PROGRAM_KEYS_DIR)/manual-claim.json
NOOP_SHIM_KEY := $(PROGRAM_KEYS_DIR)/noop-shim.json
CUSTODIAN_SET_MANAGER_KEY := keys/custodian-set-manager.json

# Program binary paths
BUILD_DIR := target/sbpf-solana-solana/release
DOGE_BRIDGE_SO := $(BUILD_DIR)/doge_bridge.so
PENDING_MINT_SO := $(BUILD_DIR)/pending_mint_buffer.so
TXO_BUFFER_SO := $(BUILD_DIR)/txo_buffer.so
GENERIC_BUFFER_SO := $(BUILD_DIR)/generic_buffer.so
MANUAL_CLAIM_SO := $(BUILD_DIR)/manual_claim.so
NOOP_SHIM_SO := $(BUILD_DIR)/noop_shim.so
CUSTODIAN_SET_MANAGER_SO := $(BUILD_DIR)/custodian_set_manager.so

# Check that required tools are installed
check-tools:
	@echo "Checking for required Solana tools..."
	@command -v solana >/dev/null 2>&1 || { \
		echo "ERROR: 'solana' CLI not found."; \
		echo "Please install Solana tools: https://docs.solana.com/cli/install-solana-cli-tools"; \
		exit 1; \
	}
	@command -v solana-test-validator >/dev/null 2>&1 || { \
		echo "ERROR: 'solana-test-validator' not found."; \
		echo "Please install Solana tools: https://docs.solana.com/cli/install-solana-cli-tools"; \
		exit 1; \
	}
	@command -v cargo-build-sbf >/dev/null 2>&1 || { \
		echo "ERROR: 'cargo-build-sbf' not found."; \
		echo "Please install: cargo install --locked cargo-build-sbf"; \
		exit 1; \
	}
	@echo "All required tools found!"
	@echo "  solana: $$(solana --version)"
	@echo "  solana-test-validator: $$(solana-test-validator --version 2>&1 | head -1)"

# Start local Solana test validator
start-validator: check-tools
	@echo "Starting local Solana test validator..."
	@if pgrep -f "solana-test-validator" > /dev/null; then \
		echo "Validator is already running."; \
	else \
		solana-test-validator --reset > /dev/null 2>&1 & \
		echo "Waiting for validator to start..."; \
		sleep 5; \
		for i in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15; do \
			if solana cluster-version --url localhost 2>/dev/null; then \
				break; \
			fi; \
			echo "Waiting... ($$i)"; \
			sleep 2; \
		done; \
		solana config set --url localhost > /dev/null 2>&1 || true; \
		echo "Airdropping SOL to default keypair..."; \
		solana airdrop 500 --url localhost 2>/dev/null || true; \
		echo "Validator started successfully!"; \
	fi

# Stop local Solana test validator
stop-validator:
	@echo "Stopping local Solana test validator..."
	@pkill -f "solana-test-validator" 2>/dev/null || echo "No validator process found."
	@echo "Validator stopped."

# Clean ledger data
clean-ledger: stop-validator
	@echo "Cleaning ledger data..."
	@rm -rf test-ledger/
	@echo "Ledger data cleaned."

# Programs to build
PROGRAMS := doge-bridge manual-claim pending-mint-buffer txo-buffer generic-buffer noop-shim custodian-set-manager

# Build all Solana programs
# Use SHIM=1 for noopshim feature, SHIM=2 for wormhole feature
build-programs: check-tools
	@echo "Building Solana programs..."
ifeq ($(SHIM),1)
	@echo "Building doge-bridge with noopshim feature..."
	cargo build-sbf -- --features solprogram --features mock-zkp --features noopshim --no-default-features -p doge-bridge
	cargo build-sbf -- --features solprogram --features mock-zkp --no-default-features \
		$(foreach prog,$(filter-out doge-bridge,$(PROGRAMS)),-p $(prog))
else ifeq ($(SHIM),2)
	@echo "Building doge-bridge with wormhole feature..."
	cargo build-sbf -- --features solprogram --features mock-zkp --features wormhole --no-default-features -p doge-bridge
	cargo build-sbf -- --features solprogram --features mock-zkp --no-default-features \
		$(foreach prog,$(filter-out doge-bridge,$(PROGRAMS)),-p $(prog))
else
	cargo build-sbf -- --features solprogram --features mock-zkp --no-default-features \
		$(foreach prog,$(PROGRAMS),-p $(prog))
endif
	@echo "Programs built successfully!"
	@ls -la $(BUILD_DIR)/*.so 2>/dev/null || echo "No .so files found in $(BUILD_DIR)"

# Show program IDs from keypairs
show-program-ids:
	@echo "=== Program IDs ==="
	@echo "Doge Bridge:    $$(solana-keygen pubkey $(DOGE_BRIDGE_KEY))"
	@echo "Pending Mint:   $$(solana-keygen pubkey $(PENDING_MINT_KEY))"
	@echo "TXO Buffer:     $$(solana-keygen pubkey $(TXO_BUFFER_KEY))"
	@echo "Generic Buffer: $$(solana-keygen pubkey $(GENERIC_BUFFER_KEY))"
	@echo "Manual Claim:   $$(solana-keygen pubkey $(MANUAL_CLAIM_KEY))"
	@echo "Noop Shim:      $$(solana-keygen pubkey $(NOOP_SHIM_KEY))"
	@echo "Custodian Set:  $$(solana-keygen pubkey $(CUSTODIAN_SET_MANAGER_KEY))"

# Deploy programs to local validator
# Use SHIM=1 for noopshim, SHIM=2 for wormhole: make deploy-programs SHIM=1
deploy-programs:
	@$(MAKE) build-programs SHIM=$(SHIM)
	@echo "Deploying programs to local validator..."
	@echo ""
	@echo "Deploying Doge Bridge..."
	solana program deploy --program-id $(DOGE_BRIDGE_KEY) $(DOGE_BRIDGE_SO) --url localhost
	@echo ""
	@echo "Deploying Pending Mint Buffer..."
	solana program deploy --program-id $(PENDING_MINT_KEY) $(PENDING_MINT_SO) --url localhost
	@echo ""
	@echo "Deploying TXO Buffer..."
	solana program deploy --program-id $(TXO_BUFFER_KEY) $(TXO_BUFFER_SO) --url localhost
	@echo ""
	@echo "Deploying Generic Buffer..."
	solana program deploy --program-id $(GENERIC_BUFFER_KEY) $(GENERIC_BUFFER_SO) --url localhost
	@echo ""
	@echo "Deploying Manual Claim..."
	solana program deploy --program-id $(MANUAL_CLAIM_KEY) $(MANUAL_CLAIM_SO) --url localhost
	@echo ""
	@echo "Deploying Noop Shim..."
	solana program deploy --program-id $(NOOP_SHIM_KEY) $(NOOP_SHIM_SO) --url localhost
	@echo ""
	@echo "Deploying Custodian Set Manager..."
	solana program deploy --program-id $(CUSTODIAN_SET_MANAGER_KEY) $(CUSTODIAN_SET_MANAGER_SO) --url localhost
	@echo ""
	@echo "All programs deployed successfully!"
	@$(MAKE) show-program-ids

# Run local network integration tests
# Note: Tests must run serially (--test-threads=1) because they share on-chain state
run-local-tests:
	@echo "Running local network integration tests..."
	@echo "Make sure validator is running and programs are deployed!"
	@echo ""
	cd tests/local-network-tests && NO_PROXY=localhost,127.0.0.1 no_proxy=localhost,127.0.0.1 cargo test -- --test-threads=1 --nocapture

# Run history sync tests only
run-history-tests:
	@echo "Running history sync integration tests..."
	@echo "Make sure validator is running and programs are deployed!"
	@echo ""
	cd tests/local-network-tests && NO_PROXY=localhost,127.0.0.1 no_proxy=localhost,127.0.0.1 cargo test --test test_history_sync -- --test-threads=1 --nocapture

# Run bridge client tests only
run-client-tests:
	@echo "Running bridge client integration tests..."
	@echo "Make sure validator is running and programs are deployed!"
	@echo ""
	cd tests/local-network-tests && NO_PROXY=localhost,127.0.0.1 no_proxy=localhost,127.0.0.1 cargo test --test test_bridge_client -- --test-threads=1 --nocapture

# Run solana-program-test based integration tests
run-integration:
	@echo "Running solana-program-test integration tests..."
	cd tests/integration && cargo test -- --nocapture

# Custodian transition testing
test-custodian-transition:
	@echo "Running custodian transition tests..."
	cd tests/integration && cargo test test_custodian -- --nocapture

# Run custodian transition tests on local validator
run-custodian-tests: run-local-tests
	cd tests/local-network-tests && NO_PROXY=localhost,127.0.0.1 no_proxy=localhost,127.0.0.1 cargo test test_custodian -- --test-threads=1 --nocapture

# Full setup: start validator, deploy programs, run tests
# Use SHIM=1 for noopshim, SHIM=2 for wormhole: make setup-and-test SHIM=1
setup-and-test: stop-validator clean-ledger start-validator
	@$(MAKE) deploy-programs SHIM=$(SHIM)
	@$(MAKE) run-local-tests
	@echo ""
	@echo "=== Setup and Test Complete ==="

# Quick development cycle (assumes validator is running)
# Use SHIM=1 for noopshim, SHIM=2 for wormhole: make dev-test SHIM=1
dev-test:
	@$(MAKE) build-programs SHIM=$(SHIM)
	@$(MAKE) deploy-programs SHIM=$(SHIM)
	@$(MAKE) run-local-tests
	@echo ""
	@echo "=== Dev Test Complete ==="

# Airdrop SOL to a keypair (usage: make airdrop KEYPAIR=/path/to/keypair.json)
airdrop:
	@if [ -z "$(KEYPAIR)" ]; then \
		echo "Usage: make airdrop KEYPAIR=/path/to/keypair.json"; \
		exit 1; \
	fi
	solana airdrop 100 $(KEYPAIR) --url localhost

# Bridge config paths
BRIDGE_CONFIG_DIR := bridge-config
DOGE_CONFIG := $(BRIDGE_CONFIG_DIR)/doge_config.json
BRIDGE_KEYS_DIR := $(BRIDGE_CONFIG_DIR)/keys
BRIDGE_USERS_DIR := $(BRIDGE_CONFIG_DIR)/users
CLI_BIN := target/debug/doge-bridge-cli

# Build the CLI tool
build-cli:
	@echo "Building doge-bridge-cli..."
	cargo build -p doge-bridge-cli
	@echo "CLI built: $(CLI_BIN)"

# Initialize bridge from doge_config.json
# Creates keys if they don't exist, creates DOGE mint, initializes bridge
init-bridge: build-cli
	@echo "=== Initializing Bridge ==="
	@if [ ! -f "$(DOGE_CONFIG)" ]; then \
		echo "ERROR: $(DOGE_CONFIG) not found!"; \
		echo "Please create the doge config file first."; \
		exit 1; \
	fi
	@mkdir -p $(BRIDGE_KEYS_DIR)
	@echo ""
	@echo "Running initialize-from-doge-data..."
	$(CLI_BIN) --rpc-url http://127.0.0.1:8899 \
		initialize-from-doge-data \
		--config $(DOGE_CONFIG) \
		--keys-dir $(BRIDGE_KEYS_DIR) \
		--output $(BRIDGE_CONFIG_DIR)/bridge-output.json \
		--airdrop \
		--yes
	@echo ""
	@echo "=== Bridge Initialized ==="

# Create test user accounts with DOGE ATAs
create-users: build-cli
	@echo "=== Creating Test User Accounts ==="
	@if [ ! -f "$(BRIDGE_CONFIG_DIR)/bridge-output.json" ]; then \
		echo "ERROR: Bridge not initialized! Run 'make init-bridge' first."; \
		exit 1; \
	fi
	@DOGE_MINT=$$(grep -o '"doge_mint": "[^"]*"' $(BRIDGE_CONFIG_DIR)/bridge-output.json | cut -d'"' -f4); \
	echo "DOGE Mint: $$DOGE_MINT"; \
	echo ""; \
	echo "Creating user1..."; \
	$(CLI_BIN) --rpc-url http://127.0.0.1:8899 \
		-k $(BRIDGE_KEYS_DIR)/payer.json \
		create-user \
		--doge-mint $$DOGE_MINT \
		--output $(BRIDGE_USERS_DIR)/user1.json; \
	echo ""; \
	echo "Creating user2..."; \
	$(CLI_BIN) --rpc-url http://127.0.0.1:8899 \
		-k $(BRIDGE_KEYS_DIR)/payer.json \
		create-user \
		--doge-mint $$DOGE_MINT \
		--output $(BRIDGE_USERS_DIR)/user2.json; \
	echo ""; \
	echo "Creating user3..."; \
	$(CLI_BIN) --rpc-url http://127.0.0.1:8899 \
		-k $(BRIDGE_KEYS_DIR)/payer.json \
		create-user \
		--doge-mint $$DOGE_MINT \
		--output $(BRIDGE_USERS_DIR)/user3.json; \
	echo ""; \
	echo "=== All Users Created ==="
	@echo ""
	@echo "User files:"
	@ls -la $(BRIDGE_USERS_DIR)/*.json 2>/dev/null || echo "No user files found"

# Setup ATAs for existing users (reads from bridge-config/users/) and sets close authority to null
setup-user-atas: build-cli
	@echo "=== Setting Up ATAs for Existing Users ==="
	@if [ ! -f "$(BRIDGE_CONFIG_DIR)/bridge-output.json" ]; then \
		echo "ERROR: Bridge not initialized! Run 'make init-bridge' first."; \
		exit 1; \
	fi
	@if [ ! -d "$(BRIDGE_USERS_DIR)" ] || [ -z "$$(ls -A $(BRIDGE_USERS_DIR)/*.json 2>/dev/null)" ]; then \
		echo "No existing user files found in $(BRIDGE_USERS_DIR). Skipping."; \
	else \
		DOGE_MINT=$$(grep -o '"doge_mint": "[^"]*"' $(BRIDGE_CONFIG_DIR)/bridge-output.json | cut -d'"' -f4); \
		echo "DOGE Mint: $$DOGE_MINT"; \
		echo ""; \
		$(CLI_BIN) --rpc-url http://127.0.0.1:8899 \
			-k $(BRIDGE_KEYS_DIR)/payer.json \
			setup-user-atas \
			--doge-mint $$DOGE_MINT \
			--users-dir $(BRIDGE_USERS_DIR) \
			--set-close-authority-null; \
	fi
	@echo ""
	@echo "=== User ATAs Setup Complete ==="

# Full bridge setup: initialize bridge and create users
setup-bridge: init-bridge create-users
	@echo ""
	@echo "=========================================="
	@echo "=== Bridge Setup Complete ==="
	@echo "=========================================="
	@echo ""
	@echo "Configuration files:"
	@echo "  Keys:    $(BRIDGE_KEYS_DIR)/"
	@echo "  Users:   $(BRIDGE_USERS_DIR)/"
	@echo "  Output:  $(BRIDGE_CONFIG_DIR)/bridge-output.json"
	@echo ""
	@cat $(BRIDGE_CONFIG_DIR)/bridge-output.json | grep -E '(bridge_state_pda|doge_mint|operator_pubkey|fee_spender_pubkey)":'

# Clean restart: stop validator, reset ledger, clean bridge config, deploy programs, init bridge + users
# Use SHIM=1 for noopshim, SHIM=2 for wormhole: make setup-bridge-clean SHIM=1
setup-bridge-clean:
	@echo "=========================================="
	@echo "=== Clean Bridge Setup ==="
	@echo "=========================================="
	@echo ""
	@echo "Stopping validator..."
	@pkill -f "solana-test-validator" 2>/dev/null || true
	@sleep 2
	@echo "Cleaning ledger and bridge config..."
	@rm -rf test-ledger/
	@rm -rf $(BRIDGE_KEYS_DIR) $(BRIDGE_USERS_DIR) $(BRIDGE_CONFIG_DIR)/bridge-output.json
	@echo "Starting fresh validator..."
	@solana-test-validator --reset > /dev/null 2>&1 & \
	echo "Waiting for validator to start..."; \
	sleep 5; \
	for i in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15; do \
		if solana cluster-version --url localhost 2>/dev/null; then \
			break; \
		fi; \
		echo "Waiting... ($$i)"; \
		sleep 2; \
	done
	@solana config set --url localhost > /dev/null 2>&1 || true
	@echo ""
	@$(MAKE) deploy-programs SHIM=$(SHIM)
	@echo ""
	@$(MAKE) setup-bridge

# Download Metaplex programs from mainnet for local validator testing
setup-metaplex-programs:
	@echo "=== Setting Up Metaplex Programs ==="
	@mkdir -p $(METAPLEX_LOCAL_DIR)
	@if [ ! -f "$(MPL_TOKEN_METADATA_SO)" ]; then \
		echo "Downloading mpl-token-metadata..."; \
		solana program dump -u m $(MPL_TOKEN_METADATA_ID) $(MPL_TOKEN_METADATA_SO); \
	else \
		echo "mpl-token-metadata already exists"; \
	fi
	@if [ ! -f "$(MPL_BUBBLEGUM_SO)" ]; then \
		echo "Downloading mpl-bubblegum..."; \
		solana program dump -u m $(MPL_BUBBLEGUM_ID) $(MPL_BUBBLEGUM_SO); \
	else \
		echo "mpl-bubblegum already exists"; \
	fi
	@if [ ! -f "$(MPL_CORE_SO)" ]; then \
		echo "Downloading mpl-core..."; \
		solana program dump -u m $(MPL_CORE_ID) $(MPL_CORE_SO); \
	else \
		echo "mpl-core already exists"; \
	fi
	@echo "=== Metaplex Programs Ready ==="
	@ls -la $(METAPLEX_LOCAL_DIR)/

# Dev environment setup: restart validator, keep user files, deploy and init bridge, setup user ATAs
# Use M=1 to include Metaplex programs: make setup-dev-env M=1
# Use SHIM=1 for noopshim, SHIM=2 for wormhole: make setup-dev-env SHIM=1
setup-dev-env:
	@echo "==========================================="
	@echo "=== Setting Up Dev Environment ==="
	@echo "==========================================="
	@echo ""
ifdef M
	@echo "Metaplex mode enabled (M=$(M))"
	@$(MAKE) setup-metaplex-programs
	@echo ""
endif
	@echo "Stopping any existing validator..."
	@pkill -f "solana-test-validator" 2>/dev/null || true
	@sleep 2
	@echo "Removing test-ledger..."
	@rm -rf test-ledger/
	@echo "Starting fresh validator..."
ifdef M
	@solana-test-validator --reset \
		--bpf-program $(MPL_TOKEN_METADATA_ID) $(MPL_TOKEN_METADATA_SO) \
		--bpf-program $(MPL_BUBBLEGUM_ID) $(MPL_BUBBLEGUM_SO) \
		--bpf-program $(MPL_CORE_ID) $(MPL_CORE_SO) \
		> /dev/null 2>&1 & \
	echo "Waiting for validator to start (with Metaplex programs)..."; \
	sleep 5; \
	for i in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15; do \
		if solana cluster-version --url localhost 2>/dev/null; then \
			break; \
		fi; \
		echo "Waiting... ($$i)"; \
		sleep 2; \
	done
else
	@solana-test-validator --reset > /dev/null 2>&1 & \
	echo "Waiting for validator to start..."; \
	sleep 5; \
	for i in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15; do \
		if solana cluster-version --url localhost 2>/dev/null; then \
			break; \
		fi; \
		echo "Waiting... ($$i)"; \
		sleep 2; \
	done
endif
	@solana config set --url localhost > /dev/null 2>&1 || true
	@echo ""
	@echo "Airdropping SOL to default keypair..."
	@solana airdrop 500 --url localhost 2>/dev/null || true
	@echo ""
	@$(MAKE) deploy-programs SHIM=$(SHIM)
	@echo ""
	@$(MAKE) init-bridge
	@echo ""
	@echo "=== Airdropping SOL to Keys ==="
	@if [ -f "$(BRIDGE_KEYS_DIR)/payer.json" ]; then \
		echo "Airdropping to payer..."; \
		solana airdrop 100 $(BRIDGE_KEYS_DIR)/payer.json --url localhost 2>/dev/null || true; \
	fi
	@if [ -f "$(BRIDGE_KEYS_DIR)/operator.json" ]; then \
		echo "Airdropping to operator..."; \
		solana airdrop 100 $(BRIDGE_KEYS_DIR)/operator.json --url localhost 2>/dev/null || true; \
	fi
	@echo ""
	@echo "=== Airdropping SOL to Users ==="
	@for user in $(BRIDGE_USERS_DIR)/*.json; do \
		if [ -f "$$user" ]; then \
			USER_PUBKEY=$$(cat "$$user" | jq -r ".pubkey"); \
			echo "Airdropping to $$user ($$USER_PUBKEY)..."; \
			solana airdrop 10 "$$USER_PUBKEY" --url localhost 2>/dev/null || true; \
		fi; \
	done
	@echo ""
	@$(MAKE) setup-user-atas
	@echo ""
	@echo "==========================================="
	@echo "=== Dev Environment Ready ==="
	@echo "==========================================="
	@echo ""
ifdef M
	@echo "Metaplex programs loaded: Token Metadata, Bubblegum, Core"
endif
ifeq ($(SHIM),1)
	@echo "doge-bridge built with: noopshim"
else ifeq ($(SHIM),2)
	@echo "doge-bridge built with: wormhole"
else
	@echo "doge-bridge built with: default (no shim)"
endif
	@echo "Bridge output: $(BRIDGE_CONFIG_DIR)/bridge-output.json"
	@if [ -f "$(BRIDGE_CONFIG_DIR)/bridge-output.json" ]; then \
		cat $(BRIDGE_CONFIG_DIR)/bridge-output.json | grep -E '(bridge_state_pda|doge_mint|doge_mint_metadata_pda|operator_pubkey)":'; \
	fi

# Fast dev environment setup: loads programs via solana-test-validator args instead of solana program deploy
# This is faster because programs are loaded at validator startup rather than deployed after
# Use M=1 to include Metaplex programs: make setup-dev-env-fast M=1
# Use SHIM=1 for noopshim, SHIM=2 for wormhole: make setup-dev-env-fast SHIM=1
setup-dev-env-fast:
	@$(MAKE) build-programs SHIM=$(SHIM)
	@echo "==========================================="
	@echo "=== Setting Up Dev Environment (Fast) ==="
	@echo "==========================================="
	@echo ""
ifdef M
	@echo "Metaplex mode enabled (M=$(M))"
	@$(MAKE) setup-metaplex-programs
	@echo ""
endif
	@echo "Stopping any existing validator..."
	@pkill -f "solana-test-validator" 2>/dev/null || true
	@sleep 2
	@echo "Removing test-ledger..."
	@rm -rf test-ledger/
	@echo "Starting validator with programs pre-loaded..."
ifdef M
	@solana-test-validator --reset \
		--bpf-program $$(solana-keygen pubkey $(DOGE_BRIDGE_KEY)) $(DOGE_BRIDGE_SO) \
		--bpf-program $$(solana-keygen pubkey $(PENDING_MINT_KEY)) $(PENDING_MINT_SO) \
		--bpf-program $$(solana-keygen pubkey $(TXO_BUFFER_KEY)) $(TXO_BUFFER_SO) \
		--bpf-program $$(solana-keygen pubkey $(GENERIC_BUFFER_KEY)) $(GENERIC_BUFFER_SO) \
		--bpf-program $$(solana-keygen pubkey $(MANUAL_CLAIM_KEY)) $(MANUAL_CLAIM_SO) \
		--bpf-program $$(solana-keygen pubkey $(NOOP_SHIM_KEY)) $(NOOP_SHIM_SO) \
		--bpf-program $$(solana-keygen pubkey $(CUSTODIAN_SET_MANAGER_KEY)) $(CUSTODIAN_SET_MANAGER_SO) \
		--bpf-program $(MPL_TOKEN_METADATA_ID) $(MPL_TOKEN_METADATA_SO) \
		--bpf-program $(MPL_BUBBLEGUM_ID) $(MPL_BUBBLEGUM_SO) \
		--bpf-program $(MPL_CORE_ID) $(MPL_CORE_SO) \
		> /dev/null 2>&1 & \
	echo "Waiting for validator to start (with all programs pre-loaded)..."; \
	sleep 5; \
	for i in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15; do \
		if solana cluster-version --url localhost 2>/dev/null; then \
			break; \
		fi; \
		echo "Waiting... ($$i)"; \
		sleep 2; \
	done
else
	@solana-test-validator --reset \
		--bpf-program $$(solana-keygen pubkey $(DOGE_BRIDGE_KEY)) $(DOGE_BRIDGE_SO) \
		--bpf-program $$(solana-keygen pubkey $(PENDING_MINT_KEY)) $(PENDING_MINT_SO) \
		--bpf-program $$(solana-keygen pubkey $(TXO_BUFFER_KEY)) $(TXO_BUFFER_SO) \
		--bpf-program $$(solana-keygen pubkey $(GENERIC_BUFFER_KEY)) $(GENERIC_BUFFER_SO) \
		--bpf-program $$(solana-keygen pubkey $(MANUAL_CLAIM_KEY)) $(MANUAL_CLAIM_SO) \
		--bpf-program $$(solana-keygen pubkey $(NOOP_SHIM_KEY)) $(NOOP_SHIM_SO) \
		--bpf-program $$(solana-keygen pubkey $(CUSTODIAN_SET_MANAGER_KEY)) $(CUSTODIAN_SET_MANAGER_SO) \
		> /dev/null 2>&1 & \
	echo "Waiting for validator to start (with programs pre-loaded)..."; \
	sleep 5; \
	for i in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15; do \
		if solana cluster-version --url localhost 2>/dev/null; then \
			break; \
		fi; \
		echo "Waiting... ($$i)"; \
		sleep 2; \
	done
endif
	@solana config set --url localhost > /dev/null 2>&1 || true
	@echo ""
	@echo "Programs loaded at startup:"
	@$(MAKE) show-program-ids
	@echo ""
	@echo "Airdropping SOL to default keypair..."
	@solana airdrop 500 --url localhost 2>/dev/null || true
	@echo ""
	@$(MAKE) init-bridge
	@echo ""
	@echo "=== Airdropping SOL to Keys ==="
	@if [ -f "$(BRIDGE_KEYS_DIR)/payer.json" ]; then \
		echo "Airdropping to payer..."; \
		solana airdrop 100 $(BRIDGE_KEYS_DIR)/payer.json --url localhost 2>/dev/null || true; \
	fi
	@if [ -f "$(BRIDGE_KEYS_DIR)/operator.json" ]; then \
		echo "Airdropping to operator..."; \
		solana airdrop 100 $(BRIDGE_KEYS_DIR)/operator.json --url localhost 2>/dev/null || true; \
	fi
	@echo ""
	@echo "=== Airdropping SOL to Users ==="
	@for user in $(BRIDGE_USERS_DIR)/*.json; do \
		if [ -f "$$user" ]; then \
			USER_PUBKEY=$$(cat "$$user" | jq -r ".pubkey"); \
			echo "Airdropping to $$user ($$USER_PUBKEY)..."; \
			solana airdrop 10 "$$USER_PUBKEY" --url localhost 2>/dev/null || true; \
		fi; \
	done
	@echo ""
	@$(MAKE) setup-user-atas
	@echo ""
	@echo "==========================================="
	@echo "=== Dev Environment Ready (Fast Mode) ==="
	@echo "==========================================="
	@echo ""
ifdef M
	@echo "Metaplex programs loaded: Token Metadata, Bubblegum, Core"
endif
ifeq ($(SHIM),1)
	@echo "doge-bridge built with: noopshim"
else ifeq ($(SHIM),2)
	@echo "doge-bridge built with: wormhole"
else
	@echo "doge-bridge built with: default (no shim)"
endif
	@echo "Bridge output: $(BRIDGE_CONFIG_DIR)/bridge-output.json"
	@if [ -f "$(BRIDGE_CONFIG_DIR)/bridge-output.json" ]; then \
		cat $(BRIDGE_CONFIG_DIR)/bridge-output.json | grep -E '(bridge_state_pda|doge_mint|doge_mint_metadata_pda|operator_pubkey)":'; \
	fi
