#!/bin/sh
set -eu

cd "$(dirname "$0")/.."
if [ -d "$HOME/.rustup-pc" ]; then
    . scripts/env.sh
fi

cargo fmt --all --check
cargo clippy --locked --all-targets -- -D warnings
cargo clippy --locked --no-default-features --features memory-backend --all-targets -- -D warnings
cargo clippy --locked --features memory-backend,http-server --all-targets -- -D warnings
cargo test --locked --features memory-backend
RUSTDOCFLAGS="-D warnings" cargo doc --locked --features memory-backend,http-server --no-deps

echo 'ok: formatting, clippy, tests and documentation'
