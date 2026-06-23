use std::path::Path;

use archiveos_contract::{
    BrowseQuery, BrowseSort, EntityPreviewSummary, LibraryListDetail, LibraryListKind,
    LibraryListMember, LibraryListQuery, LibraryListSummary, VaultError,
};
use rusqlite::{params, Connection};
use uuid::Uuid;

use crate::browse;
use crate::channels::resolve_uploaded_by_channel;
use crate::collections::{self, CollectionMemberItem};
use crate::entity::{
    resolve_entity_preview, resolve_entity_preview_at, resolve_timeline_manifest,
    resolve_timeline_sprite,
};
use crate::metadata;
use crate::playback::{list_playback_for_user, PlaybackRow};
use crate::user_lists::{
    count_user_list_members, ensure_watch_later_list, get_user_list, init_user_lists,
    list_user_list_entity_ids, list_user_lists, LIST_TYPE_WATCH_LATER,
};
use crate::playback::{default_user_id, ensure_default_user};

const SMART_RECENTLY_ADDED: &str = "smart:recently_added";
const SMART_CONTINUE_WATCHING: &str = "smart:continue_watching";
const SMART_RECENTLY_WATCHED: &str = "smart:recently_watched";
const USER_LIST_PREFIX: &str = "user:";
const SOURCE_LIST_PREFIX: &str = "source:";

pub fn list_library_lists(
    conn: &Connection,
    query: &LibraryListQuery,
) -> Result<Vec<LibraryListSummary>, VaultError> {
    init_user_lists(conn)?;
    let user_id = ensure_default_user(conn)?;
    let mut items = Vec::new();

    // Smart lists first
    let continue_count = count_continue_watching(conn, user_id, query)?;
    if continue_count > 0 {
        items.push(smart_summary(
            SMART_CONTINUE_WATCHING,
            "smart_continue_watching",
            "Continue Watching",
            continue_count,
            conn,
            user_id,
            query,
            "▶",
        )?);
    }

    let recent_count = count_recently_added(conn, query)?;
    if recent_count > 0 {
        items.push(smart_summary(
            SMART_RECENTLY_ADDED,
            "smart_recently_added",
            "Recently Added",
            recent_count,
            conn,
            user_id,
            query,
            "🕐",
        )?);
    }

    let watch_later_id = ensure_watch_later_list(conn, user_id)?;
    let watch_later_count = count_user_list_members(conn, watch_later_id)?;
    items.push(LibraryListSummary {
        id: format!("{USER_LIST_PREFIX}{watch_later_id}"),
        list_kind: LibraryListKind::User,
        list_type: LIST_TYPE_WATCH_LATER.into(),
        title: "Watch Later".into(),
        member_count: watch_later_count,
        cover_preview: cover_for_user_list(conn, watch_later_id)?,
        icon: Some("⏰".into()),
        overlay: true,
    });

    let recent_watched_count = count_recently_watched(conn, user_id, query)?;
    if recent_watched_count > 0 {
        items.push(smart_summary(
            SMART_RECENTLY_WATCHED,
            "smart_recently_watched",
            "Recently Watched",
            recent_watched_count,
            conn,
            user_id,
            query,
            "👁",
        )?);
    }

    // User playlists (excluding watch later, shown above)
    for list in list_user_lists(conn, user_id)? {
        if list.list_type == LIST_TYPE_WATCH_LATER {
            continue;
        }
        let member_count = count_user_list_members(conn, list.id)?;
        items.push(LibraryListSummary {
            id: format!("{USER_LIST_PREFIX}{}", list.id),
            list_kind: LibraryListKind::User,
            list_type: list.list_type,
            title: list.title,
            member_count,
            cover_preview: cover_for_user_list(conn, list.id)?,
            icon: None,
            overlay: false,
        });
    }

    // Source collections
    let collection_query = crate::collections::CollectionQuery {
        collection_type: source_collection_type_filter(query),
        min_member_count: Some(1),
    };
    for collection in collections::list_collections(conn, &collection_query)? {
        if !matches_section_source(conn, query, &collection.collection_type) {
            continue;
        }
        items.push(LibraryListSummary {
            id: format!("{SOURCE_LIST_PREFIX}{}", collection.id),
            list_kind: LibraryListKind::Source,
            list_type: collection.collection_type,
            title: collection.title,
            member_count: collection.member_count,
            cover_preview: collection.cover_preview,
            icon: None,
            overlay: false,
        });
    }

    Ok(items)
}

pub fn get_library_list(
    conn: &Connection,
    vault_root: Option<&Path>,
    list_id: &str,
    sort: Option<&str>,
    scope: Option<&LibraryListQuery>,
) -> Result<LibraryListDetail, VaultError> {
    init_user_lists(conn)?;
    let user_id = ensure_default_user(conn)?;
    let query = scope.cloned().unwrap_or_default();

    if list_id == SMART_RECENTLY_ADDED {
        return smart_recently_added_detail(conn, vault_root, query, sort);
    }
    if list_id == SMART_CONTINUE_WATCHING {
        return smart_continue_watching_detail(conn, vault_root, user_id, query, sort);
    }
    if list_id == SMART_RECENTLY_WATCHED {
        return smart_recently_watched_detail(conn, vault_root, user_id, query, sort);
    }
    if let Some(id) = list_id.strip_prefix(USER_LIST_PREFIX) {
        let list_uuid = Uuid::parse_str(id).map_err(|e| VaultError::Database {
            detail: e.to_string(),
        })?;
        return user_list_detail(conn, vault_root, list_uuid, sort);
    }
    if let Some(id) = list_id.strip_prefix(SOURCE_LIST_PREFIX) {
        let collection_id = Uuid::parse_str(id).map_err(|e| VaultError::Database {
            detail: e.to_string(),
        })?;
        return source_collection_detail(conn, vault_root, collection_id, sort);
    }

    // Backward compat: raw collection UUID
    if let Ok(collection_id) = Uuid::parse_str(list_id) {
        if collections::get_collection(conn, None, collection_id).is_ok() {
            return source_collection_detail(conn, vault_root, collection_id, sort);
        }
    }

    Err(VaultError::NotFound)
}

fn smart_recently_added_detail(
    conn: &Connection,
    vault_root: Option<&Path>,
    query: LibraryListQuery,
    sort: Option<&str>,
) -> Result<LibraryListDetail, VaultError> {
    let browse_sort = parse_sort(sort).unwrap_or(BrowseSort::AddedDesc);
    let members = members_from_browse(
        conn,
        vault_root,
        &browse_query_from_library(&query, 100),
        browse_sort,
        None,
    )?;
    let cover = members.first().and_then(|m| m.preview.clone());
    Ok(LibraryListDetail {
        id: SMART_RECENTLY_ADDED.into(),
        list_kind: LibraryListKind::Smart,
        list_type: "smart_recently_added".into(),
        title: "Recently Added".into(),
        member_count: members.len() as i32,
        cover_preview: cover,
        icon: Some("🕐".into()),
        overlay: true,
        members,
        sort_options: smart_sort_options(),
        default_sort: BrowseSort::AddedDesc.as_str().into(),
        can_reorder: false,
    })
}

fn smart_continue_watching_detail(
    conn: &Connection,
    vault_root: Option<&Path>,
    user_id: Uuid,
    query: LibraryListQuery,
    sort: Option<&str>,
) -> Result<LibraryListDetail, VaultError> {
    let playback_rows = list_playback_for_user(conn, user_id, true, 100)?;
    let members = members_from_playback(conn, vault_root, &playback_rows, &query, sort)?;
    let cover = members.first().and_then(|m| m.preview.clone());
    Ok(LibraryListDetail {
        id: SMART_CONTINUE_WATCHING.into(),
        list_kind: LibraryListKind::Smart,
        list_type: "smart_continue_watching".into(),
        title: "Continue Watching".into(),
        member_count: members.len() as i32,
        cover_preview: cover,
        icon: Some("▶".into()),
        overlay: true,
        members,
        sort_options: vec!["updated_desc".into()],
        default_sort: "updated_desc".into(),
        can_reorder: false,
    })
}

fn smart_recently_watched_detail(
    conn: &Connection,
    vault_root: Option<&Path>,
    user_id: Uuid,
    query: LibraryListQuery,
    sort: Option<&str>,
) -> Result<LibraryListDetail, VaultError> {
    let playback_rows = list_playback_for_user(conn, user_id, false, 100)?;
    let members = members_from_playback(conn, vault_root, &playback_rows, &query, sort)?;
    let cover = members.first().and_then(|m| m.preview.clone());
    Ok(LibraryListDetail {
        id: SMART_RECENTLY_WATCHED.into(),
        list_kind: LibraryListKind::Smart,
        list_type: "smart_recently_watched".into(),
        title: "Recently Watched".into(),
        member_count: members.len() as i32,
        cover_preview: cover,
        icon: Some("👁".into()),
        overlay: true,
        members,
        sort_options: vec!["updated_desc".into()],
        default_sort: "updated_desc".into(),
        can_reorder: false,
    })
}

fn user_list_detail(
    conn: &Connection,
    vault_root: Option<&Path>,
    list_id: Uuid,
    sort: Option<&str>,
) -> Result<LibraryListDetail, VaultError> {
    let list = get_user_list(conn, list_id)?.ok_or(VaultError::NotFound)?;
    let entity_ids = list_user_list_entity_ids(conn, list_id)?;
    let browse_sort = parse_sort(sort).unwrap_or(BrowseSort::Manual);
    let members = members_from_entity_ids(conn, vault_root, &entity_ids, browse_sort, None)?;
    let overlay = list.list_type == LIST_TYPE_WATCH_LATER;
    let icon = if overlay { Some("⏰".into()) } else { None };
    Ok(LibraryListDetail {
        id: format!("{USER_LIST_PREFIX}{list_id}"),
        list_kind: LibraryListKind::User,
        list_type: list.list_type.clone(),
        title: list.title,
        member_count: members.len() as i32,
        cover_preview: members.first().and_then(|m| m.preview.clone()),
        icon,
        overlay,
        members,
        sort_options: user_sort_options(),
        default_sort: BrowseSort::Manual.as_str().into(),
        can_reorder: true,
    })
}

fn source_collection_detail(
    conn: &Connection,
    vault_root: Option<&Path>,
    collection_id: Uuid,
    sort: Option<&str>,
) -> Result<LibraryListDetail, VaultError> {
    let detail = collections::get_collection(conn, vault_root, collection_id)?;
    let browse_sort = parse_sort(sort).unwrap_or(BrowseSort::Manual);
    let members = if browse_sort == BrowseSort::Manual {
        detail
            .members
            .iter()
            .map(collection_member_to_library_member)
            .collect()
    } else {
        let entity_ids: Vec<(Uuid, i32)> = detail
            .members
            .iter()
            .map(|m| (m.id, m.position))
            .collect();
        members_from_entity_ids(conn, vault_root, &entity_ids, browse_sort, None)?
    };

    Ok(LibraryListDetail {
        id: format!("{SOURCE_LIST_PREFIX}{collection_id}"),
        list_kind: LibraryListKind::Source,
        list_type: detail.collection_type,
        title: detail.title,
        member_count: members.len() as i32,
        cover_preview: detail.cover_preview,
        icon: None,
        overlay: false,
        members,
        sort_options: source_sort_options(),
        default_sort: BrowseSort::Manual.as_str().into(),
        can_reorder: false,
    })
}

fn collection_member_to_library_member(m: &CollectionMemberItem) -> LibraryListMember {
    LibraryListMember {
        id: m.id,
        title: m.title.clone(),
        kind: m.kind.clone(),
        mime: m.mime.clone(),
        source: m.source.clone(),
        status: m.status.clone(),
        position: m.position,
        preview: m.preview.clone(),
        timeline_sprite: m.timeline_sprite.clone(),
        timeline_manifest: m.timeline_manifest.clone(),
        primary_asset_id: m.primary_asset_id,
        primary_asset_status: m.primary_asset_status.clone(),
        duration: m.duration.clone(),
        channel: m.channel.clone(),
        channel_entity_id: m.channel_entity_id,
        channel_avatar_preview: m.channel_avatar_preview.clone(),
        uploader: m.uploader.clone(),
        webpage_url: m.webpage_url.clone(),
        playback_position: None,
        playback_progress: None,
    }
}

fn members_from_browse(
    conn: &Connection,
    vault_root: Option<&Path>,
    query: &BrowseQuery,
    sort: BrowseSort,
    playback: Option<&PlaybackRow>,
) -> Result<Vec<LibraryListMember>, VaultError> {
    let items = browse::browse_sorted_at(conn, vault_root, query, sort)?;
    Ok(items
        .into_iter()
        .enumerate()
        .map(|(i, item)| entity_list_to_member(conn, vault_root, item.id, i as i32, playback))
        .collect::<Result<Vec<_>, _>>()?)
}

fn members_from_entity_ids(
    conn: &Connection,
    vault_root: Option<&Path>,
    entity_ids: &[(Uuid, i32)],
    sort: BrowseSort,
    playback_map: Option<&[(Uuid, PlaybackRow)]>,
) -> Result<Vec<LibraryListMember>, VaultError> {
    if sort == BrowseSort::Manual {
        return entity_ids
            .iter()
            .map(|(id, position)| {
                let playback = playback_map.and_then(|map| {
                    map.iter().find(|(eid, _)| eid == id).map(|(_, row)| row)
                });
                entity_list_to_member(conn, vault_root, *id, *position, playback)
            })
            .collect();
    }

    let ids: Vec<Uuid> = entity_ids.iter().map(|(id, _)| *id).collect();
    let sorted = browse::browse_entity_ids_sorted(conn, &ids, sort)?;
    Ok(sorted
        .into_iter()
        .enumerate()
        .map(|(i, id)| {
            let position = entity_ids
                .iter()
                .find(|(eid, _)| *eid == id)
                .map(|(_, p)| *p)
                .unwrap_or(i as i32);
            let playback = playback_map.and_then(|map| {
                map.iter().find(|(eid, _)| *eid == id).map(|(_, row)| row)
            });
            entity_list_to_member(conn, vault_root, id, position, playback)
        })
        .collect::<Result<Vec<_>, _>>()?)
}

fn members_from_playback(
    conn: &Connection,
    vault_root: Option<&Path>,
    rows: &[PlaybackRow],
    query: &LibraryListQuery,
    _sort: Option<&str>,
) -> Result<Vec<LibraryListMember>, VaultError> {
    let mut members = Vec::new();
    for (i, row) in rows.iter().enumerate() {
        if !entity_matches_query(conn, row.entity_id, query)? {
            continue;
        }
        members.push(entity_list_to_member(
            conn,
            vault_root,
            row.entity_id,
            i as i32,
            Some(row),
        )?);
    }
    Ok(members)
}

fn entity_list_to_member(
    conn: &Connection,
    vault_root: Option<&Path>,
    entity_id: Uuid,
    position: i32,
    playback: Option<&PlaybackRow>,
) -> Result<LibraryListMember, VaultError> {
    let title_order = metadata::title_precedence_sql();
    let sql = format!(
        "SELECT e.id, e.mime, e.status,
                (
                    SELECT m.value FROM metadata m
                    WHERE m.entity_id = e.id AND m.key = 'title'
                    ORDER BY {title_order}
                    LIMIT 1
                ) AS title,
                (
                    SELECT sr.source FROM source_ref sr
                    WHERE sr.entity_id = e.id AND sr.kind = 'video'
                    LIMIT 1
                ) AS source,
                (
                    SELECT ea.id FROM entity_asset ea
                    WHERE ea.entity_id = e.id AND ea.role = 'primary'
                    ORDER BY ea.created_at ASC
                    LIMIT 1
                ) AS primary_asset_id,
                (
                    SELECT ea.status FROM entity_asset ea
                    WHERE ea.entity_id = e.id AND ea.role = 'primary'
                    ORDER BY ea.created_at ASC
                    LIMIT 1
                ) AS primary_asset_status,
                (
                    SELECT ea.kind FROM entity_asset ea
                    WHERE ea.entity_id = e.id AND ea.role = 'primary'
                    ORDER BY ea.created_at ASC
                    LIMIT 1
                ) AS primary_kind
         FROM entity e
         WHERE e.id = ?1 AND e.status != 'user_deleted'",
    );

    let row = conn.query_row(&sql, [entity_id.to_string()], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, Option<String>>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, Option<String>>(3)?,
            row.get::<_, Option<String>>(4)?,
            row.get::<_, Option<String>>(5)?,
            row.get::<_, Option<String>>(6)?,
            row.get::<_, Option<String>>(7)?,
        ))
    });

    let (
        _id,
        mime,
        status,
        title,
        source,
        primary_asset_id,
        primary_asset_status,
        kind,
    ) = match row {
        Ok(r) => r,
        Err(_) => return Err(VaultError::NotFound),
    };

    let primary_asset_id = primary_asset_id
        .map(|value| Uuid::parse_str(&value))
        .transpose()
        .map_err(|e| VaultError::Database {
            detail: e.to_string(),
        })?;

    let (playback_position, playback_progress) = playback.map(|p| {
        let progress = p.duration_seconds.and_then(|d| {
            if d > 0.0 {
                Some(p.position_seconds / d)
            } else {
                None
            }
        });
        (Some(p.position_seconds), progress)
    }).unwrap_or((None, None));

    let uploaded_by = resolve_uploaded_by_channel(conn, entity_id)?;

    Ok(LibraryListMember {
        id: entity_id,
        title,
        kind,
        mime,
        source,
        status,
        position,
        preview: resolve_entity_preview_at(conn, entity_id, vault_root)?.map(EntityPreviewSummary::from),
        timeline_sprite: resolve_timeline_sprite(conn, entity_id)?
            .map(EntityPreviewSummary::from),
        timeline_manifest: resolve_timeline_manifest(conn, entity_id)?
            .map(EntityPreviewSummary::from),
        primary_asset_id,
        primary_asset_status,
        duration: metadata_value(conn, entity_id, "duration")?,
        channel: metadata_value(conn, entity_id, "channel")?
            .or_else(|| uploaded_by.as_ref().and_then(|channel| channel.title.clone())),
        channel_entity_id: uploaded_by.as_ref().map(|channel| channel.channel_entity_id),
        channel_avatar_preview: uploaded_by
            .and_then(|channel| channel.avatar_preview.map(EntityPreviewSummary::from)),
        uploader: metadata_value(conn, entity_id, "uploader")?,
        webpage_url: metadata_value(conn, entity_id, "webpage_url")?,
        playback_position,
        playback_progress,
    })
}

fn metadata_value(
    conn: &Connection,
    entity_id: Uuid,
    key: &str,
) -> Result<Option<String>, VaultError> {
    let title_order = metadata::title_precedence_sql();
    let sql = format!(
        "SELECT value FROM metadata
         WHERE entity_id = ?1 AND key = ?2
         ORDER BY {title_order}
         LIMIT 1",
    );
    conn.query_row(
        &sql,
        params![entity_id.to_string(), key],
        |row| row.get(0),
    )
    .optional()
    .map_err(db_err)
}

fn smart_summary(
    id: &str,
    list_type: &str,
    title: &str,
    member_count: i32,
    conn: &Connection,
    user_id: Uuid,
    query: &LibraryListQuery,
    icon: &str,
) -> Result<LibraryListSummary, VaultError> {
    let cover = match id {
        SMART_CONTINUE_WATCHING => {
            let rows = list_playback_for_user(conn, user_id, true, 1)?;
            rows.first()
                .and_then(|r| resolve_entity_preview(conn, r.entity_id).ok().flatten())
                .map(EntityPreviewSummary::from)
        }
        SMART_RECENTLY_ADDED => {
            let items = browse::browse(
                conn,
                &browse_query_from_library(query, 1),
            )?;
            items
                .first()
                .and_then(|_| resolve_entity_preview(conn, items[0].id).ok().flatten())
                .map(EntityPreviewSummary::from)
        }
        SMART_RECENTLY_WATCHED => {
            let rows = list_playback_for_user(conn, user_id, false, 1)?;
            rows.first()
                .and_then(|r| resolve_entity_preview(conn, r.entity_id).ok().flatten())
                .map(EntityPreviewSummary::from)
        }
        _ => None,
    };
    Ok(LibraryListSummary {
        id: id.into(),
        list_kind: LibraryListKind::Smart,
        list_type: list_type.into(),
        title: title.into(),
        member_count,
        cover_preview: cover,
        icon: Some(icon.into()),
        overlay: true,
    })
}

fn count_continue_watching(conn: &Connection, user_id: Uuid, query: &LibraryListQuery) -> Result<i32, VaultError> {
    let rows = list_playback_for_user(conn, user_id, true, 200)?;
    let count = rows
        .iter()
        .filter(|r| entity_matches_query(conn, r.entity_id, query).unwrap_or(false))
        .count();
    Ok(count as i32)
}

fn count_recently_added(conn: &Connection, query: &LibraryListQuery) -> Result<i32, VaultError> {
    let items = browse::browse(conn, &browse_query_from_library(query, 100))?;
    Ok(items.len() as i32)
}

fn count_recently_watched(conn: &Connection, user_id: Uuid, query: &LibraryListQuery) -> Result<i32, VaultError> {
    let rows = list_playback_for_user(conn, user_id, false, 200)?;
    let count = rows
        .iter()
        .filter(|r| entity_matches_query(conn, r.entity_id, query).unwrap_or(false))
        .count();
    Ok(count as i32)
}

fn cover_for_user_list(conn: &Connection, list_id: Uuid) -> Result<Option<EntityPreviewSummary>, VaultError> {
    let ids = list_user_list_entity_ids(conn, list_id)?;
    let first = ids.first().map(|(id, _)| *id);
    Ok(first
        .and_then(|id| resolve_entity_preview(conn, id).ok().flatten())
        .map(EntityPreviewSummary::from))
}

fn browse_query_from_library(query: &LibraryListQuery, limit: u32) -> BrowseQuery {
    BrowseQuery {
        kind: query.kind.clone(),
        source: query.source.clone(),
        limit,
        ..BrowseQuery::default()
    }
}

fn source_collection_type_filter(query: &LibraryListQuery) -> Option<String> {
    if query.source.as_deref() == Some("youtube") {
        return None; // filter client-side by youtube types
    }
    None
}

fn matches_section_source(
    _conn: &Connection,
    query: &LibraryListQuery,
    collection_type: &str,
) -> bool {
    if query.source.as_deref() == Some("youtube") {
        return collection_type == "youtube_playlist" || collection_type == "youtube_channel_uploads";
    }
    if query.kind.as_deref() == Some("video") && query.source.is_none() {
        return true;
    }
    true
}

fn entity_matches_query(
    conn: &Connection,
    entity_id: Uuid,
    query: &LibraryListQuery,
) -> Result<bool, VaultError> {
    if query.source.is_none() && query.kind.is_none() {
        return Ok(true);
    }
    let source: Option<String> = conn
        .query_row(
            "SELECT sr.source FROM source_ref sr WHERE sr.entity_id = ?1 LIMIT 1",
            [entity_id.to_string()],
            |row| row.get(0),
        )
        .optional()
        .map_err(db_err)?;
    let kind: Option<String> = conn
        .query_row(
            "SELECT ea.kind FROM entity_asset ea
             WHERE ea.entity_id = ?1 AND ea.role = 'primary'
             ORDER BY ea.created_at ASC LIMIT 1",
            [entity_id.to_string()],
            |row| row.get(0),
        )
        .optional()
        .map_err(db_err)?;

    if let Some(ref expected) = query.source {
        if source.as_deref() != Some(expected.as_str()) {
            return Ok(false);
        }
    }
    if let Some(ref expected) = query.kind {
        if kind.as_deref() != Some(expected.as_str()) {
            return Ok(false);
        }
    }
    Ok(true)
}

fn parse_sort(sort: Option<&str>) -> Option<BrowseSort> {
    sort.and_then(BrowseSort::parse)
}

fn smart_sort_options() -> Vec<String> {
    vec![
        BrowseSort::AddedDesc.as_str().into(),
        BrowseSort::PublishedDesc.as_str().into(),
        BrowseSort::ViewsDesc.as_str().into(),
    ]
}

fn user_sort_options() -> Vec<String> {
    vec![
        BrowseSort::Manual.as_str().into(),
        BrowseSort::AddedDesc.as_str().into(),
        BrowseSort::PublishedDesc.as_str().into(),
        BrowseSort::ViewsDesc.as_str().into(),
    ]
}

fn source_sort_options() -> Vec<String> {
    user_sort_options()
}

fn db_err(err: rusqlite::Error) -> VaultError {
    VaultError::Database {
        detail: err.to_string(),
    }
}

use rusqlite::OptionalExtension;

pub fn default_user() -> Uuid {
    default_user_id()
}
