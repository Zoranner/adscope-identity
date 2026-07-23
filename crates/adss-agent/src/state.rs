use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

pub trait LocalStateStore {
    fn load(&self) -> anyhow::Result<LocalRevisionState>;
    fn save(&self, state: LocalRevisionState) -> anyhow::Result<()>;

    fn load_for_sync(&self) -> anyhow::Result<LocalStateLoad> {
        Ok(LocalStateLoad {
            state: self.load()?,
            rebuild_directory: false,
            rebuild_credentials: false,
        })
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LocalRevisionState {
    pub applied_directory_revision: u64,
    pub applied_credential_revision: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LocalStateLoad {
    pub state: LocalRevisionState,
    pub rebuild_directory: bool,
    pub rebuild_credentials: bool,
}

#[derive(Debug, Clone)]
pub struct FileLocalStateStore {
    path: PathBuf,
}

impl FileLocalStateStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl LocalStateStore for FileLocalStateStore {
    fn load(&self) -> anyhow::Result<LocalRevisionState> {
        match fs::read_to_string(&self.path) {
            Ok(contents) => parse_local_revision_state(&contents),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Ok(LocalRevisionState::default())
            }
            Err(error) => Err(error.into()),
        }
    }

    fn save(&self, state: LocalRevisionState) -> anyhow::Result<()> {
        if let Some(parent) = self
            .path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent)?;
        }

        let temp_path = self
            .path
            .with_extension(format!("tmp-{}", std::process::id()));
        {
            let mut file = fs::File::create(&temp_path)?;
            file.write_all(format_local_revision_state(state).as_bytes())?;
            file.sync_all()?;
        }
        replace_file(&temp_path, &self.path)?;

        Ok(())
    }

    fn load_for_sync(&self) -> anyhow::Result<LocalStateLoad> {
        match fs::read_to_string(&self.path) {
            Ok(contents) => match parse_local_revision_state(&contents) {
                Ok(state) => Ok(LocalStateLoad {
                    state,
                    rebuild_directory: false,
                    rebuild_credentials: false,
                }),
                Err(_) => Ok(LocalStateLoad {
                    state: LocalRevisionState::default(),
                    rebuild_directory: true,
                    rebuild_credentials: true,
                }),
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Ok(LocalStateLoad::default())
            }
            Err(error) => Err(error.into()),
        }
    }
}

fn parse_local_revision_state(contents: &str) -> anyhow::Result<LocalRevisionState> {
    let contents = contents.trim();
    let Some(body) = contents
        .strip_prefix('{')
        .and_then(|value| value.strip_suffix('}'))
    else {
        anyhow::bail!("invalid local state JSON object");
    };

    let mut applied_directory_revision = None;
    let mut applied_credential_revision = None;

    for field in body
        .split(',')
        .map(str::trim)
        .filter(|field| !field.is_empty())
    {
        let Some((key, value)) = field.split_once(':') else {
            anyhow::bail!("invalid local state field");
        };
        let key = key.trim().trim_matches('"');
        let value = value.trim().parse::<u64>()?;

        match key {
            "applied_directory_revision" => applied_directory_revision = Some(value),
            "applied_credential_revision" => applied_credential_revision = Some(value),
            _ => anyhow::bail!("unknown local state field: {key}"),
        }
    }

    Ok(LocalRevisionState {
        applied_directory_revision: applied_directory_revision
            .ok_or_else(|| anyhow::anyhow!("missing applied_directory_revision"))?,
        applied_credential_revision: applied_credential_revision
            .ok_or_else(|| anyhow::anyhow!("missing applied_credential_revision"))?,
    })
}

fn format_local_revision_state(state: LocalRevisionState) -> String {
    format!(
        "{{\"applied_directory_revision\":{},\"applied_credential_revision\":{}}}",
        state.applied_directory_revision, state.applied_credential_revision
    )
}

#[cfg(windows)]
fn replace_file(from: &Path, to: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;

    const MOVEFILE_REPLACE_EXISTING: u32 = 0x1;
    const MOVEFILE_WRITE_THROUGH: u32 = 0x8;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn MoveFileExW(
            existing_file_name: *const u16,
            new_file_name: *const u16,
            flags: u32,
        ) -> i32;
    }

    let from_wide: Vec<u16> = from.as_os_str().encode_wide().chain(Some(0)).collect();
    let to_wide: Vec<u16> = to.as_os_str().encode_wide().chain(Some(0)).collect();
    let result = unsafe {
        MoveFileExW(
            from_wide.as_ptr(),
            to_wide.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };

    if result == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(not(windows))]
fn replace_file(from: &Path, to: &Path) -> std::io::Result<()> {
    fs::rename(from, to)
}
