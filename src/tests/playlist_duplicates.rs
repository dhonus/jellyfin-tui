//! Tests for migration 0008 (positional playlist membership) against in-memory SQLite.

use crate::database::extension::{
    add_playlist_membership, remove_playlist_entries, remove_playlist_membership,
    replace_playlist_membership, run_migrations,
};
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::SqlitePool;

/// Migrated empty DB on one connection (in-memory SQLite is per-connection).
async fn migrated_pool() -> SqlitePool {
    let pool =
        SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
    run_migrations(&pool).await.unwrap();
    pool
}

async fn rows(pool: &SqlitePool, playlist_id: &str) -> Vec<(String, i64)> {
    sqlx::query_as(
        "SELECT track_id, position FROM playlist_membership
         WHERE playlist_id = ? ORDER BY position",
    )
    .bind(playlist_id)
    .fetch_all(pool)
    .await
    .unwrap()
}

async fn child_count(pool: &SqlitePool, playlist_id: &str) -> i64 {
    sqlx::query_scalar("SELECT json_extract(playlist, '$.ChildCount') FROM playlists WHERE id = ?")
        .bind(playlist_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn migration_renumbers_duplicate_positions_without_losing_rows() {
    // Old schema allowed duplicate positions per playlist, so the migration must renumber.
    let pool =
        SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();

    sqlx::raw_sql(
        r#"
        CREATE TABLE playlist_membership (
          playlist_id TEXT NOT NULL,
          track_id    TEXT NOT NULL,
          position    INTEGER NOT NULL DEFAULT 0,
          PRIMARY KEY (playlist_id, track_id)
        );
        INSERT INTO playlist_membership (playlist_id, track_id, position) VALUES
          ('p1', 't1', 0),
          ('p1', 't2', 0),
          ('p1', 't3', 1),
          ('p2', 't4', 5);
        "#,
    )
    .execute(&pool)
    .await
    .unwrap();

    sqlx::raw_sql(include_str!("../database/migrations/0008_playlist_entries.sql"))
        .execute(&pool)
        .await
        .unwrap();

    assert_eq!(rows(&pool, "p1").await, vec![("t1".into(), 0), ("t2".into(), 1), ("t3".into(), 2)]);
    assert_eq!(rows(&pool, "p2").await, vec![("t4".into(), 0)]);
}

#[tokio::test]
async fn adding_the_same_track_twice_keeps_two_rows_and_remove_by_position_drops_one() {
    let pool = migrated_pool().await;

    add_playlist_membership(&pool, "p", &["t".into(), "t".into()]).await.unwrap();
    assert_eq!(rows(&pool, "p").await, vec![("t".into(), 0), ("t".into(), 1)]);

    remove_playlist_entries(&pool, "p", &[0]).await.unwrap();
    // surviving row is renumbered to keep positions dense
    assert_eq!(rows(&pool, "p").await, vec![("t".into(), 0)]);
}

#[tokio::test]
async fn remove_playlist_membership_still_drops_every_copy_of_a_track() {
    // "remove all copies" mirrors the server's EntryIds delete.
    let pool = migrated_pool().await;

    add_playlist_membership(&pool, "p", &["t".into(), "t".into()]).await.unwrap();
    remove_playlist_membership(&pool, "p", &["t".into()]).await.unwrap();
    assert!(rows(&pool, "p").await.is_empty());
}

#[tokio::test]
async fn replace_mirrors_server_order_and_duplicates_and_refreshes_child_count() {
    let pool = migrated_pool().await;

    sqlx::query("INSERT INTO playlists (id, playlist) VALUES (?, ?)")
        .bind("p")
        .bind(r#"{"Id":"p","ChildCount":99}"#)
        .execute(&pool)
        .await
        .unwrap();

    // sync mirror: order + duplicates preserved
    replace_playlist_membership(&pool, "p", &["a".into(), "b".into(), "a".into()]).await.unwrap();
    assert_eq!(rows(&pool, "p").await, vec![("a".into(), 0), ("b".into(), 1), ("a".into(), 2)]);
    assert_eq!(child_count(&pool, "p").await, 3);

    // a shorter server list replaces it wholesale
    replace_playlist_membership(&pool, "p", &["b".into(), "a".into()]).await.unwrap();
    assert_eq!(rows(&pool, "p").await, vec![("b".into(), 0), ("a".into(), 1)]);
    assert_eq!(child_count(&pool, "p").await, 2);
}
