SHELL := /usr/bin/env bash

OPENCODE_DIR := packages/opencode
OPENCODE_BIN := $(OPENCODE_DIR)/dist/opencode-linux-x64/bin/opencode
INSTALL_BIN ?= $(HOME)/.opencode/bin/opencode
MODELS_JSON ?= $(HOME)/.cache/opencode/models.json

.PHONY: help rebuild opencode-build opencode-install opencode-verify core-build dev lint check test

help:
	@echo "OpenCode Build System"
	@echo ""
	@echo "Main targets:"
	@echo "  make rebuild           Build and install OpenCode"
	@echo "  make dev               Quick rebuild for development"
	@echo ""
	@echo " Individual targets:"
	@echo "  make opencode-build    Build local opencode binary"
	@echo "  make opencode-install  Install binary to ~/.opencode/bin/opencode"
	@echo "  make opencode-verify   Print installed version"
	@echo ""
	@echo " Rust core:"
	@echo "  make core-build        Rebuild Rust core (opencode-core)"
	@echo "  make lint               Run clippy linter"
	@echo "  make check              Type check Rust"
	@echo "  make test               Run tests"

rebuild: opencode-install
	@echo "Done! Run 'opencode' to start."

dev: opencode-install
	@echo "Rebuilt and installed."

core-build:
	@cd packages/opencode-core && bun run build:debug

opencode-build:
	@test -f "$(MODELS_JSON)" || (echo "Missing models snapshot: $(MODELS_JSON)" && exit 1)
	@cd $(OPENCODE_DIR) && MODELS_DEV_API_JSON="$(MODELS_JSON)" bun run script/build.ts --single --skip-install

opencode-install: opencode-build
	@mkdir -p "$$(dirname "$(INSTALL_BIN)")"
	@if [ -f "$(INSTALL_BIN)" ]; then cp -f "$(INSTALL_BIN)" "$(INSTALL_BIN).backup"; fi
	@cp -f "$(OPENCODE_BIN)" "$(INSTALL_BIN)"
	@chmod +x "$(INSTALL_BIN)"
	@echo "Installed: $(INSTALL_BIN)"

opencode-verify:
	@"$(INSTALL_BIN)" --version

lint:
	cd packages/opencode-core && cargo clippy --all-targets -- -D warnings

check:
	cd packages/opencode-core && cargo check --all-targets

test:
	cd packages/opencode-core && cargo test --workspace
