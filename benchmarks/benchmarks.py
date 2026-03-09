from pathlib import Path

from omnipaxos_cluster import OmnipaxosClusterBuilder
from omnipaxos_configs import ClockConfig, FlexibleQuorum, RequestInterval


# ---------------------------------------------------------------------------
# Shared workload helpers
# ---------------------------------------------------------------------------

def _clock_quality_workload() -> list[RequestInterval]:
    """Single fixed workload interval: 30s at 200 req/s, 50% reads."""
    return [RequestInterval(duration_sec=30, requests_per_sec=200, read_ratio=0.5)]


def _build_3node_cluster(cluster_id: int, clock: ClockConfig):
    """
    Build a 3-server / 3-client cluster on GCP where every server uses
    the given *clock* configuration.
    """
    workload = _clock_quality_workload()
    builder = (
        OmnipaxosClusterBuilder(cluster_id)
        .initial_leader(1)
        .server(1, "us-west2-a",          machine_type="e2-standard-2")
        .server(2, "us-central1-a",        machine_type="e2-standard-2")
        .server(3, "us-east4-b",           machine_type="e2-standard-2")
        .client(1, "us-west2-a",           requests=workload)
        .client(2, "us-central1-a",        requests=workload)
        .client(3, "us-east4-b",           requests=workload)
    )
    cluster = builder.build()

    # Inject the clock config into every server
    for server_id in [1, 2, 3]:
        cluster.change_server_config(server_id, clock=clock)

    return cluster


# ---------------------------------------------------------------------------
# Clock-quality benchmark (Assignment §4)
# ---------------------------------------------------------------------------

def clock_quality_benchmark(num_runs: int = 3):
    """
    Run the same workload under three clock quality levels and collect logs.

    Clock quality tiers (as specified in TODO.md §4):
        High   – ±10 µs uncertainty,  1 ms sync,   5 µs/s drift
        Medium – ±100 µs uncertainty, 10 ms sync,  50 µs/s drift
        Low    – ±1 ms uncertainty,  100 ms sync, 500 µs/s drift

    Logs are written to:
        ./logs/clock-quality/<High|Medium|Low>/run-<n>/
    """
    clock_configs = {
        "High":   ClockConfig.high(),
        "Medium": ClockConfig.medium(),
        "Low":    ClockConfig.low(),
    }

    # Use a shared GCP cluster (same VMs) across all quality levels to reduce
    # provisioning time; only the server TOML changes between runs.
    cluster_id = 2
    first_quality = next(iter(clock_configs))
    cluster = _build_3node_cluster(cluster_id, clock_configs[first_quality])

    try:
        for quality_name, clock_cfg in clock_configs.items():
            print(f"\n=== Clock quality: {quality_name} ({clock_cfg}) ===")

            # Reconfigure every server with the current clock quality
            for server_id in [1, 2, 3]:
                cluster.change_server_config(server_id, clock=clock_cfg)

            for run in range(num_runs):
                log_dir = Path(f"./logs/clock-quality/{quality_name}/run-{run}")
                print(f"  RUN {run}: {log_dir}")
                cluster.run(log_dir)
    finally:
        cluster.shutdown()


# ---------------------------------------------------------------------------
# Original example benchmark (unchanged)
# ---------------------------------------------------------------------------

def example_workload() -> dict[int, list[RequestInterval]]:
    experiment_duration = 10
    read_ratio = 0.50
    high_load = RequestInterval(experiment_duration, 100, read_ratio)
    low_load = RequestInterval(experiment_duration, 10, read_ratio)

    nodes = [1, 2, 3, 4, 5]
    us_nodes = [1, 2, 3]
    workload = {}
    for node in nodes:
        if node in us_nodes:
            requests = [high_load, low_load]
        else:
            requests = [low_load, high_load]
        workload[node] = requests
    return workload


def example_benchmark(num_runs: int = 3):
    workload = example_workload()
    cluster = (
        OmnipaxosClusterBuilder(1)
        .initial_leader(5)
        .server(1, "us-west2-a",           machine_type="e2-standard-2")
        .server(2, "us-south1-a",          machine_type="e2-standard-2")
        .server(3, "us-east4-b",           machine_type="e2-standard-2")
        .server(4, "europe-southwest1-a",  machine_type="e2-standard-2")
        .server(5, "europe-west4-a",       machine_type="e2-standard-2")
        .client(1, "us-west2-a",           requests=workload[1])
        .client(2, "us-south1-a",          requests=workload[2])
        .client(3, "us-east4-b",           requests=workload[3])
        .client(4, "europe-southwest1-a",  requests=workload[4])
        .client(5, "europe-west4-a",       requests=workload[5])
    ).build()
    experiment_log_dir = Path("./logs/example-experiment")

    majority_quorum = FlexibleQuorum(read_quorum_size=3, write_quorum_size=3)
    flex_quorum    = FlexibleQuorum(read_quorum_size=4, write_quorum_size=2)
    for run in range(num_runs):
        cluster.change_cluster_config(initial_flexible_quorum=majority_quorum)
        cluster.run(experiment_log_dir / f"MajorityQuorum/run-{run}")

        cluster.change_cluster_config(initial_flexible_quorum=flex_quorum)
        cluster.run(experiment_log_dir / f"FlexQuorum/run-{run}")

    cluster.shutdown()


def main():
    clock_quality_benchmark()


if __name__ == "__main__":
    main()

