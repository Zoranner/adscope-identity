use adscope_protocol::{DirectoryBatch, Group, OrganizationalUnit, User, UserStatus};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, Database, DatabaseConnection, DbErr,
    EntityTrait, QueryFilter, QueryOrder, QuerySelect, Set, SqlErr, Statement, TransactionTrait,
};
use std::collections::{BTreeMap, BTreeSet};

use crate::{
    entities,
    mapping::{domain_active_model, user_status_from_storage, user_status_to_storage},
    models::{
        AuthorizationCodeExchange, AuthorizationCodeRecord, CredentialCiphertextBatch,
        CredentialCiphertextEntry, CredentialRecord, DomainPatch, DomainRecord, OAuthClientRecord,
        UserContactPatch, UserCreateInput, UserCredentialInput, UserDirectoryPatch, UserListFilter,
    },
    oauth::{authorization_code_active_model, oauth_client_active_model},
    revision::{
        MAX_REVISION_ALLOCATION_ATTEMPTS, METADATA_KEY, SyncRevisionChannel,
        confirm_revision_update_sql, i64_to_u64_revision, load_metadata,
        try_allocate_credential_revision, try_allocate_directory_revision, u64_to_i64_revision,
    },
};

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

    pub async fn ping(&self) -> anyhow::Result<()> {
        self.db.execute_unprepared("SELECT 1").await?;
        Ok(())
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
CREATE UNIQUE INDEX IF NOT EXISTS users_username_unique ON users(username)
"#,
            )
            .await?;
        self.db
            .execute_unprepared(
                r#"
CREATE TABLE IF NOT EXISTS groups (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    organizational_unit_id TEXT NOT NULL,
    member_employee_ids TEXT NOT NULL,
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
    managed_group_id_attribute TEXT NOT NULL,
    connector_key_hash TEXT NOT NULL,
    applied_directory_revision BIGINT NOT NULL CHECK (applied_directory_revision >= 0),
    applied_credential_revision BIGINT NOT NULL CHECK (applied_credential_revision >= 0)
)
"#,
            )
            .await?;
        self.db
            .execute_unprepared(
                r#"
CREATE TABLE IF NOT EXISTS oauth_clients (
    client_id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    client_type TEXT NOT NULL,
    client_secret_hash TEXT NULL,
    redirect_uris TEXT NOT NULL,
    allowed_scopes TEXT NOT NULL,
    enabled BOOLEAN NOT NULL
)
"#,
            )
            .await?;
        self.db
            .execute_unprepared(
                r#"
CREATE TABLE IF NOT EXISTS oauth_authorization_codes (
    code_hash TEXT PRIMARY KEY NOT NULL,
    client_id TEXT NOT NULL,
    employee_id TEXT NOT NULL,
    redirect_uri TEXT NOT NULL,
    scopes TEXT NOT NULL,
    nonce TEXT NOT NULL,
    code_challenge TEXT NOT NULL,
    auth_time BIGINT NOT NULL,
    expires_at BIGINT NOT NULL
)
"#,
            )
            .await?;
        self.ensure_metadata().await?;
        Ok(())
    }

    pub async fn list_oauth_clients(&self) -> anyhow::Result<Vec<OAuthClientRecord>> {
        use entities::oauth_client;

        oauth_client::Entity::find()
            .order_by_asc(oauth_client::Column::ClientId)
            .all(&self.db)
            .await?
            .into_iter()
            .map(OAuthClientRecord::try_from)
            .collect()
    }

    pub async fn get_oauth_client(
        &self,
        client_id: &str,
    ) -> anyhow::Result<Option<OAuthClientRecord>> {
        use entities::oauth_client;

        oauth_client::Entity::find_by_id(client_id)
            .one(&self.db)
            .await?
            .map(OAuthClientRecord::try_from)
            .transpose()
    }

    pub async fn create_oauth_client(
        &self,
        client: OAuthClientRecord,
    ) -> anyhow::Result<Option<OAuthClientRecord>> {
        let client = oauth_client_active_model(client)?;
        match client.insert(&self.db).await {
            Ok(client) => Ok(Some(OAuthClientRecord::try_from(client)?)),
            Err(error) if matches!(error.sql_err(), Some(SqlErr::UniqueConstraintViolation(_))) => {
                Ok(None)
            }
            Err(error) => Err(error.into()),
        }
    }

    pub async fn update_oauth_client(
        &self,
        client: OAuthClientRecord,
    ) -> anyhow::Result<Option<OAuthClientRecord>> {
        match oauth_client_active_model(client)?.update(&self.db).await {
            Ok(client) => Ok(Some(OAuthClientRecord::try_from(client)?)),
            Err(DbErr::RecordNotUpdated) => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    pub async fn delete_oauth_client(&self, client_id: &str) -> anyhow::Result<bool> {
        use entities::oauth_client;

        Ok(oauth_client::Entity::delete_by_id(client_id)
            .exec(&self.db)
            .await?
            .rows_affected
            == 1)
    }

    pub async fn store_authorization_code(
        &self,
        record: AuthorizationCodeRecord,
    ) -> anyhow::Result<()> {
        authorization_code_active_model(record)?
            .insert(&self.db)
            .await?;
        Ok(())
    }

    pub async fn consume_authorization_code(
        &self,
        code_hash: &str,
        now: i64,
    ) -> anyhow::Result<Option<AuthorizationCodeRecord>> {
        use entities::oauth_authorization_code;

        let Some(code) = oauth_authorization_code::Entity::delete_by_id(code_hash)
            .exec_with_returning(&self.db)
            .await?
        else {
            return Ok(None);
        };
        if code.expires_at <= now {
            return Ok(None);
        }
        Ok(Some(AuthorizationCodeRecord::try_from(code)?))
    }

    pub async fn consume_authorization_code_for_exchange(
        &self,
        code_hash: &str,
        now: i64,
    ) -> anyhow::Result<Option<AuthorizationCodeExchange>> {
        use entities::{oauth_authorization_code, oauth_client, user};

        let transaction = self.db.begin().await?;
        let Some(code) = oauth_authorization_code::Entity::delete_by_id(code_hash)
            .exec_with_returning(&transaction)
            .await?
        else {
            transaction.commit().await?;
            return Ok(None);
        };
        if code.expires_at <= now {
            transaction.commit().await?;
            return Ok(None);
        }

        let client_query = oauth_client::Entity::find_by_id(code.client_id.as_str());
        let client = if transaction.get_database_backend() == sea_orm::DatabaseBackend::Postgres {
            client_query.lock_exclusive().one(&transaction).await?
        } else {
            client_query.one(&transaction).await?
        };
        let user_query = user::Entity::find_by_id(code.employee_id.as_str());
        let user = if transaction.get_database_backend() == sea_orm::DatabaseBackend::Postgres {
            user_query.lock_exclusive().one(&transaction).await?
        } else {
            user_query.one(&transaction).await?
        };
        transaction.commit().await?;

        Ok(Some(AuthorizationCodeExchange {
            code: AuthorizationCodeRecord::try_from(code)?,
            client: client.map(OAuthClientRecord::try_from).transpose()?,
            user: user.map(User::try_from).transpose()?,
        }))
    }

    pub async fn delete_expired_authorization_codes(
        &self,
        now: i64,
        limit: u64,
    ) -> anyhow::Result<u64> {
        use entities::oauth_authorization_code;

        if limit == 0 {
            return Ok(0);
        }
        let code_hashes = oauth_authorization_code::Entity::find()
            .filter(oauth_authorization_code::Column::ExpiresAt.lte(now))
            .order_by_asc(oauth_authorization_code::Column::ExpiresAt)
            .order_by_asc(oauth_authorization_code::Column::CodeHash)
            .limit(limit)
            .all(&self.db)
            .await?
            .into_iter()
            .map(|code| code.code_hash)
            .collect::<Vec<_>>();
        if code_hashes.is_empty() {
            return Ok(0);
        }
        Ok(oauth_authorization_code::Entity::delete_many()
            .filter(oauth_authorization_code::Column::CodeHash.is_in(code_hashes))
            .exec(&self.db)
            .await?
            .rows_affected)
    }

    pub async fn list_domains(&self) -> anyhow::Result<Vec<DomainRecord>> {
        use entities::domain;

        domain::Entity::find()
            .order_by_asc(domain::Column::Id)
            .all(&self.db)
            .await?
            .into_iter()
            .map(DomainRecord::try_from)
            .collect()
    }

    pub async fn upsert_domain(&self, domain: DomainRecord) -> anyhow::Result<DomainRecord> {
        use entities::domain;

        let transaction = self.db.begin().await?;
        domain::Entity::delete_by_id(domain.id.as_str())
            .exec(&transaction)
            .await?;
        let domain = domain_active_model(domain)?.insert(&transaction).await?;
        transaction.commit().await?;
        DomainRecord::try_from(domain)
    }

    pub async fn create_domain(
        &self,
        domain: DomainRecord,
    ) -> anyhow::Result<Option<DomainRecord>> {
        let domain = domain_active_model(domain)?;
        match domain.insert(&self.db).await {
            Ok(domain) => Ok(Some(DomainRecord::try_from(domain)?)),
            Err(error) if matches!(error.sql_err(), Some(SqlErr::UniqueConstraintViolation(_))) => {
                Ok(None)
            }
            Err(error) => Err(error.into()),
        }
    }

    pub async fn update_domain(
        &self,
        domain_id: &str,
        patch: DomainPatch,
    ) -> anyhow::Result<Option<DomainRecord>> {
        use entities::domain;

        let mut domain = domain::ActiveModel {
            id: Set(domain_id.to_string()),
            ..Default::default()
        };

        if let Some(name) = patch.name {
            domain.name = Set(name);
        }
        if let Some(enabled) = patch.enabled {
            domain.enabled = Set(enabled);
        }
        if let Some(mirror_root_dn) = patch.mirror_root_dn {
            domain.mirror_root_dn = Set(mirror_root_dn);
        }
        if let Some(quarantine_ou_dn) = patch.quarantine_ou_dn {
            domain.quarantine_ou_dn = Set(quarantine_ou_dn);
        }
        if let Some(upn_suffix) = patch.upn_suffix {
            domain.upn_suffix = Set(upn_suffix);
        }
        if let Some(employee_id_attribute) = patch.employee_id_attribute {
            domain.employee_id_attribute = Set(employee_id_attribute);
        }
        if let Some(managed_group_id_attribute) = patch.managed_group_id_attribute {
            domain.managed_group_id_attribute = Set(managed_group_id_attribute);
        }
        if let Some(connector_key_hash) = patch.connector_key_hash {
            domain.connector_key_hash = Set(connector_key_hash);
        }

        match domain.update(&self.db).await {
            Ok(domain) => Ok(Some(DomainRecord::try_from(domain)?)),
            Err(DbErr::RecordNotUpdated) => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    pub async fn seed_domain(&self, domain: DomainRecord) -> anyhow::Result<()> {
        self.upsert_domain(domain).await.map(|_| ())
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
                    organizational_unit_id: Set(group.organizational_unit_id.clone()),
                    member_employee_ids: Set(serde_json::to_string(&group.member_employee_ids)?),
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

    pub async fn list_organizational_units(&self) -> anyhow::Result<Vec<OrganizationalUnit>> {
        use entities::organizational_unit;

        organizational_unit::Entity::find()
            .order_by_asc(organizational_unit::Column::Id)
            .all(&self.db)
            .await?
            .into_iter()
            .map(OrganizationalUnit::try_from)
            .collect()
    }

    pub async fn get_organizational_unit(
        &self,
        ou_id: &str,
    ) -> anyhow::Result<Option<OrganizationalUnit>> {
        use entities::organizational_unit;

        organizational_unit::Entity::find_by_id(ou_id)
            .one(&self.db)
            .await?
            .map(OrganizationalUnit::try_from)
            .transpose()
    }

    pub async fn list_users(&self, filter: UserListFilter) -> anyhow::Result<Vec<User>> {
        use entities::user;

        let mut query = user::Entity::find();
        if let Some(employee_id) = filter.employee_id {
            query = query.filter(user::Column::EmployeeId.eq(employee_id));
        }
        if let Some(username) = filter.username {
            query = query.filter(user::Column::Username.eq(username));
        }
        if let Some(organizational_unit_id) = filter.organizational_unit_id {
            query = query.filter(user::Column::OrganizationalUnitId.eq(organizational_unit_id));
        }
        if let Some(status) = filter.status {
            query = query.filter(user::Column::Status.eq(user_status_to_storage(status)));
        }

        query
            .order_by_asc(user::Column::EmployeeId)
            .all(&self.db)
            .await?
            .into_iter()
            .map(User::try_from)
            .collect()
    }

    pub async fn get_user(&self, employee_id: &str) -> anyhow::Result<Option<User>> {
        use entities::user;

        user::Entity::find_by_id(employee_id)
            .one(&self.db)
            .await?
            .map(User::try_from)
            .transpose()
    }

    pub async fn get_user_by_username(&self, username: &str) -> anyhow::Result<Option<User>> {
        use entities::user;

        user::Entity::find()
            .filter(user::Column::Username.eq(username))
            .one(&self.db)
            .await?
            .map(User::try_from)
            .transpose()
    }

    pub async fn create_user_with_initial_credential(
        &self,
        input: UserCreateInput,
    ) -> anyhow::Result<Option<(User, u64, u64)>> {
        use entities::{user, user_credential};

        let UserCreateInput {
            directory,
            credential,
        } = input;
        if directory.employee_id != credential.employee_id {
            anyhow::bail!("user directory and credential employee IDs must match");
        }

        for _ in 0..MAX_REVISION_ALLOCATION_ATTEMPTS {
            let transaction = self.db.begin().await?;
            let directory_revision = match try_allocate_directory_revision(&transaction).await {
                Ok(Some(revision)) => revision,
                Ok(None) => {
                    transaction.rollback().await?;
                    continue;
                }
                Err(error) => {
                    transaction.rollback().await?;
                    return Err(error);
                }
            };

            let created = match (user::ActiveModel {
                employee_id: Set(directory.employee_id.clone()),
                username: Set(directory.username.clone()),
                display_name: Set(directory.display_name.clone()),
                email: Set(directory.email.clone()),
                mobile: Set(directory.mobile.clone()),
                telephone: Set(directory.telephone.clone()),
                organizational_unit_id: Set(directory.organizational_unit_id.clone()),
                status: Set(user_status_to_storage(directory.status).to_string()),
                changed_revision: Set(directory_revision),
            })
            .insert(&transaction)
            .await
            {
                Ok(user) => user,
                Err(error)
                    if matches!(error.sql_err(), Some(SqlErr::UniqueConstraintViolation(_))) =>
                {
                    transaction.rollback().await?;
                    return Ok(None);
                }
                Err(error) => {
                    transaction.rollback().await?;
                    return Err(error.into());
                }
            };

            let credential_revision = match try_allocate_credential_revision(&transaction).await {
                Ok(Some(revision)) => revision,
                Ok(None) => {
                    transaction.rollback().await?;
                    continue;
                }
                Err(error) => {
                    transaction.rollback().await?;
                    return Err(error);
                }
            };

            if let Err(error) = (user_credential::ActiveModel {
                employee_id: Set(credential.employee_id.clone()),
                password_ciphertext: Set(credential.password_ciphertext.clone()),
                password_verifier: Set(credential.password_verifier.clone()),
                changed_revision: Set(credential_revision),
            }
            .insert(&transaction)
            .await)
            {
                transaction.rollback().await?;
                return Err(error.into());
            }

            let user = match User::try_from(created) {
                Ok(user) => user,
                Err(error) => {
                    transaction.rollback().await?;
                    return Err(error);
                }
            };
            let directory_revision = match i64_to_u64_revision(directory_revision) {
                Ok(revision) => revision,
                Err(error) => {
                    transaction.rollback().await?;
                    return Err(error);
                }
            };
            let credential_revision = match i64_to_u64_revision(credential_revision) {
                Ok(revision) => revision,
                Err(error) => {
                    transaction.rollback().await?;
                    return Err(error);
                }
            };

            transaction.commit().await?;
            return Ok(Some((user, directory_revision, credential_revision)));
        }

        anyhow::bail!("failed to allocate revisions after retries")
    }

    pub async fn update_user_contact(
        &self,
        employee_id: &str,
        contact: UserContactPatch,
    ) -> anyhow::Result<(User, u64)> {
        use entities::user;

        for _ in 0..MAX_REVISION_ALLOCATION_ATTEMPTS {
            let transaction = self.db.begin().await?;
            let Some(revision) = try_allocate_directory_revision(&transaction).await? else {
                transaction.rollback().await?;
                continue;
            };
            let Some(existing) = user::Entity::find_by_id(employee_id)
                .one(&transaction)
                .await?
            else {
                transaction.rollback().await?;
                anyhow::bail!("unknown user: {employee_id}");
            };

            user::Entity::delete_by_id(employee_id)
                .exec(&transaction)
                .await?;
            let updated = user::ActiveModel {
                employee_id: Set(existing.employee_id),
                username: Set(existing.username),
                display_name: Set(existing.display_name),
                email: Set(contact.email),
                mobile: Set(contact.mobile),
                telephone: Set(contact.telephone),
                organizational_unit_id: Set(existing.organizational_unit_id),
                status: Set(existing.status),
                changed_revision: Set(revision),
            }
            .insert(&transaction)
            .await?;

            transaction.commit().await?;
            return Ok((User::try_from(updated)?, i64_to_u64_revision(revision)?));
        }

        anyhow::bail!("failed to allocate directory revision after retries")
    }

    pub async fn list_directory_changed_after(
        &self,
        domain_id: &str,
        applied_revision: u64,
        rebuild: bool,
        limit: usize,
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

        let mut revisions = organizational_unit::Entity::find()
            .select_only()
            .column(organizational_unit::Column::ChangedRevision)
            .filter(organizational_unit::Column::ChangedRevision.gt(threshold))
            .into_tuple::<i64>()
            .all(&self.db)
            .await?;
        revisions.extend(
            user::Entity::find()
                .select_only()
                .column(user::Column::ChangedRevision)
                .filter(user::Column::ChangedRevision.gt(threshold))
                .into_tuple::<i64>()
                .all(&self.db)
                .await?,
        );
        revisions.extend(
            group::Entity::find()
                .select_only()
                .column(group::Column::ChangedRevision)
                .filter(group::Column::ChangedRevision.gt(threshold))
                .into_tuple::<i64>()
                .all(&self.db)
                .await?,
        );
        revisions.sort_unstable();
        revisions.dedup();

        let limit = limit.max(1);
        let has_more = revisions.len() > limit;
        let batch_revision = revisions
            .get(limit - 1)
            .or_else(|| revisions.last())
            .copied()
            .map(i64_to_u64_revision)
            .transpose()?
            .unwrap_or(server_revision);
        let batch_revision = u64_to_i64_revision(batch_revision)?;

        let users = user::Entity::find()
            .filter(user::Column::ChangedRevision.gt(threshold))
            .filter(user::Column::ChangedRevision.lte(batch_revision))
            .order_by_asc(user::Column::ChangedRevision)
            .order_by_asc(user::Column::EmployeeId)
            .all(&self.db)
            .await?
            .into_iter()
            .map(User::try_from)
            .collect::<anyhow::Result<Vec<_>>>()?;
        let groups = group::Entity::find()
            .filter(group::Column::ChangedRevision.gt(threshold))
            .filter(group::Column::ChangedRevision.lte(batch_revision))
            .order_by_asc(group::Column::ChangedRevision)
            .order_by_asc(group::Column::Id)
            .all(&self.db)
            .await?
            .into_iter()
            .map(Group::try_from)
            .collect::<anyhow::Result<Vec<_>>>()?;
        let organizational_units = organizational_unit::Entity::find()
            .filter(organizational_unit::Column::ChangedRevision.gt(threshold))
            .filter(organizational_unit::Column::ChangedRevision.lte(batch_revision))
            .order_by_asc(organizational_unit::Column::ChangedRevision)
            .order_by_asc(organizational_unit::Column::Id)
            .all(&self.db)
            .await?
            .into_iter()
            .map(OrganizationalUnit::try_from)
            .collect::<anyhow::Result<Vec<_>>>()?;

        let organizational_unit_dns = self
            .organizational_unit_dns_for_batch(
                &domain.mirror_root_dn,
                &organizational_units,
                &users,
                &groups,
            )
            .await?;

        Ok(DirectoryBatch {
            server_revision,
            batch_revision: i64_to_u64_revision(batch_revision)?,
            organizational_units,
            organizational_unit_dns,
            users,
            groups,
            has_more,
        })
    }

    async fn organizational_unit_dns_for_batch(
        &self,
        mirror_root_dn: &str,
        organizational_units: &[OrganizationalUnit],
        users: &[User],
        groups: &[Group],
    ) -> anyhow::Result<BTreeMap<String, String>> {
        use entities::organizational_unit;

        let mut involved_ou_ids = BTreeSet::new();
        involved_ou_ids.extend(organizational_units.iter().map(|ou| ou.id.clone()));
        involved_ou_ids.extend(users.iter().map(|user| user.organizational_unit_id.clone()));
        involved_ou_ids.extend(
            groups
                .iter()
                .map(|group| group.organizational_unit_id.clone()),
        );

        let mut organizational_units_by_id: BTreeMap<String, OrganizationalUnit> = BTreeMap::new();
        let mut organizational_unit_dns: BTreeMap<String, String> = BTreeMap::new();
        for organizational_unit_id in involved_ou_ids {
            let mut chain = Vec::new();
            let mut current_id = organizational_unit_id;
            let mut visited = BTreeSet::new();

            loop {
                if organizational_unit_dns.contains_key(&current_id) {
                    break;
                }
                if !visited.insert(current_id.clone()) {
                    anyhow::bail!("cyclic OU hierarchy at {current_id}");
                }
                let organizational_unit = if let Some(organizational_unit) =
                    organizational_units_by_id.get(&current_id)
                {
                    organizational_unit.clone()
                } else {
                    let organizational_unit = organizational_unit::Entity::find_by_id(&current_id)
                        .one(&self.db)
                        .await?
                        .ok_or_else(|| {
                            anyhow::anyhow!("missing OU {current_id} required by directory batch")
                        })?;
                    let organizational_unit = OrganizationalUnit::try_from(organizational_unit)?;
                    organizational_units_by_id
                        .insert(current_id.clone(), organizational_unit.clone());
                    organizational_unit
                };
                current_id = match &organizational_unit.parent_id {
                    Some(parent_id) => parent_id.clone(),
                    None => {
                        chain.push(organizational_unit);
                        break;
                    }
                };
                chain.push(organizational_unit);
            }

            for organizational_unit in chain.into_iter().rev() {
                let parent_dn = match &organizational_unit.parent_id {
                    Some(parent_id) => organizational_unit_dns
                        .get(parent_id)
                        .ok_or_else(|| anyhow::anyhow!("missing parent OU DN {parent_id}"))?,
                    None => mirror_root_dn,
                };
                organizational_unit_dns.insert(
                    organizational_unit.id,
                    format!(
                        "OU={},{}",
                        escape_ldap_dn_value(&organizational_unit.name),
                        parent_dn
                    ),
                );
            }
        }

        Ok(organizational_unit_dns)
    }

    pub async fn list_groups(&self) -> anyhow::Result<Vec<Group>> {
        use entities::group;

        group::Entity::find()
            .order_by_asc(group::Column::Id)
            .all(&self.db)
            .await?
            .into_iter()
            .map(Group::try_from)
            .collect()
    }

    pub async fn get_group(&self, group_id: &str) -> anyhow::Result<Option<Group>> {
        use entities::group;

        group::Entity::find_by_id(group_id)
            .one(&self.db)
            .await?
            .map(Group::try_from)
            .transpose()
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
        use entities::{user, user_credential};

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

        let credential_models = user_credential::Entity::find()
            .filter(user_credential::Column::ChangedRevision.gt(threshold))
            .filter(
                user_credential::Column::ChangedRevision.lte(u64_to_i64_revision(batch_revision)?),
            )
            .order_by_asc(user_credential::Column::ChangedRevision)
            .order_by_asc(user_credential::Column::EmployeeId)
            .all(&self.db)
            .await?;
        let mut credentials = Vec::with_capacity(credential_models.len());
        for model in credential_models {
            let status = user::Entity::find_by_id(model.employee_id.as_str())
                .one(&self.db)
                .await?
                .map(|user| user_status_from_storage(&user.status))
                .transpose()?
                .unwrap_or(UserStatus::Active);
            credentials.push(CredentialCiphertextEntry {
                employee_id: model.employee_id,
                password_ciphertext: model.password_ciphertext,
                status,
                changed_revision: i64_to_u64_revision(model.changed_revision)?,
            });
        }

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

    pub async fn current_directory_revision(&self) -> anyhow::Result<u64> {
        i64_to_u64_revision(load_metadata(&self.db).await?.directory_revision)
    }

    pub async fn current_credential_revision(&self) -> anyhow::Result<u64> {
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

fn escape_ldap_dn_value(value: &str) -> String {
    let mut escaped = String::new();
    let mut chars = value.char_indices().peekable();

    while let Some((index, character)) = chars.next() {
        let is_first = index == 0;
        let is_last = chars.peek().is_none();
        if (is_first && (character == ' ' || character == '#'))
            || (is_last && character == ' ')
            || matches!(character, ',' | '+' | '"' | '\\' | '<' | '>' | ';' | '=')
        {
            escaped.push('\\');
        }
        escaped.push(character);
    }

    escaped
}
