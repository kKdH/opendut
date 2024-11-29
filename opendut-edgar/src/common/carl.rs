use std::net::IpAddr;
use std::ops::Not;
use std::time::Duration;

use anyhow::bail;
use config::Config;
use tracing::{debug, info, warn};

use opendut_carl_api::carl::{broker, CaCertInfo, CarlClient};
use opendut_carl_api::proto::services::peer_messaging_broker;
use opendut_types::peer::PeerId;
use opendut_util::project;

pub async fn connect(settings: &Config) -> anyhow::Result<CarlClient> {
    opendut_util::crypto::install_default_provider();

    debug!("Connecting to CARL...");

    let host = settings.get_string("network.carl.host")?;
    let port = u16::try_from(settings.get_int("network.carl.port")?)?;
    let ca_cert_path = CaCertInfo::Path(
        project::make_path_absolute(settings.get_string("network.tls.ca")?)?
    );
    let domain_name_override = settings.get_string("network.tls.domain.name.override")?;
    let domain_name_override = domain_name_override.is_empty().not().then_some(domain_name_override);

    let retries = settings.get_int("network.connect.retries")?;
    let interval = Duration::from_millis(u64::try_from(settings.get_int("network.connect.interval.ms")?)?);

    for retries_left in (0..retries).rev() {
        match CarlClient::create(&host, port, &ca_cert_path, &domain_name_override, settings).await {
            Ok(carl) => {
                info!("Connected to CARL.");
                return Ok(carl);
            }
            Err(cause) => {
                if retries_left > 0 {
                    warn!("Could not connect to CARL at '{host}:{port}'. Retrying in {interval} ms. {retries_left} retries left.\n  {cause}", interval=interval.as_millis());
                    tokio::time::sleep(interval).await;
                }
            }
        }
    }
    bail!("Failed to connect to CARL after {retries}*{interval} ms.", interval=interval.as_millis());
}

pub async fn open_stream(
    self_id: PeerId,
    remote_address: &IpAddr,
    carl: &mut CarlClient,
) -> anyhow::Result<(broker::Downstream, broker::Upstream), broker::error::OpenStream> {
    debug!("Opening peer messaging stream...");
    let (rx_inbound, tx_outbound) = carl.broker.open_stream(self_id, remote_address).await?;

    tx_outbound.send(peer_messaging_broker::Upstream {
        message: Some(peer_messaging_broker::upstream::Message::Ping(peer_messaging_broker::Ping {})),
        context: None
    }).await
        .map_err(|cause| broker::error::OpenStream { message: format!("Error while sending initial ping: {cause}") })?;

    info!("Peer messaging stream opened.");
    Ok((rx_inbound, tx_outbound))
}
