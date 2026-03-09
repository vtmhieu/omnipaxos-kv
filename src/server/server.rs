use crate::{configs::OmniPaxosKVConfig, database::Database, network::Network};
use chrono::Utc;
use log::*;
use omnipaxos::{
    messages::Message,
    util::{LogEntry, NodeId},
    OmniPaxos, OmniPaxosConfig,
};
use omnipaxos_kv::common::{kv::*, messages::*, utils::Timestamp};
use omnipaxos_storage::memory_storage::MemoryStorage;
use serde::Serialize;
use std::{cmp::Ordering, collections::BinaryHeap, sync::Mutex};
use std::{fs::File, io::Write, time::Duration};

// ---------------------------------------------------------------------------
// Metrics
// ---------------------------------------------------------------------------
#[derive(Debug, Default, Serialize, Clone)]
pub struct MetricsState {
    pub fast_path_count: u64,              // requests released via early-buffer
    pub slow_path_count: u64,              // requests processed via late-buffer
    pub committed_count: u64,              // total commands decided
    pub total_consensus_latency_us: i64,   // sum of (decide_time - enqueue_time) in µs
}

impl MetricsState {
    pub fn avg_consensus_latency_us(&self) -> f64 {
        if self.committed_count == 0 {
            return 0.0;
        }
        self.total_consensus_latency_us as f64 / self.committed_count as f64
    }

    pub fn fast_path_ratio(&self) -> f64 {
        let total = self.fast_path_count + self.slow_path_count;
        if total == 0 {
            return 0.0;
        }
        self.fast_path_count as f64 / total as f64
    }
}

// --- Add the Wrapper for BinaryHeap Ordering ---
#[derive(Clone, Debug)]
pub struct BufferedCommand(pub Command);

impl PartialEq for BufferedCommand {
    fn eq(&self, other: &Self) -> bool {
        self.0.deadline_us == other.0.deadline_us && self.0.id == other.0.id
    }
}
impl Eq for BufferedCommand {}

impl PartialOrd for BufferedCommand {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for BufferedCommand {
    fn cmp(&self, other: &Self) -> Ordering {
        // Reverse order: smallest deadline_us is at the top of the max-heap (making it a min-heap)
        other
            .0
            .deadline_us
            .cmp(&self.0.deadline_us)
            .then_with(|| other.0.id.cmp(&self.0.id))
    }
}
// -----------------------------------------------

type OmniPaxosInstance = OmniPaxos<Command, MemoryStorage<Command>>;
const NETWORK_BATCH_SIZE: usize = 100;
const LEADER_WAIT: Duration = Duration::from_secs(1);
const ELECTION_TIMEOUT: Duration = Duration::from_secs(1);

pub struct OmniPaxosServer {
    id: NodeId,
    database: Database,
    network: Network,
    omnipaxos: OmniPaxosInstance,
    current_decided_idx: usize,
    omnipaxos_msg_buffer: Vec<Message<Command>>,
    config: OmniPaxosKVConfig,
    peers: Vec<NodeId>,
    clock: Mutex<crate::clock::SimulatedClock>,

    early_buffer: BinaryHeap<BufferedCommand>,
    late_buffer: Vec<Command>,
    metrics: MetricsState,
}

impl OmniPaxosServer {
    pub async fn new(config: OmniPaxosKVConfig) -> Self {
        // Initialize OmniPaxos instance
        let storage: MemoryStorage<Command> = MemoryStorage::default();
        let omnipaxos_config: OmniPaxosConfig = config.clone().into();
        let omnipaxos_msg_buffer = Vec::with_capacity(omnipaxos_config.server_config.buffer_size);
        let omnipaxos = omnipaxos_config.build(storage).unwrap();

        let network = Network::new(config.clone(), NETWORK_BATCH_SIZE).await;

        let clock_config = &config.local.clock;
        let sync_interval = Duration::from_millis(clock_config.sync_interval_ms);
        let clock = Mutex::new(crate::clock::SimulatedClock::new(
            clock_config.drift_rate_us_per_s,
            clock_config.uncertainty_us,
            sync_interval,
        ));

        OmniPaxosServer {
            id: config.local.server_id,
            database: Database::new(),
            network,
            omnipaxos,
            current_decided_idx: 0,
            omnipaxos_msg_buffer,
            peers: config.get_peers(config.local.server_id),
            config,
            clock,
            early_buffer: BinaryHeap::new(),
            late_buffer: Vec::new(),
            metrics: MetricsState::default(),
        }
    }

    pub async fn run(&mut self) {
        // Save config to output file
        self.save_output().expect("Failed to write to file");
        let mut client_msg_buf = Vec::with_capacity(NETWORK_BATCH_SIZE);
        let mut cluster_msg_buf = Vec::with_capacity(NETWORK_BATCH_SIZE);
        // We don't use Omnipaxos leader election at first and instead force a specific initial leader
        self.establish_initial_leader(&mut cluster_msg_buf, &mut client_msg_buf)
            .await;
        // Main event loop with leader election
        let mut election_interval = tokio::time::interval(ELECTION_TIMEOUT);
        // Add a fast interval to constantly check the buffers against the clock
        let mut buffer_interval = tokio::time::interval(Duration::from_millis(10));

        loop {
            tokio::select! {
                _ = election_interval.tick() => {
                    self.omnipaxos.tick();
                    self.send_outgoing_msgs();
                },
                _ = buffer_interval.tick() => {
                    // Check if any buffered messages are ready to be released
                    self.process_buffers();
                    self.send_outgoing_msgs();
                },
                _ = self.network.cluster_messages.recv_many(&mut cluster_msg_buf, NETWORK_BATCH_SIZE) => {
                    self.handle_cluster_messages(&mut cluster_msg_buf).await;
                },
                _ = self.network.client_messages.recv_many(&mut client_msg_buf, NETWORK_BATCH_SIZE) => {
                    self.handle_client_messages(&mut client_msg_buf).await;
                },
            }
        }
    }

    // Ensures cluster is connected and initial leader is promoted before returning.
    // Once the leader is established it chooses a synchronization point which the
    // followers relay to their clients to begin the experiment.
    async fn establish_initial_leader(
        &mut self,
        cluster_msg_buffer: &mut Vec<(NodeId, ClusterMessage)>,
        client_msg_buffer: &mut Vec<(ClientId, ClientMessage)>,
    ) {
        let mut leader_takeover_interval = tokio::time::interval(LEADER_WAIT);
        loop {
            tokio::select! {
                _ = leader_takeover_interval.tick(), if self.config.cluster.initial_leader == self.id => {
                    if let Some((curr_leader, is_accept_phase)) = self.omnipaxos.get_current_leader(){
                        if curr_leader == self.id && is_accept_phase {
                            info!("{}: Leader fully initialized", self.id);
                            let experiment_sync_start = (Utc::now() + Duration::from_secs(2)).timestamp_millis();
                            self.send_cluster_start_signals(experiment_sync_start);
                            self.send_client_start_signals(experiment_sync_start);
                            break;
                        }
                    }
                    info!("{}: Attempting to take leadership", self.id);
                    self.omnipaxos.try_become_leader();
                    self.send_outgoing_msgs();
                },
                _ = self.network.cluster_messages.recv_many(cluster_msg_buffer, NETWORK_BATCH_SIZE) => {
                    let recv_start = self.handle_cluster_messages(cluster_msg_buffer).await;
                    if recv_start {
                        break;
                    }
                },
                _ = self.network.client_messages.recv_many(client_msg_buffer, NETWORK_BATCH_SIZE) => {
                    self.handle_client_messages(client_msg_buffer).await;
                },
            }
        }
    }

    fn handle_decided_entries(&mut self) {
        let new_decided_idx = self.omnipaxos.get_decided_idx();
        if self.current_decided_idx < new_decided_idx {
            let current_time_us = self.get_time();

            let decided_entries = self
                .omnipaxos
                .read_decided_suffix(self.current_decided_idx)
                .unwrap();
            self.current_decided_idx = new_decided_idx;
            debug!("Decided {} at time {} us", new_decided_idx, current_time_us);

            let decided_commands = decided_entries
                .into_iter()
                .filter_map(|e| match e {
                    LogEntry::Decided(cmd) => Some(cmd),
                    _ => unreachable!(),
                })
                .collect();
            self.update_database_and_respond(decided_commands);

            // Flush metrics after every batch of decided entries so the output file
            // is always current even if the process is killed without graceful shutdown.
            if let Err(e) = self.save_output() {
                warn!("Failed to flush metrics to output file: {e}");
            }
        }
    }


    fn update_database_and_respond(&mut self, commands: Vec<Command>) {
        let now_us = Utc::now().timestamp_micros();
        for command in commands {
            // Accumulate consensus latency
            let latency = now_us - command.enqueue_time_us;
            self.metrics.total_consensus_latency_us += latency;
            self.metrics.committed_count += 1;

            let read = self.database.handle_command(command.kv_cmd);
            if command.coordinator_id == self.id {
                let response = match read {
                    Some(read_result) => ServerMessage::Read(command.id, read_result),
                    None => ServerMessage::Write(command.id),
                };
                self.network.send_to_client(command.client_id, response);
            }
        }
    }

    fn send_outgoing_msgs(&mut self) {
        self.omnipaxos
            .take_outgoing_messages(&mut self.omnipaxos_msg_buffer);
        for msg in self.omnipaxos_msg_buffer.drain(..) {
            let to = msg.get_receiver();
            let cluster_msg = ClusterMessage::OmniPaxosMessage(msg);
            self.network.send_to_cluster(to, cluster_msg);
        }
    }

    async fn handle_client_messages(&mut self, messages: &mut Vec<(ClientId, ClientMessage)>) {
        for (from, message) in messages.drain(..) {
            match message {
                ClientMessage::Append(command_id, kv_command, deadline_offset_us) => {
                    let current_time_us = self.get_time() as i64;
                    let absolute_deadline_us = current_time_us + deadline_offset_us;
                    self.append_to_log(from, command_id, kv_command, absolute_deadline_us)
                }
            }
        }
        self.send_outgoing_msgs();
    }

    async fn handle_cluster_messages(
        &mut self,
        messages: &mut Vec<(NodeId, ClusterMessage)>,
    ) -> bool {
        let mut received_start_signal = false;
        for (from, message) in messages.drain(..) {
            trace!("{}: Received {message:?}", self.id);
            match message {
                ClusterMessage::OmniPaxosMessage(m) => {
                    self.omnipaxos.handle_incoming(m);
                    self.handle_decided_entries();
                }
                ClusterMessage::LeaderStartSignal(start_time) => {
                    debug!("Received start message from peer {from}");
                    received_start_signal = true;
                    self.send_client_start_signals(start_time);
                }
            }
        }
        self.send_outgoing_msgs();
        received_start_signal
    }

    fn append_to_log(
        &mut self,
        from: ClientId,
        command_id: CommandId,
        kv_command: KVCommand,
        deadline_us: i64,
    ) {
        let enqueue_time_us = Utc::now().timestamp_micros();
        let command = Command {
            client_id: from,
            coordinator_id: self.id,
            id: command_id,
            kv_cmd: kv_command,
            deadline_us,
            enqueue_time_us,
        };

        let current_time = self.get_time() as i64;
        let uncertainty = self.get_uncertainty() as i64;

        // Route to the appropriate buffer based on deadline AND uncertainty window.
        // If current_time >= deadline - ε the true deadline may already have passed → slow path.
        if current_time >= deadline_us - uncertainty {
            debug!(
                "Late arrival or overlapping uncertainty for Command {} (slow path)",
                command_id
            );
            self.metrics.slow_path_count += 1;
            self.late_buffer.push(command);
        } else {
            trace!("Early arrival for Command {} (fast path)", command_id);
            self.metrics.fast_path_count += 1;
            self.early_buffer.push(BufferedCommand(command));
        }
    }

    // --- Process the buffers based on time ---
    fn process_buffers(&mut self) {
        let current_time = self.get_time() as i64;
        let uncertainty = self.get_uncertainty() as i64;

        // 1. Release Rule
        while let Some(top) = self.early_buffer.peek() {
            // Safe release: current_time must be >= deadline + uncertainty
            if current_time >= top.0.deadline_us + uncertainty {
                let cmd = self.early_buffer.pop().unwrap().0;
                debug!("Releasing Command {} from early buffer", cmd.id);
                self.omnipaxos
                    .append(cmd)
                    .expect("Append to Omnipaxos log failed");
            } else {
                break;
            }
        }

        // 2. Leader Intervention (Arbitrate late buffer)
        // Drain the late buffer on ALL nodes.
        // If it's a follower, OmniPaxos automatically forwards it to the leader for arbitration.
        for cmd in self.late_buffer.drain(..) {
            warn!(
                "Sending late/uncertain Command {} for leader arbitration",
                cmd.id
            );
            self.omnipaxos
                .append(cmd)
                .expect("Append to Omnipaxos log failed");
        }
    }

    fn send_cluster_start_signals(&mut self, start_time: Timestamp) {
        for peer in &self.peers {
            debug!("Sending start message to peer {peer}");
            let msg = ClusterMessage::LeaderStartSignal(start_time);
            self.network.send_to_cluster(*peer, msg);
        }
    }

    fn send_client_start_signals(&mut self, start_time: Timestamp) {
        for client_id in 1..self.config.local.num_clients as ClientId + 1 {
            debug!("Sending start message to client {client_id}");
            let msg = ServerMessage::StartSignal(start_time);
            self.network.send_to_client(client_id, msg);
        }
    }

    fn save_output(&mut self) -> Result<(), std::io::Error> {
        // Build a combined output document with config + metrics
        let output = serde_json::json!({
            "config": &self.config,
            "metrics": {
                "fast_path_count": self.metrics.fast_path_count,
                "slow_path_count": self.metrics.slow_path_count,
                "committed_count": self.metrics.committed_count,
                "fast_path_ratio": self.metrics.fast_path_ratio(),
                "avg_consensus_latency_us": self.metrics.avg_consensus_latency_us(),
                "total_consensus_latency_us": self.metrics.total_consensus_latency_us,
            }
        });
        let output_json = serde_json::to_string_pretty(&output)?;
        let mut output_file = File::create(&self.config.local.output_filepath)?;
        output_file.write_all(output_json.as_bytes())?;
        output_file.flush()?;
        Ok(())
    }

    pub fn get_time(&self) -> u128 {
        let mut clock = self.clock.lock().unwrap();
        clock.get_time()
    }

    pub fn get_uncertainty(&self) -> u64 {
        let clock = self.clock.lock().unwrap();
        clock.get_uncertainty()
    }
}
