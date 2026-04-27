use adss_contract::{AuditEvent, DesiredState, DomainConfig};
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
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StoreSnapshot {
    pub desired_state: DesiredState,
    pub domains: Vec<DomainConfig>,
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
}
