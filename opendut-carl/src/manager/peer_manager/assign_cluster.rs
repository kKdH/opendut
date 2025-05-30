use crate::manager::peer_messaging_broker::PeerMessagingBrokerRef;
use crate::resource::persistence::error::PersistenceError;
use crate::resource::storage::ResourcesStorageApi;
use opendut_carl_api::proto::services::peer_messaging_broker::{downstream, ApplyPeerConfiguration};
use opendut_types::cluster::ClusterAssignment;
use opendut_types::peer::configuration::parameter;
use opendut_types::peer::configuration::{OldPeerConfiguration, ParameterTarget, PeerConfiguration};
use opendut_types::peer::{PeerDescriptor, PeerId};
use opendut_types::util::net::{NetworkInterfaceDescriptor, NetworkInterfaceName};
use tracing::debug;
use crate::resource::api::resources::Resources;

pub struct AssignClusterParams {
    pub peer_messaging_broker: PeerMessagingBrokerRef,
    pub peer_id: PeerId,
    pub device_interfaces: Vec<NetworkInterfaceDescriptor>,
    pub cluster_assignment: ClusterAssignment,
    pub options: AssignClusterOptions,
}

#[derive(Clone)]
pub struct AssignClusterOptions {
    pub bridge_name_default: NetworkInterfaceName
}

#[derive(thiserror::Error, Debug)]
pub enum AssignClusterError {
    #[error("Assigning cluster for peer <{0}> failed, because a peer with that ID does not exist!")]
    PeerNotFound(PeerId),
    #[error("Sending PeerConfiguration with ClusterAssignment to peer <{peer_id}> failed: {cause}")]
    SendingToPeerFailed { peer_id: PeerId, cause: String },
    #[error("Error while persisting ClusterAssignment for peer <{peer_id}>.")]
    Persistence { peer_id: PeerId, #[source] source: PersistenceError },
}

impl Resources<'_> {
    pub async fn assign_cluster(&mut self, params: AssignClusterParams) -> Result<(), AssignClusterError> {
        let AssignClusterParams { peer_messaging_broker, peer_id, cluster_assignment, device_interfaces, options } = params;

        debug!("Assigning cluster to peer <{peer_id}>.");

        let (old_peer_configuration, peer_configuration) = {
            let old_peer_configuration = OldPeerConfiguration {
                cluster_assignment: Some(cluster_assignment),
            };
            self.insert(peer_id, Clone::clone(&old_peer_configuration))
                .map_err(|source| AssignClusterError::Persistence { peer_id, source })?;

            let peer_configuration = {
                let peer_descriptor = self.get::<PeerDescriptor>(peer_id)
                    .map_err(|source| AssignClusterError::Persistence { peer_id, source })?
                    .ok_or(AssignClusterError::PeerNotFound(peer_id))?;

                let mut peer_configuration = self.get::<PeerConfiguration>(peer_id)
                    .map_err(|source| AssignClusterError::Persistence { peer_id, source })?
                    .unwrap_or_default();

                for device_interface in device_interfaces.into_iter() {
                    let device_interface = parameter::DeviceInterface { descriptor: device_interface };
                    peer_configuration.set(device_interface, ParameterTarget::Present); //TODO not always Present
                }

                {
                    let ethernet_bridge = peer_descriptor.clone().network.bridge_name
                        .unwrap_or(options.bridge_name_default);
                    let bridge = parameter::EthernetBridge { name: ethernet_bridge };

                    peer_configuration.set(bridge, ParameterTarget::Present); //TODO not always Present
                }

                for executor_descriptor in Clone::clone(&peer_descriptor.executors).executors.into_iter() {
                    let executor = parameter::Executor { descriptor: executor_descriptor };
                    peer_configuration.set(executor, ParameterTarget::Present); //TODO not always Present
                }

                peer_configuration
            };
            self.insert(peer_id, Clone::clone(&peer_configuration))
                .map_err(|source| AssignClusterError::Persistence { peer_id, source })?;

            (old_peer_configuration, peer_configuration)
        };

        peer_messaging_broker.send_to_peer(
            peer_id,
            downstream::Message::ApplyPeerConfiguration(ApplyPeerConfiguration {
                old_configuration: Some(old_peer_configuration.into()),
                configuration: Some(peer_configuration.into()),
            }),
        ).await
        .map_err(|cause| AssignClusterError::SendingToPeerFailed {
            peer_id,
            cause: cause.to_string()
        })?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manager::peer_messaging_broker::{PeerMessagingBroker, PeerMessagingBrokerOptions};
    use crate::resource::manager::ResourceManager;
    use googletest::prelude::*;
    use opendut_types::cluster::{ClusterAssignment, ClusterId};
    use std::net::IpAddr;
    use std::str::FromStr;
    use std::sync::Arc;
    use opendut_carl_api::carl::broker::stream_header;
    use crate::manager::peer_manager::tests::create_peer_descriptor;
    use crate::manager::testing::PeerFixture;

    #[tokio::test]
    async fn should_update_peer_configuration() -> anyhow::Result<()> {
        let peer = PeerFixture::new();

        let settings = crate::settings::load_defaults()?;
        let peer_id = peer.id;

        let resource_manager = ResourceManager::new_in_memory();
        let peer_messaging_broker = PeerMessagingBroker::new(
            Arc::clone(&resource_manager),
            PeerMessagingBrokerOptions::load(&settings.config).unwrap(),
        ).await;

        let old_peer_configuration = OldPeerConfiguration {
            cluster_assignment: None,
        };
        let peer_configuration = PeerConfiguration::default();
        resource_manager.resources_mut(async |resources| {
            resources.insert(peer_id, create_peer_descriptor())?;
            resources.insert(peer_id, Clone::clone(&old_peer_configuration))?;
            resources.insert(peer_id, Clone::clone(&peer_configuration))
        }).await??;

        let (_, mut receiver) = peer_messaging_broker.open(peer_id, IpAddr::from_str("1.2.3.4")?, stream_header::ExtraHeaders::default()).await?;
        let received = receiver.recv().await.unwrap()
            .message.unwrap();
        assert_that!(
            received,
            eq(&downstream::Message::ApplyPeerConfiguration(ApplyPeerConfiguration {
                old_configuration: Some(Clone::clone(&old_peer_configuration).into()),
                configuration: Some(Clone::clone(&peer_configuration).into()),
            }))
        );


        let cluster_assignment = ClusterAssignment {
            id: ClusterId::random(),
            leader: PeerId::random(),
            assignments: vec![],
        };


        resource_manager.resources_mut(async |resources|
            resources.assign_cluster(AssignClusterParams {
                peer_messaging_broker: Arc::clone(&peer_messaging_broker),
                peer_id,
                cluster_assignment: Clone::clone(&cluster_assignment),
                device_interfaces: vec![],
                options: AssignClusterOptions {
                    bridge_name_default: NetworkInterfaceName::try_from("br-opendut").unwrap(),
                }
            }).await
        ).await??;


        let old_peer_configuration = OldPeerConfiguration {
            cluster_assignment: Some(cluster_assignment),
        };
        assert_that!(
            resource_manager.get::<OldPeerConfiguration>(peer_id).await?.as_ref(),
            some(eq(&old_peer_configuration))
        );

        let mut peer_configuration = PeerConfiguration::default();
        peer_configuration.set(
            parameter::EthernetBridge { name: NetworkInterfaceName::try_from("br-opendut-1")? },
            ParameterTarget::Present
        );

        let received = receiver.recv().await.unwrap()
            .message.unwrap();

        let downstream::Message::ApplyPeerConfiguration(ApplyPeerConfiguration {
            old_configuration, configuration
        }) = received else { panic!() };

        assert_that!(OldPeerConfiguration::try_from(old_configuration.unwrap())?, eq(&old_peer_configuration));
        assert_that!(PeerConfiguration::try_from(configuration.unwrap())?, eq(&peer_configuration));

        Ok(())
    }

}
