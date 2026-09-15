#!/usr/bin/env bash
set -e

# Run all Phase 1 and Audio Foundation automated tests
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORKSPACE_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

echo "Running Noise Remover Linux Automated Test Suite (Phases 1 through 6)..."
cargo test --manifest-path "$SCRIPT_DIR/Cargo.toml" -- --nocapture "$@"
