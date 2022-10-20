use crate::{
    actions::{Action, ActionState},
    execute_command, set_env,
};

use glob::glob;
use serde::Serialize;
use tokio::process::Command;

use crate::actions::{ActionDescription, Actionable};

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct SetupDefaultProfile {
    channels: Vec<String>,
    action_state: ActionState,
}

impl SetupDefaultProfile {
    #[tracing::instrument(skip_all)]
    pub async fn plan(channels: Vec<String>) -> Result<Self, SetupDefaultProfileError> {
        Ok(Self {
            channels,
            action_state: ActionState::Uncompleted,
        })
    }
}

#[async_trait::async_trait]
impl Actionable for SetupDefaultProfile {
    type Error = SetupDefaultProfileError;

    fn describe_execute(&self) -> Vec<ActionDescription> {
        if self.action_state == ActionState::Completed {
            vec![]
        } else {
            vec![ActionDescription::new(
                "Setup the default Nix profile".to_string(),
                vec!["TODO".to_string()],
            )]
        }
    }

    #[tracing::instrument(skip_all, fields(
        channels = %self.channels.join(","),
    ))]
    async fn execute(&mut self) -> Result<(), Self::Error> {
        let Self {
            channels,
            action_state,
        } = self;
        if *action_state == ActionState::Completed {
            tracing::trace!("Already completed: Setting up default profile");
            return Ok(());
        }
        tracing::debug!("Setting up default profile");

        // Find an `nix` package
        let nix_pkg_glob = "/nix/store/*-nix-*";
        let mut found_nix_pkg = None;
        for entry in glob(nix_pkg_glob).map_err(Self::Error::GlobPatternError)? {
            match entry {
                Ok(path) => {
                    // TODO(@Hoverbear): Should probably ensure is unique
                    found_nix_pkg = Some(path);
                    break;
                },
                Err(_) => continue, /* Ignore it */
            };
        }
        let nix_pkg = if let Some(nix_pkg) = found_nix_pkg {
            nix_pkg
        } else {
            return Err(Self::Error::NoNssCacert); // TODO(@hoverbear): Fix this error
        };

        // Install `nix` itself into the store
        execute_command(
            Command::new(nix_pkg.join("bin/nix-env"))
                .arg("-i")
                .arg(&nix_pkg),
        )
        .await
        .map_err(SetupDefaultProfileError::Command)?;

        // Find an `nss-cacert` package, add it too.
        let nss_ca_cert_pkg_glob = "/nix/store/*-nss-cacert-*";
        let mut found_nss_ca_cert_pkg = None;
        for entry in glob(nss_ca_cert_pkg_glob).map_err(Self::Error::GlobPatternError)? {
            match entry {
                Ok(path) => {
                    // TODO(@Hoverbear): Should probably ensure is unique
                    found_nss_ca_cert_pkg = Some(path);
                    break;
                },
                Err(_) => continue, /* Ignore it */
            };
        }
        let nss_ca_cert_pkg = if let Some(nss_ca_cert_pkg) = found_nss_ca_cert_pkg {
            nss_ca_cert_pkg
        } else {
            return Err(Self::Error::NoNssCacert);
        };

        // Install `nss-cacert` into the store
        execute_command(
            Command::new(nix_pkg.join("bin/nix-env"))
                .arg("-i")
                .arg(&nss_ca_cert_pkg),
        )
        .await
        .map_err(SetupDefaultProfileError::Command)?;

        set_env(
            "NIX_SSL_CERT_FILE",
            "/nix/var/nix/profiles/default/etc/ssl/certs/ca-bundle.crt",
        );

        if !channels.is_empty() {
            let mut command = Command::new(nix_pkg.join("bin/nix-channel"));
            command.arg("--update");
            for channel in channels {
                command.arg(channel);
            }
            command.env(
                "NIX_SSL_CERT_FILE",
                "/nix/var/nix/profiles/default/etc/ssl/certs/ca-bundle.crt",
            );

            execute_command(&mut command)
                .await
                .map_err(SetupDefaultProfileError::Command)?;
        }

        tracing::trace!("Set up default profile");
        *action_state = ActionState::Completed;
        Ok(())
    }

    fn describe_revert(&self) -> Vec<ActionDescription> {
        if self.action_state == ActionState::Uncompleted {
            vec![]
        } else {
            vec![ActionDescription::new(
                "Unset the default Nix profile".to_string(),
                vec!["TODO".to_string()],
            )]
        }
    }

    #[tracing::instrument(skip_all, fields(
        channels = %self.channels.join(","),
    ))]
    async fn revert(&mut self) -> Result<(), Self::Error> {
        let Self {
            channels: _,
            action_state,
        } = self;
        if *action_state == ActionState::Uncompleted {
            tracing::trace!("Already reverted: Unset default profile");
            return Ok(());
        }
        tracing::debug!("Unsetting default profile (mostly noop)");

        std::env::remove_var("NIX_SSL_CERT_FILE");

        tracing::trace!("Unset default profile (mostly noop)");
        *action_state = ActionState::Completed;
        Ok(())
    }
}

impl From<SetupDefaultProfile> for Action {
    fn from(v: SetupDefaultProfile) -> Self {
        Action::SetupDefaultProfile(v)
    }
}

#[derive(Debug, thiserror::Error, Serialize)]
pub enum SetupDefaultProfileError {
    #[error("Glob pattern error")]
    GlobPatternError(
        #[from]
        #[source]
        #[serde(serialize_with = "crate::serialize_error_to_display")]
        glob::PatternError,
    ),
    #[error("Glob globbing error")]
    GlobGlobError(
        #[from]
        #[source]
        #[serde(serialize_with = "crate::serialize_error_to_display")]
        glob::GlobError,
    ),
    #[error("Unarchived Nix store did not appear to include a `nss-cacert` location")]
    NoNssCacert,
    #[error("Failed to execute command")]
    Command(
        #[source]
        #[serde(serialize_with = "crate::serialize_error_to_display")]
        std::io::Error,
    ),
}