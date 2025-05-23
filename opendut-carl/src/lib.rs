use pem::Pem;
use std::net::SocketAddr;
use std::str::FromStr;
use anyhow::Context;
use tonic::service::Routes;
use tonic_async_interceptor::async_interceptor;
use tower::make::Shared;
use tower::ServiceExt as _;
use tracing::{debug, info};
use uuid::Uuid;

use opendut_auth::confidential::pem::PemFromConfig;
use opendut_auth::registration::client::RegistrationClient;
use opendut_auth::registration::resources::ResourceHomeUrl;
use opendut_util::settings::LoadedConfig;
use opendut_util::telemetry::logging::LoggingConfig;
use opendut_util::telemetry::opentelemetry_types;
use opendut_util::telemetry::opentelemetry_types::Opentelemetry;
use opendut_util::{project, telemetry};
use auth::in_memory_cache::CustomInMemoryCache;

use crate::auth::grpc_auth_layer::GrpcAuthenticationLayer;
use crate::auth::json_web_key::JwkCacheValue;
use crate::http::state::CarlInstallDirectory;
use crate::resource::manager::ResourceManager;
use crate::resource::storage::PersistenceOptions;
use crate::startup::tls::TlsConfig;

shadow_rs::shadow!(app_info);

mod auth;
mod http;
mod manager;
pub mod resource;
pub mod settings;
mod startup;

#[tracing::instrument]
pub async fn create_with_telemetry(settings_override: config::Config) -> anyhow::Result<()> {
    opendut_util::crypto::install_default_provider();

    let settings = settings::load_with_overrides(settings_override)?;

    let logging_config = LoggingConfig::load(&settings.config)?;
    let service_metadata = opentelemetry_types::ServiceMetadata {
        instance_id: format!("carl-{}", Uuid::new_v4()),
        version: app_info::PKG_VERSION.to_owned(),
    };
    let opentelemetry = Opentelemetry::load(&settings.config, service_metadata).await?;

    let mut shutdown = telemetry::initialize_with_config(logging_config, opentelemetry).await?;

    create(settings).await?;

    shutdown.shutdown();

    Ok(())
}

pub async fn create(settings: LoadedConfig) -> anyhow::Result<()> {
    opendut_util::crypto::install_default_provider();

    info!("Started with configuration: {settings:?}");
    let settings = settings.config;

    let resource_manager = {
        let persistence_options = PersistenceOptions::load(&settings)?;

        let resource_manager = ResourceManager::create(&persistence_options).await
            .context("Creating ResourceManager failed")?;

        #[cfg(feature="postgres")]
        if let Some(value) = std::env::var_os("OPENDUT_CARL_POSTGRES_MIGRATION") {
            tracing::info!("Found environment variable `OPENDUT_CARL_POSTGRES_MIGRATION`. Starting migration.");
            assert!(!value.is_empty());

            startup::postgres_migration::load_data_from_postgres_into_key_value_store(resource_manager.clone(), &persistence_options).await
                .expect("Migration from Postgres to Key-Value Store should complete without errors");

            tracing::info!("Migration complete. Exiting.");
            std::process::exit(0);
        }

        resource_manager
    };

    let carl_url = ResourceHomeUrl::try_from(&settings)?;

    let ca_certificate = Pem::from_config_path("network.tls.ca", &settings).await?;

    let oidc_registration_client = RegistrationClient::from_settings(&settings).await
        .expect("Failed to load oidc registration client!");

    let grpc_facades = startup::grpc::GrpcFacades::create(
        resource_manager,
        &carl_url,
        ca_certificate.clone(),
        oidc_registration_client.clone(),
        &settings
    ).await?;

    let http = {
        let carl_installation_directory = CarlInstallDirectory::determine()
            .expect("Could not determine installation directory.");

        if !project::is_running_in_development() {
            startup::cleo::create_cleo_install_script(
                ca_certificate,
                &carl_installation_directory.path,
                startup::cleo::script::CleoScript::from_setting(&settings)
                    .expect("Could not read settings to extract CLEO script information.")
            ).expect("Could not create CLEO install script.");
        }

        let http_state = startup::http::create_http_state(&carl_url, carl_installation_directory, &settings)?;

        startup::http::create_http_service(&settings)?
            .with_state(http_state)
    };

    let grpc = {
        let grpc_auth_layer = match oidc_registration_client {
            None => GrpcAuthenticationLayer::AuthDisabled,
            Some(oidc_client_ref) => {
                let jwk_cache: CustomInMemoryCache<String, JwkCacheValue> = CustomInMemoryCache::new();

                GrpcAuthenticationLayer::GrpcAuthLayerEnabled {
                    issuer_url: oidc_client_ref.inner.config.issuer_url.clone(),
                    issuer_remote_url: oidc_client_ref.config.issuer_remote_url.clone(),
                    cache: jwk_cache,
                }
            }
        };

        let mut routes_builder = Routes::builder();

        routes_builder
            .add_service(grpc_facades.cluster_manager_facade.into_grpc_service())
            .add_service(grpc_facades.metadata_provider_facade.into_grpc_service())
            .add_service(grpc_facades.peer_manager_facade.into_grpc_service())
            .add_service(grpc_facades.peer_messaging_broker_facade.into_grpc_service())
            .add_service(grpc_facades.observer_messaging_broker_facade.into_grpc_service());

        routes_builder
            .routes()
            .into_axum_router()
            .layer(async_interceptor(move |request| {
                Clone::clone(&grpc_auth_layer).auth_interceptor(request)
            }))
    };

    let http_grpc = tower::steer::Steer::new(vec![http, grpc], |request: &axum::extract::Request, _services: &[_]| {
        let is_grpc = request
            .headers()
            .get(axum::http::header::CONTENT_TYPE)
            .map(|content_type|
                content_type
                    .as_bytes()
                    .starts_with(b"application/grpc")
            )
            .unwrap_or(false);

        if is_grpc { 1 } else { 0 }
    })
    .map_request(|request: ::http::Request<hyper::body::Incoming>| -> ::http::Request<axum::body::Body> {
        request.map(axum::body::Body::new)
    });


    let address: SocketAddr = {
        let host = settings.get_string("network.bind.host")?;
        let port = settings.get_int("network.bind.port")?;
        SocketAddr::from_str(&format!("{host}:{port}"))?
    };

    match TlsConfig::load(&settings).await? {
        TlsConfig::Enabled(tls_config) => {
            info!("Server listening at {address}...");
            axum_server::bind_rustls(address, tls_config)
                .serve(Shared::new(http_grpc))
                .await?;
        }
        TlsConfig::Disabled => {
            // Disable TLS in case a load balancer with TLS termination is present
            debug!("TLS is disabled in the configuration.");

            info!("Server listening at {address}...");
            axum_server::bind(address)
                .serve(Shared::new(http_grpc))
                .await?;
        }
    }

    Ok(())
}
