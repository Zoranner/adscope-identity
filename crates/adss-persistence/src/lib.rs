use adss_contract::{DirectoryBatch, Group, OrganizationalUnit, User, UserStatus};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, Database, DatabaseConnection, DbBackend,
    EntityTrait, QueryFilter, QueryOrder, Set, Statement, TransactionTrait,
};
use serde::{Deserialize, Serialize};

mod entities {
    pub mod sync_metadata {
        use sea_orm::entity::prelude::*;

        #[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
        #[sea_orm(table_name = "sync_metadata")]
        pub struct Model {
            #[sea_orm(primary_key, auto_increment = false)]
            pub key: String,
            pub directory_revision: i64,
            pub credential_revision: i64,
        }

        #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
        pub enum Relation {}

        impl ActiveModelBehavior for ActiveModel {}
    }

    pub mod organizational_unit {
        use sea_orm::entity::prelude::*;

        #[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
        #[sea_orm(table_name = "organizational_units")]
        pub struct Model {
            #[sea_orm(primary_key, auto_increment = false)]
            pub id: String,
            pub name: String,
            pub parent_id: Option<String>,
            pub changed_revision: i64,
        }

        #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
        pub enum Relation {}

        impl ActiveModelBehavior for ActiveModel {}
    }

    pub mod user {
        use sea_orm::entity::prelude::*;

        #[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
        #[sea_orm(table_name = "users")]
        pub struct Model {
            #[sea_orm(primary_key, auto_increment = false)]
            pub employee_id: String,
            pub username: String,
            pub display_name: String,
            pub email: Option<String>,
            pub mobile: Option<String>,
            pub telephone: Option<String>,
            pub organizational_unit_id: String,
            pub status: String,
            pub changed_revision: i64,
        }

        #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
        pub enum Relation {}

        impl ActiveModelBehavior for ActiveModel {}
    }

    pub mod group {
        use sea_orm::entity::prelude::*;

        #[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
        #[sea_orm(table_name = "groups")]
        pub struct Model {
            #[sea_orm(primary_key, auto_increment = false)]
            pub id: String,
            pub name: String,
            pub member_employee_ids_json: String,
            pub changed_revision: i64,
        }

        #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
        pub enum Relation {}

        impl ActiveModelBehavior for ActiveModel {}
    }

    pub mod user_credential {
        use sea_orm::entity::prelude::*;

        #[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
        #[sea_orm(table_name = "user_credentials")]
        pub struct Model {
            #[sea_orm(primary_key, auto_increment = false)]
            pub employee_id: String,
            pub password_ciphertext: String,
            pub password_verifier: String,
            pub changed_revision: i64,
        }

        #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
        pub enum Relation {}

        impl ActiveModelBehavior for ActiveModel {}
    }

    pub mod domain {
        use sea_orm::entity::prelude::*;

        #[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
        #[sea_orm(table_name = "domains")]
        pub struct Model {
            #[sea_orm(primary_key, auto_increment = false)]
            pub id: String,
            pub name: String,
            pub enabled: bool,
            pub mirror_root_dn: String,
            pub quarantine_ou_dn: String,
            pub upn_suffix: String,
            pub employee_id_attribute: String,
            pub agent_key_hash: String,
            pub applied_directory_revision: i64,
            pub applied_credential_revision: i64,
        }

        #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
        pub enum Relation {}

        impl ActiveModelBehavior for ActiveModel {}
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DomainRecord {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub mirror_root_dn: String,
    pub quarantine_ou_dn: String,
    pub upn_suffix: String,
    pub employee_id_attribute: String,
    pub agent_key_hash: String,
    pub applied_directory_revision: u64,
    pub applied_credential_revision: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UserDirectoryPatch {
    pub employee_id: String,
    pub username: String,
    pub display_name: String,
    pub email: Option<String>,
    pub mobile: Option<String>,
    pub telephone: Option<String>,
    pub organizational_unit_id: String,
    pub status: UserStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UserCredentialInput {
    pub employee_id: String,
    pub password_ciphertext: String,
    pub password_verifier: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CredentialRecord {
    pub employee_id: String,
    pub password_ciphertext: String,
    pub password_verifier: String,
    pub changed_revision: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CredentialCiphertextEntry {
    pub employee_id: String,
    pub password_ciphertext: String,
    pub changed_revision: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CredentialCiphertextBatch {
    pub server_revision: u64,
    pub batch_revision: u64,
    pub credentials: Vec<CredentialCiphertextEntry>,
    pub has_more: bool,
}

#[derive(Clone)]
pub struct Repository {
    db: DatabaseConnection,
}

impl Repository {
    pub async fn connect(database_url: &str) -> anyhow::Result<Self> {
        Ok(Self {
            db: Database::connect(database_url).await?,
        })
    }

    pub fn from_connection(db: DatabaseConnection) -> Self {
        Self { db }
    }

    pub async fn initialize_schema(&self) -> anyhow::Result<()> {
        self.db
            .execute_unprepared(
                r#"
CREATE TABLE IF NOT EXISTS sync_metadata (
    key TEXT PRIMARY KEY NOT NULL,
    directory_revision BIGINT NOT NULL CHECK (directory_revision >= 0),
    credential_revision BIGINT NOT NULL CHECK (credential_revision >= 0)
)
"#,
            )
            .await?;
        self.db
            .execute_unprepared(
                r#"
CREATE TABLE IF NOT EXISTS organizational_units (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    parent_id TEXT NULL,
    changed_revision BIGINT NOT NULL CHECK (changed_revision >= 0)
)
"#,
            )
            .await?;
        self.db
            .execute_unprepared(
                r#"
CREATE TABLE IF NOT EXISTS users (
    employee_id TEXT PRIMARY KEY NOT NULL,
    username TEXT NOT NULL,
    display_name TEXT NOT NULL,
    email TEXT NULL,
    mobile TEXT NULL,
    telephone TEXT NULL,
    organizational_unit_id TEXT NOT NULL,
    status TEXT NOT NULL,
    changed_revision BIGINT NOT NULL CHECK (changed_revision >= 0)
)
"#,
            )
            .await?;
        self.db
            .execute_unprepared(
                r#"
CREATE TABLE IF NOT EXISTS groups (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    member_employee_ids_json TEXT NOT NULL,
    changed_revision BIGINT NOT NULL CHECK (changed_revision >= 0)
)
"#,
            )
            .await?;
        self.db
            .execute_unprepared(
                r#"
CREATE TABLE IF NOT EXISTS user_credentials (
    employee_id TEXT PRIMARY KEY NOT NULL,
    password_ciphertext TEXT NOT NULL,
    password_verifier TEXT NOT NULL,
    changed_revision BIGINT NOT NULL CHECK (changed_revision >= 0)
)
"#,
            )
            .await?;
        self.db
            .execute_unprepared(
                r#"
CREATE TABLE IF NOT EXISTS domains (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    enabled BOOLEAN NOT NULL,
    mirror_root_dn TEXT NOT NULL,
    quarantine_ou_dn TEXT NOT NULL,
    upn_suffix TEXT NOT NULL,
    employee_id_attribute TEXT NOT NULL,
    agent_key_hash TEXT NOT NULL,
    applied_directory_revision BIGINT NOT NULL CHECK (applied_directory_revision >= 0),
    applied_credential_revision BIGINT NOT NULL CHECK (applied_credential_revision >= 0)
)
"#,
            )
            .await?;
        self.ensure_metadata().await?;
        Ok(())
    }

    pub async fn seed_domain(&self, domain: DomainRecord) -> anyhow::Result<()> {
        use entities::domain;

        let transaction = self.db.begin().await?;
        domain::Entity::delete_by_id(domain.id.as_str())
            .exec(&transaction)
            .await?;
        domain_active_model(domain)?.insert(&transaction).await?;
        transaction.commit().await?;
        Ok(())
    }

    pub async fn get_domain(&self, domain_id: &str) -> anyhow::Result<Option<DomainRecord>> {
        use entities::domain;

        let Some(model) = domain::Entity::find_by_id(domain_id).one(&self.db).await? else {
            return Ok(None);
        };

        Ok(Some(DomainRecord::try_from(model)?))
    }

    pub async fn upsert_directory(
        &self,
        ous: Vec<OrganizationalUnit>,
        users: Vec<UserDirectoryPatch>,
        groups: Vec<Group>,
    ) -> anyhow::Result<u64> {
        use entities::{group, organizational_unit, user};

        if ous.is_empty() && users.is_empty() && groups.is_empty() {
            return self.current_directory_revision().await;
        }

        for _ in 0..MAX_REVISION_ALLOCATION_ATTEMPTS {
            let transaction = self.db.begin().await?;
            let Some(revision) = try_allocate_directory_revision(&transaction).await? else {
                transaction.rollback().await?;
                continue;
            };

            for ou in &ous {
                organizational_unit::Entity::delete_by_id(ou.id.as_str())
                    .exec(&transaction)
                    .await?;
                organizational_unit::ActiveModel {
                    id: Set(ou.id.clone()),
                    name: Set(ou.name.clone()),
                    parent_id: Set(ou.parent_id.clone()),
                    changed_revision: Set(revision),
                }
                .insert(&transaction)
                .await?;
            }

            for user in &users {
                user::Entity::delete_by_id(user.employee_id.as_str())
                    .exec(&transaction)
                    .await?;
                user::ActiveModel {
                    employee_id: Set(user.employee_id.clone()),
                    username: Set(user.username.clone()),
                    display_name: Set(user.display_name.clone()),
                    email: Set(user.email.clone()),
                    mobile: Set(user.mobile.clone()),
                    telephone: Set(user.telephone.clone()),
                    organizational_unit_id: Set(user.organizational_unit_id.clone()),
                    status: Set(user_status_to_storage(user.status).to_string()),
                    changed_revision: Set(revision),
                }
                .insert(&transaction)
                .await?;
            }

            for group in &groups {
                group::Entity::delete_by_id(group.id.as_str())
                    .exec(&transaction)
                    .await?;
                group::ActiveModel {
                    id: Set(group.id.clone()),
                    name: Set(group.name.clone()),
                    member_employee_ids_json: Set(serde_json::to_string(
                        &group.member_employee_ids,
                    )?),
                    changed_revision: Set(revision),
                }
                .insert(&transaction)
                .await?;
            }

            transaction.commit().await?;
            return i64_to_u64_revision(revision);
        }

        anyhow::bail!("failed to allocate directory revision after retries")
    }

    pub async fn list_directory_changed_after(
        &self,
        domain_id: &str,
        applied_revision: u64,
        rebuild: bool,
        _limit: usize,
    ) -> anyhow::Result<DirectoryBatch> {
        use entities::{group, organizational_unit, user};

        let domain = self.require_domain(domain_id).await?;
        let server_revision = self.current_directory_revision().await?;
        let threshold = if rebuild { 0 } else { applied_revision };
        let threshold = u64_to_i64_revision(threshold)?;
        if applied_revision > domain.applied_directory_revision {
            anyhow::bail!("requested directory revision exceeds confirmed domain revision");
        }
        if !rebuild && applied_revision > server_revision {
            anyhow::bail!("applied directory revision exceeds server revision");
        }

        let organizational_units = organizational_unit::Entity::find()
            .filter(organizational_unit::Column::ChangedRevision.gt(threshold))
            .order_by_asc(organizational_unit::Column::ChangedRevision)
            .order_by_asc(organizational_unit::Column::Id)
            .all(&self.db)
            .await?
            .into_iter()
            .map(OrganizationalUnit::try_from)
            .collect::<anyhow::Result<Vec<_>>>()?;
        let users = user::Entity::find()
            .filter(user::Column::ChangedRevision.gt(threshold))
            .order_by_asc(user::Column::ChangedRevision)
            .order_by_asc(user::Column::EmployeeId)
            .all(&self.db)
            .await?
            .into_iter()
            .map(User::try_from)
            .collect::<anyhow::Result<Vec<_>>>()?;
        let groups = group::Entity::find()
            .filter(group::Column::ChangedRevision.gt(threshold))
            .order_by_asc(group::Column::ChangedRevision)
            .order_by_asc(group::Column::Id)
            .all(&self.db)
            .await?
            .into_iter()
            .map(Group::try_from)
            .collect::<anyhow::Result<Vec<_>>>()?;

        Ok(DirectoryBatch {
            server_revision,
            batch_revision: server_revision,
            organizational_units,
            users,
            groups,
            has_more: false,
        })
    }

    pub async fn change_user_password(&self, input: UserCredentialInput) -> anyhow::Result<u64> {
        use entities::user_credential;

        for _ in 0..MAX_REVISION_ALLOCATION_ATTEMPTS {
            let transaction = self.db.begin().await?;
            let Some(revision) = try_allocate_credential_revision(&transaction).await? else {
                transaction.rollback().await?;
                continue;
            };

            user_credential::Entity::delete_by_id(input.employee_id.as_str())
                .exec(&transaction)
                .await?;
            user_credential::ActiveModel {
                employee_id: Set(input.employee_id.clone()),
                password_ciphertext: Set(input.password_ciphertext.clone()),
                password_verifier: Set(input.password_verifier.clone()),
                changed_revision: Set(revision),
            }
            .insert(&transaction)
            .await?;

            transaction.commit().await?;
            return i64_to_u64_revision(revision);
        }

        anyhow::bail!("failed to allocate credential revision after retries")
    }

    pub async fn get_credential_record(
        &self,
        employee_id: &str,
    ) -> anyhow::Result<Option<CredentialRecord>> {
        use entities::user_credential;

        user_credential::Entity::find_by_id(employee_id)
            .one(&self.db)
            .await?
            .map(CredentialRecord::try_from)
            .transpose()
    }

    pub async fn list_credentials_changed_after(
        &self,
        domain_id: &str,
        applied_revision: u64,
        rebuild: bool,
        limit: usize,
    ) -> anyhow::Result<CredentialCiphertextBatch> {
        self.list_credentials_changed_after_for_domain(domain_id, applied_revision, rebuild, limit)
            .await
    }

    async fn list_credentials_changed_after_for_domain(
        &self,
        domain_id: &str,
        applied_revision: u64,
        rebuild: bool,
        limit: usize,
    ) -> anyhow::Result<CredentialCiphertextBatch> {
        use entities::user_credential;

        let domain = self.require_domain(domain_id).await?;
        let server_revision = self.current_credential_revision().await?;
        let threshold = if rebuild { 0 } else { applied_revision };
        let threshold = u64_to_i64_revision(threshold)?;
        if applied_revision > domain.applied_credential_revision {
            anyhow::bail!("requested credential revision exceeds confirmed domain revision");
        }
        if !rebuild && applied_revision > server_revision {
            anyhow::bail!("applied credential revision exceeds server revision");
        }

        let mut revisions = user_credential::Entity::find()
            .filter(user_credential::Column::ChangedRevision.gt(threshold))
            .order_by_asc(user_credential::Column::ChangedRevision)
            .all(&self.db)
            .await?
            .into_iter()
            .map(|model| i64_to_u64_revision(model.changed_revision))
            .collect::<anyhow::Result<Vec<_>>>()?;
        revisions.sort_unstable();
        revisions.dedup();

        let limit = limit.max(1);
        let has_more = revisions.len() > limit;
        let batch_revision = revisions
            .get(limit - 1)
            .or_else(|| revisions.last())
            .copied()
            .unwrap_or(server_revision);

        let credentials = user_credential::Entity::find()
            .filter(user_credential::Column::ChangedRevision.gt(threshold))
            .filter(
                user_credential::Column::ChangedRevision.lte(u64_to_i64_revision(batch_revision)?),
            )
            .order_by_asc(user_credential::Column::ChangedRevision)
            .order_by_asc(user_credential::Column::EmployeeId)
            .all(&self.db)
            .await?
            .into_iter()
            .map(|model| {
                Ok(CredentialCiphertextEntry {
                    employee_id: model.employee_id,
                    password_ciphertext: model.password_ciphertext,
                    changed_revision: i64_to_u64_revision(model.changed_revision)?,
                })
            })
            .collect::<anyhow::Result<Vec<_>>>()?;

        Ok(CredentialCiphertextBatch {
            server_revision,
            batch_revision,
            credentials,
            has_more,
        })
    }

    pub async fn confirm_directory_revision(
        &self,
        domain_id: &str,
        revision: u64,
    ) -> anyhow::Result<()> {
        let server_revision = self.current_directory_revision().await?;
        self.confirm_revision(
            domain_id,
            revision,
            server_revision,
            SyncRevisionChannel::Directory,
        )
        .await
    }

    pub async fn confirm_credential_revision(
        &self,
        domain_id: &str,
        revision: u64,
    ) -> anyhow::Result<()> {
        let server_revision = self.current_credential_revision().await?;
        self.confirm_revision(
            domain_id,
            revision,
            server_revision,
            SyncRevisionChannel::Credential,
        )
        .await
    }

    async fn ensure_metadata(&self) -> anyhow::Result<()> {
        use entities::sync_metadata;

        if sync_metadata::Entity::find_by_id(METADATA_KEY)
            .one(&self.db)
            .await?
            .is_none()
        {
            sync_metadata::ActiveModel {
                key: Set(METADATA_KEY.to_string()),
                directory_revision: Set(0),
                credential_revision: Set(0),
            }
            .insert(&self.db)
            .await?;
        }
        Ok(())
    }

    async fn current_directory_revision(&self) -> anyhow::Result<u64> {
        i64_to_u64_revision(load_metadata(&self.db).await?.directory_revision)
    }

    async fn current_credential_revision(&self) -> anyhow::Result<u64> {
        i64_to_u64_revision(load_metadata(&self.db).await?.credential_revision)
    }

    async fn require_domain(&self, domain_id: &str) -> anyhow::Result<DomainRecord> {
        self.get_domain(domain_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("unknown domain: {domain_id}"))
    }

    async fn confirm_revision(
        &self,
        domain_id: &str,
        revision: u64,
        server_revision: u64,
        channel: SyncRevisionChannel,
    ) -> anyhow::Result<()> {
        let revision = u64_to_i64_revision(revision)?;
        let server_revision = u64_to_i64_revision(server_revision)?;
        let backend = self.db.get_database_backend();
        let statement = Statement::from_sql_and_values(
            backend,
            confirm_revision_update_sql(backend, channel),
            vec![
                revision.into(),
                domain_id.into(),
                revision.into(),
                revision.into(),
                server_revision.into(),
            ],
        );
        let result = self.db.execute_raw(statement).await?;
        if result.rows_affected() != 1 {
            anyhow::bail!("confirmed revision was rejected");
        }
        Ok(())
    }
}

const METADATA_KEY: &str = "current";
const MAX_REVISION_ALLOCATION_ATTEMPTS: usize = 5;

#[derive(Debug, Clone, Copy)]
enum SyncRevisionChannel {
    Directory,
    Credential,
}

async fn load_metadata<C>(connection: &C) -> anyhow::Result<entities::sync_metadata::Model>
where
    C: ConnectionTrait,
{
    use entities::sync_metadata;

    sync_metadata::Entity::find_by_id(METADATA_KEY)
        .one(connection)
        .await?
        .ok_or_else(|| anyhow::anyhow!("sync metadata is not initialized"))
}

async fn try_allocate_directory_revision<C>(connection: &C) -> anyhow::Result<Option<i64>>
where
    C: ConnectionTrait,
{
    let metadata = load_metadata(connection).await?;
    let next_revision = metadata
        .directory_revision
        .checked_add(1)
        .ok_or_else(|| anyhow::anyhow!("directory revision exceeds i64::MAX"))?;
    let backend = connection.get_database_backend();
    let statement = Statement::from_sql_and_values(
        backend,
        revision_compare_update_sql(
            backend,
            "directory_revision",
            "UPDATE sync_metadata SET directory_revision = ",
        ),
        vec![
            next_revision.into(),
            METADATA_KEY.into(),
            metadata.directory_revision.into(),
        ],
    );
    let result = connection.execute_raw(statement).await?;

    Ok((result.rows_affected() == 1).then_some(next_revision))
}

async fn try_allocate_credential_revision<C>(connection: &C) -> anyhow::Result<Option<i64>>
where
    C: ConnectionTrait,
{
    let metadata = load_metadata(connection).await?;
    let next_revision = metadata
        .credential_revision
        .checked_add(1)
        .ok_or_else(|| anyhow::anyhow!("credential revision exceeds i64::MAX"))?;
    let backend = connection.get_database_backend();
    let statement = Statement::from_sql_and_values(
        backend,
        revision_compare_update_sql(
            backend,
            "credential_revision",
            "UPDATE sync_metadata SET credential_revision = ",
        ),
        vec![
            next_revision.into(),
            METADATA_KEY.into(),
            metadata.credential_revision.into(),
        ],
    );
    let result = connection.execute_raw(statement).await?;

    Ok((result.rows_affected() == 1).then_some(next_revision))
}

fn revision_compare_update_sql(
    backend: DbBackend,
    revision_column: &'static str,
    prefix: &'static str,
) -> String {
    match backend {
        DbBackend::Postgres => {
            format!("{prefix}$1 WHERE key = $2 AND {revision_column} = $3")
        }
        DbBackend::MySql | DbBackend::Sqlite => {
            format!("{prefix}? WHERE key = ? AND {revision_column} = ?")
        }
        _ => format!("{prefix}? WHERE key = ? AND {revision_column} = ?"),
    }
}

fn confirm_revision_update_sql(backend: DbBackend, channel: SyncRevisionChannel) -> String {
    let column = match channel {
        SyncRevisionChannel::Directory => "applied_directory_revision",
        SyncRevisionChannel::Credential => "applied_credential_revision",
    };
    match backend {
        DbBackend::Postgres => {
            format!(
                "UPDATE domains SET {column} = $1 WHERE id = $2 AND {column} <= $3 AND $4 <= $5"
            )
        }
        DbBackend::MySql | DbBackend::Sqlite => {
            format!("UPDATE domains SET {column} = ? WHERE id = ? AND {column} <= ? AND ? <= ?")
        }
        _ => format!("UPDATE domains SET {column} = ? WHERE id = ? AND {column} <= ? AND ? <= ?"),
    }
}

fn u64_to_i64_revision(value: u64) -> anyhow::Result<i64> {
    i64::try_from(value).map_err(|_| anyhow::anyhow!("revision exceeds i64::MAX"))
}

fn i64_to_u64_revision(value: i64) -> anyhow::Result<u64> {
    u64::try_from(value).map_err(|_| anyhow::anyhow!("negative revision in database"))
}

fn user_status_to_storage(status: UserStatus) -> &'static str {
    match status {
        UserStatus::Active => "active",
        UserStatus::Disabled => "disabled",
    }
}

fn user_status_from_storage(status: &str) -> anyhow::Result<UserStatus> {
    match status {
        "active" => Ok(UserStatus::Active),
        "disabled" => Ok(UserStatus::Disabled),
        _ => anyhow::bail!("unsupported user status: {status}"),
    }
}

fn domain_active_model(record: DomainRecord) -> anyhow::Result<entities::domain::ActiveModel> {
    Ok(entities::domain::ActiveModel {
        id: Set(record.id),
        name: Set(record.name),
        enabled: Set(record.enabled),
        mirror_root_dn: Set(record.mirror_root_dn),
        quarantine_ou_dn: Set(record.quarantine_ou_dn),
        upn_suffix: Set(record.upn_suffix),
        employee_id_attribute: Set(record.employee_id_attribute),
        agent_key_hash: Set(record.agent_key_hash),
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
            agent_key_hash: model.agent_key_hash,
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
            member_employee_ids: serde_json::from_str(&model.member_employee_ids_json)?,
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
