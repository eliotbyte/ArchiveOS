use archiveos_contract::{SubscriptionOptions, VaultError, YtdlpJobInput};
use chrono::{Duration, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceSubscription {
    pub id: Uuid,
    pub source: String,
    pub kind: String,
    pub url: String,
    pub target_vault: String,
    pub interval_minutes: i32,
    pub next_run_at: String,
    pub last_checked_at: Option<String>,
    pub status: String,
    pub created_at: String,
    pub options: Option<SubscriptionOptions>,
}

#[derive(Debug, Clone)]
pub struct CreateSubscriptionInput {
    pub source: String,
    pub kind: String,
    pub url: String,
    pub target_vault: String,
    pub interval_minutes: i32,
    pub options: Option<SubscriptionOptions>,
}

pub fn create_subscription(
    conn: &Connection,
    input: &CreateSubscriptionInput,
) -> Result<SourceSubscription, VaultError> {
    let id = Uuid::new_v4();
    let now = Utc::now();
    let created_at = now.to_rfc3339();
    let next_run_at = now.to_rfc3339();

    let options_json = input
        .options
        .as_ref()
        .map(serde_json::to_string)
        .transpose()?;

    conn.execute(
        "INSERT INTO source_subscription (
            id, source, kind, url, target_vault, interval_minutes,
            next_run_at, last_checked_at, status, created_at, options_json
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, 'active', ?8, ?9)",
        params![
            id.to_string(),
            input.source,
            input.kind,
            input.url,
            input.target_vault,
            input.interval_minutes,
            next_run_at,
            created_at,
            options_json,
        ],
    )
    .map_err(db_err)?;

    get_subscription(conn, id)?.ok_or(VaultError::NotFound)
}

pub fn list_subscriptions(conn: &Connection) -> Result<Vec<SourceSubscription>, VaultError> {
    let mut stmt = conn
        .prepare(
            "SELECT id, source, kind, url, target_vault, interval_minutes,
                    next_run_at, last_checked_at, status, created_at, options_json
             FROM source_subscription
             ORDER BY created_at DESC",
        )
        .map_err(db_err)?;
    let rows = stmt.query_map([], map_subscription).map_err(db_err)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(db_err)
}

pub fn delete_subscription(conn: &Connection, id: Uuid) -> Result<(), VaultError> {
    let changed = conn
        .execute("DELETE FROM source_subscription WHERE id = ?1", [id.to_string()])
        .map_err(db_err)?;
    if changed == 0 {
        return Err(VaultError::NotFound);
    }
    Ok(())
}

pub fn subscription_exists_for_url(conn: &Connection, url: &str) -> Result<bool, VaultError> {
    let exists: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM source_subscription WHERE url = ?1",
            [url],
            |row| row.get(0),
        )
        .map_err(db_err)?;
    Ok(exists > 0)
}

pub fn due_subscriptions(conn: &Connection) -> Result<Vec<SourceSubscription>, VaultError> {
    let now = Utc::now().to_rfc3339();
    let mut stmt = conn
        .prepare(
            "SELECT id, source, kind, url, target_vault, interval_minutes,
                    next_run_at, last_checked_at, status, created_at, options_json
             FROM source_subscription
             WHERE status = 'active' AND next_run_at <= ?1
             ORDER BY next_run_at ASC",
        )
        .map_err(db_err)?;
    let rows = stmt
        .query_map([now], map_subscription)
        .map_err(db_err)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(db_err)
}

pub fn mark_subscription_checked(
    conn: &Connection,
    id: Uuid,
    interval_minutes: i32,
) -> Result<(), VaultError> {
    let now = Utc::now();
    let next = (now + Duration::minutes(i64::from(interval_minutes))).to_rfc3339();
    let changed = conn
        .execute(
            "UPDATE source_subscription
             SET last_checked_at = ?1, next_run_at = ?2
             WHERE id = ?3",
            params![now.to_rfc3339(), next, id.to_string()],
        )
        .map_err(db_err)?;
    if changed == 0 {
        return Err(VaultError::NotFound);
    }
    Ok(())
}

pub fn get_subscription(conn: &Connection, id: Uuid) -> Result<Option<SourceSubscription>, VaultError> {
    conn.query_row(
        "SELECT id, source, kind, url, target_vault, interval_minutes,
                next_run_at, last_checked_at, status, created_at, options_json
         FROM source_subscription WHERE id = ?1",
        [id.to_string()],
        map_subscription,
    )
    .optional()
    .map_err(db_err)
}

fn map_subscription(row: &rusqlite::Row<'_>) -> rusqlite::Result<SourceSubscription> {
    Ok(SourceSubscription {
        id: Uuid::parse_str(&row.get::<_, String>(0)?).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
        })?,
        source: row.get(1)?,
        kind: row.get(2)?,
        url: row.get(3)?,
        target_vault: row.get(4)?,
        interval_minutes: row.get(5)?,
        next_run_at: row.get(6)?,
        last_checked_at: row.get(7)?,
        status: row.get(8)?,
        created_at: row.get(9)?,
        options: parse_options(row.get(10)?)?,
    })
}

fn parse_options(raw: Option<String>) -> rusqlite::Result<Option<SubscriptionOptions>> {
    match raw {
        Some(json) if !json.is_empty() => serde_json::from_str(&json).map_err(|err| {
            rusqlite::Error::FromSqlConversionFailure(
                10,
                rusqlite::types::Type::Text,
                Box::new(err),
            )
        }),
        _ => Ok(None),
    }
}

pub fn subscription_job_input(sub: &SourceSubscription) -> Result<String, VaultError> {
    let options = sub.options.clone().unwrap_or_default();
    YtdlpJobInput {
        url: sub.url.clone(),
        mode: "subscription".into(),
        resync: options.resync,
        removed_items: options.removed_items,
        asset_policy: options.asset_policy,
    }
    .to_json()
}

pub fn run_subscription_now(
    conn: &Connection,
    id: Uuid,
    target_vault: &str,
) -> Result<crate::jobs::Job, VaultError> {
    let sub = get_subscription(conn, id)?.ok_or(VaultError::NotFound)?;
    let input = subscription_job_input(&sub)?;
    crate::jobs::create_job(conn, "yt-dlp", target_vault, &input)
}

#[derive(Debug, Clone)]
pub struct EnrichedSubscription {
    pub subscription: SourceSubscription,
    pub collection_id: Option<Uuid>,
    pub collection_title: Option<String>,
}

pub fn enrich_subscriptions(
    conn: &Connection,
    subs: Vec<SourceSubscription>,
) -> Result<Vec<EnrichedSubscription>, VaultError> {
    let mut out = Vec::with_capacity(subs.len());
    for sub in subs {
        let collection_id = sub
            .options
            .as_ref()
            .and_then(|o| o.collection_entity_id.as_ref())
            .and_then(|id| Uuid::parse_str(id).ok())
            .or_else(|| {
                crate::import::db::find_entity_id_by_source_url(conn, &sub.url)
                    .ok()
                    .flatten()
            });
        let collection_title = collection_id.and_then(|id| {
            conn.query_row(
                "SELECT title FROM collection WHERE id = ?1",
                [id.to_string()],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .ok()
            .flatten()
        });
        out.push(EnrichedSubscription {
            subscription: sub,
            collection_id,
            collection_title,
        });
    }
    Ok(out)
}

pub fn bind_subscription_collection(
    conn: &Connection,
    subscription_url: &str,
    collection_entity_id: Uuid,
) -> Result<(), VaultError> {
    let mut subs = list_subscriptions(conn)?;
    subs.retain(|s| s.url == subscription_url);
    for sub in subs {
        let mut options = sub.options.clone().unwrap_or_default();
        options.collection_entity_id = Some(collection_entity_id.to_string());
        let options_json = serde_json::to_string(&options).map_err(VaultError::from)?;
        conn.execute(
            "UPDATE source_subscription SET options_json = ?1 WHERE id = ?2",
            params![options_json, sub.id.to_string()],
        )
        .map_err(db_err)?;
    }
    Ok(())
}

fn db_err(err: rusqlite::Error) -> VaultError {
    VaultError::Database {
        detail: err.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Vault;
    use tempfile::tempdir;

    #[test]
    fn create_and_list_subscription() {
        let dir = tempdir().unwrap();
        let vault = Vault::init(dir.path()).unwrap();
        let conn = vault.connection();

        let sub = create_subscription(
            conn,
            &CreateSubscriptionInput {
                source: "youtube".into(),
                kind: "playlist".into(),
                url: "https://youtube.com/playlist?list=PLtest".into(),
                target_vault: "archiveos".into(),
                interval_minutes: 60,
                options: None,
            },
        )
        .unwrap();
        assert_eq!(sub.status, "active");

        let listed = list_subscriptions(conn).unwrap();
        assert_eq!(listed.len(), 1);
    }

    #[test]
    fn due_subscription_advances_next_run() {
        let dir = tempdir().unwrap();
        let vault = Vault::init(dir.path()).unwrap();
        let conn = vault.connection();

        let sub = create_subscription(
            conn,
            &CreateSubscriptionInput {
                source: "youtube".into(),
                kind: "playlist".into(),
                url: "https://youtube.com/playlist?list=PLtest".into(),
                target_vault: "archiveos".into(),
                interval_minutes: 30,
                options: None,
            },
        )
        .unwrap();

        let due = due_subscriptions(conn).unwrap();
        assert_eq!(due.len(), 1);

        mark_subscription_checked(conn, sub.id, sub.interval_minutes).unwrap();
        let due_after = due_subscriptions(conn).unwrap();
        assert!(due_after.is_empty());
    }

    #[test]
    fn subscription_job_input_uses_structured_json() {
        let dir = tempdir().unwrap();
        let vault = Vault::init(dir.path()).unwrap();
        let conn = vault.connection();
        let sub = create_subscription(
            conn,
            &CreateSubscriptionInput {
                source: "youtube".into(),
                kind: "playlist".into(),
                url: "https://youtube.com/playlist?list=PLtest".into(),
                target_vault: "archiveos".into(),
                interval_minutes: 30,
                options: Some(SubscriptionOptions {
                    resync: true,
                    removed_items: "mark_removed".into(),
                    asset_policy: archiveos_contract::AssetPolicy {
                        video: "best_1080p".into(),
                        ..Default::default()
                    },
                    ..Default::default()
                }),
            },
        )
        .unwrap();
        let raw = subscription_job_input(&sub).unwrap();
        assert!(raw.contains("subscription"));
        assert!(raw.contains("best_1080p"));
    }
}
