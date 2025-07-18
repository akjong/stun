.PHONY: build test clean install run-example check fmt clippy doc

# Default target
all: check test build

# Build the project
build:
	cargo build --release

# Run tests
test:
	cargo test

# Check the project for errors
check:
	cargo check

# Format code
fmt:
	cargo fmt

# Run clippy for linting
clippy:
	cargo clippy -- -D warnings

# Generate documentation
doc:
	cargo doc --no-deps --open

# Clean build artifacts
clean:
	cargo clean

# Install the binary
install: build
	cargo install --path .

# Run example with sample config
run-example:
	RUST_LOG=info cargo run -- -c config.json

# Run example showing programmatic usage
run-example-programmatic:
	cargo run --example programmatic

# Run example showing file config usage
run-example-file:
	cargo run --example file_config

# Build and run tests with verbose output
test-verbose:
	cargo test -- --nocapture

# Check dependencies for updates
outdated:
	cargo outdated

# Audit dependencies for security issues
audit:
	cargo audit

# Run all quality checks
quality: fmt clippy test

# Development workflow
dev: fmt check test

# Release workflow
release: fmt clippy test build doc

# Show help
help:
	@echo "Available targets:"
	@echo "  build              - Build the project in release mode"
	@echo "  test               - Run all tests"
	@echo "  check              - Check project for compilation errors"
	@echo "  fmt                - Format code"
	@echo "  clippy             - Run linting checks"
	@echo "  doc                - Generate and open documentation"
	@echo "  clean              - Clean build artifacts"
	@echo "  install            - Install the binary"
	@echo "  run-example        - Run CLI with sample config"
	@echo "  run-example-programmatic - Run programmatic example"
	@echo "  run-example-file   - Run file config example"
	@echo "  test-verbose       - Run tests with verbose output"
	@echo "  quality            - Run formatting, linting, and tests"
	@echo "  dev                - Development workflow (fmt, check, test)"
	@echo "  release            - Release workflow (all quality checks + build + doc)"
	@echo "  help               - Show this help message"
