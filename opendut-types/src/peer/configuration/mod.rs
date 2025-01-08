use crate::cluster::ClusterAssignment;
use crate::peer::executor::ExecutorDescriptor;

pub mod api;
pub use crate::peer::configuration::api::*;

pub mod parameter;
use parameter::{DeviceInterface, EthernetBridge};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OldPeerConfiguration {
    pub cluster_assignment: Option<ClusterAssignment>,
    // Please add new fields into PeerConfiguration instead.
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PeerConfiguration {
    pub device_interfaces: Vec<Parameter<DeviceInterface>>,
    pub ethernet_bridges: Vec<Parameter<EthernetBridge>>,
    pub executors: Vec<Parameter<ExecutorDescriptor>>,
    //TODO migrate more parameters
}
impl PeerConfiguration {
    pub fn set<T: ParameterValue>(&mut self, value: T, target: ParameterTarget) {
        let parameter = Parameter {
            id: value.parameter_identifier(),
            dependencies: vec![], //TODO
            target,
            value,
        };

        let parameters = T::peer_configuration_field(self);

        parameters.retain(|existing_parameter| {
            existing_parameter.id != parameter.id
        });

        parameters.push(parameter);
    }
}


#[cfg(test)]
mod tests {
    use crate::peer::executor::{ExecutorId, ExecutorKind, ResultsUrl};
    use crate::util::net::NetworkInterfaceName;
    use super::*;

    #[test]
    fn should_replace_a_previous_parameter_when_it_is_set_another_time() -> anyhow::Result<()> {

        let parameter_value = EthernetBridge { name: NetworkInterfaceName::try_from("br-opendut")? };

        let mut testee = PeerConfiguration::default();
        testee.set(parameter_value.clone(), ParameterTarget::Present);


        testee.set(parameter_value.clone(), ParameterTarget::Present);
        assert_eq!(testee.ethernet_bridges.len(), 1);

        testee.set(parameter_value.clone(), ParameterTarget::Absent);
        assert_eq!(testee.ethernet_bridges.len(), 1);
        assert_eq!(testee.ethernet_bridges[0].target, ParameterTarget::Absent);

        Ok(())
    }


    #[test]
    fn should_update_the_value_of_a_parameter() -> anyhow::Result<()> {

        let parameter_value = ExecutorDescriptor {
            id: ExecutorId::random(),
            kind: ExecutorKind::Executable,
            results_url: Some(ResultsUrl::try_from("https://example.com")?),
        };

        let mut testee = PeerConfiguration::default();
        testee.set(parameter_value.clone(), ParameterTarget::Present);


        let expected = None;
        let parameter_value = ExecutorDescriptor {
            results_url: expected.clone(),
            ..parameter_value
        };

        testee.set(parameter_value, ParameterTarget::Present);
        assert_eq!(testee.executors.len(), 1);
        assert_eq!(testee.executors[0].value.results_url, expected);

        Ok(())
    }
}
