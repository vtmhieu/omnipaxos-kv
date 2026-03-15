import toml
from dataclasses import dataclass, asdict

@dataclass(frozen=True)
class ClockConfig:
    drift_per_sec: float
    uncertainty_us: int
    sync_interval_ms: int

@dataclass(frozen=True)
class OmniPaxosKVServerConfig:
    location: str
    server_id: int
    listen_address: str
    listen_port: int
    num_clients: int
    output_filepath: str
    clock: ClockConfig

@dataclass(frozen=True)
class ServerConfig:
    omnipaxos_server_config: OmniPaxosKVServerConfig
    
    def generate_server_toml(self) -> str:
        return toml.dumps(asdict(self.omnipaxos_server_config))

def test_toml_generation():
    clock = ClockConfig(drift_per_sec=2.5, uncertainty_us=50, sync_interval_ms=1000)
    config = ServerConfig(
        omnipaxos_server_config=OmniPaxosKVServerConfig(
            location="us-central1-a",
            server_id=1,
            listen_address="0.0.0.0",
            listen_port=8000,
            num_clients=1,
            output_filepath="test.json",
            clock=clock
        )
    )
    
    toml_str = config.generate_server_toml()
    print("Generated TOML:")
    print(toml_str)
    
    parsed = toml.loads(toml_str)
    assert "clock" in parsed
    assert parsed["clock"]["drift_per_sec"] == 2.5
    assert parsed["clock"]["uncertainty_us"] == 50
    assert parsed["clock"]["sync_interval_ms"] == 1000
    print("\nVerification successful!")

if __name__ == "__main__":
    test_toml_generation()
