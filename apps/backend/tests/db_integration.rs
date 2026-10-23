//! Database-backed integration tests.
//!
//! These tests are `#[ignore]`d because they need a disposable PostgreSQL
//! database. Nothing in the default `cargo test` run touches the network.
//!
//! Run them with:
//!
//! ```text
//! $env:TEST_DATABASE_URL = "postgres://iconcrafter:iconcrafter@127.0.0.1:5432/iconcrafter_test"
//! cargo test -p iconcrafter-server -- --ignored --test-threads=1
//! ```
//!
//! The database is expected to be disposable: the tests create rows inside the
//! existing `public` schema and delete them again. They are serialised with
//! `--test-threads=1` because they share tables.

use std::time::Duration;

use iconcrafter_server::auth::session::{
    create_session, delete_session, lookup_session, touch_session,
};
use iconcrafter_server::auth::tokens::token_hash;
use iconcrafter_server::authz::{Capability, require_file};
use iconcrafter_server::services::audit;
use iconcrafter_server::ws::oplog::{OpLog, PgOpLog};
use iconcrafter_server::ws::protocol::OpsBatch;
use serde_json::json;
use sqlx::postgres::PgPoolOptions;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

fn database_url() -> String {
    std::env::var("TEST_DATABASE_URL").expect(
        "TEST_DATABASE_URL must be set to a disposable PostgreSQL database for ignored tests",
    )
}

async fn pool() -> PgPool {
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&database_url())
        .await
        .expect("connecting to TEST_DATABASE_URL");
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("running migrations");
    pool
}

async fn insert_user<'e>(
    tx: &mut Transaction<'e, Postgres>,
    prefix: &str,
) -> Result<Uuid, sqlx::Error> {
    let email = format!("{}-{}@example.invalid", prefix, Uuid::new_v4());
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO users (email, display_name, password_hash) \
         VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(&email)
    .bind(prefix)
    .bind("$argon2id$v=19$m=19456,t=2,p=1$c2FsdHNhbHQ$aGFzaGhhc2g")
    .fetch_one(&mut **tx)
    .await?;
    Ok(id)
}

/// Seeds a team/project/file whose team is owned by `owner`.
async fn seed_file(pool: &PgPool, owner: Uuid) -> (Uuid, Uuid, Uuid) {
    let mut tx = pool.begin().await.expect("begin seed");
    let team: Uuid = sqlx::query_scalar(
        "INSERT INTO teams (name, created_by) VALUES ('integration team', $1) RETURNING id",
    )
    .bind(owner)
    .fetch_one(&mut *tx)
    .await
    .expect("insert team");
    sqlx::query("INSERT INTO team_members (team_id, user_id, role) VALUES ($1, $2, 'owner')")
        .bind(team)
        .bind(owner)
        .execute(&mut *tx)
        .await
        .expect("insert team member");
    let project: Uuid = sqlx::query_scalar(
        "INSERT INTO projects (team_id, name, created_by) \
         VALUES ($1, 'integration project', $2) RETURNING id",
    )
    .bind(team)
    .bind(owner)
    .fetch_one(&mut *tx)
    .await
    .expect("insert project");
    let file: Uuid = sqlx::query_scalar(
        "INSERT INTO files (project_id, name, created_by) VALUES ($1, 'integration file', $2) RETURNING id",
    )
    .bind(project)
    .bind(owner)
    .fetch_one(&mut *tx)
    .await
    .expect("insert file");
    let document = dp_model::Document::new("integration file")
        .to_json_value()
        .expect("serialize document");
    let version: Uuid = sqlx::query_scalar(
        "INSERT INTO file_versions (file_id, label, kind, snapshot_json, created_by) \
         VALUES ($1, 'Initial', 'branch_base', $2, $3) RETURNING id",
    )
    .bind(file)
    .bind(document)
    .bind(owner)
    .fetch_one(&mut *tx)
    .await
    .expect("insert version");
    sqlx::query("UPDATE files SET head_version_id = $1 WHERE id = $2")
        .bind(version)
        .bind(file)
        .execute(&mut *tx)
        .await
        .expect("set head");
    tx.commit().await.expect("commit seed");
    (team, project, file)
}

async fn cleanup_file(pool: &PgPool, team: Uuid, project: Uuid, file: Uuid, owner: Uuid) {
    // Reverse order of the foreign keys; delete even if a test failed midway.
    let _ = sqlx::query("DELETE FROM op_journal WHERE file_id = $1")
        .bind(file)
        .execute(pool)
        .await;
    let _ = sqlx::query("DELETE FROM file_versions WHERE file_id = $1")
        .bind(file)
        .execute(pool)
        .await;
    let _ = sqlx::query("DELETE FROM files WHERE id = $1")
        .bind(file)
        .execute(pool)
        .await;
    let _ = sqlx::query("DELETE FROM projects WHERE id = $1")
        .bind(project)
        .execute(pool)
        .await;
    let _ = sqlx::query("DELETE FROM team_members WHERE team_id = $1")
        .bind(team)
        .execute(pool)
        .await;
    let _ = sqlx::query("DELETE FROM teams WHERE id = $1")
        .bind(team)
        .execute(pool)
        .await;
    let _ = sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(owner)
        .execute(pool)
        .await;
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn migrations_run_twice_and_schema_is_present() {
    let url = database_url();
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await
        .expect("connect");
    for round in 0..2 {
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .unwrap_or_else(|error| panic!("migration round {round} failed: {error}"));
    }
    let users: Option<String> = sqlx::query_scalar("SELECT to_regclass('public.users')::text")
        .fetch_one(&pool)
        .await
        .expect("regclass query");
    assert_eq!(users.as_deref(), Some("users"));
    let journal: Option<String> =
        sqlx::query_scalar("SELECT to_regclass('public.op_journal')::text")
            .fetch_one(&pool)
            .await
            .expect("regclass query");
    assert_eq!(journal.as_deref(), Some("op_journal"));
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn sessions_round_trip_through_the_database() {
    let pool = pool().await;
    let mut tx = pool.begin().await.expect("begin");
    let user_id = insert_user(&mut tx, "it-session").await.expect("user");
    tx.commit().await.expect("commit");

    let raw = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    let hashed = token_hash(raw);
    let session_id = create_session(
        &pool,
        user_id,
        &hashed,
        Duration::from_secs(3600),
        Some("integration-test/1.0"),
        Some("127.0.0.1"),
    )
    .await
    .expect("create session");

    let found = lookup_session(&pool, &hashed)
        .await
        .expect("lookup session")
        .expect("session exists");
    assert_eq!(found.session_id, session_id);
    assert_eq!(found.user_id, user_id);

    touch_session(&pool, session_id)
        .await
        .expect("touch session");
    assert!(
        delete_session(&pool, &hashed)
            .await
            .expect("delete session")
    );
    assert!(
        lookup_session(&pool, &hashed)
            .await
            .expect("lookup deleted")
            .is_none()
    );
    assert!(
        !delete_session(&pool, &hashed)
            .await
            .expect("delete missing")
    );

    let _ = sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(user_id)
        .execute(&pool)
        .await;
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn authorization_resolves_file_roles() {
    let pool = pool().await;
    let mut tx = pool.begin().await.expect("begin");
    let owner = insert_user(&mut tx, "it-owner").await.expect("owner");
    let stranger = insert_user(&mut tx, "it-stranger").await.expect("stranger");
    tx.commit().await.expect("commit");
    let (team, project, file) = seed_file(&pool, owner).await;

    let access = require_file(&pool, owner, file, Capability::Manage)
        .await
        .expect("owner has manage");
    assert_eq!(access.project_id, project);
    assert_eq!(access.team_id, team);

    let denied = require_file(&pool, stranger, file, Capability::View)
        .await
        .expect_err("stranger must be denied");
    assert_eq!(denied.status, axum::http::StatusCode::FORBIDDEN);

    let missing = require_file(&pool, owner, Uuid::new_v4(), Capability::View)
        .await
        .expect_err("unknown file");
    assert_eq!(missing.status, axum::http::StatusCode::NOT_FOUND);

    cleanup_file(&pool, team, project, file, owner).await;
    let _ = sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(stranger)
        .execute(&pool)
        .await;
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn op_journal_is_idempotent_per_batch() {
    let pool = pool().await;
    let mut tx = pool.begin().await.expect("begin");
    let owner = insert_user(&mut tx, "it-journal").await.expect("owner");
    tx.commit().await.expect("commit");
    let (team, project, file) = seed_file(&pool, owner).await;

    let log = PgOpLog::new(pool.clone());
    let batch = OpsBatch {
        batch_id: Uuid::new_v4(),
        ops: vec![json!({"op": "set_node_field", "node": "n1", "field": "name"})],
        stamp: None,
    };

    let first = log
        .append(file, owner, None, &batch)
        .await
        .expect("first append");
    assert!(!first.duplicate);
    assert!(first.seq >= 1);

    let second = log
        .append(file, owner, None, &batch)
        .await
        .expect("duplicate append");
    assert!(second.duplicate);
    assert_eq!(second.seq, first.seq);

    assert_eq!(log.max_seq(file).await.expect("max seq"), first.seq);
    let entries = log.since(file, 0, 10).await.expect("journal entries");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].batch_id, batch.batch_id);

    cleanup_file(&pool, team, project, file, owner).await;
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn audit_events_are_recorded() {
    let pool = pool().await;
    let marker = format!("integration-test.{}", Uuid::new_v4());
    audit::record_pool(
        &pool,
        None,
        &marker,
        "test",
        Some("integration"),
        json!({"suite": "db_integration"}),
        Some("127.0.0.1"),
    )
    .await
    .expect("record audit event");
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM audit_events WHERE action = $1")
        .bind(&marker)
        .fetch_one(&pool)
        .await
        .expect("count audit rows");
    assert_eq!(count, 1);
    let _ = sqlx::query("DELETE FROM audit_events WHERE action = $1")
        .bind(&marker)
        .execute(&pool)
        .await;
}
