use async_trait::async_trait;

use opendut_types::cluster::ClusterId;
use opendut_types::peer::PeerId;
use opendut_types::vpn::VpnPeerConfiguration;

#[async_trait]
pub trait VpnManagementClient {

    async fn create_cluster(&self, cluster_id: ClusterId, peers: &[PeerId]) -> Result<(), CreateClusterError>;

    async fn delete_cluster(&self, cluster_id: ClusterId) -> Result<(), DeleteClusterError>;

    async fn create_peer(&self, peer_id: PeerId) -> Result<(), CreatePeerError>;

    async fn delete_peer(&self, peer_id: PeerId) -> Result<(), DeletePeerError>;

    async fn generate_vpn_peer_configuration(&self, peer_id: PeerId) -> Result<VpnPeerConfiguration, CreateVpnPeerConfigurationError>;
}

#[derive(thiserror::Error, Debug)]
pub enum CreateClusterError {
    #[error("Peer <{peer_id}> of cluster <{cluster_id}> could not be resolved")]
    PeerResolutionFailure {
        peer_id: PeerId,
        cluster_id: ClusterId,
        #[source] error: Box<dyn std::error::Error + Send + Sync>,
    },
    #[error("An error occurred while creating cluster <{cluster_id}>")]
    CreationFailure {
        cluster_id: ClusterId,
        #[source] error: Box<dyn std::error::Error + Send + Sync>
    },
    #[error("An error occurred while creating access control rule for cluster <{cluster_id}>")]
    AccessPolicyCreationFailure {
        cluster_id: ClusterId,
        #[source] error: Box<dyn std::error::Error + Send + Sync>
    }
}

#[derive(thiserror::Error, Debug)]
pub enum DeleteClusterError {
    #[error("No cluster <{cluster_id}> could be found: {message}")]
    NotFound {
        cluster_id: ClusterId,
        message: String,
    },
    #[error("An error occurred while deleting cluster <{cluster_id}>")]
    DeletionFailure {
        cluster_id: ClusterId,
        #[source] error: Box<dyn std::error::Error + Send + Sync>
    },
}

#[derive(thiserror::Error, Debug)]
pub enum CreatePeerError {
    #[error("An error occurred while creating peer <{peer_id}>")]
    CreationFailure {
        peer_id: PeerId,
        #[source] error: Box<dyn std::error::Error + Send + Sync>
    }
}

#[derive(thiserror::Error, Debug)]
pub enum DeletePeerError {
    #[error("Peer <{peer_id}> could not be resolved")]
    ResolutionFailure {
        peer_id: PeerId,
        #[source] error: Box<dyn std::error::Error + Send + Sync>
    },
    #[error("An error occurred while deleting peer <{peer_id}>")]
    DeletionFailure {
        peer_id: PeerId,
        #[source] error: Box<dyn std::error::Error + Send + Sync>
    },
}

#[derive(thiserror::Error, Debug)]
pub enum CreateVpnPeerConfigurationError {
    #[error("An error occurred while creating a vpn configuration for peer <{peer_id}>")]
    CreationFailure {
        peer_id: PeerId,
        #[source] error: Box<dyn std::error::Error + Send + Sync>
    },
}
