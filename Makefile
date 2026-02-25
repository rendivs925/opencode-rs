SHELL := /usr/bin/env bash

OPENCODE_DIR := packages/opencode
OPENCODE_BIN := $(OPENCODE_DIR)/dist/opencode-linux-x64/bin/opencode
INSTALL_BIN ?= $(HOME)/.opencode/bin/opencode
MODELS_JSON ?= $(HOME)/.cache/opencode/models.json

.PHONY: help opencode-build opencode-install opencode-verify

help:
	@echo "Targets:"
	@echo "  make opencode-build    Build local opencode binary (offline-friendly)"
	@echo "  make opencode-install  Build and install binary to $$HOME/.opencode/bin/opencode"
	@echo "  make opencode-verify   Print installed opencode version"

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
