.PHONY: all build test release clean install uninstall docker help

# Variables
BINARY_NAME := reap
INSTALL_PATH := /usr/local/bin
VERSION := $(shell grep '^version' Cargo.toml | sed 's/.*"\(.*\)"/\1/')
TARGET := x86_64-unknown-linux-gnu

# Colors for output
RED := \033[0;31m
GREEN := \033[0;32m
YELLOW := \033[1;33m
NC := \033[0m # No Color

help: ## Show this help message
	@echo "Reaper Build System v${VERSION}"
	@echo ""
	@echo "Usage: make [target]"
	@echo ""
	@echo "Targets:"
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | sort | awk 'BEGIN {FS = ":.*?## "}; {printf "  ${GREEN}%-15s${NC} %s\n", $$1, $$2}'

all: build test ## Build and test everything

build: ## Build debug binary
	@echo "${YELLOW}Building Reaper...${NC}"
	cargo build
	@echo "${GREEN}✓ Build complete${NC}"

test: ## Run all tests
	@echo "${YELLOW}Running tests...${NC}"
	cargo test
	@echo "${GREEN}✓ All tests passed${NC}"

release: ## Build optimized release binary
	@echo "${YELLOW}Building release binary...${NC}"
	RUSTFLAGS="-C target-cpu=native" cargo build --release
	strip target/release/$(BINARY_NAME)
	@echo "${GREEN}✓ Release build complete${NC}"
	@ls -lh target/release/$(BINARY_NAME)

install: release ## Install binary to system
	@echo "${YELLOW}Installing Reaper to $(INSTALL_PATH)...${NC}"
	sudo cp target/release/$(BINARY_NAME) $(INSTALL_PATH)/
	sudo ln -sf $(INSTALL_PATH)/$(BINARY_NAME) $(INSTALL_PATH)/reaper
	@echo "${GREEN}✓ Reaper installed successfully${NC}"

uninstall: ## Remove installed binary
	@echo "${YELLOW}Removing Reaper...${NC}"
	sudo rm -f $(INSTALL_PATH)/$(BINARY_NAME)
	sudo rm -f $(INSTALL_PATH)/reaper
	@echo "${GREEN}✓ Reaper uninstalled${NC}"

clean: ## Clean build artifacts
	@echo "${YELLOW}Cleaning build artifacts...${NC}"
	cargo clean
	rm -rf dist/
	@echo "${GREEN}✓ Cleanup complete${NC}"

fmt: ## Format code
	@echo "${YELLOW}Formatting code...${NC}"
	cargo fmt
	@echo "${GREEN}✓ Code formatted${NC}"

lint: ## Run clippy linter
	@echo "${YELLOW}Running clippy...${NC}"
	cargo clippy -- -D warnings
	@echo "${GREEN}✓ No linting issues${NC}"

audit: ## Run security audit
	@echo "${YELLOW}Running security audit...${NC}"
	cargo audit
	@echo "${GREEN}✓ No security vulnerabilities found${NC}"

docker: ## Build Docker image
	@echo "${YELLOW}Building Docker image...${NC}"
	docker build -t reaper:$(VERSION) .
	docker tag reaper:$(VERSION) reaper:latest
	@echo "${GREEN}✓ Docker image built: reaper:$(VERSION)${NC}"

docker-run: docker ## Run Docker container
	docker run --rm -it reaper:latest

dist: release ## Create distribution package
	@echo "${YELLOW}Creating distribution package...${NC}"
	mkdir -p dist
	tar czf dist/reaper-$(VERSION)-$(TARGET).tar.gz -C target/release $(BINARY_NAME)
	cd dist && sha256sum reaper-$(VERSION)-$(TARGET).tar.gz > reaper-$(VERSION)-$(TARGET).tar.gz.sha256
	@echo "${GREEN}✓ Distribution package created in dist/${NC}"
	@ls -lh dist/

bench: ## Run benchmarks
	@echo "${YELLOW}Running benchmarks...${NC}"
	cargo bench
	@echo "${GREEN}✓ Benchmarks complete${NC}"

check: ## Check code without building
	@echo "${YELLOW}Checking code...${NC}"
	cargo check --all-features
	@echo "${GREEN}✓ Code check complete${NC}"

dev: ## Run in development mode with auto-reload
	@echo "${YELLOW}Starting development mode...${NC}"
	cargo watch -x 'run -- --help'

ci: fmt lint test ## Run CI checks locally
	@echo "${GREEN}✓ All CI checks passed${NC}"