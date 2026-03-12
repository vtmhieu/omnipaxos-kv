use std::time::{Duration, Instant};

pub struct SimulatedClock {
    start_instant: Instant,
    drift_per_sec: f64,
    uncertainty_us: u64,
    sync_interval: Duration,
    last_sync: Instant,
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
        }
    }

    pub fn get_time(&mut self) -> u128 {
        let now = Instant::now();
        let elapsed = now.duration_since(self.start_instant);

        // Resync logic
        if now.duration_since(self.last_sync) >= self.sync_interval {
            self.last_sync = now;
        }

        let elapsed_from_last_sync = now.duration_since(self.last_sync);

        // Apply drift
        let drift = elapsed_from_last_sync.as_secs_f64() * self.drift_per_sec;

        elapsed.as_micros() + (drift as u128)
    }

    pub fn get_uncertainty(&self) -> u64 {
        self.uncertainty_us
    }
}

// //for manual testing
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
