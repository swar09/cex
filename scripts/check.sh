#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

FIX=0
if [[ "${1:-}" == "--fix" ]]; then
    FIX=1
fi

FAILED=0

FMT_CMD="cargo fmt"
if rustup toolchain list 2>/dev/null | grep -q "nightly"; then
    FMT_CMD="cargo +nightly fmt"
fi

if command -v typos &> /dev/null; then
    if [[ $FIX -eq 1 ]]; then
        typos --write-changes || FAILED=1
    else
        typos || FAILED=1
    fi
fi

if [[ $FIX -eq 1 ]]; then
    $FMT_CMD --all || FAILED=1
    cargo clippy --workspace --all-targets --all-features --fix --allow-dirty --allow-staged || FAILED=1
else
    $FMT_CMD --all -- --check || FAILED=1
    cargo clippy --workspace --all-targets --all-features -- -D warnings || FAILED=1
fi

if command -v pnpm &> /dev/null; then
    if [[ $FIX -eq 1 ]]; then
        pnpm -r --if-present run lint:fix || true
    else
        pnpm -r --if-present run lint || true
    fi
fi

if [[ $FAILED -ne 0 ]]; then
    echo "[x] Checks failed!"
    exit 1
else
    echo "[v] All checks passed!"
fi
