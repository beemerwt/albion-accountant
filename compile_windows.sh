#!/usr/bin/env bash
set -euo pipefail

target="${WINDOWS_TARGET:-x86_64-pc-windows-gnu}"

rustup target add "$target"

CARGO_TARGET_DIR=target_windows cargo build --release --target "$target"
