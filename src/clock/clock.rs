use std::time::{Duration, Instant};

pub struct SimulatedClock {
    start_instant: Instant,
    drift_per_sec: f64,      // microseconds per second
    uncertainty_us: u64,     // ± uncertainty in microseconds
    sync_interval: Duration, // how often clock resyncs
    last_sync: Instant,
    /// Elapsed microseconds at the time of the last sync (the true-time baseline).
    /// Drift accumulates only from `last_sync` onwards.
    offset_at_last_sync_us: f64,
}

impl SimulatedClock {
    pub fn new(drift_per_sec: f64, uncertainty_us: u64, sync_interval: Duration) -> Self {
        let now = Instant::now();
        Self {
            start_instant: now,
            drift_per_sec,
            uncertainty_us,
            sync_interval,
            last_sync: now,
            offset_at_last_sync_us: 0.0,
        }
    }

    pub fn get_time(&mut self) -> u128 {
        let now = Instant::now();

        // Check if it's time to resync.
        if now.duration_since(self.last_sync) >= self.sync_interval {
            // Record the true elapsed time at this sync point so future drift
            // accumulates only from here.
            self.offset_at_last_sync_us =
                now.duration_since(self.start_instant).as_micros() as f64;
            self.last_sync = now;
        }

        // Drift accumulates only since the last resync.
        let since_last_sync_us = now.duration_since(self.last_sync).as_micros() as f64;
        let drift_us = since_last_sync_us / 1_000_000.0 * self.drift_per_sec;

        let true_elapsed_us = now.duration_since(self.start_instant).as_micros() as f64;
        (true_elapsed_us + drift_us) as u128
    }

    pub fn get_uncertainty(&self) -> u64 {
        self.uncertainty_us
    }
}

//for manual testing
// fn main() {
//     let mut clock = SimulatedClock::new(
//         50.0,
//         100,
//         Duration::from_secs(10),
//     );

//     loop {
//         println!("Sim time: {}", clock.get_time());
//         std::thread::sleep(Duration::from_secs(1));
//     }
// }

