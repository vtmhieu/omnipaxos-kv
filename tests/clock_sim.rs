/// Unit tests for `SimulatedClock` — §5.1 of the assignment.
///
/// The tests validate three distinct behaviours:
///   1. Drift accumulates linearly between syncs.
///   2. A resync bounds the time error back towards zero.
///   3. `get_uncertainty()` always returns the configured ±ε.
use omnipaxos_kv::clock::SimulatedClock;
use std::time::Duration;

// -----------------------------------------------------------------------
// Helper
// -----------------------------------------------------------------------

/// How many real microseconds does `std::thread::sleep(d)` actually take?
/// We give it a generous ±20 % tolerance to avoid flaky CI.
const SLEEP_TOLERANCE: f64 = 0.20;

// -----------------------------------------------------------------------
// Test 1 – drift accumulates between syncs
// -----------------------------------------------------------------------
#[test]
fn test_drift_accumulates() {
    // A clock with 1_000_000 µs/s drift (= 1 second of drift per real second)
    // and a very long sync interval so drift is never reset during this test.
    let drift_rate = 1_000_000.0_f64; // µs per real second
    let uncertainty = 0_u64;
    let sync_interval = Duration::from_secs(3600); // never resyncs during test

    let mut clock = SimulatedClock::new(drift_rate, uncertainty, sync_interval);

    let sleep_dur = Duration::from_millis(100);
    std::thread::sleep(sleep_dur);

    let sim_time = clock.get_time() as f64;
    let real_us = sleep_dur.as_micros() as f64;

    // Expected: real_time + drift = real_time * (1 + drift_rate/1e6)
    // With drift_rate = 1_000_000 µs/s the sim clock runs at 2× wall speed.
    let expected = real_us * 2.0; // approx
    let tolerance = expected * SLEEP_TOLERANCE;

    assert!(
        (sim_time - expected).abs() < tolerance,
        "sim_time={sim_time} expected≈{expected} (±{tolerance})"
    );
}

// -----------------------------------------------------------------------
// Test 2 – after resync, within-interval drift is bounded
// -----------------------------------------------------------------------
#[test]
fn test_resync_resets_drift() {
    // With drift_rate D and sync_interval S, the maximum accumulated drift
    // within a single sync interval is D * S_seconds.
    // Here: D = 200_000 µs/s, S = 20 ms → max drift per interval = 200_000 * 0.02 = 4_000 µs.
    let drift_rate = 200_000.0_f64; // µs per real second
    let uncertainty = 0_u64;
    let sync_interval = Duration::from_millis(20);

    let mut clock = SimulatedClock::new(drift_rate, uncertainty, sync_interval);

    // Wait for several sync cycles so the clock has resynced at least twice.
    std::thread::sleep(Duration::from_millis(80));

    // Measure a step that is shorter than one sync interval.
    // The drift accumulated during `step` is at most drift_rate * step_s.
    let step = Duration::from_millis(5);
    let t0 = clock.get_time() as f64;
    std::thread::sleep(step);
    let t1 = clock.get_time() as f64;

    let delta = t1 - t0;
    let real_us = step.as_micros() as f64;

    // Max drift in 5 ms = 200_000 µs/s * 0.005 s = 1_000 µs.
    // macOS thread::sleep can overshoot by up to 100%, so we allow 2× real + drift.
    let max_drift_us = drift_rate * (step.as_secs_f64());
    let max_expected = real_us * 2.0 + max_drift_us;
    let min_expected = real_us * (1.0 - SLEEP_TOLERANCE);

    assert!(
        delta >= min_expected && delta <= max_expected,
        "post-resync step: delta={delta:.0}µs, expected in [{min_expected:.0}, {max_expected:.0}]"
    );
}


// -----------------------------------------------------------------------
// Test 3 – get_uncertainty() is always the configured ε
// -----------------------------------------------------------------------
#[test]
fn test_uncertainty_constant() {
    let expected_uncertainty: u64 = 12_345;
    let clock = SimulatedClock::new(0.0, expected_uncertainty, Duration::from_secs(10));

    // Call multiple times; value must never change.
    for _ in 0..100 {
        assert_eq!(
            clock.get_uncertainty(),
            expected_uncertainty,
            "get_uncertainty() returned wrong value"
        );
    }
}

// -----------------------------------------------------------------------
// Test 4 – zero drift clock tracks real time closely
// -----------------------------------------------------------------------
#[test]
fn test_zero_drift_tracks_real_time() {
    let mut clock = SimulatedClock::new(0.0, 0, Duration::from_secs(3600));

    let step = Duration::from_millis(50);
    let t0 = clock.get_time() as f64;
    std::thread::sleep(step);
    let t1 = clock.get_time() as f64;

    let delta = t1 - t0;
    let expected = step.as_micros() as f64;
    let tolerance = expected * SLEEP_TOLERANCE;

    assert!(
        (delta - expected).abs() < tolerance,
        "zero-drift delta={delta}µs expected≈{expected}µs (±{tolerance})"
    );
}
