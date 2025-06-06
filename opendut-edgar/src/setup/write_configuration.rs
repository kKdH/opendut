use crate::fs;
use std::ops::Not;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use anyhow::{anyhow, Context};
use tracing::{debug, error, info};
use url::Url;

use opendut_types::peer::PeerId;
use opendut_types::util::net::AuthConfig;

use crate::common::settings;
use crate::setup::constants;
use crate::setup::util::create_file_and_ensure_it_can_only_be_read_or_modified_by_owner;

pub struct WriteConfiguration {
    config_file_to_write_to: PathBuf,
    config_merge_suggestion_file: PathBuf,
    config_override: ConfigOverride,
    no_confirm: bool,
    user_attended: bool,
}
pub struct ConfigOverride {
    pub peer_id: PeerId,
    pub carl_url: Url,
    pub auth_config: AuthConfig,
}

impl WriteConfiguration {
    pub async fn execute(&self) -> anyhow::Result<()> {
        let original_settings = self.load_current_settings()
            .unwrap_or_else(|| {
                debug!("Could not load settings from configuration file at '{}'. Continuing as if no previous configuration exists.", self.config_file_to_write_to.display());
                toml_edit::DocumentMut::new()
            });

        let new_settings_string = {
            let mut new_settings = Clone::clone(&original_settings);

            let peer_id = self.config_override.peer_id.to_string();
            let carl_host = self.config_override.carl_url.host_str().expect("Host name should be defined in CARL URL.");
            let carl_port = self.config_override.carl_url.port().unwrap_or(443);

            if new_settings.get("peer").is_none() {
                new_settings["peer"] = toml_edit::table();
            }
            new_settings["peer"]["id"] = toml_edit::value(peer_id);

            if new_settings.get("network").and_then(|network| network.get("carl")).is_none() {
                new_settings["network"] = toml_edit::table();
                new_settings["network"]["carl"] = toml_edit::table();
                new_settings["network"]["carl"].as_table_mut().unwrap().set_dotted(true);
            }
            new_settings["network"]["carl"]["host"] = toml_edit::value(carl_host);
            new_settings["network"]["carl"]["port"] = toml_edit::value(i64::from(carl_port));

            match &self.config_override.auth_config {
                AuthConfig::Disabled => {
                    if new_settings.get("network").and_then(|network| network.get("oidc")).is_none() {
                        new_settings["network"]["oidc"] = toml_edit::table();
                    }
                    new_settings["network"]["oidc"]["enabled"] = toml_edit::value(false);
                }
                AuthConfig::Enabled { client_id, client_secret, issuer_url, scopes } => {
                    let network_oidc_client_id = client_id.clone().value();
                    let network_oidc_client_secret = client_secret.clone().value();
                    let network_oidc_client_issuer_url: String = issuer_url.clone().into();
                    let network_oidc_client_scopes = scopes.clone().into_iter().map(|scope| scope.value()).collect::<Vec<_>>().join(",");

                    if new_settings.get("network").and_then(|network| network.get("oidc")).is_none() {
                        new_settings["network"]["oidc"] = toml_edit::table();
                    }
                    new_settings["network"]["oidc"]["enabled"] = toml_edit::value(true);

                    if new_settings.get("network")
                        .and_then(|network| network.get("oidc"))
                        .and_then(|network| network.get("client"))
                        .is_none() {

                        new_settings["network"]["oidc"]["client"] = toml_edit::table();
                        new_settings["network"]["oidc"]["client"]["issuer"] = toml_edit::table();
                        new_settings["network"]["oidc"]["client"]["issuer"].as_table_mut().unwrap().set_dotted(true);
                    }
                    new_settings["network"]["oidc"]["client"]["id"] = toml_edit::value(network_oidc_client_id);
                    new_settings["network"]["oidc"]["client"]["secret"] = toml_edit::value(network_oidc_client_secret);
                    new_settings["network"]["oidc"]["client"]["scopes"] = toml_edit::value(network_oidc_client_scopes);
                    new_settings["network"]["oidc"]["client"]["issuer"]["url"] = toml_edit::value(network_oidc_client_issuer_url);
                }
            };

            new_settings.to_string()
        };

        if original_settings.to_string() == new_settings_string {
            debug!("The configuration on disk already matches the overrides we wanted to apply.");
            return Ok(())
        }

        let target_file_empty =
            self.config_file_to_write_to.exists().not()
            || self.config_file_to_write_to.metadata()?.len() == 0;

        let should_overwrite = if target_file_empty || self.no_confirm {
            true
        } else if self.user_attended {
            crate::setup::user_confirmation_prompt("Settings file already exists, but contains mismatched configurations! Do you want to overwrite it?")?
        } else {
            false
        };

        if should_overwrite {
            write_settings(&self.config_file_to_write_to, &new_settings_string)
                .context("Error while writing new configuration file.")?;

            info!("Successfully wrote peer configuration to: {}", self.config_file_to_write_to.display());
            Ok(())
        } else {
            write_settings(&self.config_merge_suggestion_file, &new_settings_string)
                .context("Error while writing configuration merge suggestion file.")?;

            let message =
                String::from("Settings file already exists, but contains mismatched configurations!\n")
                    + &format!("A suggestion for a merged configuration has been generated at '{}'.\n", self.config_merge_suggestion_file.display())
                    + &format!("Please check, if the configuration matches your expectation and if so, replace the configuration file at '{}'.", self.config_file_to_write_to.display());
            Err(anyhow!(message))
        }
    }
}
impl WriteConfiguration {
    pub fn with_override(config_override: ConfigOverride, no_confirm: bool) -> Self {
        Self {
            config_file_to_write_to: settings::default_config_file_path(),
            config_merge_suggestion_file: constants::default_config_merge_suggestion_file_path(),
            config_override,
            no_confirm,
            user_attended: console::user_attended(),
        }
    }

    fn load_current_settings(&self) -> Option<toml_edit::DocumentMut> {

        if self.config_file_to_write_to.exists().not() {
            return None;
        }

        let current_settings = match fs::read_to_string(&self.config_file_to_write_to) {
            Ok(content) => content,
            Err(cause) => {
                error!("Failed to read existing configuration file at '{}'.\n  {cause}", self.config_file_to_write_to.display());
                return None;
            }
        };

        match toml_edit::DocumentMut::from_str(&current_settings) {
            Ok(current_settings) => Some(current_settings),
            Err(cause) => {
                error!("Failed to parse existing configuration as TOML.\n  {cause}");
                None
            }
        }
    }
}

fn write_settings(target: &Path, settings_string: &str) -> anyhow::Result<()> {
    let parent_dir = target
        .parent()
        .ok_or(anyhow!("Expected configuration file '{}' to have a parent directory.", target.display()))?;
    fs::create_dir_all(parent_dir)?;
    create_file_and_ensure_it_can_only_be_read_or_modified_by_owner(target)?;

    fs::write(target, settings_string)
        .context(format!("Error while writing to configuration file at '{}'.", target.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use assert_fs::fixture::ChildPath;
    use assert_fs::prelude::*;
    use assert_fs::TempDir;
    use indoc::indoc;
    use predicates::boolean::PredicateBooleanExt;
    use predicates::Predicate;
    use predicates::prelude::predicate;
    use uuid::uuid;
    use googletest::prelude::*;
    use opendut_types::util::net::{ClientId, ClientSecret, OAuthScope};

    use super::*;

    const HOST: &str = "example.com";
    const PORT: u16 = 1234;
    const CLIENT_ID: &str = "ClientId";
    const CLIENT_SECRET: &str = "ClientSecret";
    const OIDC_ENABLED: bool = true;
    const ISSUER_URL: &str = "https://test.com:1234/";
    const SCOPES: &str = "test";

    #[tokio::test]
    async fn should_write_a_fresh_configuration_with_auth_config_enabled() -> anyhow::Result<()> {
        let fixture = Fixture::new();
        let write_configuration = create_write_configuration(&fixture, AuthEnabled::Yes);

        assert!(predicate::path::missing().eval(&write_configuration.config_file_to_write_to));

        let path = write_configuration.config_file_to_write_to.clone();

        write_configuration.execute().await?;

        assert!(predicate::path::exists().eval(&path));
        let file_content = fs::read_to_string(&path)?;

        assert_that!(file_content, eq(indoc!(r#"
            [peer]
            id = "dc72f6d9-d700-455f-8c31-9f15438e7503"

            [network]
            carl.host = "example.com"
            carl.port = 1234

            [network.oidc]
            enabled = true

            [network.oidc.client]
            issuer.url = "https://test.com:1234/"
            id = "ClientId"
            secret = "ClientSecret"
            scopes = "test"
        "#)));

        Ok(())
    }

    #[tokio::test]
    async fn should_write_a_fresh_configuration_with_auth_config_disabled() -> anyhow::Result<()> {
        let fixture = Fixture::new();
        let write_configuration = create_write_configuration(&fixture, AuthEnabled::No);

        let path = write_configuration.config_file_to_write_to.clone();

        assert!(predicate::path::missing().eval(&path));

        write_configuration.execute().await?;

        assert!(predicate::path::exists().eval(&path));
        let file_content = fs::read_to_string(&path)?;

        assert_that!(file_content, eq(indoc!(r#"
            [peer]
            id = "dc72f6d9-d700-455f-8c31-9f15438e7503"

            [network]
            carl.host = "example.com"
            carl.port = 1234

            [network.oidc]
            enabled = false
        "#)));

        Ok(())
    }

    #[tokio::test]
    async fn should_provide_an_merge_suggestion_for_an_already_existing_configuration_but_should_not_delete_existing_unknown_keys() -> anyhow::Result<()> {
        let fixture = Fixture::new();
        let write_configuration = create_write_configuration(&fixture, AuthEnabled::Yes);

        let config_file = ChildPath::new(write_configuration.config_file_to_write_to.clone());
        let config_merge_suggestion_file = ChildPath::new(write_configuration.config_merge_suggestion_file.clone());

        config_file.write_str(indoc!(r#"
            [peer]
            id = "eef8997e-56a0-4d8d-978e-40d1f2e68db0"
            [peer.unknown]
            key = "value"
            [Hallo.Welt]
            key = "value"
        "#))?;

        let file_content = fs::read_to_string(&config_file)?;
        assert!(predicate::str::is_empty().not().eval(&file_content));
        assert!(predicate::path::missing().eval(&config_merge_suggestion_file));

        let result = write_configuration.execute().await;
        assert!(result.is_err());

        assert!(predicate::path::exists().eval(&config_merge_suggestion_file));
        let merge_suggestion = fs::read_to_string(config_merge_suggestion_file)?;
        assert!(predicate::str::contains(fixture.peer_id.to_string()).eval(&merge_suggestion));
        assert!(predicate::str::contains("[peer.unknown]".to_string()).eval(&merge_suggestion));
        assert!(predicate::str::contains("[Hallo.Welt]".to_string()).eval(&merge_suggestion));

        Ok(())
    }

    #[tokio::test]
    async fn should_provide_an_merge_suggestion_for_an_already_existing_configuration_with_auth_config_disabled() -> anyhow::Result<()> {
        let fixture = Fixture::new();
        let write_configuration = create_write_configuration(&fixture, AuthEnabled::No);

        let config_file = ChildPath::new(write_configuration.config_file_to_write_to.clone());
        let config_merge_suggestion_file = ChildPath::new(write_configuration.config_merge_suggestion_file.clone());

        config_file.write_str(&format!(indoc!(r#"
            [peer]
            id = "eef8997e-56a0-4d8d-978e-40d1f2e68db0"

            [network.oidc]
            enabled = {}

            [network.oidc.client]
            id = "{}"
            secret = "{}"
        "#), OIDC_ENABLED, CLIENT_ID, CLIENT_SECRET))?;

        let file_content = fs::read_to_string(&config_file)?;
        assert!(predicate::str::is_empty().not().eval(&file_content));
        assert!(predicate::path::missing().eval(&config_merge_suggestion_file));

        let result = write_configuration.execute().await;
        assert!(result.is_err());

        assert!(predicate::path::exists().eval(&config_merge_suggestion_file));
        let merge_suggestion = fs::read_to_string(config_merge_suggestion_file)?;
        assert!(predicate::str::contains(fixture.peer_id.to_string()).eval(&merge_suggestion));
        assert!(predicate::str::contains("enabled = false".to_string()).eval(&merge_suggestion));
        assert!(predicate::str::contains("secret = \"ClientSecret\"".to_string()).not().eval(&merge_suggestion));

        Ok(())
    }

    #[tokio::test]
    async fn should_not_provide_a_merge_suggestion_if_the_existing_config_matches() -> anyhow::Result<()> {
        let fixture = Fixture::new();
        let write_configuration = create_write_configuration(&fixture, AuthEnabled::Yes);

        let config_file = ChildPath::new(write_configuration.config_file_to_write_to.clone());
        let config_merge_suggestion_file = ChildPath::new(write_configuration.config_merge_suggestion_file.clone());

        config_file.write_str(&format!(indoc!(r#"
            [peer]
            id = "{}"

            [network.carl]
            host = "{}"
            port = {}

            [network.oidc]
            enabled = {}

            [network.oidc.client]
            issuer.url = "{}"
            id = "{}"
            secret = "{}"
            scopes = "{}"
        "#), fixture.peer_id, HOST, PORT, OIDC_ENABLED, ISSUER_URL, CLIENT_ID, CLIENT_SECRET, SCOPES))?;

        let file_content = fs::read_to_string(&config_file)?;
        assert!(predicate::str::is_empty().not().eval(&file_content));
        assert!(predicate::path::missing().eval(&config_merge_suggestion_file));

        let result = write_configuration.execute().await;
        assert!(result.is_ok());
        assert!(predicate::path::missing().eval(&config_merge_suggestion_file));

        Ok(())
    }

    fn create_write_configuration(
        fixture: &Fixture,
        auth_enabled: AuthEnabled,
    ) -> WriteConfiguration {

        let config_override = match auth_enabled {
            AuthEnabled::Yes => ConfigOverride {
                peer_id: fixture.peer_id,
                carl_url: Url::parse("https://example.com:1234").unwrap(),
                auth_config: AuthConfig::Enabled {
                    issuer_url: Url::parse("https://test.com:1234").unwrap(),
                    client_secret: ClientSecret::from(CLIENT_SECRET),
                    client_id: ClientId::from(CLIENT_ID),
                    scopes: vec![OAuthScope("test".to_string())],
                },
            },
            AuthEnabled::No => ConfigOverride {
                peer_id: fixture.peer_id,
                carl_url: Url::parse("https://example.com:1234").unwrap(),
                auth_config: AuthConfig::Disabled,
            },
        };

        WriteConfiguration {
            config_file_to_write_to: fixture.config_file_to_write_to.to_path_buf(),
            config_merge_suggestion_file: fixture.config_merge_suggestion_file.to_path_buf(),
            config_override,
            no_confirm: false,
            user_attended: false, // prevent hanging in unit tests
        }
    }
    enum AuthEnabled { Yes, No }

    struct Fixture {
        _temp_dir: TempDir,
        config_file_to_write_to: ChildPath,
        config_merge_suggestion_file: ChildPath,
        peer_id: PeerId,
    }
    impl Fixture {
        fn new() -> Self {
            let temp_dir = TempDir::new().unwrap();

            let config_file_to_write_to = temp_dir.child("edgar.toml");

            let config_merge_suggestion_file = temp_dir.child("edgar-merge-suggestion.toml");

            let peer_id = PeerId::from(uuid!("dc72f6d9-d700-455f-8c31-9f15438e7503"));

            Self {
                _temp_dir: temp_dir,
                config_file_to_write_to,
                config_merge_suggestion_file,
                peer_id,
            }
        }
    }
}
