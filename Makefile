.PHONY: ci fmt clippy build test

ci:
	cargo fmt --check
	cargo clippy -- -D warnings
	cargo build --verbose
	cargo test --verbose

fmt:
	cargo fmt

clippy:
	cargo clippy -- -D warnings

build:
	cargo build

test:
	cargo test