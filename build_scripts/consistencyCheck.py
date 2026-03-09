import json
import sys
import itertools

def load_log(path):
    with open(path) as f:
        snapshots = json.load(f)
    return [[entry["op"] for entry in snapshot] for snapshot in snapshots]

def is_prefix(shorter, longer):
    return longer[:len(shorter)] == shorter

def check_prefix_consistency(num_log_files):
    all_logs = {}
    log_files = [f"logs/server-{i+1}-decided-log.json" for i in range(num_log_files)]
    for path in log_files:
        node = path.split("/")[-1].replace("-decided-log.json", "")
        print(node)
        all_logs[node] = load_log(path)
        print(f"Loaded {node}: {len(all_logs[node])} snapshots")

    violations = 0

    
    for (node_a, snapshots_a), (node_b, snapshots_b) in itertools.combinations(all_logs.items(), 2):
        pairwiseViolations = 0
        print(f"\nChecking {node_a} vs {node_b}...")

        for i, snap_a in enumerate(snapshots_a):
            for j, snap_b in enumerate(snapshots_b):
                shorter, longer = (snap_a, snap_b) if len(snap_a) <= len(snap_b) else (snap_b, snap_a)
                shorter_name = f"{node_a}[{i}]" if len(snap_a) <= len(snap_b) else f"{node_b}[{j}]"
                longer_name  = f"{node_b}[{j}]" if len(snap_a) <= len(snap_b) else f"{node_a}[{i}]"

                if not is_prefix(shorter, longer):
                    first_diff = next(
                        k for k, (a, b) in enumerate(zip(shorter, longer)) if a != b
                    )
                    print(f"  ✗ VIOLATION: {shorter_name} (len={len(shorter)}) is not a prefix of {longer_name} (len={len(longer)})")
                    print(f"    First divergence at idx {first_diff}:")
                    print(f"      {shorter_name}: {shorter[first_diff]}")
                    print(f"      {longer_name}:  {longer[first_diff]}")
                    pairwiseViolations += 1

        if pairwiseViolations == 0:
            print(f"  ✓ All snapshot pairs between {node_a} and {node_b} are prefix-consistent")
        else:
            print(f"  ✗ Found {pairwiseViolations} violation(s) between {node_a} and {node_b}")
            violations += pairwiseViolations

    print(f"\n{'='*50}")
    if violations == 0:
        print(f"✓ PASSED — no prefix violations found across {num_log_files} nodes")
    else:
        print(f"✗ FAILED — {violations} prefix violation(s) found")

    return violations == 0


if __name__ == "__main__":
    if len(sys.argv) != 2:
        print("Usage: python consistencyCheck.py <num_log_files>")
        sys.exit(1)
    num_log_files = int(sys.argv[1])
    if (num_log_files) < 2:
        print("Please provide at least two log files for consistency checking")
        sys.exit(1)
    print(f"Checking prefix consistency across {num_log_files} node(s)...")
    success = check_prefix_consistency(num_log_files)
    sys.exit(0 if success else 1)