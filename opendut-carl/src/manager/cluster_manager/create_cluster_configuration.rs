use opendut_types::cluster::{ClusterConfiguration, ClusterId, ClusterName};
use tracing::{debug, info};
use crate::resource::api::resources::Resources;
use crate::resource::persistence::error::PersistenceError;
use crate::resource::storage::ResourcesStorageApi;

pub struct CreateClusterConfigurationParams {
    pub cluster_configuration: ClusterConfiguration,
}

impl Resources<'_> {
    #[tracing::instrument(skip_all, level="trace")]
    pub fn create_cluster_configuration(&mut self, params: CreateClusterConfigurationParams) -> Result<ClusterId, CreateClusterConfigurationError> {

        let cluster_id = params.cluster_configuration.id;
        let cluster_name = Clone::clone(&params.cluster_configuration.name);

        debug!("Creating cluster configuration '{cluster_name}' <{cluster_id}>.");

        self.insert(cluster_id, params.cluster_configuration)
            .map_err(|source| CreateClusterConfigurationError::Persistence { cluster_id, cluster_name: cluster_name.clone(), source })?;

        info!("Successfully created cluster configuration '{cluster_name}' <{cluster_id}>.");

        Ok(cluster_id)
    }
}

#[derive(thiserror::Error, Debug)]
#[error("ClusterConfigration '{cluster_name}' <{cluster_id}> could not be created")]
pub enum CreateClusterConfigurationError {
    Persistence {
        cluster_id: ClusterId,
        cluster_name: ClusterName,
        #[source] source: PersistenceError
    }
}


#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use crate::manager::testing::ClusterFixture;
    use super::*;

    /// Our data model does not match well with RDBMS, since we send full models to the backend, whereas RDBMSs work with diffs.
    /// As such, this verifies that we correctly diff the full models to delete removed child elements.
    #[test_with::no_env(SKIP_DATABASE_CONTAINER_TESTS)]
    #[tokio::test]
    async fn updating_should_correctly_remove_devices_from_the_database() -> anyhow::Result<()> {
        let db = crate::resource::persistence::testing::spawn_and_connect_resource_manager().await?;
        let resource_manager = db.resource_manager;
        let cluster = ClusterFixture::create(resource_manager.clone()).await?;
        resource_manager.insert(cluster.id, cluster.configuration.clone()).await?;

        assert_eq!(
            resource_manager.get::<ClusterConfiguration>(cluster.id).await?.unwrap(),
            cluster.configuration
        );

        let updated_cluster_configuration = ClusterConfiguration {
            devices: HashSet::from([cluster.peer_a.device_1]),
            ..cluster.configuration
        };
        resource_manager.insert(cluster.id, updated_cluster_configuration.clone()).await?;

        assert_eq!(
            resource_manager.get::<ClusterConfiguration>(cluster.id).await?.unwrap(),
            updated_cluster_configuration
        );

        Ok(())
    }
}
