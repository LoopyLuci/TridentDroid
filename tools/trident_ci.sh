#!/usr/bin/env bash
# TridentDroid CI harness.
# Usage: ./tools/trident_ci.sh [kernel_path] [system.img] [test_script]
#
# Exit codes:
#   0 — all tests passed
#   1 — test failure
#   2 — infrastructure failure (server didn't start, etc.)
set -euo pipefail

KERNEL="${1:-bzImage}"
SYSTEM_IMG="${2:-system.img}"
TEST_SCRIPT="${3:-}"
GRPC_ADDR="[::1]:9550"
TRIDENTD="./target/release/tridentd"
LOG_FILE="/tmp/tridentd-ci-$$.log"
PID_FILE="/tmp/tridentd-ci-$$.pid"

# ── Helpers ──────────────────────────────────────────────────

log()  { echo "[CI] $*"; }
fail() { echo "[CI] FAIL: $*" >&2; cleanup; exit 1; }
infra_fail() { echo "[CI] INFRA: $*" >&2; cleanup; exit 2; }

cleanup() {
    if [[ -f "$PID_FILE" ]]; then
        kill "$(cat "$PID_FILE")" 2>/dev/null || true
        rm -f "$PID_FILE"
    fi
}
trap cleanup EXIT INT TERM

# ── Build ─────────────────────────────────────────────────────

log "Building tridentd (release)..."
RUSTFLAGS="-C target-cpu=native" cargo build --release 2>&1 || infra_fail "cargo build failed"

# ── Certs ────────────────────────────────────────────────────

if [[ ! -f certs/server.crt ]]; then
    log "Generating mTLS certificates..."
    bash tools/gen_certs.sh || infra_fail "Certificate generation failed"
fi

# ── Start daemon ─────────────────────────────────────────────

log "Starting tridentd..."
"$TRIDENTD" --serve > "$LOG_FILE" 2>&1 &
echo $! > "$PID_FILE"

# Wait for the server to be ready (up to 10s)
for i in $(seq 1 20); do
    if grpcurl -insecure "$GRPC_ADDR" tridentd.TridentDaemon/Ping > /dev/null 2>&1; then
        log "Server is up (attempt $i)"
        break
    fi
    sleep 0.5
    if [[ $i -eq 20 ]]; then
        cat "$LOG_FILE"
        infra_fail "Server did not become ready in 10s"
    fi
done

# ── Launch instance ───────────────────────────────────────────

log "Launching Android instance..."
INSTANCE=$(grpcurl -insecure \
    -cert certs/client.crt -key certs/client.key \
    -d "{\"kernel_path\":\"$KERNEL\",\"system_image\":\"$SYSTEM_IMG\",\"vcpu_count\":4,\"memory_mib\":4096}" \
    "$GRPC_ADDR" tridentd.TridentDaemon/LaunchInstance 2>&1) || fail "LaunchInstance RPC failed"

INSTANCE_ID=$(echo "$INSTANCE" | python3 -c "import sys,json; print(json.load(sys.stdin)['instanceId'])")
log "Instance launched: $INSTANCE_ID"

# ── Wait for boot ────────────────────────────────────────────

log "Waiting for ADB to come up..."
ADB_PORT=$(echo "$INSTANCE" | python3 -c "import sys,json; print(json.load(sys.stdin)['adbPort'])")
for i in $(seq 1 60); do
    if adb connect "127.0.0.1:$ADB_PORT" 2>&1 | grep -q "connected"; then
        log "ADB connected (attempt $i / 60)"
        break
    fi
    sleep 1
    if [[ $i -eq 60 ]]; then
        fail "ADB did not connect within 60s"
    fi
done

# ── Run tests ────────────────────────────────────────────────

if [[ -n "$TEST_SCRIPT" ]]; then
    log "Running test script: $TEST_SCRIPT"
    ADB_SERIAL="127.0.0.1:$ADB_PORT" bash "$TEST_SCRIPT" || fail "Test script failed"
else
    log "No test script provided — running smoke test"
    PROP=$(adb -s "127.0.0.1:$ADB_PORT" shell getprop ro.build.version.release 2>&1)
    log "Android version: $PROP"
    [[ -n "$PROP" ]] || fail "getprop returned empty (boot incomplete?)"
fi

# ── Stop instance ────────────────────────────────────────────

log "Stopping instance $INSTANCE_ID..."
grpcurl -insecure \
    -cert certs/client.crt -key certs/client.key \
    -d "{\"instance_id\":\"$INSTANCE_ID\"}" \
    "$GRPC_ADDR" tridentd.TridentDaemon/StopInstance > /dev/null

log "CI PASSED"
