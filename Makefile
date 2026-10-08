.PHONY: check test lint fmt run appimage appdir check-packages

check:
	cargo check --workspace

test:
	cargo test --workspace

lint:
	cargo fmt --check
	cargo clippy --workspace --all-targets -- -D warnings

fmt:
	cargo fmt

run:
	cargo run -p bindle-cli --

appdir:
	packaging/appimage/build-appimage.sh --appdir-only

appimage:
	packaging/appimage/build-appimage.sh

check-packages:
	python3 scripts/check-packages.py
