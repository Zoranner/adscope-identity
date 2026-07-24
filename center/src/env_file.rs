use std::{env, fs, path::Path};

pub fn load_env_file(path: impl AsRef<Path>) -> anyhow::Result<()> {
    let path = path.as_ref();
    let content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };

    for (index, line) in content.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let Some((name, value)) = line.split_once('=') else {
            anyhow::bail!("invalid .env line {}: missing '='", index + 1);
        };
        let name = name.trim();
        if name.is_empty() {
            anyhow::bail!("invalid .env line {}: empty variable name", index + 1);
        }
        if env::var_os(name).is_some() {
            continue;
        }

        let value = unquote_env_value(value.trim());
        unsafe {
            env::set_var(name, value);
        }
    }

    Ok(())
}

fn unquote_env_value(value: &str) -> String {
    if value.len() >= 2
        && ((value.starts_with('"') && value.ends_with('"'))
            || (value.starts_with('\'') && value.ends_with('\'')))
    {
        value[1..value.len() - 1].to_string()
    } else {
        value.to_string()
    }
}
