#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

# ANSI Colors
GREEN="\033[0;32m"
RED="\033[0;31m"
CYAN="\033[0;36m"
BOLD="\033[1m"
RESET="\033[0m"

# Symbols
TICK="✔"
CROSS="✖"
RUNNING="⏳"

# Determine interactive terminal capability
IS_INTERACTIVE=0
if [[ -t 1 && "${TERM:-}" != "dumb" && -n "${TERM:-}" ]]; then
    IS_INTERACTIVE=1
fi

RUN_UNIT=0
RUN_CONTRACT=0
RUN_INTEGRATION=0
RUN_E2E=0
VERBOSE=0

show_help() {
    cat <<EOF
Usage: ./scripts/test.sh [options]

Runs polyglot test suites quietly with summary symbols.

Options:
  --unit          Run only Unit Testing
  --contract      Run only Contract Testing
  --integration   Run only Integration Testing
  --e2e           Run only E2E Testing
  -v, --verbose   Show complete output even on passing tests
  -h, --help      Show this help message

If no category flag is specified, all test types are executed.
EOF
}

for arg in "$@"; do
    case "$arg" in
        --unit)
            RUN_UNIT=1
            ;;
        --contract)
            RUN_CONTRACT=1
            ;;
        --integration)
            RUN_INTEGRATION=1
            ;;
        --e2e)
            RUN_E2E=1
            ;;
        -v|--verbose)
            VERBOSE=1
            ;;
        -h|--help)
            show_help
            exit 0
            ;;
        *)
            echo -e "${RED}Unknown option: $arg${RESET}" >&2
            show_help >&2
            exit 1
            ;;
    esac
done

if [[ $RUN_UNIT -eq 0 && $RUN_CONTRACT -eq 0 && $RUN_INTEGRATION -eq 0 && $RUN_E2E -eq 0 ]]; then
    RUN_UNIT=1
    RUN_CONTRACT=1
    RUN_INTEGRATION=1
    RUN_E2E=1
fi

TMP_LOG_DIR="$(mktemp -d -t cex-tests-XXXXXX)"
cleanup() {
    rm -rf "$TMP_LOG_DIR"
}
trap cleanup EXIT

TOTAL_COUNT=0
PASSED_COUNT=0
FAILED_COUNT=0

run_suite() {
    local label="$1"
    local command_str="$2"
    local log_file="$TMP_LOG_DIR/suite_$TOTAL_COUNT.log"
    TOTAL_COUNT=$((TOTAL_COUNT + 1))

    if [[ $IS_INTERACTIVE -eq 1 ]]; then
        printf "  %b%s%b %s" "$CYAN" "$RUNNING" "$RESET" "$label"
    fi

    local status=0
    eval "$command_str" > "$log_file" 2>&1 || status=$?

    if [[ $status -eq 0 ]]; then
        PASSED_COUNT=$((PASSED_COUNT + 1))
        if [[ $IS_INTERACTIVE -eq 1 ]]; then
            printf "\r\033[K"
        fi
        echo -e "  ${GREEN}${TICK}${RESET} ${label}"
        if [[ $VERBOSE -eq 1 ]]; then
            cat "$log_file"
        fi
    else
        FAILED_COUNT=$((FAILED_COUNT + 1))
        if [[ $IS_INTERACTIVE -eq 1 ]]; then
            printf "\r\033[K"
        fi
        echo -e "  ${RED}${CROSS}${RESET} ${label}"
        cat "$log_file"
        echo ""
    fi
}

echo -e "\n${BOLD}Running Tests...${RESET}\n"

# 1. Unit Testing (Rust workspace)
if [[ $RUN_UNIT -eq 1 ]]; then
    run_suite "Unit Testing" "cargo test --workspace"
fi

# Ensure API gateway binary is built for any test needing it
if [[ $RUN_INTEGRATION -eq 1 || $RUN_E2E -eq 1 ]]; then
    cargo build -p api-gateway --quiet
fi

# 2. Contract Testing (Vitest schema contracts)
if [[ $RUN_CONTRACT -eq 1 ]]; then
    run_suite "Contract Testing" "(cd test && bun run test:contract)"
fi

# 3. Integration Testing (Vitest gateway integration)
if [[ $RUN_INTEGRATION -eq 1 ]]; then
    run_suite "Integration Testing" "(cd test && bun run test:integration)"
fi

# 4. E2E Testing (Vitest end-to-end flows)
if [[ $RUN_E2E -eq 1 ]]; then
    run_suite "E2E Testing" "(cd test && bun run test:e2e)"
fi

echo ""
if [[ $FAILED_COUNT -eq 0 ]]; then
    echo -e "${GREEN}${BOLD}${TICK} All test suites passed (${PASSED_COUNT}/${TOTAL_COUNT})${RESET}\n"
    exit 0
else
    echo -e "${RED}${BOLD}${CROSS} ${FAILED_COUNT} of ${TOTAL_COUNT} test suites failed${RESET}\n"
    exit 1
fi
