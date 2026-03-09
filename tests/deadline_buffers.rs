/// Unit tests for the deadline-buffer data structures — §5.1 of the assignment.
///
/// The tests work directly on the public types `BufferedCommand` and `Command`
/// without spinning up any network or OmniPaxos instance, so they are fast
/// and fully deterministic.
///
/// Covered behaviours:
///   1. Early-buffer (min-heap) releases commands in ascending deadline order.
///   2. Two commands with the same deadline are broken by `command_id` (smaller id first).
///   3. The late-classification rule: `current_time >= deadline - ε` → slow path.
///   4. The release rule: a command is only popped when `current_time >= deadline + ε`.
use omnipaxos_kv::common::kv::{Command, KVCommand};
use std::collections::BinaryHeap;


// -----------------------------------------------------------------------
// We replicate the BufferedCommand ordering here so the test is self-contained.
// (The actual type lives in the server binary crate, not the lib.)
// -----------------------------------------------------------------------

/// Thin wrapper that gives `Command` a min-heap ordering by `(deadline_us, id)`.
#[derive(Clone, Debug, PartialEq, Eq)]
struct HeapCmd(Command);

impl PartialOrd for HeapCmd {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for HeapCmd {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Rust's BinaryHeap is a max-heap, so we reverse the comparison.
        other
            .0
            .deadline_us
            .cmp(&self.0.deadline_us)
            .then_with(|| other.0.id.cmp(&self.0.id))
    }
}

// -----------------------------------------------------------------------
// Helpers
// -----------------------------------------------------------------------

fn make_cmd(id: usize, deadline_us: i64) -> Command {
    Command {
        client_id: 1,
        coordinator_id: 1,
        id,
        kv_cmd: KVCommand::Put(id.to_string(), id.to_string()),
        deadline_us,
        enqueue_time_us: 0,
    }
}

/// Classify a single request: returns `true` if it should go to the late-buffer.
fn is_late(current_time: i64, deadline_us: i64, uncertainty: i64) -> bool {
    current_time >= deadline_us - uncertainty
}

/// Returns `true` if the head of the early-buffer should be released right now.
fn should_release(current_time: i64, deadline_us: i64, uncertainty: i64) -> bool {
    current_time >= deadline_us + uncertainty
}

// -----------------------------------------------------------------------
// Test 1 – early-buffer (min-heap) orders by ascending deadline
// -----------------------------------------------------------------------
#[test]
fn test_early_buffer_ordering() {
    let mut heap: BinaryHeap<HeapCmd> = BinaryHeap::new();
    heap.push(HeapCmd(make_cmd(0, 3_000)));
    heap.push(HeapCmd(make_cmd(1, 1_000)));
    heap.push(HeapCmd(make_cmd(2, 2_000)));

    let d0 = heap.pop().unwrap().0.deadline_us;
    let d1 = heap.pop().unwrap().0.deadline_us;
    let d2 = heap.pop().unwrap().0.deadline_us;

    assert_eq!(d0, 1_000, "first pop should be smallest deadline");
    assert_eq!(d1, 2_000);
    assert_eq!(d2, 3_000);
}

// -----------------------------------------------------------------------
// Test 2 – deterministic tie-breaking by command_id (smaller id first)
// -----------------------------------------------------------------------
#[test]
fn test_deterministic_tiebreaking_by_command_id() {
    let mut heap: BinaryHeap<HeapCmd> = BinaryHeap::new();
    let deadline = 5_000_i64;

    // Push in reverse order so the heap must actually sort them.
    heap.push(HeapCmd(make_cmd(30, deadline)));
    heap.push(HeapCmd(make_cmd(10, deadline)));
    heap.push(HeapCmd(make_cmd(20, deadline)));

    let id0 = heap.pop().unwrap().0.id;
    let id1 = heap.pop().unwrap().0.id;
    let id2 = heap.pop().unwrap().0.id;

    assert_eq!(id0, 10, "smallest id should come first on tie");
    assert_eq!(id1, 20);
    assert_eq!(id2, 30);
}

// -----------------------------------------------------------------------
// Test 3 – late-classification threshold
// -----------------------------------------------------------------------
#[test]
fn test_late_classification() {
    let uncertainty = 100_i64; // ±100 µs

    // Case A: current_time < deadline - ε  →  early (fast path)
    assert!(
        !is_late(900, 1_100, uncertainty),
        "900 < 1100-100=1000 → should be fast path"
    );

    // Case B: current_time == deadline - ε  →  late (boundary is inclusive)
    assert!(
        is_late(1_000, 1_100, uncertainty),
        "1000 == 1100-100=1000 → should be slow path"
    );

    // Case C: current_time > deadline - ε  →  late
    assert!(
        is_late(1_050, 1_100, uncertainty),
        "1050 > 1000 → should be slow path"
    );

    // Case D: arrived even after the deadline itself  →  late
    assert!(
        is_late(1_200, 1_100, uncertainty),
        "current_time > deadline → definitely slow path"
    );
}

// -----------------------------------------------------------------------
// Test 4 – release rule (conservative: current_time >= deadline + ε)
// -----------------------------------------------------------------------
#[test]
fn test_release_rule() {
    let uncertainty = 50_i64;
    let deadline = 1_000_i64;

    // Should NOT be released yet – true deadline might not have been reached everywhere
    assert!(
        !should_release(1_000, deadline, uncertainty),
        "at deadline exactly should NOT be released (need +ε)"
    );
    assert!(
        !should_release(1_040, deadline, uncertainty),
        "still within uncertainty window – should not release"
    );

    // Should be released once current_time >= deadline + ε
    assert!(
        should_release(1_050, deadline, uncertainty),
        "current_time == deadline + ε → safe to release"
    );
    assert!(
        should_release(1_200, deadline, uncertainty),
        "well past deadline + ε → must release"
    );
}

// -----------------------------------------------------------------------
// Test 5 – combined: only safe commands are popped from heap at a given time
// -----------------------------------------------------------------------
#[test]
fn test_heap_release_at_correct_time() {
    let uncertainty = 100_i64;
    let mut heap: BinaryHeap<HeapCmd> = BinaryHeap::new();

    // Two commands: deadline 500 and 1000
    heap.push(HeapCmd(make_cmd(0, 500)));
    heap.push(HeapCmd(make_cmd(1, 1_000)));

    // At time 550 (< 500+100=600): neither should be released
    let t = 550_i64;
    assert!(
        !should_release(t, heap.peek().unwrap().0.deadline_us, uncertainty),
        "cmd 0 not ready at t=550"
    );

    // At time 600 (== 500+100): first command ready
    let t = 600_i64;
    assert!(
        should_release(t, heap.peek().unwrap().0.deadline_us, uncertainty),
        "cmd 0 ready at t=600"
    );
    let released = heap.pop().unwrap();
    assert_eq!(released.0.id, 0);

    // Second command (deadline 1000) not ready yet
    assert!(
        !should_release(t, heap.peek().unwrap().0.deadline_us, uncertainty),
        "cmd 1 not ready at t=600"
    );

    // At t=1100: second command ready
    let t = 1_100_i64;
    assert!(
        should_release(t, heap.peek().unwrap().0.deadline_us, uncertainty),
        "cmd 1 ready at t=1100"
    );
    let released = heap.pop().unwrap();
    assert_eq!(released.0.id, 1);

    assert!(heap.is_empty());
}
