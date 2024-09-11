use std::any::Any;
use std::fmt::Debug;
use std::ops::Not;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use config::Config;
use opentelemetry::propagation::text_map_propagator::TextMapPropagator;
use opentelemetry_sdk::propagation::TraceContextPropagator;
use tokio::sync::mpsc::Sender;
use tokio::time::sleep;
use tonic::Code;
use tracing::{debug, error, info, trace, warn, Span};
use tracing_opentelemetry::OpenTelemetrySpanExt;

use opendut_carl_api::proto::services::peer_messaging_broker;
use opendut_carl_api::proto::services::peer_messaging_broker::downstream::Message;
use opendut_carl_api::proto::services::peer_messaging_broker::{ApplyPeerConfiguration, TracingContext};
use opendut_types::cluster::{ClusterAssignment, PeerClusterAssignment};
use opendut_types::peer::configuration::{OldPeerConfiguration, ParameterTarget, PeerConfiguration};
use opendut_types::peer::PeerId;
use opendut_types::util::net::NetworkInterfaceName;
use opendut_util::project;
use opendut_util::settings::LoadedConfig;
use opendut_util::telemetry;
use opendut_util::telemetry::logging::LoggingConfig;
use opendut_util::telemetry::opentelemetry_types::Opentelemetry;

use crate::common::task::{runner, Task};
use crate::common::{carl, settings};
use crate::service::can_manager::{CanManager, CanManagerRef};
use crate::service::network_interface::manager::{NetworkInterfaceManager, NetworkInterfaceManagerRef};
use crate::service::network_metrics;
use crate::service::test_execution::executor_manager::{ExecutorManager, ExecutorManagerRef};
use crate::service::{cluster_assignment, tasks, vpn};
use crate::setup::RunMode;

const BANNER: &str = r"
                         _____     _______
                        |  __ \   |__   __|
   ___  _ __   ___ _ __ | |  | |_   _| |
  / _ \| '_ \ / _ \ '_ \| |  | | | | | |
 | (_) | |_) |  __/ | | | |__| | |_| | |
  \___/| .__/ \___|_| |_|_____/ \__,_|_|
       | |  ______ _____   _____          _____
       |_| |  ____|  __ \ / ____|   /\   |  __ \
           | |__  | |  | | |  __   /  \  | |__) |
           |  __| | |  | | | |_ | / /\ \ |  _  /
           | |____| |__| | |__| |/ ____ \| | \ \
           |______|_____/ \_____/_/    \_\_|  \_\";

pub async fn launch(id_override: Option<PeerId>) -> anyhow::Result<()> {
    println!("{}", crate::app_info::formatted_with_banner(BANNER));

    let settings_override = Config::builder()
        .set_override_option(settings::key::peer::id, id_override.map(|id| id.to_string()))?
        .build()?;

    create_with_telemetry(settings_override).await
}

pub async fn create_with_telemetry(settings_override: config::Config) -> anyhow::Result<()> {
    let settings = settings::load_with_overrides(settings_override)?;

    let self_id = settings.config.get::<PeerId>(settings::key::peer::id)
        .context("Failed to read ID from configuration.\n\nRun `edgar setup` before launching the service.")?;

    let service_instance_id = self_id.to_string();

    let logging_config = LoggingConfig::load(&settings.config)?;
    let opentelemetry = Opentelemetry::load(&settings.config, service_instance_id).await?;
    
    let mut shutdown = telemetry::initialize_with_config(logging_config, opentelemetry.clone()).await?;

    if let (Opentelemetry::Enabled { cpu_collection_interval_ms, .. },
        Some(meter_providers, ..)) = (opentelemetry, &shutdown.meter_providers) {
        telemetry::metrics::initialize_metrics_collection(cpu_collection_interval_ms, meter_providers);
    }

    create(self_id, settings).await?;

    shutdown.shutdown();

    Ok(())
}

pub async fn create(self_id: PeerId, settings: LoadedConfig) -> anyhow::Result<()> {

    info!("Started with ID <{self_id}> and configuration: {settings:?}");

    let network_interface_manager: NetworkInterfaceManagerRef = NetworkInterfaceManager::create()?;
    let can_manager: CanManagerRef = CanManager::create(Arc::clone(&network_interface_manager));
    let executor_manager: ExecutorManagerRef = ExecutorManager::create();

    let network_interface_management_enabled = settings.config.get::<bool>("network.interface.management.enabled")?;

    let remote_address = vpn::retrieve_remote_host(&settings).await?;
    
    let ping_interval = Duration::from_millis(settings.config.get::<u64>("opentelemetry.metrics.cluster.ping.interval.ms")?);
    let target_bandwidth_kbit_per_second = settings.config.get::<u64>("opentelemetry.metrics.cluster.target.bandwidth.kilobit.per.second")?;
    let rperf_backoff_max_elapsed_time = Duration::from_millis(settings.config.get::<u64>("opentelemetry.metrics.cluster.rperf.backoff.max.elapsed.time.ms")?);

    let setup_cluster_info = SetupClusterInfo {
        self_id,
        network_interface_management_enabled,
        network_interface_manager,
        can_manager,
        executor_manager,
        ping_interval,
        target_bandwidth_kbit_per_second,
        rperf_backoff_max_elapsed_time,
    };

    let timeout_duration = Duration::from_millis(settings.config.get::<u64>("carl.disconnect.timeout.ms")?);

    let mut carl = carl::connect(&settings.config).await?;

    let (mut rx_inbound, tx_outbound) = carl::open_stream(self_id, &remote_address, &mut carl).await?;

    loop {
        let received = tokio::time::timeout(
            timeout_duration,
            rx_inbound.message()
        ).await;

        match received {
            Ok(received) => match received {
                Ok(Some(message)) => {
                    handle_stream_message(
                        message,
                        &setup_cluster_info,
                        &tx_outbound,
                    ).await?
                }
                Err(status) => {
                    warn!("CARL sent a gRPC error status: {status}");

                    match status.code() {
                        Code::Ok | Code::AlreadyExists => continue, //ignore

                        Code::DeadlineExceeded | Code::Unavailable => { //ignore, but delay reading the stream again, as this may result in rapid triggering of errors otherwise
                            tokio::time::sleep(Duration::from_secs(1)).await;
                            continue
                        }

                        Code::Aborted
                        | Code::Cancelled
                        | Code::DataLoss
                        | Code::FailedPrecondition
                        | Code::Internal
                        | Code::InvalidArgument
                        | Code::NotFound
                        | Code::OutOfRange
                        | Code::PermissionDenied
                        | Code::ResourceExhausted
                        | Code::Unimplemented
                        | Code::Unauthenticated
                        | Code::Unknown
                        => panic!("Received potentially bad gRPC error: {status}"), //In production, SystemD will restart EDGAR with a delay. A crash is mainly more visible.
                    }
                }
                Ok(None) => {
                    info!("CARL disconnected!");
                    break;
                }
            }
            Err(_) => {
                error!("No message from CARL within {} ms.", timeout_duration.as_millis());
                break;
            }
        }
    }

    Ok(())
}


async fn handle_stream_message(
    message: peer_messaging_broker::Downstream,
    setup_cluster_info: &SetupClusterInfo,
    tx_outbound: &Sender<peer_messaging_broker::Upstream>,
) -> anyhow::Result<()> {

    if let peer_messaging_broker::Downstream { message: Some(message), context } = message {
        if matches!(message, Message::Pong(_)).not() {
            trace!("Received message: {:?}", message);
        }

        match message {
            Message::Pong(_) => {
                sleep(Duration::from_secs(5)).await;
                let message = peer_messaging_broker::Upstream {
                    message: Some(peer_messaging_broker::upstream::Message::Ping(peer_messaging_broker::Ping {})),
                    context: None
                };
                let _ignore_error =
                    tx_outbound.send(message).await
                        .inspect_err(|cause| debug!("Failed to send ping to CARL: {cause}"));
            }
            Message::ApplyPeerConfiguration(message) => { apply_peer_configuration(message, context, setup_cluster_info).await? }
        }
    } else {
        ignore(message)
    }

    Ok(())
}

#[tracing::instrument(skip_all, level="trace")]
async fn apply_peer_configuration(message: ApplyPeerConfiguration, context: Option<TracingContext>, setup_cluster_info: &SetupClusterInfo) -> anyhow::Result<()> {

    match message.clone() {
        ApplyPeerConfiguration {
            old_configuration: Some(old_configuration),
            configuration: Some(configuration),
        } => {

            let span = Span::current();
            set_parent_context(&span, context);
            let _span = span.enter();

            info!("Received OldPeerConfiguration: {old_configuration:?}");
            info!("Received PeerConfiguration: {configuration:?}");
            match OldPeerConfiguration::try_from(old_configuration) {
                Err(error) => error!("Illegal OldPeerConfiguration: {error}"),
                Ok(old_configuration) => {
                    match PeerConfiguration::try_from(configuration) {
                        Err(error) => error!("Illegal PeerConfiguration: {error}"),
                        Ok(configuration) => {
                            {
                                let mut tasks: Vec<Box<dyn Task>> = vec![];

                                if setup_cluster_info.network_interface_management_enabled {
                                    for parameter in configuration.ethernet_bridges.iter().cloned() {
                                        tasks.push(Box::new(tasks::create_ethernet_bridge::CreateEthernetBridge {
                                            parameter,
                                            network_interface_manager: Arc::clone(&setup_cluster_info.network_interface_manager),
                                        }));
                                    }
                                }

                                let no_confirm = true;
                                runner::run(RunMode::Service, no_confirm, &tasks).await?;
                            }

                            {
                                let maybe_bridge = configuration.ethernet_bridges.iter()
                                    .find(|bridge| bridge.target == ParameterTarget::Present); //we currently expect only one bridge to be Present (for one cluster)

                                match maybe_bridge {
                                    Some(bridge) => {
                                        let _ = setup_cluster(
                                            &old_configuration.cluster_assignment,
                                            setup_cluster_info,
                                            &bridge.value.name,
                                        ).await;
                                    }
                                    None => {
                                        debug!("PeerConfiguration contained no info for bridge. Not setting up cluster.");
                                    }
                                }
                            }

                            let mut executor_manager = setup_cluster_info.executor_manager.lock().unwrap();
                            executor_manager.terminate_executors();
                            executor_manager.create_new_executors(configuration.executors);

                            setup_cluster_metrics(
                                &old_configuration.cluster_assignment,
                                setup_cluster_info,
                            )?;
                        }
                    }
                }
            };
        }
        _ => ignore(message),
    }
    Ok(())
}

struct SetupClusterInfo {
    self_id: PeerId,
    network_interface_management_enabled: bool,
    network_interface_manager: NetworkInterfaceManagerRef,
    can_manager: CanManagerRef,
    executor_manager: ExecutorManagerRef,
    ping_interval: Duration,
    target_bandwidth_kbit_per_second: u64,
    rperf_backoff_max_elapsed_time: Duration,
}
#[tracing::instrument(skip_all)]
async fn setup_cluster( //TODO make idempotent
    cluster_assignment: &Option<ClusterAssignment>,
    info: &SetupClusterInfo,
    bridge_name: &NetworkInterfaceName,
) -> anyhow::Result<()> {

    match cluster_assignment {
        Some(cluster_assignment) => {
            trace!("Received ClusterAssignment: {cluster_assignment:?}");
            info!("Was assigned to cluster <{}>", cluster_assignment.id);

            if info.network_interface_management_enabled {
                cluster_assignment::setup_ethernet_gre_interfaces(
                    cluster_assignment,
                    info.self_id,
                    bridge_name,
                    Arc::clone(&info.network_interface_manager),
                ).await
                .inspect_err(|error| error!("Failed to configure Ethernet GRE interfaces: {error}"))?;

                cluster_assignment::join_ethernet_interfaces_to_bridge(
                    cluster_assignment,
                    info.self_id,
                    bridge_name,
                    Arc::clone(&info.network_interface_manager),
                ).await
                .inspect_err(|error| error!("Failed to join Ethernet interfaces to bridge: {error}"))?;

                cluster_assignment::setup_can_interfaces(
                    cluster_assignment,
                    info.self_id,
                    Arc::clone(&info.can_manager),
                ).await
                .inspect_err(|error| error!("Failed to configure CAN interfaces: {error}"))?;
            } else {
                debug!("Skipping changes to network interfaces after receiving ClusterAssignment, as this is disabled via configuration.");
            }
        }
        None => {
            debug!("No ClusterAssignment in peer configuration.");
            //TODO teardown cluster, if configuration changed
        }
    }
    Ok(())
}

#[tracing::instrument(skip_all)]
fn setup_cluster_metrics( //TODO make idempotent
    cluster_assignment: &Option<ClusterAssignment>,
    setup_cluster_info: &SetupClusterInfo,
) -> anyhow::Result<()> {
    debug!("Setting up cluster metrics.");

    match cluster_assignment {
        None => {}
        Some(cluster_assignment) => {
            let local_peer_assignment = cluster_assignment.assignments.iter().find(|assignment| {
                assignment.peer_id == setup_cluster_info.self_id
            }).ok_or(cluster_assignment::Error::LocalPeerAssignmentNotFound { self_id: setup_cluster_info.self_id })?;

            let local_ip = local_peer_assignment.vpn_address;

            let peers: Vec<PeerClusterAssignment> = cluster_assignment.assignments.iter()
                .filter(|peer_cluster_assignment | peer_cluster_assignment.vpn_address != local_ip)
                .cloned().collect();

            let ping_interval_ms = setup_cluster_info.ping_interval;
            let target_bandwidth_kbit_per_second = setup_cluster_info.target_bandwidth_kbit_per_second;
            let rperf_backoff_max_elapsed_time_ms = setup_cluster_info.rperf_backoff_max_elapsed_time;

            tokio::spawn(async move {
                network_metrics::ping::cluster_ping(peers.clone(), ping_interval_ms).await;

                if project::is_running_in_development().not() {
                    let _ = network_metrics::rperf::server::exponential_backoff_launch_rperf_server(rperf_backoff_max_elapsed_time_ms).await //ignore errors during startup of rperf server, as we do not want to crash EDGAR for this
                        .inspect_err(|cause| error!("Failed to start rperf server:\n  {cause}"));
                    network_metrics::rperf::client::launch_rperf_clients(peers, target_bandwidth_kbit_per_second, rperf_backoff_max_elapsed_time_ms).await;
                }
            });
        }
    }
    Ok(())
}

fn set_parent_context(span: &Span, context: Option<TracingContext>) {
    if let Some(context) = context {
        let propagator = TraceContextPropagator::new();
        let parent_context = propagator.extract(&context.values);
        span.set_parent(parent_context);
    }
}
fn ignore(message: impl Any + Debug) {
    warn!("Ignoring illegal message: {message:?}");
}
