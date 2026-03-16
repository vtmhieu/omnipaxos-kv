pub mod messages {
    use omnipaxos::{messages::Message as OmniPaxosMessage, util::NodeId};
    use serde::{Deserialize, Serialize};
        

    use crate::common::kv::CommitPath;

    use super::{
        kv::{Command, CommandId, KVCommand, ClientId},
        utils::Timestamp,
    };

    #[derive(Clone, Debug, Serialize, Deserialize)]
    pub enum RegistrationMessage {
        NodeRegister(NodeId),
        ClientRegister,
    }

    #[derive(Clone, Debug, Serialize, Deserialize)]
    pub struct LeaderResponse {
        pub command_id: CommandId,
        pub client_id: ClientId,
        pub response: ServerMessage,
    }

    #[derive(Clone, Debug, Serialize, Deserialize)]
    pub struct SyncIndex {
        pub client_id: ClientId,
        pub command_id: CommandId,
        pub deadline: Timestamp,
        pub log_index: usize,
    }

     #[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
    pub struct SlowPathReply {
        pub command_id: CommandId,
        pub client_id: ClientId,
        pub replica_id: NodeId,
    }

    #[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
    pub struct Reply {
        pub command_id: CommandId,
        pub client_id: ClientId,
        pub coordinator_id: NodeId,
        pub replica_id: NodeId,
        pub is_leader: bool,
        pub log_hash: u64,
        pub is_slow_path: bool,
    }

    #[derive(Clone, Debug, Serialize, Deserialize)]
    pub enum ClusterMessage {
        OmniPaxosMessage(OmniPaxosMessage<Command>),
        LeaderStartSignal(Timestamp),
        Command(Command),
        LeaderResponse(LeaderResponse),
        SyncIndex(SyncIndex),
        Reply(Reply),
        SlowPathReply(SlowPathReply),
    }

    #[derive(Clone, Debug, Serialize, Deserialize)]
    pub enum ClientMessage {
        Append(CommandId, KVCommand),
    }

    #[derive(Clone, Debug, Serialize, Deserialize)]
    pub enum ServerMessage {
        Write(CommandId, String),
        Read(CommandId, Option<String>, String),
        StartSignal(Timestamp),
    }

    impl ServerMessage {
        pub fn command_id(&self) -> CommandId {
            match self {
                ServerMessage::Write(id, commit_path) => *id,
                ServerMessage::Read(id, _, commit_path) => *id,
                ServerMessage::StartSignal(_) => unimplemented!(),
            }
        }

        pub fn commit_path(&self) -> String {
            match self {
                ServerMessage::Write(_, path) => path.to_string(),
                ServerMessage::Read(_, _, path) => path.to_string(),
                ServerMessage::StartSignal(_) => unimplemented!(),
            }
        }
    }
}

pub mod kv {
    use super::utils::Timestamp;
    use omnipaxos::{macros::Entry, storage::Snapshot};
    use serde::{Deserialize, Serialize};
    use std::cmp::Eq;
    use std::cmp::PartialEq;
    use std::collections::HashMap;

    pub type CommandId = usize;
    pub type ClientId = u64;
    pub type NodeId = omnipaxos::util::NodeId;
    pub type InstanceId = NodeId;

    #[derive(Debug, Clone, Entry, Serialize, Deserialize, Eq, PartialEq, Hash)]
    pub struct Command {
        pub client_id: ClientId,
        pub coordinator_id: NodeId,
        pub id: CommandId,
        pub kv_cmd: KVCommand,
        pub deadline: Timestamp,
        pub path: CommitPath,
    }

    #[derive(Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
    pub enum CommitPath {
        Fast,
        Slow,
    }

    impl Ord for Command {
        fn cmp(&self, other: &Self) -> std::cmp::Ordering {
            self.deadline.cmp(&other.deadline)
        }
    }

    impl PartialOrd for Command {
        fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
            Some(self.cmp(other))
        }
    }

    impl Command {
        pub fn request_key(&self) -> (ClientId, CommandId) {
            (self.client_id, self.id)
        }
    }

    #[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, Hash)]
    pub enum KVCommand {
        Put(String, String),
        Delete(String),
        Get(String),
    }

    #[derive(Clone, Debug, Serialize, Deserialize)]
    pub struct KVSnapshot {
        snapshotted: HashMap<String, String>,
        deleted_keys: Vec<String>,
    }

    impl Snapshot<Command> for KVSnapshot {
        fn create(entries: &[Command]) -> Self {
            let mut snapshotted = HashMap::new();
            let mut deleted_keys: Vec<String> = Vec::new();
            for e in entries {
                match &e.kv_cmd {
                    KVCommand::Put(key, value) => {
                        snapshotted.insert(key.clone(), value.clone());
                    }
                    KVCommand::Delete(key) => {
                        if snapshotted.remove(key).is_none() {
                            // key was not in the snapshot
                            deleted_keys.push(key.clone());
                        }
                    }
                    KVCommand::Get(_) => (),
                }
            }
            // remove keys that were put back
            deleted_keys.retain(|k| !snapshotted.contains_key(k));
            Self {
                snapshotted,
                deleted_keys,
            }
        }

        fn merge(&mut self, delta: Self) {
            for (k, v) in delta.snapshotted {
                self.snapshotted.insert(k, v);
            }
            for k in delta.deleted_keys {
                self.snapshotted.remove(&k);
            }
            self.deleted_keys.clear();
        }

        fn use_snapshots() -> bool {
            true
        }
    }
}

pub mod clock_c {
    use serde::{Deserialize, Serialize};

    #[derive(Clone, Debug, Serialize, Deserialize)]
    pub struct ClockConfig {
        pub drift_per_sec: f64,
        pub uncertainty_us: u64,
        pub sync_interval_ms: u64,
    }
}

pub mod utils {
    use super::messages::*;
    use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
    use tokio::net::TcpStream;
    use tokio_serde::{formats::Bincode, Framed};
    use tokio_util::codec::{Framed as CodecFramed, FramedRead, FramedWrite, LengthDelimitedCodec};

    pub type Timestamp = i64;

    pub type RegistrationConnection = Framed<
        CodecFramed<TcpStream, LengthDelimitedCodec>,
        RegistrationMessage,
        RegistrationMessage,
        Bincode<RegistrationMessage, RegistrationMessage>,
    >;

    pub fn frame_registration_connection(stream: TcpStream) -> RegistrationConnection {
        let length_delimited = CodecFramed::new(stream, LengthDelimitedCodec::new());
        Framed::new(length_delimited, Bincode::default())
    }

    pub type FromNodeConnection = Framed<
        FramedRead<OwnedReadHalf, LengthDelimitedCodec>,
        ClusterMessage,
        (),
        Bincode<ClusterMessage, ()>,
    >;
    pub type ToNodeConnection = Framed<
        FramedWrite<OwnedWriteHalf, LengthDelimitedCodec>,
        (),
        ClusterMessage,
        Bincode<(), ClusterMessage>,
    >;

    pub fn frame_cluster_connection(stream: TcpStream) -> (FromNodeConnection, ToNodeConnection) {
        let (reader, writer) = stream.into_split();
        let stream = FramedRead::new(reader, LengthDelimitedCodec::new());
        let sink = FramedWrite::new(writer, LengthDelimitedCodec::new());
        (
            FromNodeConnection::new(stream, Bincode::default()),
            ToNodeConnection::new(sink, Bincode::default()),
        )
    }

    // pub type ServerConnection = Framed<
    //     CodecFramed<TcpStream, LengthDelimitedCodec>,
    //     ServerMessage,
    //     ClientMessage,
    //     Bincode<ServerMessage, ClientMessage>,
    // >;

    pub type FromServerConnection = Framed<
        FramedRead<OwnedReadHalf, LengthDelimitedCodec>,
        ServerMessage,
        (),
        Bincode<ServerMessage, ()>,
    >;

    pub type ToServerConnection = Framed<
        FramedWrite<OwnedWriteHalf, LengthDelimitedCodec>,
        (),
        ClientMessage,
        Bincode<(), ClientMessage>,
    >;

    pub type FromClientConnection = Framed<
        FramedRead<OwnedReadHalf, LengthDelimitedCodec>,
        ClientMessage,
        (),
        Bincode<ClientMessage, ()>,
    >;

    pub type ToClientConnection = Framed<
        FramedWrite<OwnedWriteHalf, LengthDelimitedCodec>,
        (),
        ServerMessage,
        Bincode<(), ServerMessage>,
    >;

    pub fn frame_clients_connection(
        stream: TcpStream,
    ) -> (FromServerConnection, ToServerConnection) {
        let (reader, writer) = stream.into_split();
        let stream = FramedRead::new(reader, LengthDelimitedCodec::new());
        let sink = FramedWrite::new(writer, LengthDelimitedCodec::new());
        (
            FromServerConnection::new(stream, Bincode::default()),
            ToServerConnection::new(sink, Bincode::default()),
        )
    }

    // pub fn frame_clients_connection(stream: TcpStream) -> ServerConnection {
    //     let length_delimited = CodecFramed::new(stream, LengthDelimitedCodec::new());
    //     Framed::new(length_delimited, Bincode::default())
    // }

    pub fn frame_servers_connection(
        stream: TcpStream,
    ) -> (FromClientConnection, ToClientConnection) {
        let (reader, writer) = stream.into_split();
        let stream = FramedRead::new(reader, LengthDelimitedCodec::new());
        let sink = FramedWrite::new(writer, LengthDelimitedCodec::new());
        (
            FromClientConnection::new(stream, Bincode::default()),
            ToClientConnection::new(sink, Bincode::default()),
        )
    }
}
