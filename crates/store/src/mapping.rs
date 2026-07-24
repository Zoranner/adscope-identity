use adss_protocol::{Group, OrganizationalUnit, User, UserStatus};
use sea_orm::Set;

use crate::{
    entities,
    models::{CredentialRecord, DomainRecord},
    revision::{i64_to_u64_revision, u64_to_i64_revision},
};

pub(crate) fn user_status_to_storage(status: UserStatus) -> &'static str {
    match status {
        UserStatus::Active => "active",
        UserStatus::Disabled => "disabled",
    }
}

pub(crate) fn user_status_from_storage(status: &str) -> anyhow::Result<UserStatus> {
    match status {
        "active" => Ok(UserStatus::Active),
        "disabled" => Ok(UserStatus::Disabled),
        _ => anyhow::bail!("unsupported user status: {status}"),
    }
}

pub(crate) fn domain_active_model(
    record: DomainRecord,
) -> anyhow::Result<entities::domain::ActiveModel> {
    Ok(entities::domain::ActiveModel {
        id: Set(record.id),
        name: Set(record.name),
        enabled: Set(record.enabled),
        mirror_root_dn: Set(record.mirror_root_dn),
        quarantine_ou_dn: Set(record.quarantine_ou_dn),
        upn_suffix: Set(record.upn_suffix),
        employee_id_attribute: Set(record.employee_id_attribute),
        managed_group_id_attribute: Set(record.managed_group_id_attribute),
        connector_key_hash: Set(record.connector_key_hash),
        applied_directory_revision: Set(u64_to_i64_revision(record.applied_directory_revision)?),
        applied_credential_revision: Set(u64_to_i64_revision(record.applied_credential_revision)?),
    })
}

impl TryFrom<entities::domain::Model> for DomainRecord {
    type Error = anyhow::Error;

    fn try_from(model: entities::domain::Model) -> anyhow::Result<Self> {
        Ok(Self {
            id: model.id,
            name: model.name,
            enabled: model.enabled,
            mirror_root_dn: model.mirror_root_dn,
            quarantine_ou_dn: model.quarantine_ou_dn,
            upn_suffix: model.upn_suffix,
            employee_id_attribute: model.employee_id_attribute,
            managed_group_id_attribute: model.managed_group_id_attribute,
            connector_key_hash: model.connector_key_hash,
            applied_directory_revision: i64_to_u64_revision(model.applied_directory_revision)?,
            applied_credential_revision: i64_to_u64_revision(model.applied_credential_revision)?,
        })
    }
}

impl TryFrom<entities::organizational_unit::Model> for OrganizationalUnit {
    type Error = anyhow::Error;

    fn try_from(model: entities::organizational_unit::Model) -> anyhow::Result<Self> {
        Ok(Self {
            id: model.id,
            name: model.name,
            parent_id: model.parent_id,
            changed_revision: i64_to_u64_revision(model.changed_revision)?,
        })
    }
}

impl TryFrom<entities::user::Model> for User {
    type Error = anyhow::Error;

    fn try_from(model: entities::user::Model) -> anyhow::Result<Self> {
        Ok(Self {
            employee_id: model.employee_id,
            username: model.username,
            display_name: model.display_name,
            email: model.email,
            mobile: model.mobile,
            telephone: model.telephone,
            organizational_unit_id: model.organizational_unit_id,
            status: user_status_from_storage(&model.status)?,
            changed_revision: i64_to_u64_revision(model.changed_revision)?,
        })
    }
}

impl TryFrom<entities::group::Model> for Group {
    type Error = anyhow::Error;

    fn try_from(model: entities::group::Model) -> anyhow::Result<Self> {
        Ok(Self {
            id: model.id,
            name: model.name,
            member_employee_ids: serde_json::from_str(&model.member_employee_ids)?,
            changed_revision: i64_to_u64_revision(model.changed_revision)?,
        })
    }
}

impl TryFrom<entities::user_credential::Model> for CredentialRecord {
    type Error = anyhow::Error;

    fn try_from(model: entities::user_credential::Model) -> anyhow::Result<Self> {
        Ok(Self {
            employee_id: model.employee_id,
            password_ciphertext: model.password_ciphertext,
            password_verifier: model.password_verifier,
            changed_revision: i64_to_u64_revision(model.changed_revision)?,
        })
    }
}
