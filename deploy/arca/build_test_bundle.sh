#!/bin/bash
set -euo pipefail
test -f Cargo.lock
test -d /build/target
mkdir -p /build/arca-artifacts
export CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0
export CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
export RUST_MIN_STACK=16777216
export AETHER_BUILD_VERSION=arca-20261016 AETHER_VERSION=arca-20261016
export AETHER_BUILD_TYPE=debug

cargo fmt --all --check
cargo clippy --locked -p aether-gateway -p aether-data -p aether-data-postgres --features aether-gateway/jemalloc --lib -- -D warnings
cargo test --locked --no-run --all-targets \
  -p aether-gateway -p aether-data -p aether-data-postgres \
  -p aether-ai-formats -p aether-data-contracts -p aether-usage-runtime \
  -p aether-ai-serving -p aether-provider-transport -p aether-scheduler-core \
  -p aether-model-fetch -p aether-admin -p aether-data-schema \
  --features aether-gateway/jemalloc \
  --message-format=json > /build/arca-artifacts/test-artifacts.jsonl
cargo build --locked -p aether-gateway --bin aether-gateway --features jemalloc
install -m 0755 /build/target/debug/aether-gateway /build/arca-artifacts/aether-gateway
strip --strip-debug /build/arca-artifacts/aether-gateway
python3 deploy/arca/collect_test_artifacts.py
sha256sum /build/arca-artifacts/aether-gateway
