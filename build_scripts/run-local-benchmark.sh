#!/usr/bin/env bash
# run-local-benchmark.sh — runs High/Medium/Low clock benchmark locally
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(dirname "$SCRIPT_DIR")"
NUM_RUNS="${1:-1}"
LOG_BASE="${LOG_BASE:-${SCRIPT_DIR}/bench-logs}"
CLUSTER_CFG="${SCRIPT_DIR}/cluster-config.toml"
BIN="${ROOT}/target/debug"

echo "=== Building binaries ==="
cargo build --bin server --bin client --manifest-path="${ROOT}/Cargo.toml" 2>&1 | tail -3

kill_servers() {
    lsof -t -iTCP:8001 -iTCP:8002 -iTCP:8003 2>/dev/null | xargs kill 2>/dev/null || true
    sleep 1
}

run_one() {
    local quality="$1"   # high | medium | low
    local run_idx="$2"
    local log_dir="${LOG_BASE}/${quality}/run-${run_idx}"
    mkdir -p "$log_dir"

    echo ""
    echo "--- ${quality} / run-${run_idx} (logs -> ${log_dir}) ---"

    kill_servers  # ensure clean slate

    # Generate server configs with correct log paths and run index
    for sid in 1 2 3; do
        local nc=0; [[ $sid -le 2 ]] && nc=1
        sed \
            -e "s|./bench-logs/.*server-|${log_dir}/server-|g" \
            -e "s|output_filepath = .*|output_filepath = \"${log_dir}/server-${sid}.json\"|" \
            "${SCRIPT_DIR}/bench-${quality}/server-${sid}.toml" \
            > "${SCRIPT_DIR}/.tmp-server-${sid}.toml"
        # ensure num_clients is right in case toml doesn't have it set
    done

    # Generate client configs pointing to this run's log dir
    for cid in 1 2; do
        sed \
            -e "s|QUALITY|${quality}/run-${run_idx}|g" \
            -e "s|./bench-logs|${LOG_BASE}|g" \
            "${SCRIPT_DIR}/bench-client-${cid}.toml" \
            > "${SCRIPT_DIR}/.tmp-client-${cid}.toml"
    done

    # Start 3 servers
    for sid in 1 2 3; do
        SERVER_CONFIG_FILE="${SCRIPT_DIR}/.tmp-server-${sid}.toml" \
        CLUSTER_CONFIG_FILE="${CLUSTER_CFG}" \
        RUST_LOG=warn \
        "${BIN}/server" &
    done
    sleep 3  # wait for leader election

    # Run 2 clients, wait for both
    CONFIG_FILE="${SCRIPT_DIR}/.tmp-client-1.toml" RUST_LOG=warn "${BIN}/client" &
    local p1=$!
    CONFIG_FILE="${SCRIPT_DIR}/.tmp-client-2.toml" RUST_LOG=warn "${BIN}/client" &
    local p2=$!
    wait "$p1" "$p2" 2>/dev/null || true

    kill_servers
    echo "  Done. Results in ${log_dir}/"
}

for quality in high medium low; do
    for ((run=0; run<NUM_RUNS; run++)); do
        run_one "$quality" "$run"
    done
done

echo ""
echo "========================================================================"
echo " BENCHMARK SUMMARY"
echo "========================================================================"

python3 - <<PYEOF
import csv, json, os
from pathlib import Path
from statistics import mean, median

BASE = Path(os.environ.get("LOG_BASE", "${LOG_BASE}"))

header = f"{'Quality':<10} {'Run':<6} {'ε(µs)':<8} {'Committed':<11} {'FastPath%':<11} {'SlowPath':<10} {'AvgConsensus(µs)':<19} {'ThroughputRPS':<15} {'AvgE2E(ms)'}"
print("\n" + header)
print("-" * len(header))

for quality in ["high", "medium", "low"]:
    for run_dir in sorted((BASE / quality).glob("run-*")):
        run_idx = run_dir.name

        # Leader metrics from server-1
        eps = committed = fast_pct = slow = avg_consensus = 0
        sfile = run_dir / "server-1.json"
        if sfile.exists():
            try:
                d = json.loads(sfile.read_text())
                m = d.get("metrics", {})
                eps          = d.get("config", {}).get("clock", {}).get("uncertainty_us", "?")
                committed    = m.get("committed_count", 0)
                fast_pct     = m.get("fast_path_ratio", 0) * 100
                slow         = m.get("slow_path_count", 0)
                avg_consensus = m.get("avg_consensus_latency_us", 0)
            except Exception as e:
                pass

        # Client-side E2E latency + throughput
        latencies, resp_times = [], []
        for cf in sorted(run_dir.glob("client-*.csv")):
            for row in csv.DictReader(open(cf)):
                if row.get("response_time"):
                    latencies.append(int(row["response_time"]) - int(row["request_time"]))
                    resp_times.append(int(row["response_time"]))

        e2e_avg = mean(latencies) if latencies else 0
        span_s  = (max(resp_times) - min(resp_times)) / 1000 if len(resp_times) > 1 else 1
        rps     = len(latencies) / span_s if span_s else 0

        print(f"{quality.capitalize():<10} {run_idx:<6} {eps:<8} {committed:<11} {fast_pct:<11.1f} {slow:<10} {avg_consensus:<19.0f} {rps:<15.1f} {e2e_avg:.0f}")

PYEOF
