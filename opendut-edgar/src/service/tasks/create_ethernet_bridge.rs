use crate::common::task::{Success, Task, TaskFulfilled};
use crate::service::cluster_assignment::Error;
use crate::service::network_interface::bridge;
use crate::service::network_interface::manager::NetworkInterfaceManagerRef;
use opendut_types::peer::configuration::{Parameter, ParameterTarget};
use opendut_types::peer::configuration::parameter;
use std::sync::Arc;
use async_trait::async_trait;

pub struct CreateEthernetBridge {
    pub parameter: Parameter<parameter::EthernetBridge>,
    pub network_interface_manager: NetworkInterfaceManagerRef,
}
#[async_trait]
impl Task for CreateEthernetBridge {
    fn description(&self) -> String {
        format!("Create bridge '{}'", self.parameter.value.name)
    }

    async fn check_fulfilled(&self) -> anyhow::Result<TaskFulfilled> {
        match self.parameter.target {
            ParameterTarget::Present => Ok(TaskFulfilled::Unchecked), //TODO we currently run it always, because we re-create the bridge
            ParameterTarget::Absent => todo!("Setting Ethernet bridges absent is not yet implemented.")
        }
    }

    async fn execute(&self) -> anyhow::Result<Success> {
        match self.parameter.target {
            ParameterTarget::Present => {
                let bridge = &self.parameter.value;

                bridge::recreate(&bridge.name, Arc::clone(&self.network_interface_manager)).await
                    .map_err(Error::BridgeRecreationFailed)?;

                Ok(Success::default())
            }
            ParameterTarget::Absent => {
                todo!("Setting Ethernet bridges absent is not yet implemented.")
            }
        }
    }
}
