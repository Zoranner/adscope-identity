use axum::http::HeaderMap;
use sha2::{Digest, Sha256};

use crate::error::ApiError;

const CONNECTOR_KEY_HEADER: &str = "x-adscope-connector-key";

pub(crate) async fn authorize_domain_connector(
    repository: &adscope_store::Repository,
    domain_id: &str,
    connector_key: Option<&str>,
) -> Result<adscope_store::DomainRecord, ApiError> {
    let domain = repository
        .get_domain(domain_id)
        .await
        .map_err(|_| ApiError::Persistence)?
        .ok_or(ApiError::Unauthorized)?;

    let Some(connector_key) = connector_key else {
        return Err(ApiError::Unauthorized);
    };

    let provided_hash = connector_key_hash(connector_key);
    if !constant_time_eq(
        domain.connector_key_hash.as_bytes(),
        provided_hash.as_bytes(),
    ) {
        return Err(ApiError::Unauthorized);
    }

    if !domain.enabled {
        return Err(ApiError::Forbidden);
    }

    Ok(domain)
}

pub(crate) fn connector_key_from_headers(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(CONNECTOR_KEY_HEADER)
        .and_then(|value| value.to_str().ok())
}

pub(crate) fn connector_key_hash(connector_key: &str) -> String {
    format!("sha256:{}", sha256_hex(connector_key.as_bytes()))
}

pub(crate) fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    let max_len = left.len().max(right.len());
    let mut diff = left.len() ^ right.len();

    for index in 0..max_len {
        let left_byte = left.get(index).copied().unwrap_or_default();
        let right_byte = right.get(index).copied().unwrap_or_default();
        diff |= usize::from(left_byte ^ right_byte);
    }

    diff == 0
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}
