# AgentHub build environment for this machine.
# The system rustc on PATH is Homebrew 1.87 (too old for Tauri 2.12);
# the rustup-managed stable toolchain (>=1.90) lives in ~/.cargo/bin.
export PATH="$HOME/.cargo/bin:$PATH"
export RUSTUP_TOOLCHAIN="${RUSTUP_TOOLCHAIN:-stable}"
export CARGO_HOME="${AGENTHUB_CARGO_HOME:-/tmp/agenthub-cargo}"
export RUSTUP_HOME="${RUSTUP_HOME:-$HOME/.rustup}"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$PWD/target}"
