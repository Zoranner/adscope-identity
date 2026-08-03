use std::ffi::OsString;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectorCommand {
    Console { runtime_dir: PathBuf },
    Service { runtime_dir: PathBuf },
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
        let mut version = false;

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
            if service || runtime_dir.is_some() {
                anyhow::bail!("--version cannot be combined with other arguments");
            }
            return Ok(Self::Version);
        }

        let runtime_dir = match runtime_dir {
            Some(path) => path,
            None => std::env::current_dir()?,
        };
        if service {
            Ok(Self::Service { runtime_dir })
        } else {
            Ok(Self::Console { runtime_dir })
        }
    }
}
