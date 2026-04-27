use adss_contract::{AuditEvent, DesiredState, DomainConfig, PasswordTask};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, Database, DatabaseConnection, EntityTrait,
    QueryFilter, QueryOrder, Set,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

mod entities {
    pub mod state_document {
        use sea_orm::entity::prelude::*;

        #[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
        #[sea_orm(table_name = "state_documents")]
        pub struct Model {
            #[sea_orm(primary_key, auto_increment = false)]
            pub key: String,
            pub value_json: String,
        }

        #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
        pub enum Relation {}

        impl ActiveModelBehavior for ActiveModel {}
    }

    pub mod audit_event {
        use sea_orm::entity::prelude::*;

        #[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
        #[sea_orm(table_name = "audit_events")]
        pub struct Model {
            #[sea_orm(primary_key, auto_increment = false)]
            pub sequence: i64,
            pub actor: String,
            pub action: String,
            pub target: String,
            pub result: String,
            pub detail_json: String,
        }

        #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
        pub enum Relation {}

        impl ActiveModelBehavior for ActiveModel {}
    }

    pub mod password_task {
        use sea_orm::entity::prelude::*;

        #[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
        #[sea_orm(table_name = "password_tasks")]
        pub struct Model {
            #[sea_orm(primary_key, auto_increment = false)]
            pub task_id: i64,
            pub domain_id: String,
            pub employee_id: String,
            pub encrypted_password: String,
        }

        #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
        pub enum Relation {}

        impl ActiveModelBehavior for ActiveModel {}
    }

    pub mod agent_cursor {
        use sea_orm::entity::prelude::*;

        #[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
        #[sea_orm(table_name = "agent_cursors")]
        pub struct Model {
            #[sea_orm(primary_key, auto_increment = false)]
            pub agent_id: String,
            pub domain_id: String,
            pub structure_version: i64,
            pub password_task_cursor: i64,
        }

        #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
        pub enum Relation {}

        impl ActiveModelBehavior for ActiveModel {}
    }

    pub mod drift_report {
        use sea_orm::entity::prelude::*;

        #[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
        #[sea_orm(table_name = "drift_reports")]
        pub struct Model {
            #[sea_orm(primary_key, auto_increment = false)]
            pub id: i64,
            pub domain_id: String,
            pub agent_id: String,
            pub observed_structure_version: i64,
            pub drifted_objects_json: String,
        }

        #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
        pub enum Relation {}

        impl ActiveModelBehavior for ActiveModel {}
    }

    pub mod registration_token {
        use sea_orm::entity::prelude::*;

        #[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
        #[sea_orm(table_name = "registration_tokens")]
        pub struct Model {
            #[sea_orm(primary_key, auto_increment = false)]
            pub token: String,
            pub domain_id: String,
        }

        #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
        pub enum Relation {}

        impl ActiveModelBehavior for ActiveModel {}
    }

    pub mod agent_credential {
        use sea_orm::entity::prelude::*;

        #[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
        #[sea_orm(table_name = "agent_credentials")]
        pub struct Model {
            #[sea_orm(primary_key, auto_increment = false)]
            pub agent_id: String,
            pub domain_id: String,
            pub agent_key: String,
        }

        #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
        pub enum Relation {}

        impl ActiveModelBehavior for ActiveModel {}
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StoreSnapshot {
    pub desired_state: DesiredState,
    pub domains: Vec<DomainConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentCursorRecord {
    pub agent_id: String,
    pub domain_id: String,
    pub structure_version: u64,
    pub password_task_cursor: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DriftReportRecord {
    pub id: u64,
    pub domain_id: String,
    pub agent_id: String,
    pub observed_structure_version: u64,
    pub drifted_objects: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentCredentialRecord {
    pub agent_id: String,
    pub domain_id: String,
    pub agent_key: String,
}

#[derive(Clone)]
pub struct OrmRepository {
    db: DatabaseConnection,
}

impl OrmRepository {
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
CREATE TABLE IF NOT EXISTS state_documents (
    key TEXT PRIMARY KEY NOT NULL,
    value_json TEXT NOT NULL
)
"#,
            )
            .await?;
        self.db
            .execute_unprepared(
                r#"
CREATE TABLE IF NOT EXISTS audit_events (
    sequence BIGINT PRIMARY KEY NOT NULL,
    actor TEXT NOT NULL,
    action TEXT NOT NULL,
    target TEXT NOT NULL,
    result TEXT NOT NULL,
    detail_json TEXT NOT NULL
)
"#,
            )
            .await?;
        self.db
            .execute_unprepared(
                r#"
CREATE TABLE IF NOT EXISTS password_tasks (
    task_id BIGINT PRIMARY KEY NOT NULL,
    domain_id TEXT NOT NULL,
    employee_id TEXT NOT NULL,
    encrypted_password TEXT NOT NULL
)
"#,
            )
            .await?;
        self.db
            .execute_unprepared(
                r#"
CREATE TABLE IF NOT EXISTS agent_cursors (
    agent_id TEXT PRIMARY KEY NOT NULL,
    domain_id TEXT NOT NULL,
    structure_version BIGINT NOT NULL,
    password_task_cursor BIGINT NOT NULL
)
"#,
            )
            .await?;
        self.db
            .execute_unprepared(
                r#"
CREATE TABLE IF NOT EXISTS drift_reports (
    id BIGINT PRIMARY KEY NOT NULL,
    domain_id TEXT NOT NULL,
    agent_id TEXT NOT NULL,
    observed_structure_version BIGINT NOT NULL,
    drifted_objects_json TEXT NOT NULL
)
"#,
            )
            .await?;
        self.db
            .execute_unprepared(
                r#"
CREATE TABLE IF NOT EXISTS registration_tokens (
    token TEXT PRIMARY KEY NOT NULL,
    domain_id TEXT NOT NULL
)
"#,
            )
            .await?;
        self.db
            .execute_unprepared(
                r#"
CREATE TABLE IF NOT EXISTS agent_credentials (
    agent_id TEXT PRIMARY KEY NOT NULL,
    domain_id TEXT NOT NULL,
    agent_key TEXT NOT NULL
)
"#,
            )
            .await?;
        Ok(())
    }

    pub async fn save_snapshot(&self, snapshot: &StoreSnapshot) -> anyhow::Result<()> {
        use entities::state_document;

        state_document::Entity::delete_by_id("store")
            .exec(&self.db)
            .await?;
        state_document::ActiveModel {
            key: Set("store".to_string()),
            value_json: Set(serde_json::to_string(snapshot)?),
        }
        .insert(&self.db)
        .await?;
        Ok(())
    }

    pub async fn load_snapshot(&self) -> anyhow::Result<Option<StoreSnapshot>> {
        use entities::state_document;

        let Some(model) = state_document::Entity::find_by_id("store")
            .one(&self.db)
            .await?
        else {
            return Ok(None);
        };
        Ok(Some(serde_json::from_str(&model.value_json)?))
    }

    pub async fn append_audit_event(&self, event: &AuditEvent) -> anyhow::Result<()> {
        use entities::audit_event;

        audit_event::ActiveModel {
            sequence: Set(event.sequence as i64),
            actor: Set(event.actor.clone()),
            action: Set(event.action.clone()),
            target: Set(event.target.clone()),
            result: Set(event.result.clone()),
            detail_json: Set(serde_json::to_string(&event.detail)?),
        }
        .insert(&self.db)
        .await?;
        Ok(())
    }

    pub async fn list_audit_events(&self) -> anyhow::Result<Vec<AuditEvent>> {
        use entities::audit_event;

        let models = audit_event::Entity::find()
            .order_by_asc(audit_event::Column::Sequence)
            .all(&self.db)
            .await?;
        models
            .into_iter()
            .map(|model| {
                Ok(AuditEvent {
                    sequence: model.sequence as u64,
                    actor: model.actor,
                    action: model.action,
                    target: model.target,
                    result: model.result,
                    detail: serde_json::from_str::<BTreeMap<String, String>>(&model.detail_json)?,
                })
            })
            .collect()
    }

    pub async fn list_audit_events_by_action(
        &self,
        action: &str,
    ) -> anyhow::Result<Vec<AuditEvent>> {
        use entities::audit_event;

        let models = audit_event::Entity::find()
            .filter(audit_event::Column::Action.eq(action))
            .order_by_asc(audit_event::Column::Sequence)
            .all(&self.db)
            .await?;
        models
            .into_iter()
            .map(|model| {
                Ok(AuditEvent {
                    sequence: model.sequence as u64,
                    actor: model.actor,
                    action: model.action,
                    target: model.target,
                    result: model.result,
                    detail: serde_json::from_str::<BTreeMap<String, String>>(&model.detail_json)?,
                })
            })
            .collect()
    }

    pub async fn append_password_task(
        &self,
        task_id: u64,
        domain_id: &str,
        employee_id: &str,
        encrypted_password: &str,
    ) -> anyhow::Result<()> {
        use entities::password_task;

        password_task::ActiveModel {
            task_id: Set(task_id as i64),
            domain_id: Set(domain_id.to_string()),
            employee_id: Set(employee_id.to_string()),
            encrypted_password: Set(encrypted_password.to_string()),
        }
        .insert(&self.db)
        .await?;
        Ok(())
    }

    pub async fn list_password_tasks_after(
        &self,
        domain_id: &str,
        cursor: u64,
    ) -> anyhow::Result<Vec<PasswordTask>> {
        use entities::password_task;

        let models = password_task::Entity::find()
            .filter(password_task::Column::DomainId.eq(domain_id))
            .filter(password_task::Column::TaskId.gt(cursor as i64))
            .order_by_asc(password_task::Column::TaskId)
            .all(&self.db)
            .await?;
        Ok(models
            .into_iter()
            .map(|model| PasswordTask {
                task_id: model.task_id as u64,
                domain_id: model.domain_id,
                employee_id: model.employee_id,
                encrypted_password: model.encrypted_password,
            })
            .collect())
    }

    pub async fn upsert_agent_cursor(&self, cursor: &AgentCursorRecord) -> anyhow::Result<()> {
        use entities::agent_cursor;

        agent_cursor::Entity::delete_by_id(cursor.agent_id.as_str())
            .exec(&self.db)
            .await?;
        agent_cursor::ActiveModel {
            agent_id: Set(cursor.agent_id.clone()),
            domain_id: Set(cursor.domain_id.clone()),
            structure_version: Set(cursor.structure_version as i64),
            password_task_cursor: Set(cursor.password_task_cursor as i64),
        }
        .insert(&self.db)
        .await?;
        Ok(())
    }

    pub async fn load_agent_cursor(
        &self,
        agent_id: &str,
    ) -> anyhow::Result<Option<AgentCursorRecord>> {
        use entities::agent_cursor;

        let Some(model) = agent_cursor::Entity::find_by_id(agent_id)
            .one(&self.db)
            .await?
        else {
            return Ok(None);
        };

        Ok(Some(AgentCursorRecord {
            agent_id: model.agent_id,
            domain_id: model.domain_id,
            structure_version: model.structure_version as u64,
            password_task_cursor: model.password_task_cursor as u64,
        }))
    }

    pub async fn append_drift_report(&self, report: &DriftReportRecord) -> anyhow::Result<()> {
        use entities::drift_report;

        drift_report::ActiveModel {
            id: Set(report.id as i64),
            domain_id: Set(report.domain_id.clone()),
            agent_id: Set(report.agent_id.clone()),
            observed_structure_version: Set(report.observed_structure_version as i64),
            drifted_objects_json: Set(serde_json::to_string(&report.drifted_objects)?),
        }
        .insert(&self.db)
        .await?;
        Ok(())
    }

    pub async fn list_drift_reports(
        &self,
        domain_id: &str,
    ) -> anyhow::Result<Vec<DriftReportRecord>> {
        use entities::drift_report;

        let models = drift_report::Entity::find()
            .filter(drift_report::Column::DomainId.eq(domain_id))
            .order_by_asc(drift_report::Column::Id)
            .all(&self.db)
            .await?;
        models
            .into_iter()
            .map(|model| {
                Ok(DriftReportRecord {
                    id: model.id as u64,
                    domain_id: model.domain_id,
                    agent_id: model.agent_id,
                    observed_structure_version: model.observed_structure_version as u64,
                    drifted_objects: serde_json::from_str::<Vec<String>>(
                        &model.drifted_objects_json,
                    )?,
                })
            })
            .collect()
    }

    pub async fn insert_registration_token(
        &self,
        token: &str,
        domain_id: &str,
    ) -> anyhow::Result<()> {
        use entities::registration_token;

        registration_token::Entity::delete_by_id(token)
            .exec(&self.db)
            .await?;
        registration_token::ActiveModel {
            token: Set(token.to_string()),
            domain_id: Set(domain_id.to_string()),
        }
        .insert(&self.db)
        .await?;
        Ok(())
    }

    pub async fn consume_registration_token(&self, token: &str) -> anyhow::Result<Option<String>> {
        use entities::registration_token;

        let Some(model) = registration_token::Entity::find_by_id(token)
            .one(&self.db)
            .await?
        else {
            return Ok(None);
        };
        registration_token::Entity::delete_by_id(token)
            .exec(&self.db)
            .await?;
        Ok(Some(model.domain_id))
    }

    pub async fn upsert_agent_credential(
        &self,
        credential: &AgentCredentialRecord,
    ) -> anyhow::Result<()> {
        use entities::agent_credential;

        agent_credential::Entity::delete_by_id(credential.agent_id.as_str())
            .exec(&self.db)
            .await?;
        agent_credential::ActiveModel {
            agent_id: Set(credential.agent_id.clone()),
            domain_id: Set(credential.domain_id.clone()),
            agent_key: Set(credential.agent_key.clone()),
        }
        .insert(&self.db)
        .await?;
        Ok(())
    }

    pub async fn load_agent_credential(
        &self,
        agent_id: &str,
    ) -> anyhow::Result<Option<AgentCredentialRecord>> {
        use entities::agent_credential;

        let Some(model) = agent_credential::Entity::find_by_id(agent_id)
            .one(&self.db)
            .await?
        else {
            return Ok(None);
        };

        Ok(Some(AgentCredentialRecord {
            agent_id: model.agent_id,
            domain_id: model.domain_id,
            agent_key: model.agent_key,
        }))
    }

    pub async fn list_agent_credentials(&self) -> anyhow::Result<Vec<AgentCredentialRecord>> {
        use entities::agent_credential;

        let models = agent_credential::Entity::find()
            .order_by_asc(agent_credential::Column::AgentId)
            .all(&self.db)
            .await?;
        Ok(models
            .into_iter()
            .map(|model| AgentCredentialRecord {
                agent_id: model.agent_id,
                domain_id: model.domain_id,
                agent_key: model.agent_key,
            })
            .collect())
    }
}
