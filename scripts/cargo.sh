#!/bin/sh
# Wrapper so every build uses the rustup stable toolchain and a writable CARGO_HOME.
set -e
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
. "$SCRIPT_DIR/cargo-env.sh"
exec cargo "$@"
