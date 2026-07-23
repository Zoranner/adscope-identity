use std::{
    io::Write,
    process::{Command, Stdio},
};

use sha2::{Digest, Sha256};

use super::PasswordEnvelope;

#[derive(Debug)]
pub(crate) struct LocalPasswordEnvelope {
    key: Vec<u8>,
}

impl LocalPasswordEnvelope {
    pub(crate) fn new(key: impl Into<String>) -> Self {
        Self {
            key: key.into().into_bytes(),
        }
    }
}

impl PasswordEnvelope for LocalPasswordEnvelope {
    fn seal(&self, plaintext: &str) -> anyhow::Result<String> {
        if self.key.is_empty() {
            anyhow::bail!("local password envelope key must not be empty");
        }

        Ok(format!(
            "local-envelope:v1:{}",
            hex::encode(xor_with_password_stream(plaintext.as_bytes(), &self.key))
        ))
    }

    fn open(&self, ciphertext: &str) -> anyhow::Result<String> {
        if self.key.is_empty() {
            anyhow::bail!("local password envelope key must not be empty");
        }

        let encoded = ciphertext
            .strip_prefix("local-envelope:v1:")
            .ok_or_else(|| anyhow::anyhow!("unsupported password envelope"))?;
        let ciphertext = hex::decode(encoded)?;
        Ok(String::from_utf8(xor_with_password_stream(
            &ciphertext,
            &self.key,
        ))?)
    }
}

#[derive(Debug)]
pub(super) struct CommandPasswordEnvelope {
    command: String,
}

impl CommandPasswordEnvelope {
    pub(super) fn new(command: impl Into<String>) -> Self {
        Self {
            command: command.into(),
        }
    }

    fn run(&self, action: &str, input: &str) -> anyhow::Result<String> {
        let mut child = Command::new(&self.command)
            .arg(action)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;

        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow::anyhow!("password envelope command stdin unavailable"))?;
        stdin.write_all(input.as_bytes())?;
        drop(stdin);

        let output = child.wait_with_output()?;
        if !output.status.success() {
            anyhow::bail!("password envelope command failed");
        }

        let output = String::from_utf8(output.stdout)?;
        Ok(output.trim_end_matches(['\r', '\n']).to_string())
    }
}

impl PasswordEnvelope for CommandPasswordEnvelope {
    fn seal(&self, plaintext: &str) -> anyhow::Result<String> {
        self.run("seal", plaintext)
    }

    fn open(&self, ciphertext: &str) -> anyhow::Result<String> {
        self.run("open", ciphertext)
    }
}

fn xor_with_password_stream(input: &[u8], password_envelope_key: &[u8]) -> Vec<u8> {
    let mut output = Vec::with_capacity(input.len());
    let mut counter = 0_u64;

    while output.len() < input.len() {
        let mut hasher = Sha256::new();
        hasher.update(b"adss:local-password-envelope:v1");
        hasher.update(password_envelope_key);
        hasher.update(counter.to_be_bytes());
        let block = hasher.finalize();

        for byte in block {
            if output.len() == input.len() {
                break;
            }
            output.push(input[output.len()] ^ byte);
        }

        counter += 1;
    }

    output
}
