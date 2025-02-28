use std::fmt::{Display, Formatter};

use cli_table::{print_stdout, Table, WithTitle};
use serde::Serialize;

use opendut_carl_api::carl::CarlClient;
use opendut_types::peer::{PeerDescriptor, PeerId, PeerLocation, PeerName};
use opendut_types::peer::state::PeerState;
use crate::ListOutputFormat;

/// List all peers
#[derive(clap::Parser)]
pub struct ListPeersCli;

#[derive(Serialize, Debug)]
struct SerializablePeer {
    name: PeerName,
    id: PeerId,
    status: PeerStatus,
    location: PeerLocation,
    network_interfaces: Vec<String>,
}

#[derive(Table)]
struct PeerTable {
    #[table(title = "Name")]
    name: PeerName,
    #[table(title = "PeerID")]
    id: PeerId,
    #[table(title = "Status")]
    status: PeerStatus,
    #[table(title = "Location")]
    location: PeerLocation,
    #[table(title = "NetworkInterfaces")]
    network_interfaces: String,
}
impl From<SerializablePeer> for PeerTable {
    fn from(peer: SerializablePeer) -> Self {
        let SerializablePeer { name, id, status, location, network_interfaces } = peer;

        PeerTable {
            name,
            id,
            status,
            location,
            network_interfaces: network_interfaces.join(", "),
        }
    }
}


#[derive(Debug, PartialEq, Serialize)]
enum PeerStatus {
    Connected,
    Disconnected,
}

impl Display for PeerStatus {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            PeerStatus::Connected => write!(f, "Connected"),
            PeerStatus::Disconnected => write!(f, "Disconnected"),
        }
    }
}

impl ListPeersCli {
    pub async fn execute(self, carl: &mut CarlClient, output: ListOutputFormat) -> crate::Result<()> {
        let all_peers = carl
            .peers
            .list_peer_descriptors()
            .await
            .map_err(|error| format!("Could not list peers.\n  {}", error))?;
        
        let mut serializable_peers = vec![];
        for peer in all_peers {
            let peer_state = carl.peers.get_peer_state(peer.id).await.map_err(|_| {
                format!("Failed to retrieve state for peer <{}>", peer.id)
            })?;
            serializable_peers.push(add_peer_status(peer, peer_state));
        };
        match output {
            ListOutputFormat::Table => {
                let peer_table = serializable_peers.into_iter()
                    .map(PeerTable::from)
                    .collect::<Vec<_>>();

                print_stdout(peer_table.with_title())
                    .expect("List of clusters should be printable as table.");
            }
            ListOutputFormat::Json => {
                let json = serde_json::to_string(&serializable_peers).unwrap();
                println!("{}", json);
            }
            ListOutputFormat::PrettyJson => {
                let json = serde_json::to_string_pretty(&serializable_peers).unwrap();
                println!("{}", json);
            }
        }
        Ok(())
    }
}

fn add_peer_status(
    peer: PeerDescriptor,
    peer_state: PeerState
) -> SerializablePeer {
    let status = match peer_state {
        PeerState::Down => { PeerStatus::Disconnected }
        PeerState::Up { .. } => { PeerStatus::Connected }
    };
    let network_interfaces = peer.network.interfaces.iter()
        .map(|interface| interface.name.to_string())
        .collect::<Vec<_>>();

    SerializablePeer {
        name: Clone::clone(&peer.name),
        id: peer.id,
        location: Clone::clone(&peer.location.clone().unwrap_or_default()),
        network_interfaces,
        status
    }
}

#[cfg(test)]
mod test {
    use googletest::prelude::*;

    use opendut_types::peer::{PeerDescriptor, PeerId, PeerLocation, PeerName, PeerNetworkDescriptor};
    use opendut_types::peer::executor::ExecutorDescriptors;
    use opendut_types::util::net::{NetworkInterfaceConfiguration, NetworkInterfaceDescriptor, NetworkInterfaceId, NetworkInterfaceName};

    use super::*;

    #[test]
    fn test() {
        let peer = PeerDescriptor {
            id: PeerId::random(),
            name: PeerName::try_from("MyPeer").unwrap(),
            location: Some(PeerLocation::try_from("SiFi").unwrap()),
            network: PeerNetworkDescriptor{
                interfaces: vec!(NetworkInterfaceDescriptor {
                    id: NetworkInterfaceId::random(),
                    name: NetworkInterfaceName::try_from("eth0").unwrap(),
                    configuration: NetworkInterfaceConfiguration::Ethernet,
                }),
                bridge_name: Some(NetworkInterfaceName::try_from("br-opendut-1").unwrap())
            },
            topology: Default::default(),
            executors: ExecutorDescriptors {
                executors: vec![]
            }
        };
        assert_that!(
            add_peer_status(peer.clone(), PeerState::Down),
            matches_pattern!(SerializablePeer {
                name: eq(&peer.name),
                id: eq(&peer.id),
                status: eq(&PeerStatus::Disconnected),
            })
        );
    }
}
