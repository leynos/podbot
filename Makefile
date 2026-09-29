# `make fmt` and `make check-fmt` call mdtablefix directly. `--git` selects the
# Markdown files Git tracks and `--include-untracked` adds the untracked files
# Git does not ignore, so a new document is formatted before it is staged.
# Both modes need mdtablefix 0.6.0 or later; CI pins the version at its
# install-mdtablefix step.
MDTABLEFIX ?= mdtablefix
MDTABLEFIX_SELECT = --git --include-untracked
MDTABLEFIX_RULES = --wrap --renumber --breaks --ellipsis --fences

.PHONY: help all clean test build release lint typecheck fmt check-fmt audit rust-audit \
        markdownlint nixie spelling test-workflow-contracts workflow-contracts

SHELL := bash

TARGET ?= podbot

CARGO ?= $(shell command -v cargo 2>/dev/null || printf '%s' "$$HOME/.cargo/bin/cargo")
BUILD_JOBS ?=
RUST_FLAGS ?= -D warnings
CARGO_FLAGS ?= --all-targets --all-features
CLIPPY_FLAGS ?= $(CARGO_FLAGS) -- $(RUST_FLAGS)
TEST_FLAGS ?= $(CARGO_FLAGS)
MDLINT ?= $(shell command -v markdownlint-cli2 2>/dev/null || printf '%s' "$$HOME/.bun/bin/markdownlint-cli2")
WHITAKER ?= whitaker
NIXIE ?= nixie
UV ?= uv
UV_ENV = UV_CACHE_DIR=.uv-cache UV_TOOL_DIR=.uv-tools
RUFF_VERSION ?= 0.15.12
PYYAML_VERSION ?= 6.0.3
HYPOTHESIS_VERSION ?= 6.151.9
TYPOS_CONFIG_BUILDER_VERSION ?= v0.1.3
TYPOS_CONFIG_BUILDER = $(UV_ENV) $(UV) tool run --python 3.14 --from \
	"git+https://github.com/leynos/typos-config-builder.git@$(TYPOS_CONFIG_BUILDER_VERSION)" \
	typos-config-builder
WORKFLOW_CONTRACTS_DIR := tests/workflow_contracts
WORKFLOW_CONTRACTS_PYTEST = $(UV_ENV) $(UV) run --no-project --python 3.14 \
	--with pytest==9.0.2 --with pyyaml==$(PYYAML_VERSION) \
	--with hypothesis==$(HYPOTHESIS_VERSION) python -m pytest
WORKFLOW_PY_SRCS := \
	scripts/workflow_contracts.py scripts/workflow_commands.py \
	scripts/workflow_coverage.py scripts/workflow_placement.py \
	scripts/check_sccache_health.py scripts/tests/test_check_sccache_health.py \
	scripts/tests/conftest.py scripts/tests/test_workflow_contracts.py \
	scripts/tests/test_command_contracts.py \
	scripts/tests/test_coverage_contracts.py \
	scripts/tests/test_workflow_inventory.py \
	scripts/tests/test_runner_placement_rule.py
WORKFLOW_PY_TESTS := $(filter scripts/tests/test_%,$(WORKFLOW_PY_SRCS))
WORKFLOW_PYTEST = $(UV_ENV) $(UV) run --no-project --python 3.14 \
	--with pytest==9.0.2 --with pyyaml==6.0.3 python -m pytest

build: target/debug/$(TARGET) ## Build debug binary
release: target/release/$(TARGET) ## Build release binary

all: check-fmt lint test-workflow-contracts workflow-contracts test spelling ## Perform a comprehensive check of code

clean: ## Remove build artefacts
	$(CARGO) clean
	rm -rf .uv-cache .uv-tools

test: ## Run tests with warnings treated as errors
	RUSTFLAGS="$(RUST_FLAGS)" $(CARGO) test $(TEST_FLAGS) $(BUILD_JOBS)

target/%/$(TARGET): ## Build binary in debug or release mode
	$(CARGO) build $(BUILD_JOBS) $(if $(findstring release,$(@)),--release) --bin $(TARGET)

lint: ## Run Clippy and the Whitaker Dylint suite with warnings denied
	RUSTDOCFLAGS="$(RUSTDOC_FLAGS)" $(CARGO) doc --no-deps
	$(CARGO) clippy $(CLIPPY_FLAGS)
	RUSTFLAGS="$(RUST_FLAGS)" $(WHITAKER) --all -- $(CARGO_FLAGS)

typecheck: ## Type-check the workspace
	RUSTFLAGS="$(RUST_FLAGS)" $(CARGO) check $(CARGO_FLAGS) $(BUILD_JOBS)

fmt: ## Format Rust and Markdown sources
	$(CARGO) fmt --all
	$(MDTABLEFIX) --in-place $(MDTABLEFIX_SELECT) $(MDTABLEFIX_RULES)
	$(MDLINT) --fix "**/*.md"

check-fmt: ## Verify formatting
	$(CARGO) fmt --all -- --check
	$(MDTABLEFIX) --check $(MDTABLEFIX_SELECT) $(MDTABLEFIX_RULES)

markdownlint: spelling ## Lint Markdown files and enforce spelling
	$(MDLINT) "**/*.md" "#.uv-cache" "#.uv-tools"

spelling: ## Enforce en-GB-oxendict spelling and shared phrase corrections
	$(TYPOS_CONFIG_BUILDER) gate --repository .

test-workflow-contracts: ## Assert what the workflow files must say
	$(UV_ENV) $(UV) tool run ruff@$(RUFF_VERSION) format --isolated --target-version py313 --check $(WORKFLOW_CONTRACTS_DIR)
	$(UV_ENV) $(UV) tool run ruff@$(RUFF_VERSION) check --isolated --target-version py313 $(WORKFLOW_CONTRACTS_DIR)
	$(WORKFLOW_CONTRACTS_PYTEST) $(WORKFLOW_CONTRACTS_DIR) --doctest-modules \
		-c /dev/null --rootdir=. -p no:cacheprovider -q

workflow-contracts: ## Assert what the workflow files must say
	@$(UV_ENV) $(UV) tool run ruff@$(RUFF_VERSION) format --isolated --target-version py313 --check $(WORKFLOW_PY_SRCS)
	@$(UV_ENV) $(UV) tool run ruff@$(RUFF_VERSION) check --isolated --target-version py313 $(WORKFLOW_PY_SRCS)
	@$(WORKFLOW_PYTEST) $(WORKFLOW_PY_TESTS) \
		scripts/workflow_contracts.py scripts/workflow_commands.py \
		scripts/workflow_coverage.py scripts/workflow_placement.py \
		scripts/check_sccache_health.py \
		--doctest-modules \
		-c /dev/null --rootdir=. -p no:cacheprovider

nixie: ## Validate Mermaid diagrams
	$(NIXIE) --no-sandbox

audit: rust-audit ## Audit dependencies for known vulnerabilities

rust-audit: ## Audit the Rust workspace for known vulnerabilities
	set -eo pipefail; \
	manifest_list=$$(mktemp); \
	trap 'rm -f "$$manifest_list"' EXIT; \
	$(CARGO) metadata --no-deps --format-version 1 | python3 -c 'import json, sys; metadata = json.load(sys.stdin); members = set(metadata["workspace_members"]); print(metadata["workspace_root"]); [print(package["manifest_path"]) for package in metadata["packages"] if package["id"] in members]' > "$$manifest_list"; \
	workspace_root=$$(sed -n '1p' "$$manifest_list"); \
	printf "Auditing Rust workspace %s\n" "$$workspace_root"; \
	sed -n '2,$$p' "$$manifest_list" | while IFS= read -r manifest; do \
		manifest_dir=$$(dirname "$$manifest"); \
		printf "Workspace Rust manifest %s\n" "$$manifest_dir/Cargo.toml"; \
	done; \
	(cd "$$workspace_root" && $(CARGO) audit)

help: ## Show available targets
	@grep -E '^[a-zA-Z_-]+:.*?##' $(MAKEFILE_LIST) | \
	awk 'BEGIN {FS=":"; printf "Available targets:\n"} {printf "  %-20s %s\n", $$1, $$2}'
