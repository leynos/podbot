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
TEST_TIMEOUT ?= $(if $(PODBOT_TEST_TIMEOUT),$(PODBOT_TEST_TIMEOUT),1800)
MDLINT ?= $(shell command -v markdownlint-cli2 2>/dev/null || printf '%s' "$$HOME/.bun/bin/markdownlint-cli2")
WHITAKER ?= whitaker
NIXIE ?= nixie
UV ?= uv
UV_ENV = UV_CACHE_DIR=.uv-cache UV_TOOL_DIR=.uv-tools

# The CV-005 CodeScene contracts live in shared-actions and run from a full
# commit, so a fix is a pin bump. `.github/cv005.toml` holds this repository's
# only parameters.
CV005_CONTRACTS_REF ?= a38feb9be25755c30eca5bda96bd3786a5b89c6b
CV005_CONTRACTS = $(UV_ENV) $(UV) tool run --python 3.13 \
	--from 'git+https://github.com/leynos/shared-actions@$(CV005_CONTRACTS_REF)\#subdirectory=packages/cv005-contracts' \
	cv005-contracts

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
	scripts/test_runner.py scripts/test_runner_cargo.py \
	scripts/test_runner_context.py \
	scripts/test_runner_nested.py scripts/test_runner_phases.py \
	scripts/test_runner_target_arguments.py \
	scripts/test_runner_supervise.py \
	scripts/test_runner_commands.py \
	scripts/test_runner_diagnostics.py scripts/test_runner_process_io.py \
	scripts/test_runner_process_tree.py \
	scripts/test_runner_supervisor.py \
	scripts/test_runner_models.py scripts/test_runner_options.py \
	scripts/test_runner_plan.py scripts/test_runner_registry.py \
	scripts/test_runner_selection.py \
	scripts/check_sccache_health.py scripts/tests/test_check_sccache_health.py \
	scripts/tests/conftest.py scripts/tests/test_workflow_contracts.py \
	scripts/tests/test_command_contracts.py \
	scripts/tests/test_coverage_contracts.py \
	scripts/tests/test_workflow_inventory.py \
	scripts/tests/test_runner_placement_rule.py \
	scripts/tests/test_runner_fixtures.py \
	scripts/tests/test_test_runner_plan.py \
	scripts/tests/test_test_runner_cargo.py \
	scripts/tests/test_test_runner_execution.py \
	scripts/tests/test_test_runner_process_tree.py \
	scripts/tests/test_test_runner_supervise.py \
	scripts/tests/test_test_runner_supervisor.py \
	scripts/tests/test_sccache_fallback_contract.py
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
	RUSTFLAGS="$(RUST_FLAGS)" $(UV_ENV) $(UV) run --no-project --python 3.14 \
		python scripts/test_runner.py --cargo "$(CARGO)" \
		--timeout "$(TEST_TIMEOUT)" -- \
		$(if $(strip $(BUILD_JOBS)),$(BUILD_JOBS) )$(TEST_FLAGS)

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
	$(CV005_CONTRACTS) check --repository .
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
		scripts/test_runner.py scripts/test_runner_cargo.py \
		scripts/test_runner_context.py \
		scripts/test_runner_nested.py scripts/test_runner_phases.py \
		scripts/test_runner_target_arguments.py \
		scripts/test_runner_supervise.py \
		scripts/test_runner_commands.py \
		scripts/test_runner_diagnostics.py scripts/test_runner_process_io.py \
		scripts/test_runner_process_tree.py \
		scripts/test_runner_supervisor.py \
		scripts/test_runner_models.py scripts/test_runner_options.py \
		scripts/test_runner_plan.py scripts/test_runner_registry.py \
		scripts/test_runner_selection.py \
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
