use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;
use tracing::debug;

use crate::service::can_manager::CanManagerRef;
use crate::service::network_interface;
use crate::service::network_interface::gre;
use crate::service::network_interface::manager::NetworkInterfaceManagerRef;
use opendut_types::cluster::{ClusterAssignment, PeerClusterAssignment};
use opendut_types::peer::configuration::{parameter, Parameter, ParameterTarget};
use opendut_types::peer::PeerId;
use opendut_types::util::net::{NetworkInterfaceConfiguration, NetworkInterfaceName};
use parameter::DeviceInterface;

#[tracing::instrument(skip_all, level="trace")]
pub async fn setup_ethernet_gre_interfaces(
    cluster_assignment: &ClusterAssignment,
    self_id: PeerId,
    bridge_name: &NetworkInterfaceName,
    network_interface_manager: NetworkInterfaceManagerRef,
) -> Result<(), Error> {
    debug!("Setting up Ethernet GRE interfaces.");

    let local_peer_assignment = cluster_assignment.assignments.iter().find(|assignment| {
        assignment.peer_id == self_id
    }).ok_or(Error::LocalPeerAssignmentNotFound { self_id })?;

    let local_ip = local_peer_assignment.vpn_address;
    let local_ip = require_ipv4_for_gre(local_ip)?;

    let remote_ips = determine_remote_ips(cluster_assignment, self_id)?;
    let remote_ips = remote_ips.into_iter()
        .map(require_ipv4_for_gre)
        .collect::<Result<Vec<_>, _>>()?;

    gre::setup_interfaces(
        &local_ip,
        &remote_ips,
        bridge_name,
        Arc::clone(&network_interface_manager),
    ).await
    .map_err(Error::GreInterfaceSetupFailed)?;

    Ok(())
}

#[tracing::instrument(skip_all, level="trace")]
pub async fn join_ethernet_interfaces_to_bridge(
    device_interfaces: &[Parameter<DeviceInterface>],
    bridge_name: &NetworkInterfaceName,
    network_interface_manager: NetworkInterfaceManagerRef,
) -> Result<(), Error> {
    debug!("Joining Ethernet interfaces to bridge '{bridge_name}'.");

    let ethernet_interfaces = filter_ethernet_interfaces(device_interfaces.to_owned())?;

    let ethernet_interfaces: Vec<DeviceInterface> = ethernet_interfaces.into_iter()
        .filter(|parameter| parameter.target == ParameterTarget::Present)
        .map(|parameter| parameter.value)
        .collect();

    join_device_interfaces_to_bridge(&ethernet_interfaces, bridge_name, Arc::clone(&network_interface_manager)).await
        .map_err(Error::JoinDeviceInterfaceToBridgeFailed)?;

    Ok(())
}

#[tracing::instrument(skip_all, level="trace")]
pub async fn setup_can_interfaces(
    cluster_assignment: &ClusterAssignment,
    self_id: PeerId,
    device_interfaces: &[Parameter<DeviceInterface>],
    can_manager: CanManagerRef
) -> Result<(), Error> {
    let can_interfaces = filter_can_interfaces(device_interfaces.to_owned())?;

    let can_interfaces = can_interfaces.into_iter()
        .filter(|parameter| parameter.target == ParameterTarget::Present)
        .map(|parameter| parameter.value.descriptor)
        .collect::<Vec<_>>();

    if let sudo::RunningAs::User = sudo::check() {
        if can_interfaces.is_empty() {
            //Since we don't have the correct permissions to run the CAN setup code,
            //no previous CAN interfaces exist which we might need to clean up,
            //so we can safely skip this code, which allows us to run without root,
            //when CAN is not used.
            debug!("No CAN interfaces to set up. Skipping.");
            return Ok(());
        } else {
            panic!("CARL requested to setup CAN interfaces, but EDGAR is not running with root permissions, which is currently required."); //TODO report problem to CARL
        }
    }

    debug!("Setting up CAN interfaces.");

    let can_bridge_name = crate::common::default_can_bridge_name();
    can_manager.setup_local_routing(
        &can_bridge_name,
        can_interfaces,
    ).await
    .map_err(Error::LocalCanRoutingSetupFailed)?;

    let local_peer_assignment = cluster_assignment.assignments.iter().find(|assignment| {
        assignment.peer_id == self_id
    }).ok_or(Error::LocalPeerAssignmentNotFound { self_id })?;

    let is_leader = cluster_assignment.leader == self_id;

    let server_port = local_peer_assignment.can_server_port;

    if is_leader {

        let remote_assignments = determine_remote_assignments(cluster_assignment, self_id)?;
        can_manager.setup_remote_routing_server(
            &can_bridge_name, 
            &remote_assignments
        ).await
        .map_err(Error::RemoteCanRoutingSetupFailed)?;

    } else {        

        let leader_assignment = determine_leader_assignment(cluster_assignment)?;
        can_manager.setup_remote_routing_client(
            &can_bridge_name, 
            &leader_assignment.vpn_address,
            &server_port
        ).await
        .map_err(Error::RemoteCanRoutingSetupFailed)?;
    }

    Ok(())
}

fn determine_remote_ips(cluster_assignment: &ClusterAssignment, self_id: PeerId) -> Result<Vec<IpAddr>, Error> {
    let remote_assignments = determine_remote_assignments(cluster_assignment, self_id);
    let remote_ips = remote_assignments?.iter().map(|remote_assignment| remote_assignment.vpn_address).collect();

    Ok(remote_ips)
}

fn determine_remote_assignments(cluster_assignment: &ClusterAssignment, self_id: PeerId) -> Result<Vec<PeerClusterAssignment>, Error> {
    let is_leader = cluster_assignment.leader == self_id;

    let remote_peer_cluster_assignments = if is_leader {
        cluster_assignment.assignments.iter()
            .filter(|assignment| assignment.peer_id != self_id).cloned()
            .collect::<Vec<PeerClusterAssignment>>()
    }
    else {
        let leader_ip = determine_leader_assignment(cluster_assignment)?;

        vec![leader_ip.clone()]
    };

    Ok(remote_peer_cluster_assignments)
}

fn determine_leader_assignment(cluster_assignment: &ClusterAssignment) -> Result<&PeerClusterAssignment, Error>{
    let leader_assignment = cluster_assignment.assignments
        .iter().find(|peer_assignment| 
            peer_assignment.peer_id == cluster_assignment.leader
            ).ok_or(Error::LeaderNotDeterminable)?;

    Ok(leader_assignment)
}

fn require_ipv4_for_gre(ip_address: IpAddr) -> Result<Ipv4Addr, Error> {
    match ip_address {
        IpAddr::V4(ip_address) => Ok(ip_address),
        IpAddr::V6(_) => Err(Error::Ipv6NotSupported),
    }
}

fn filter_ethernet_interfaces(
    device_interfaces: Vec<Parameter<DeviceInterface>>
) -> Result<Vec<Parameter<DeviceInterface>>, Error> {

    let own_ethernet_interfaces = device_interfaces.iter()
        .filter(|interface| interface.value.descriptor.configuration == NetworkInterfaceConfiguration::Ethernet)
        .cloned()
        .collect::<Vec<_>>();

    Ok(own_ethernet_interfaces)
}

fn filter_can_interfaces(
    device_interfaces: Vec<Parameter<DeviceInterface>>
) -> Result<Vec<Parameter<DeviceInterface>>, Error> {

    let own_can_interfaces: Vec<_> = device_interfaces.iter()
        .filter(|interface| matches!(interface.value.descriptor.configuration, NetworkInterfaceConfiguration::Can { .. }))
        .cloned()
        .collect::<Vec<_>>();

    Ok(own_can_interfaces)
}

async fn join_device_interfaces_to_bridge(
    device_interfaces: &Vec<DeviceInterface>,
    bridge_name: &NetworkInterfaceName,
    network_interface_manager: NetworkInterfaceManagerRef
) -> Result<(), network_interface::manager::Error> {
    let bridge = network_interface_manager.try_find_interface(bridge_name).await?;

    for interface in device_interfaces {
        let interface = network_interface_manager.try_find_interface(&interface.descriptor.name).await?;
        network_interface_manager.join_interface_to_bridge(&interface, &bridge).await?;
        debug!("Joined device interface {interface} to bridge {bridge}.");
    }
    Ok(())
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("(Re-)Creating the bridge failed: {0}")]
    BridgeRecreationFailed(network_interface::manager::Error),
    #[error("Could not find PeerAssignment for this peer (<{self_id}>) in the ClusterAssignment.")]
    LocalPeerAssignmentNotFound { self_id: PeerId },
    #[error("Could not determine leader from ClusterAssignment.")]
    LeaderNotDeterminable,
    #[error("IPv6 isn't yet supported for GRE interfaces.")]
    Ipv6NotSupported,
    #[error("GRE interface setup failed: {0}")]
    GreInterfaceSetupFailed(gre::Error),
    #[error("Local CAN routing setup failed: {0}")]
    LocalCanRoutingSetupFailed(crate::service::can_manager::Error),
    #[error("Remote CAN routing setup failed: {0}")]
    RemoteCanRoutingSetupFailed(crate::service::can_manager::Error),
    #[error("Joining device interface to bridge failed: {0}")]
    JoinDeviceInterfaceToBridgeFailed(network_interface::manager::Error),
}
