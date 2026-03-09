use std::env;

use config::{Config, ConfigError, Environment, File};
use omnipaxos::{
    util::{FlexibleQuorum, NodeId},
    ClusterConfig as OmnipaxosClusterConfig, OmniPaxosConfig,
    ServerConfig as OmnipaxosServerConfig,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ClusterConfig {
    pub nodes: Vec<NodeId>,
    pub node_addrs: Vec<String>,
    pub initial_leader: NodeId,
    pub initial_flexible_quorum: Option<FlexibleQuorum>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct ClockConfig {
    /// Clock drift rate in microseconds per second
    #[serde(default = "default_drift_rate")]
    pub drift_rate_us_per_s: f64,
    /// Synchronization uncertainty bound (±ε microseconds)
    #[serde(default = "default_uncertainty_bound")]
    pub uncertainty_us: u64,
    /// Synchronization interval in milliseconds
    #[serde(default = "default_sync_interval_ms")]
    pub sync_interval_ms: u64,
}

fn default_drift_rate() -> f64 {
    0.0 // Default: no drift
}

fn default_uncertainty_bound() -> u64 {
    100_000 // Default: ±100ms (100,000 microseconds)
}

fn default_sync_interval_ms() -> u64 {
    10_000 // Default: 10 seconds (10,000 milliseconds)
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LocalConfig {
    pub location: Option<String>,
    pub server_id: NodeId,
    pub listen_address: String,
    pub listen_port: u16,
    pub num_clients: usize,
    pub output_filepath: String,
    /// Clock configuration (optional, uses defaults if not specified)
    #[serde(default)]
    pub clock: ClockConfig,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OmniPaxosKVConfig {
    #[serde(flatten)]
    pub local: LocalConfig,
    #[serde(flatten)]
    pub cluster: ClusterConfig,
}

impl Into<OmniPaxosConfig> for OmniPaxosKVConfig {
    fn into(self) -> OmniPaxosConfig {
        let cluster_config = OmnipaxosClusterConfig {
            configuration_id: 1,
            nodes: self.cluster.nodes,
            flexible_quorum: self.cluster.initial_flexible_quorum,
        };
        let server_config = OmnipaxosServerConfig {
            pid: self.local.server_id,
            ..Default::default()
        };
        OmniPaxosConfig {
            cluster_config,
            server_config,
        }
    }
}

impl OmniPaxosKVConfig {
    pub fn new() -> Result<Self, ConfigError> {
        let local_config_file = match env::var("SERVER_CONFIG_FILE") {
            Ok(file_path) => file_path,
            Err(_) => panic!("Requires SERVER_CONFIG_FILE environment variable to be set"),
        };
        let cluster_config_file = match env::var("CLUSTER_CONFIG_FILE") {
            Ok(file_path) => file_path,
            Err(_) => panic!("Requires CLUSTER_CONFIG_FILE environment variable to be set"),
        };
        let config = Config::builder()
            .add_source(File::with_name(&local_config_file))
            .add_source(File::with_name(&cluster_config_file))
            // Add-in/overwrite settings with environment variables (with a prefix of OMNIPAXOS)
            .add_source(
                Environment::with_prefix("OMNIPAXOS")
                    .try_parsing(true)
                    .list_separator(",")
                    .with_list_parse_key("node_addrs"),
            )
            .build()?;
        config.try_deserialize()
    }

    pub fn get_peers(&self, node: NodeId) -> Vec<NodeId> {
        self.cluster
            .nodes
            .iter()
            .cloned()
            .filter(|&id| id != node)
            .collect()
    }
}
