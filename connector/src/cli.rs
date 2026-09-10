use std::ffi::OsString;
use std::path::PathBuf;

use crate::ConnectorInstallConfig;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectorCommand {
    Console {
        runtime_dir: PathBuf,
    },
    Service {
        runtime_dir: PathBuf,
    },
    Configure {
        runtime_dir: PathBuf,
        config: ConnectorInstallConfig,
    },
    Version,
}

impl ConnectorCommand {
    pub fn parse<I, S>(arguments: I) -> anyhow::Result<Self>
    where
        I: IntoIterator<Item = S>,
        S: Into<OsString>,
    {
        let mut arguments = arguments.into_iter().map(Into::into);
        let _program = arguments.next();
        let mut runtime_dir = None;
        let mut service = false;
        let mut configure = false;
        let mut version = false;
        let mut center_url = None;
        let mut directory_domain_id = None;
        let mut connector_access_key = None;
        let mut domain_controller_ldap_url = None;
        let mut match_legacy_users_by_username = false;

        while let Some(argument) = arguments.next() {
            match argument.to_string_lossy().as_ref() {
                "--runtime-dir" => {
                    if runtime_dir.is_some() {
                        anyhow::bail!("--runtime-dir can only be specified once");
                    }
                    let value = arguments
                        .next()
                        .ok_or_else(|| anyhow::anyhow!("--runtime-dir requires a value"))?;
                    runtime_dir = Some(PathBuf::from(value));
                }
                "--service" => {
                    if service {
                        anyhow::bail!("--service can only be specified once");
                    }
                    service = true;
                }
                "--configure" => {
                    if configure {
                        anyhow::bail!("--configure can only be specified once");
                    }
                    configure = true;
                }
                "--center-url" => {
                    center_url = Some(required_argument(&mut arguments, "--center-url")?);
                }
                "--directory-domain-id" => {
                    directory_domain_id =
                        Some(required_argument(&mut arguments, "--directory-domain-id")?);
                }
                "--connector-access-key" => {
                    connector_access_key =
                        Some(required_argument(&mut arguments, "--connector-access-key")?);
                }
                "--domain-controller-ldap-url" => {
                    domain_controller_ldap_url = Some(required_argument(
                        &mut arguments,
                        "--domain-controller-ldap-url",
                    )?);
                }
                "--match-legacy-users-by-username" => {
                    let value =
                        required_argument(&mut arguments, "--match-legacy-users-by-username")?;
                    match_legacy_users_by_username = match value.as_str() {
                        "1" | "true" => true,
                        "0" | "false" => false,
                        _ => anyhow::bail!(
                            "--match-legacy-users-by-username must be 0, 1, false, or true"
                        ),
                    };
                }
                "--version" => {
                    if version {
                        anyhow::bail!("--version can only be specified once");
                    }
                    version = true;
                }
                unknown => anyhow::bail!("unknown argument: {unknown}"),
            }
        }

        if version {
            if service || configure || runtime_dir.is_some() {
                anyhow::bail!("--version cannot be combined with other arguments");
            }
            return Ok(Self::Version);
        }

        let runtime_dir = match runtime_dir {
            Some(path) => path,
            None => std::env::current_dir()?,
        };
        if configure {
            if service {
                anyhow::bail!("--configure cannot be combined with --service");
            }
            return Ok(Self::Configure {
                runtime_dir,
                config: ConnectorInstallConfig {
                    center_url: required_option(center_url, "--center-url")?,
                    directory_domain_id: required_option(
                        directory_domain_id,
                        "--directory-domain-id",
                    )?,
                    connector_access_key: required_option(
                        connector_access_key,
                        "--connector-access-key",
                    )?,
                    domain_controller_ldap_url: required_option(
                        domain_controller_ldap_url,
                        "--domain-controller-ldap-url",
                    )?,
                    match_legacy_users_by_username,
                },
            });
        }
        if center_url.is_some()
            || directory_domain_id.is_some()
            || connector_access_key.is_some()
            || domain_controller_ldap_url.is_some()
        {
            anyhow::bail!("configuration options require --configure");
        }
        if service {
            Ok(Self::Service { runtime_dir })
        } else {
            Ok(Self::Console { runtime_dir })
        }
    }
}

fn required_argument<I>(arguments: &mut I, name: &str) -> anyhow::Result<String>
where
    I: Iterator<Item = OsString>,
{
    arguments
        .next()
        .map(|value| value.to_string_lossy().into_owned())
        .ok_or_else(|| anyhow::anyhow!("{name} requires a value"))
}

fn required_option(value: Option<String>, name: &str) -> anyhow::Result<String> {
    value.ok_or_else(|| anyhow::anyhow!("{name} is required with --configure"))
}
