pub(crate) mod sync_metadata {
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

pub(crate) mod organizational_unit {
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

pub(crate) mod user {
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

pub(crate) mod group {
    use sea_orm::entity::prelude::*;

    #[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
    #[sea_orm(table_name = "groups")]
    pub struct Model {
        #[sea_orm(primary_key, auto_increment = false)]
        pub id: String,
        pub name: String,
        pub organizational_unit_id: String,
        pub member_employee_ids: String,
        pub changed_revision: i64,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

pub(crate) mod user_credential {
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

pub(crate) mod domain {
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
        pub managed_group_id_attribute: String,
        pub connector_key_hash: String,
        pub applied_directory_revision: i64,
        pub applied_credential_revision: i64,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

pub(crate) mod oauth_client {
    use sea_orm::entity::prelude::*;
    use std::fmt;

    #[derive(Clone, PartialEq, DeriveEntityModel)]
    #[sea_orm(table_name = "oauth_clients")]
    pub struct Model {
        #[sea_orm(primary_key, auto_increment = false)]
        pub client_id: String,
        pub name: String,
        pub client_type: String,
        pub client_secret_hash: Option<String>,
        pub redirect_uris: String,
        pub allowed_scopes: String,
        pub enabled: bool,
    }

    impl fmt::Debug for Model {
        fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter
                .debug_struct("Model")
                .field("client_id", &self.client_id)
                .field("name", &self.name)
                .field("client_type", &self.client_type)
                .field(
                    "client_secret_hash_present",
                    &self.client_secret_hash.is_some(),
                )
                .field("redirect_uris", &self.redirect_uris)
                .field("allowed_scopes", &self.allowed_scopes)
                .field("enabled", &self.enabled)
                .finish()
        }
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

pub(crate) mod oauth_authorization_code {
    use sea_orm::entity::prelude::*;

    #[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
    #[sea_orm(table_name = "oauth_authorization_codes")]
    pub struct Model {
        #[sea_orm(primary_key, auto_increment = false)]
        pub code_hash: String,
        pub client_id: String,
        pub employee_id: String,
        pub redirect_uri: String,
        pub scopes: String,
        pub nonce: String,
        pub code_challenge: String,
        pub auth_time: i64,
        pub expires_at: i64,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}
