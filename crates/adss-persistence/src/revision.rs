use sea_orm::{ConnectionTrait, DbBackend, EntityTrait, Statement};

use crate::entities;

pub(crate) const METADATA_KEY: &str = "current";
pub(crate) const MAX_REVISION_ALLOCATION_ATTEMPTS: usize = 5;

#[derive(Debug, Clone, Copy)]
pub(crate) enum SyncRevisionChannel {
    Directory,
    Credential,
}

pub(crate) async fn load_metadata<C>(
    connection: &C,
) -> anyhow::Result<entities::sync_metadata::Model>
where
    C: ConnectionTrait,
{
    use crate::entities::sync_metadata;

    sync_metadata::Entity::find_by_id(METADATA_KEY)
        .one(connection)
        .await?
        .ok_or_else(|| anyhow::anyhow!("sync metadata is not initialized"))
}

pub(crate) async fn try_allocate_directory_revision<C>(
    connection: &C,
) -> anyhow::Result<Option<i64>>
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

pub(crate) async fn try_allocate_credential_revision<C>(
    connection: &C,
) -> anyhow::Result<Option<i64>>
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

pub(crate) fn confirm_revision_update_sql(
    backend: DbBackend,
    channel: SyncRevisionChannel,
) -> String {
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

pub(crate) fn u64_to_i64_revision(value: u64) -> anyhow::Result<i64> {
    i64::try_from(value).map_err(|_| anyhow::anyhow!("revision exceeds i64::MAX"))
}

pub(crate) fn i64_to_u64_revision(value: i64) -> anyhow::Result<u64> {
    u64::try_from(value).map_err(|_| anyhow::anyhow!("negative revision in database"))
}
