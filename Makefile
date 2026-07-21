.PHONY: ci fmt fmt-check clippy build test audit install-tools

ci: fmt-check clippy build test audit

fmt:
	cargo fmt

fmt-check:
	cargo fmt --check

clippy:
	cargo clippy -- -D warnings

build:
	cargo build

test:
	cargo test

audit: install-tools
	cargo audit

install-tools:
	@cargo audit --version >/dev/null 2>&1 || cargo install cargo-audit --locked