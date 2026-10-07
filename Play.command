#!/bin/bash
# Double-click to build (the first time takes a few minutes) and play.
cd "$(dirname "$0")/ss_port" || exit 1
if ! command -v cargo >/dev/null 2>&1; then
  echo "Rust is not installed: get it from https://rustup.rs, then run this again."
  read -n 1 -s -r -p "Press any key to close."
  exit 1
fi
cargo run --release -- "$@"
