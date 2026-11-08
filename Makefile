.PHONY: check test lint fmt run appimage appdir check-packages gui gui-test gui-shots

# The default workspace members: core, run, cli. No Qt needed.
check:
	cargo check

test:
	cargo test

lint:
	cargo fmt --check
	cargo clippy --all-targets -- -D warnings

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

# The desktop wizard: needs qt6-base, qt6-declarative and kirigami.
gui:
	cargo run -p bindle-gui

gui-test:
	cargo clippy -p bindle-gui --all-targets -- -D warnings
	cargo test -p bindle-gui

# Every screen in each uQ theme fixture, plus the desktop's own theme
# ("none"), into dist/shots/<theme>/.
gui-shots:
	cargo build -p bindle-gui
	scripts/gui-shots.sh
