use crate::{configs::ClientConfig, data_collection::ClientData, network::Network};
use chrono::Utc;
use log::*;
use omnipaxos_kv::common::{kv::*, messages::*};
use rand::Rng;
use std::{fs::File, io::Write, time::{Duration, Instant}};
use tokio::time::interval;

const NETWORK_BATCH_SIZE: usize = 100;

pub struct Client {
    id: ClientId,
    network: Network,
    client_data: ClientData,
    config: ClientConfig,
    active_server: NodeId,
    final_request_count: Option<usize>,
    next_request_id: usize,
    latency_sum: u128,
    start_time: Instant,
    metric_report: bool,
}

impl Client {
    pub async fn new(config: ClientConfig) -> Self {
        let network = Network::new(
            vec![(config.server_id, config.server_address.clone())],
            NETWORK_BATCH_SIZE,
        )
        .await;
        Client {
            id: config.server_id,
            network,
            client_data: ClientData::new(),
            active_server: config.server_id,
            config,
            final_request_count: None,
            next_request_id: 0,
            latency_sum: 0,
            start_time: Instant::now(),
            metric_report: true,
        }
    }

    pub async fn run(&mut self) {
        // Wait for server to signal start
        if self.metric_report{
            let path = format!(
            "{}-metrics.json",
            self.config.output_filepath.trim_end_matches(".csv")
            );
            let _ = std::fs::remove_file(&path);
        }
        info!("{}: Waiting for start signal from server", self.id);
        match self.network.server_messages.recv().await {
            Some(ServerMessage::StartSignal(start_time)) => {
                Self::wait_until_sync_time(&mut self.config, start_time).await;
                self.start_time = Instant::now(); // ← after the sync wait
            }
            _ => panic!("Error waiting for start signal"),
        }

        // Early end
        let intervals = self.config.requests.clone();
        if intervals.is_empty() {
            self.save_results(Instant::now()).expect("Failed to save results");
            return;
        }

        // Initialize intervals
        let mut rng = rand::thread_rng();
        let mut intervals = intervals.iter();
        let first_interval = intervals.next().unwrap();
        let mut read_ratio = first_interval.get_read_ratio();
        let mut request_interval = interval(first_interval.get_request_delay());
        let mut next_interval = interval(first_interval.get_interval_duration());
        let _ = next_interval.tick().await;

        // Main event loop
        info!("{}: Starting requests", self.id);
        loop {
            tokio::select! {
                biased;
                Some(msg) = self.network.server_messages.recv() => {
                    self.handle_server_message(msg);
                    if self.run_finished() {
                        break;
                    }
                }
                _ = request_interval.tick(), if self.final_request_count.is_none() => {
                    let is_write = rng.gen::<f64>() > read_ratio;
                    self.send_request(is_write).await;
                },
                _ = next_interval.tick() => {
                    match intervals.next() {
                        Some(new_interval) => {
                            read_ratio = new_interval.read_ratio;
                            next_interval = interval(new_interval.get_interval_duration());
                            next_interval.tick().await;
                            request_interval = interval(new_interval.get_request_delay());
                        },
                        None => {
                            self.final_request_count = Some(self.client_data.request_count());
                            if self.run_finished() {
                                break;
                            }
                        },
                    }
                },
            }
        }

        info!(
            "{}: Client finished: collected {} responses",
            self.id,
            self.client_data.response_count(),
        );
        self.network.shutdown();
        let end_time = Instant::now();
        self.save_results(end_time).expect("Failed to save results");
    }

    fn handle_server_message(&mut self, msg: ServerMessage) {
        debug!("Recieved {msg:?}");
        match msg {
            ServerMessage::StartSignal(_) => (),
            server_response => {
                let cmd_id = server_response.command_id();
                let path: String = server_response.commit_path();
                self.client_data.new_response(cmd_id, path);
                self.latency_sum += self.client_data.response_latency(cmd_id) as u128;
            }
        }
    }

    async fn send_request(&mut self, is_write: bool) {
        let key = self.next_request_id.to_string();
        let cmd = match is_write {
            true => KVCommand::Put(key.clone(), key),
            false => KVCommand::Get(key),
        };
        let request = ClientMessage::Append(self.next_request_id, cmd);
        debug!("Sending {request:?}");
        self.network.send(self.active_server, request).await;
        self.client_data.new_request(is_write);
        self.next_request_id += 1;
    }

    fn run_finished(&self) -> bool {
        if let Some(count) = self.final_request_count {
            if self.client_data.request_count() >= count {
                return true;
            }
        }
        return false;
    }

    // Wait until the scheduled start time to synchronize client starts.
    // If start time has already passed, start immediately.
    async fn wait_until_sync_time(config: &mut ClientConfig, scheduled_start_utc_ms: i64) {
        // // Desync the clients a bit
        // let mut rng = rand::thread_rng();
        // let scheduled_start_utc_ms = scheduled_start_utc_ms + rng.gen_range(1..100);
        let now = Utc::now();
        let milliseconds_until_sync = scheduled_start_utc_ms - now.timestamp_millis();
        config.sync_time = Some(milliseconds_until_sync);
        if milliseconds_until_sync > 0 {
            tokio::time::sleep(Duration::from_millis(milliseconds_until_sync as u64)).await;
        } else {
            warn!("Started after synchronization point!");
        }
    }

    fn save_results(&self, end_time: Instant) -> Result<(), std::io::Error> {
        self.client_data.save_summary(self.config.clone())?;
        self.client_data
            .to_csv(self.config.output_filepath.clone())?;
        if self.metric_report {
            self.save_metrics(end_time)?;
        }
        Ok(())
    }

    pub fn save_metrics(&self, end_time: Instant) -> Result<(), std::io::Error> {
        let path = format!(
            "{}-metrics.json",
            self.config.output_filepath.trim_end_matches(".csv")
        );
        let mut metrics_file: Vec<serde_json::Value> = if let Ok(contents) = std::fs::read_to_string(&path) {
            serde_json::from_str(&contents).unwrap_or_default()
        } else {
            vec![]
        };
        let total_runtime = end_time.duration_since(self.start_time).as_millis();
        let throughput = (self.client_data.response_count() as f64) / (total_runtime as f64 / 1000.0);

        let metrics = serde_json::json!({
            "total_latency_ms": self.latency_sum,
            "response_count": self.client_data.response_count(),
            "runtime_ms": total_runtime,
            "throughput_ops_per_sec": throughput,
            "avg_latency_ms": if self.client_data.response_count() > 0 { self.latency_sum / self.client_data.response_count() as u128 } else { 0 },
            "fastpath_count": self.client_data.fastpath_count(),
            "fastpath_ratio": self.client_data.fastpath_count() as f64 / self.client_data.response_count() as f64,
        });

        metrics_file.push(serde_json::to_value(&metrics).unwrap());

        if let Ok(json) = serde_json::to_string_pretty(&metrics_file) {
            if let Ok(mut f) = File::create(&path) {
                let _ = f.write_all(json.as_bytes());
            }
        }
        Ok(())
    }
}
