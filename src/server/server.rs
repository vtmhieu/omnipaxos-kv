use crate::{configs::OmniPaxosKVConfig, database::Database, network::Network};
use chrono::Utc;
use log::*;
use omnipaxos::{
    messages::Message,
    util::{LogEntry, NodeId},
    OmniPaxos, OmniPaxosConfig,
};
use omnipaxos_kv::clock::SimulatedClock;
use omnipaxos_kv::common::{clock_c::*, kv::*, messages::*, utils::Timestamp};
use omnipaxos_storage::memory_storage::MemoryStorage;
use serde::Serialize;
use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::{fs::File, io::Write, time::Duration};

use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

type OmniPaxosInstance = OmniPaxos<Command, MemoryStorage<Command>>;
const NETWORK_BATCH_SIZE: usize = 100;
const LEADER_WAIT: Duration = Duration::from_secs(1);
const ELECTION_TIMEOUT: Duration = Duration::from_secs(1);
const LATENCY_BOUND_US: i64 = 200;
const FASTPATH_INTERVAL: Duration = Duration::from_millis(1);

pub struct OmniPaxosServer {
    id: NodeId,
    database: Database,
    network: Network,
    omnipaxos: OmniPaxosInstance,
    current_decided_idx: usize,
    omnipaxos_msg_buffer: Vec<Message<Command>>,
    config: OmniPaxosKVConfig,
    peers: Vec<NodeId>,
    consistency_check: bool,
    clock: SimulatedClock,
    early_buffer: BinaryHeap<Reverse<Command>>,
    last_log_deadline: Timestamp,
    fast_replies: HashMap<CommandId, FastReplyTracker>,
    leader_responses: HashMap<CommandId, (ClientId, ServerMessage)>,
}

struct FastReplyTracker {
    leader_hash: Option<u64>,
    follower_hashes: HashMap<u64, usize>,
}

impl OmniPaxosServer {
    pub async fn new(config: OmniPaxosKVConfig) -> Self {
        // Initialize OmniPaxos instance
        let storage: MemoryStorage<Command> = MemoryStorage::default();
        let omnipaxos_config: OmniPaxosConfig = config.clone().into();
        let omnipaxos_msg_buffer = Vec::with_capacity(omnipaxos_config.server_config.buffer_size);
        let omnipaxos = omnipaxos_config.build(storage).unwrap();
        // Waits for client and server network connections to be established
        let network = Network::new(config.clone(), NETWORK_BATCH_SIZE).await;

        let clock = SimulatedClock::new(
            config.local.clock.drift_per_sec,
            config.local.clock.uncertainty_us,
            Duration::from_millis(config.local.clock.sync_interval_ms),
        );

        OmniPaxosServer {
            id: config.local.server_id,
            database: Database::new(),
            network,
            omnipaxos,
            current_decided_idx: 0,
            omnipaxos_msg_buffer,
            peers: config.get_peers(config.local.server_id),
            config,
            consistency_check: false,
            clock,
            early_buffer: BinaryHeap::new(),
            last_log_deadline: 0,
            fast_replies: HashMap::new(),
            leader_responses: HashMap::new(),
        }
    }

    pub async fn run(&mut self) {
        // Save config to output file
        if self.consistency_check {
            let path = format!(
                "{}-decided-log.json",
                self.config.local.output_filepath.trim_end_matches(".json")
            );
            let _ = std::fs::remove_file(&path);
        }
        self.save_output().expect("Failed to write to file");
        let mut client_msg_buf = Vec::with_capacity(NETWORK_BATCH_SIZE);
        let mut cluster_msg_buf = Vec::with_capacity(NETWORK_BATCH_SIZE);
        // We don't use Omnipaxos leader election at first and instead force a specific initial leader
        self.establish_initial_leader(&mut cluster_msg_buf, &mut client_msg_buf)
            .await;
        // Main event loop with leader election
        let mut election_interval = tokio::time::interval(ELECTION_TIMEOUT);

        let mut fast_path_interval = tokio::time::interval(FASTPATH_INTERVAL);

        loop {
            tokio::select! {
                _ = fast_path_interval.tick() => {
                    self.release_from_early_buffer();
                },
                _ = election_interval.tick() => {
                    self.omnipaxos.tick();
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
        // TODO: Can use a read_raw here to avoid allocation
        let new_decided_idx = self.omnipaxos.get_decided_idx();
        if self.current_decided_idx < new_decided_idx {
            let decided_entries = self
                .omnipaxos
                .read_decided_suffix(self.current_decided_idx)
                .unwrap();
            self.current_decided_idx = new_decided_idx;
            debug!("Decided {new_decided_idx}");
            let decided_commands = decided_entries
                .into_iter()
                .filter_map(|e| match e {
                    LogEntry::Decided(cmd) => {
                        // info!(
                        //     "Server {} decided cmd {} deadline {}",
                        //     self.id,
                        //     cmd.id,
                        //     cmd.deadline
                        // );
                        Some(cmd)
                    }
                    _ => unreachable!(),
                })
                .collect();

            // send fast reply
            let log_hash = self.log_hash();
            let is_leader = self.is_current_leader();

            for cmd in &decided_commands {
                let cmd: &Command = cmd;

                let fast_reply = FastReply {
                    command_id: cmd.id,
                    client_id: cmd.client_id,
                    coordinator_id: cmd.coordinator_id,
                    replica_id: self.id,
                    is_leader,
                    log_hash,
                };

                let msg = ClusterMessage::FastReply(fast_reply.clone());

                if cmd.coordinator_id == self.id {
                    // coordinator itself
                    self.handle_fast_reply(fast_reply);
                } else {
                    self.network.send_to_cluster(cmd.coordinator_id, msg);
                }
            }

            self.update_database_and_respond(decided_commands);
            if self.consistency_check {
                self.snapshot_decided_log();
            }
        }
    }

    pub fn get_decided_log(&self) -> Vec<Command> {
        self.omnipaxos
            .read_decided_suffix(0)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|e| match e {
                LogEntry::Decided(cmd) => Some(cmd),
                _ => None,
            })
            .collect()
    }

    fn snapshot_decided_log(&self) {
        let log = self.get_decided_log();

        #[derive(Serialize)]
        struct CommandEntry {
            idx: usize,
            op: String,
        }

        let commands: Vec<CommandEntry> = log
            .iter()
            .enumerate()
            .map(|(idx, cmd)| {
                let op = match &cmd.kv_cmd {
                    KVCommand::Put(key, value) => format!("Put({}, {})", key, value),
                    KVCommand::Get(key) => format!("Get({})", key),
                    KVCommand::Delete(key) => format!("Delete({})", key),
                };
                CommandEntry { idx, op }
            })
            .collect();

        let path = format!(
            "{}-decided-log.json",
            self.config.local.output_filepath.trim_end_matches(".json")
        );

        let mut snapshots: Vec<serde_json::Value> =
            if let Ok(contents) = std::fs::read_to_string(&path) {
                serde_json::from_str(&contents).unwrap_or_default()
            } else {
                vec![]
            };

        snapshots.push(serde_json::to_value(&commands).unwrap());

        if let Ok(json) = serde_json::to_string_pretty(&snapshots) {
            if let Ok(mut f) = File::create(&path) {
                let _ = f.write_all(json.as_bytes());
            }
        }
    }

    fn is_current_leader(&self) -> bool {
        if let Some((leader_id, accept_phase)) = self.omnipaxos.get_current_leader() {
            leader_id == self.id && accept_phase
        } else {
            false
        }
    }

    fn update_database_and_respond(&mut self, commands: Vec<Command>) {
        let is_leader = self.is_current_leader();

        // TODO: batching responses possible here (batch at handle_cluster_messages)
        for command in commands {
            let read = self.database.handle_command(command.kv_cmd);

            if is_leader {
                let response = match read {
                    Some(read_result) => ServerMessage::Read(command.id, read_result),
                    None => ServerMessage::Write(command.id),
                };

                if command.coordinator_id == self.id {
                    // Leader is coordinator
                    self.leader_responses
                        .insert(command.id, (command.client_id, response));
                } else {
                    // Send to coordinator
                    self.network.send_to_cluster(
                        command.coordinator_id,
                        ClusterMessage::LeaderResponse(LeaderResponse {
                            command_id: command.id,
                            client_id: command.client_id,
                            response,
                        }),
                    );
                }
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
                ClientMessage::Append(command_id, kv_command) => {
                    let current_ts = self.clock.get_time();
                    let uncertainty = self.clock.get_uncertainty() as i64;
                    let deadline = current_ts + uncertainty + LATENCY_BOUND_US;

                    self.append_to_log(from, command_id, kv_command, deadline)
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
                ClusterMessage::FastReply(reply) => {
                    self.handle_fast_reply(reply);
                }
                ClusterMessage::LeaderResponse(leader_response) => {
                    self.leader_responses.insert(
                        leader_response.command_id,
                        (leader_response.client_id, leader_response.response),
                    );
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
        deadline: Timestamp,
    ) {
        let command = Command {
            client_id: from,
            coordinator_id: self.id,
            id: command_id,
            kv_cmd: kv_command,
            deadline,
        };

        // check deadline and last_log_deadline
        if command.deadline < self.last_log_deadline {
            // debug!(
            //     "Command {} from client {} with deadline {} is too late (last log deadline {})",
            //     command_id, from, command.deadline, self.last_log_deadline
            // );
            self.omnipaxos
                .append(command)
                .expect("Append to Omnipaxos log failed");
        } else {
            // debug!(
            //     "Command {} from client {} with deadline {} is into early_buffer (last log deadline {})",
            //     command_id, from, command.deadline, self.last_log_deadline
            // );
            self.early_buffer.push(Reverse(command));
        }

        // info!(
        //     "Server {} appended command {} with deadline {}",
        //     self.id,
        //     command_id,
        //     deadline
        // );
    }

    fn release_from_early_buffer(&mut self) {
        while let Some(Reverse(cmd)) = self.early_buffer.peek() {
            let current_time = self.clock.get_time();
            if cmd.deadline <= current_time {
                let Reverse(cmd) = self.early_buffer.pop().unwrap();

                self.last_log_deadline = cmd.deadline;

                // debug!(
                //     "Releasing command {} from client {} with deadline {} from early_buffer (last log deadline {})",
                //     cmd.id, cmd.client_id, cmd.deadline, self.last_log_deadline
                // );

                self.omnipaxos.append(cmd).expect("Append failed");
            } else {
                break;
            }
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
        let config_json = serde_json::to_string_pretty(&self.config)?;
        let mut output_file = File::create(&self.config.local.output_filepath)?;
        output_file.write_all(config_json.as_bytes())?;
        output_file.flush()?;
        Ok(())
    }

    fn log_hash(&self) -> u64 {
        let mut hasher = DefaultHasher::new();

        for cmd in self.get_decided_log() {
            cmd.hash(&mut hasher);
        }

        hasher.finish()
    }

    fn handle_fast_reply(&mut self, reply: FastReply) {
        let tracker = self
            .fast_replies
            .entry(reply.command_id)
            .or_insert(FastReplyTracker {
                leader_hash: None,
                follower_hashes: HashMap::new(),
            });

        if reply.is_leader {
            tracker.leader_hash = Some(reply.log_hash);
        } else {
            *tracker.follower_hashes.entry(reply.log_hash).or_insert(0) += 1;
        }

        let cluster_size = self.peers.len() + 1;
        let f = (cluster_size - 1) / 2;
        let required_followers = f + f / 2;

        if let Some(leader_hash) = tracker.leader_hash {
            let follower_count = tracker
                .follower_hashes
                .get(&leader_hash)
                .copied()
                .unwrap_or(0);

            if follower_count >= required_followers {
                // info!(
                //     "Command {} committed with hash {}",
                //     reply.command_id,
                //     leader_hash
                // );d

                if let Some((client_id, response)) = self.leader_responses.remove(&reply.command_id)
                {
                    self.network.send_to_client(client_id, response);
                }

                self.fast_replies.remove(&reply.command_id);
            }
        }
    }
}
