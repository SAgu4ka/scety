.PHONY: ci fmt fmt-check clippy build test audit load-smoke install-tools

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

load-smoke:
	./scripts/load_smoke.sh $(URL)

audit: install-tools
	cargo audit

install-tools:
	@cargo audit --version >/dev/null 2>&1 || cargo install cargo-audit --locked