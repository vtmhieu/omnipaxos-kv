---
title: "Assignment 2.1 TODO (Clock + Deadline-Ordered OmniPaxos)"
---

## Goal (what you are building)

Implement **deadline-ordered request processing** for the `omnipaxos-kv` system, where deadlines are derived from **synchronized (imperfect) clocks**. The system must remain **linearizable for all clock qualities**; only **performance** should degrade as clocks get worse.

This repo’s key integration points:
- **Message types**: `src/common.rs` (`messages::ClientMessage`, `messages::ClusterMessage`)
- **Client request send**: `src/client/client.rs` (`ClientMessage::Append(...)`)
- **Server request handling / append**: `src/server/server.rs` (`handle_client_messages()` → `append_to_log()` → `omnipaxos.append(...)`)

---

## Definitions / expectations (write these down before coding)

- **Clock parameters**
  - **drift rate**: microseconds per second ($\mu s/s$)
  - **sync uncertainty bound**: $±\varepsilon$ microseconds (error bound between local estimate and “true” time)
  - **sync frequency**: resynchronization interval
- **Clock API per node**
  - `get_time()` → returns node’s current **synchronized time estimate** (e.g., microseconds since epoch or since experiment start)
  - `get_uncertainty()` → returns current $±\varepsilon$ (may be constant or grow since last sync; choose a model and document it)
- **Deadline processing**
  - Requests are **tagged with a deadline timestamp** in synchronized time.
  - **Early-buffer**: a priority queue ordered by deadline (soonest deadline first).
  - **Late-buffer**: requests received after their deadline “has passed”.
  - **Release rule**: a request becomes eligible when `current_time ≥ deadline`.
    - With uncertainty, define precisely whether you use:
      - **lower bound**: `get_time() - ε ≥ deadline` (conservative; never execute “before” true time), or
      - **estimate**: `get_time() ≥ deadline` (aggressive; may violate deadline semantics under worst-case error).
  - **Leader intervention**: needed when (a) late arrivals occur, or (b) **uncertainty windows overlap** such that two deadlines cannot be safely ordered without a single arbiter.

---

## 1) Clock simulator (required API)

- [ ] **Decide time units and representation**
  - Pick one: `i64 micros` recommended (consistent with “μs” requirement).
  - Document conversion to/from existing `Timestamp = i64` in `src/common.rs`.

- [ ] **Add a clock module**
  - Suggested file: `src/clock.rs` (or `src/common/clock.rs`).
  - Provide a trait:
    - `trait SynchronizedClock { fn get_time_micros(&self) -> i64; fn get_uncertainty_micros(&self) -> i64; }`

- [ ] **Implement a simulator clock**
  - Inputs:
    - `drift_rate_us_per_s: i64`
    - `base_uncertainty_us: i64` (the configured ±ε)
    - `sync_interval: Duration`
  - Behavior:
    - Maintains an internal offset vs “true time”.
    - Drift accumulates between syncs.
    - At each sync, offset is corrected (e.g., reset to a random value within ±ε, or bounded correction).
  - Expose:
    - `get_time_micros()`
    - `get_uncertainty_micros()` (constant or time-varying; if varying, explain formula)

- [ ] **Wire the clock into nodes**
  - Server: add a clock instance to `OmniPaxosServer` (`src/server/server.rs`).
  - Client: decide if clients generate deadlines or servers do.
    - Option A (simpler): client sends `deadline_offset_us` and server converts to absolute deadline using its synchronized clock.
    - Option B: client includes absolute `deadline_us` computed from its own clock (requires client clock too).

Acceptance criteria:
- Each node can be queried for **(time, uncertainty)**.
- You can run with different clock configurations without changing code (config/env/TOML).

---

## 2) Deadline-tagged messages + buffers

### 2.1 Extend protocol to carry deadlines

- [ ] **Extend `ClientMessage` to include deadlines**
  - File: `src/common.rs`
  - Update `ClientMessage::Append(...)` to include either:
    - `deadline_us: i64`, or
    - `deadline_offset_us: i64`, plus enough info to compute absolute deadline.
- [ ] **(Optional but useful) Include client-send timestamp**
  - Helps measure lateness and debug ordering under delay.

- [ ] **Propagate deadline into the replicated log command**
  - File: `src/common.rs` (`kv::Command`)
  - Add field: `deadline_us: i64` (and potentially `flags` / `path` / `was_late`)
  - Ensure `Command` remains `Entry + Serialize + Deserialize`.

### 2.2 Implement early/late buffers on the leader-side append path

- [ ] **Create request wrapper type**
  - Contains: `(client_id, command_id, kv_cmd, deadline_us, arrival_time_us, arrival_uncertainty_us, source_node_id?)`
  - Must implement ordering for a min-heap by deadline (tie-breakers deterministic).

- [ ] **Add buffers**
  - Early-buffer: `BinaryHeap` (min-heap via `Reverse`) or `BTreeMap` keyed by `(deadline, tie_breaker)`.
  - Late-buffer: `Vec`/`VecDeque` + metrics.

- [ ] **Change server request handling**
  - File: `src/server/server.rs`
  - Instead of immediately calling `append_to_log(...)` in `handle_client_messages()`:
    - Insert request into early-buffer or late-buffer based on deadline vs local time (and uncertainty rule you chose).
    - Regularly “release” requests whose deadline has been reached, in deadline order, by calling `omnipaxos.append(...)`.
  - Trigger release:
    - on a timer tick (e.g., every 100–500µs), and/or
    - whenever a new request arrives, and/or
    - whenever a sync event updates time/uncertainty.

Acceptance criteria:
- Requests are appended to OmniPaxos **in nondecreasing deadline order** *as observed by the leader’s policy*, except where leader arbitration explicitly overrides (see next section).
- Late requests are detected and moved to late-buffer.

---

## 3) Leader intervention + arbitration rules (correctness-critical)

You must define exactly when leader intervention is required and how it resolves ambiguity **without breaking linearizability**.

- [ ] **Define “overlapping uncertainty windows”**
  - Example definition (one option):
    - A request deadline `d` has an uncertainty interval `[d-ε, d+ε]` (or use arrival-time interval).
    - If two intervals overlap, their “true” order may be ambiguous → leader must arbitrate.

- [ ] **Implement arbitration**
  - Choose a deterministic tie-breaker:
    - e.g., `(deadline_us, client_id, command_id)` or `(deadline_us, arrival_time_us, source_node_id, command_id)`
  - Ensure arbitration happens in exactly one place (leader) to avoid split-brain ordering.

- [ ] **Non-leader behavior**
  - Decide and implement:
    - **Forward-to-leader**: non-leader servers forward client requests (with deadlines) to leader.
    - or **Single ingress**: clients only connect to leader during experiments (simpler, but document limitation).

- [ ] **Track “fast path” vs “leader intervention”**
  - Define fast-path as: request appended without needing arbitration beyond normal PQ ordering and not late.
  - Count slow-path causes separately:
    - `late_arrival`
    - `uncertainty_overlap`
    - `forwarded_from_follower`

Acceptance criteria:
- System remains linearizable (see tests section).
- Fast-path ratio is measurable.

---

## 4) Benchmark: performance vs clock quality (3 configs)

Clock quality configurations to run:
- **High**: ±10µs uncertainty, 1ms sync interval
- **Medium**: ±100µs uncertainty, 10ms sync interval
- **Low**: ±1ms uncertainty, 100ms sync interval

Repo benchmarking hooks:
- Python harness in `benchmarks/`
- Client already logs request/response times (`src/client/data_collection.rs`)

Tasks:
- [ ] **Add clock quality knobs to server/client configs**
  - Prefer: environment variables (`OMNIPAXOS_*`) or existing TOML configs in `build_scripts/`.
  - Ensure benchmarks can set (uncertainty, sync interval, drift rate) per run.

- [ ] **Add instrumentation output**
  - Server-side metrics (per node, aggregated by leader):
    - consensus latency (commit - enqueue time)
    - throughput (committed ops/sec)
    - fast-path ratio
    - late-buffer count / rate
  - Decide output format:
    - JSON line logs, or
    - write to a metrics file path from config (similar to `output_filepath` usage).

- [ ] **Extend `benchmarks/` to run 3 configs**
  - File(s): `benchmarks/benchmarks.py`, `benchmarks/omnipaxos_configs.py`
  - For each config:
    - run fixed workload (same request rate mix)
    - collect latency/throughput/fast-path ratio
    - produce a summary table/CSV

Acceptance criteria:
- You can produce a plot/table showing **clock quality vs latency/throughput/fast-path ratio**.
- Performance degrades as uncertainty increases / sync frequency decreases.

---

## 5) Correctness tests (linearizability under imperfect clocks)

You need tests that demonstrate **deadline-ordered processing does not break correctness**, even with skew, drift, delay, and failures.

### 5.1 Unit tests (fast, deterministic)

- [ ] **Clock simulator tests**
  - Drift accumulates between syncs.
  - Resync bounds time error within ±ε (per your model).
  - `get_uncertainty()` behaves as documented.

- [ ] **Buffer/release rule tests**
  - Given a fake clock, enqueue requests with deadlines:
    - ensure early-buffer orders correctly
    - ensure release triggers at correct time boundary (including uncertainty logic)
    - ensure late-buffer classification correct

- [ ] **Arbitration tests**
  - Construct two requests with overlapping uncertainty windows:
    - verify deterministic ordering
    - verify they are not executed in contradictory orders across nodes

Suggested location:
- `tests/clock_sim.rs`
- `tests/deadline_buffers.rs`

### 5.2 Integration tests (tokio, multiple nodes)

- [ ] **Linearizability smoke tests**
  - Start 3-node cluster in-process (or as subprocesses).
  - Run mixed Put/Get with deadlines + injected delays.
  - Verify that observed responses correspond to some legal sequential history.
    - Practical approach: validate reads see the latest preceding write in the decided log order.

- [ ] **Network delay + skew**
  - Add artificial delay on one node’s message handling path.
  - Give nodes different drift rates.
  - Ensure no safety violations (only more leader interventions / latency).

- [ ] **Node failure**
  - Kill/restart follower during workload.
  - Ensure system continues and remains linearizable.

Acceptance criteria:
- Tests pass for high/medium/low clock qualities.
- No correctness test depends on “good” clocks.

---

## 6) Deliverables checklist

- [ ] Clock simulator with `get_time()` and `get_uncertainty()` per node.
- [ ] Deadline-tagged messages and commands.
- [ ] Early-buffer + late-buffer + release rule.
- [ ] Leader arbitration for late/overlap cases + fast-path ratio instrumentation.
- [ ] Benchmarks across 3 clock qualities: latency, throughput, fast-path ratio.
- [ ] Test suite demonstrating linearizability under skew/drift/delay/failures.

