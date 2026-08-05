use sea_orm::Set;

use crate::{
    entities::{oauth_authorization_code, oauth_client},
    models::{AuthorizationCodeRecord, OAuthClientRecord, OAuthClientType},
};

pub(crate) fn oauth_client_type_to_storage(client_type: OAuthClientType) -> &'static str {
    match client_type {
        OAuthClientType::Web => "web",
        OAuthClientType::Desktop => "desktop",
    }
}

fn oauth_client_type_from_storage(client_type: &str) -> anyhow::Result<OAuthClientType> {
    match client_type {
        "web" => Ok(OAuthClientType::Web),
        "desktop" => Ok(OAuthClientType::Desktop),
        _ => anyhow::bail!("unsupported OAuth client type: {client_type}"),
    }
}

pub(crate) fn oauth_client_active_model(
    record: OAuthClientRecord,
) -> anyhow::Result<oauth_client::ActiveModel> {
    Ok(oauth_client::ActiveModel {
        client_id: Set(record.client_id),
        name: Set(record.name),
        client_type: Set(oauth_client_type_to_storage(record.client_type).to_string()),
        client_secret_hash: Set(record.client_secret_hash),
        redirect_uris: Set(serde_json::to_string(&record.redirect_uris)?),
        allowed_scopes: Set(serde_json::to_string(&record.allowed_scopes)?),
        enabled: Set(record.enabled),
    })
}

pub(crate) fn authorization_code_active_model(
    record: AuthorizationCodeRecord,
) -> anyhow::Result<oauth_authorization_code::ActiveModel> {
    Ok(oauth_authorization_code::ActiveModel {
        code_hash: Set(record.code_hash),
        client_id: Set(record.client_id),
        employee_id: Set(record.employee_id),
        redirect_uri: Set(record.redirect_uri),
        scopes: Set(serde_json::to_string(&record.scopes)?),
        nonce: Set(record.nonce),
        code_challenge: Set(record.code_challenge),
        auth_time: Set(record.auth_time),
        expires_at: Set(record.expires_at),
    })
}

impl TryFrom<oauth_client::Model> for OAuthClientRecord {
    type Error = anyhow::Error;

    fn try_from(model: oauth_client::Model) -> anyhow::Result<Self> {
        Ok(Self {
            client_id: model.client_id,
            name: model.name,
            client_type: oauth_client_type_from_storage(&model.client_type)?,
            client_secret_hash: model.client_secret_hash,
            redirect_uris: serde_json::from_str(&model.redirect_uris)?,
            allowed_scopes: serde_json::from_str(&model.allowed_scopes)?,
            enabled: model.enabled,
        })
    }
}

impl TryFrom<oauth_authorization_code::Model> for AuthorizationCodeRecord {
    type Error = anyhow::Error;

    fn try_from(model: oauth_authorization_code::Model) -> anyhow::Result<Self> {
        Ok(Self {
            code_hash: model.code_hash,
            client_id: model.client_id,
            employee_id: model.employee_id,
            redirect_uri: model.redirect_uri,
            scopes: serde_json::from_str(&model.scopes)?,
            nonce: model.nonce,
            code_challenge: model.code_challenge,
            auth_time: model.auth_time,
            expires_at: model.expires_at,
        })
    }
}
