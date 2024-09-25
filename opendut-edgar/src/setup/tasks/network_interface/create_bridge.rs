use std::sync::Arc;
use anyhow::Result;
use async_trait::async_trait;
use opendut_types::util::net::NetworkInterfaceName;

use crate::service::network_interface::manager::NetworkInterfaceManagerRef;
use crate::common::task::{Success, Task, TaskFulfilled};

pub struct CreateBridge {
    pub network_interface_manager: NetworkInterfaceManagerRef,
    pub bridge_name: NetworkInterfaceName,
}

#[async_trait]
impl Task for CreateBridge {
    fn description(&self) -> String {
        format!("Create Bridge \"{}\"", self.bridge_name)
    }
    async fn check_fulfilled(&self) -> Result<TaskFulfilled> {
        let bridge_exists = self.network_interface_manager.find_interface(&self.bridge_name).await?
            .is_some();

        if bridge_exists {
            Ok(TaskFulfilled::Yes)
        } else {
            Ok(TaskFulfilled::No)
        }
    }
    async fn execute(&self) -> Result<Success> {
        crate::service::network_interface::bridge::create(&self.bridge_name, Arc::clone(&self.network_interface_manager)).await?;

        Ok(Success::default())
    }
}
