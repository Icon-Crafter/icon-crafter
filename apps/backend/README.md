# iconcrafter-server

The collaboration and publishing backend for Icon Crafter: a real Axum service that owns
accounts, teams, projects, files, document persistence, comments, shares, assets, audit
history, published sites, and the WebSocket relay that journals collaborative operation
batches.

The crate is a workspace member (`apps/backend`, package `iconcrafter-server`) and uses
`dp-model` for document validation and structural diffing. `dp-collab` is not wired yet; the
WebSocket layer talks to a small internal `OpLog` trait so the relay can later delegate
convergence to `dp-collab` without touching HTTP handlers.

## Architecture

```
src/
  main.rs            process boot: tracing, pool, migrations, listener, graceful shutdown
  lib.rs             public module surface + router()
  config.rs          environment parsing and validation (no hardcoded secrets)
  error.rs           ApiError -> {"error":{"code","message","details"?}} JSON envelope
  state.rs           AppState: config, PgPool, ObjectStore, rate limiter, WS rooms, OpLog
  validation.rs      e-mail/name/password/comment/label/slug validation
  pagination.rs      page/limit parsing, capped at 200
  models.rs          API response structs
  authz.rs           Capability / Role resolution over team_members + file_collaborators
  auth/              password hashing, tokens, CSRF, sessions, in-process rate limiting
  storage/           ObjectStore trait, LocalFsStore, S3Store (feature `s3`), MIME sniffing
  routes/            one module per resource; all handlers return AppResult<T>
  services/          audit events, document diff, publish HTML renderer
  ws/                protocol, per-file rooms, op journal, upgrade handler
migrations/          sqlx migrations, embedded at compile time
tests/               #[ignore] Postgres integration tests
```

Request flow: `routes/*` -> `authz::require_*` (authorization) -> `sqlx` query/transaction ->
`services::audit` (audit event) -> JSON response. Every mutating request that authenticates
with the session cookie must present the CSRF double-submit header; `Authorization: Bearer`
requests are exempt from CSRF because the token is not ambient.

## Endpoints

All endpoints return JSON. Errors use the envelope
`{"error":{"code":"...","message":"...","details":?}}`. Capabilities are ordered
`view < comment < edit < manage`; `manage` comes from team owner/admin or file-delete rights.

### Auth and session

| Method | Path | Auth | Notes |
| --- | --- | --- | --- |
| POST | `/api/auth/register` | public, rate limited | 201, sets session + CSRF cookies |
| POST | `/api/auth/login` | public, rate limited | 200, sets session + CSRF cookies |
| POST | `/api/auth/logout` | session or bearer | 204, clears cookies |
| GET | `/api/auth/me` | session or bearer | current user + session expiry |

### Teams and members

| Method | Path | Required capability |
| --- | --- | --- |
| GET | `/api/teams` | any authenticated user (own memberships) |
| POST | `/api/teams` | any authenticated user |
| GET | `/api/teams/{team_id}` | `view` |
| PATCH | `/api/teams/{team_id}` | `manage` |
| DELETE | `/api/teams/{team_id}` | `manage` (owner only, soft delete) |
| GET | `/api/teams/{team_id}/members` | `view` |
| POST | `/api/teams/{team_id}/members` | `manage` (granting `owner` requires owner) |
| PATCH | `/api/teams/{team_id}/members/{user_id}` | `manage` (last owner protected) |
| DELETE | `/api/teams/{team_id}/members/{user_id}` | `manage` (team creator protected) |

### Projects and files

| Method | Path | Required capability |
| --- | --- | --- |
| GET | `/api/teams/{team_id}/projects` | team `view` |
| POST | `/api/teams/{team_id}/projects` | team `edit` |
| GET | `/api/projects/{project_id}` | project `view` |
| PATCH | `/api/projects/{project_id}` | `edit` |
| DELETE | `/api/projects/{project_id}` | `manage` (soft delete) |
| GET | `/api/projects/{project_id}/files` | `view` |
| POST | `/api/projects/{project_id}/files` | `edit` |
| GET | `/api/files/{file_id}` | `view` |
| PATCH | `/api/files/{file_id}` | `edit` |
| DELETE | `/api/files/{file_id}` | `manage` (soft delete) |
| GET | `/api/files/{file_id}/doc` | `view` |
| PUT | `/api/files/{file_id}/doc` | `edit` |
| GET | `/api/files/{file_id}/versions` | `view` |
| POST | `/api/files/{file_id}/versions` | `edit` (named version) |
| GET | `/api/files/{file_id}/versions/diff` | `view` |
| POST | `/api/files/{file_id}/versions/{version_id}/restore` | `edit` |
| GET | `/api/files/{file_id}/collaborators` | `view` |
| POST | `/api/files/{file_id}/collaborators` | `manage` |
| DELETE | `/api/files/{file_id}/collaborators/{user_id}` | `manage` |
| GET | `/api/files/{file_id}/comments` | `view` |
| POST | `/api/files/{file_id}/comments` | `comment` |
| PATCH | `/api/files/{file_id}/comments/{comment_id}` | `comment` (body edits author-only) |
| DELETE | `/api/files/{file_id}/comments/{comment_id}` | `comment` (author or `manage`) |
| GET | `/api/files/{file_id}/shares` | `edit` |
| POST | `/api/files/{file_id}/shares` | `edit` |
| DELETE | `/api/files/{file_id}/shares/{share_id}` | `edit` |
| GET | `/api/files/{file_id}/assets` | `view` |
| POST | `/api/files/{file_id}/assets` | `edit` (multipart) |
| GET | `/api/files/{file_id}/assets/{asset_id}` | `view` |
| DELETE | `/api/files/{file_id}/assets/{asset_id}` | `edit` |
| POST | `/api/files/{file_id}/publish` | `edit` |
| GET | `/api/activity` | authenticated; own events, `file_id` filter needs file `view` |

### Public endpoints

| Method | Path | Notes |
| --- | --- | --- |
| POST | `/api/shares/{token}/access` | public, rate limited, optional share password |
| GET | `/p/{slug}` | published site bundle, 404 when absent |
| GET | `/healthz` | liveness, version and uptime |
| GET | `/readyz` | readiness, DB check with timeout |
| GET | `/ws/files/{file_id}` | WebSocket upgrade, session or bearer auth, file `view` |

`PUT /api/files/{file_id}/doc` validates the body with
`dp_model::Document::from_json_value` and `Document::validate`, returning
`422 validation_failed` on hard validation errors and `422 invalid_document` on structurally
invalid documents. Version diffs compare snapshot documents node-by-node and report
added/removed/changed node ids plus changed top-level field names only (no document blobs).

`GET /api/files/{file_id}/versions/diff?from=<version>&to=<version>` requires both ids.

## WebSocket protocol

`GET /ws/files/{file_id}` upgrades after cookie/bearer authentication and a `view` check on
the file. Messages are JSON text frames with a `type` tag:

- client: `ops` (`{base_seq?, batch:{batch_id, ops[], stamp?}}`), `presence`, `cursor`,
  `selection`, `ping`, `leave`
- server: `welcome`, `ack`, `reject`, `rebroadcast`, `presence`, `cursor`, `selection`,
  `pong`, `peer_joined`, `peer_left`, `error`

Accepted batches are appended to `op_journal` in a transaction guarded by
`pg_advisory_xact_lock`; `(file_id, batch_id)` uniqueness makes retries idempotent and an
`ack` with `duplicate: true` returns the previously assigned sequence. A per-file room keeps
a monotonic server sequence and a bounded broadcast channel; a client that falls behind the
channel is closed with an `error` frame instead of stalling the room. Limits: 256 KiB frames,
500 operations per batch, 64 KiB per operation. Edits require `edit` capability; viewers may
send presence/cursor/selection only. Presence leaves are broadcast on disconnect. This is a
relay + journal, not CRDT convergence: convergence/repair is planned on top of `dp-collab`.

## Object storage

The `ObjectStore` trait has `put`, `get`, `head`, and `delete`.

- `LocalFsStore` (always available) writes through a temporary file in the target directory
  and renames it into place, rejects path traversal and reserved Windows names, and streams
  reads.
- `S3Store` is compiled behind the `s3` cargo feature and uses `aws-sdk-s3` with optional
  custom endpoint, credentials, path-style access, and key prefix (MinIO-compatible).

Keys are content-addressed (`assets/<sha256>.<ext>`, `publishes/<slug>/<sha256>.html`).
Client file names are never used in storage keys; the original name is only metadata. Upload
MIME types come from magic-byte sniffing (PNG, JPEG, WebP, SVG, TTF, OTF, WOFF2), not from
the request header. `sha256` is globally unique, so the same bytes are stored once and linked
to multiple files through `file_assets`.

## Configuration

Configuration is read from the environment only (the binary does not parse `.env` files).
See [`.env.example`](.env.example) for placeholders.

| Variable | Default | Notes |
| --- | --- | --- |
| `DATABASE_URL` | required | `postgres://` or `postgresql://` |
| `LISTEN_ADDR` | `127.0.0.1:8080` | socket address |
| `PUBLIC_BASE_URL` | `http://127.0.0.1:8080` | used for share and publish URLs; trailing slash removed |
| `CORS_ORIGINS` | empty | comma-separated origins; CORS disabled when empty |
| `SERVE_STATIC_DIR` | unset | optional `ServeDir` at `/static` |
| `SESSION_COOKIE_NAME` | `ic_session` | CSRF cookie is `<name>_csrf` |
| `SESSION_TTL_HOURS` | `720` | 1..=8760 |
| `COOKIE_SECURE` | derived | defaults to true when `PUBLIC_BASE_URL` is https |
| `RATE_LIMIT_PER_MINUTE` | `60` | per-IP token bucket, 1..=100000 |
| `DATABASE_MAX_CONNECTIONS` | `10` | 1..=100 |
| `STORAGE_KIND` | `local` | `local` or `s3` (requires the `s3` feature) |
| `STORAGE_LOCAL_DIR` | `./data/storage` | root for `LocalFsStore` |
| `S3_BUCKET`, `S3_REGION` | required for `s3` | |
| `S3_ENDPOINT` | unset | custom endpoint (MinIO); enables path-style by default |
| `S3_ACCESS_KEY_ID`, `S3_SECRET_ACCESS_KEY` | unset | both or neither; else ambient credentials |
| `S3_FORCE_PATH_STYLE` | derived | defaults to `true` when `S3_ENDPOINT` is set |
| `S3_PREFIX` | `iconcrafter/` | key prefix |
| `MAX_UPLOAD_BYTES` | `26214400` (25 MiB) | 1024..=536870912 |
| `LOG_FORMAT` | `text` | `text` or `json` |
| `RUST_LOG` | `info,tower_http=warn,sqlx=warn` | tracing filter |
| `TEST_DATABASE_URL` | unset | only for `#[ignore]` integration tests |

Invalid values fail at startup with an explicit message naming the variable.

## Local development without Docker

Docker is not required to build or unit-test this crate. You need a PostgreSQL instance only
to run migrations, start the server, or run the ignored integration tests.

```powershell
# 1. connection and basic settings
$env:DATABASE_URL = "postgres://postgres:postgres@127.0.0.1:5432/iconcrafter"
$env:STORAGE_LOCAL_DIR = "$PWD\data\storage"
$env:SESSION_COOKIE_NAME = "ic_session"

# 2. build and test without a database (unit tests)
cargo test -p iconcrafter-server

# 3. run the server; migrations are applied automatically at startup
cargo run -p iconcrafter-server
```

The server applies `migrations/` through `sqlx::migrate!` on boot. Without a reachable
PostgreSQL it exits with `failed to connect to the database configured by DATABASE_URL`, not a
panic. `GET /healthz` answers before touching the database; `GET /readyz` checks it with a
timeout.

## Migrations

`migrations/0001_initial_schema.sql` creates the full schema and is purely additive:

- `users`, `sessions`, `teams`, `team_members`, `projects`, `files`, `file_versions`,
  `file_collaborators`, `comments`, `shares`, `assets`, `file_assets`, `op_journal`,
  `audit_events`, `published_sites`
- `pgcrypto` for `gen_random_uuid()`, foreign keys, check constraints, lookup indexes, and one
  shared `updated_at` trigger function

Everything uses `CREATE TABLE/INDEX IF NOT EXISTS` and `CREATE OR REPLACE FUNCTION`; no table
or column is dropped, so replaying the migration on a live database is safe. Tokens are
stored as 32-byte SHA-256 hashes (`bytea`), never clear text. File and project deletion is
soft (`is_deleted`), and `op_journal` is unique on both `(file_id, batch_id)` (idempotency)
and `(file_id, seq)` (ordering).

## Security notes

- Passwords: Argon2id with per-password salt (`argon2` crate defaults), verified in
  `spawn_blocking`; login and registration are rate limited per IP and route.
- Sessions: 32 random bytes from the OS CSPRNG, returned as 64 hex characters, stored only as
  SHA-256 in `sessions.token_hash`, delivered in an `HttpOnly`, `SameSite=Lax` cookie
  (`Secure` when `COOKIE_SECURE`), comparison uses constant-time equality.
- CSRF: double-submit token (`<session>_csrf` cookie + `X-CSRF-Token` header) enforced on
  every state-changing `/api` request that uses the session cookie; bearer tokens are exempt.
- Authorization: every handler resolves the caller from the session, then re-derives the
  effective role from the database (`team_members`, `file_collaborators`, ownership). Client
  supplied ids are only looked up, never trusted. Responses are `401` unauthenticated,
  `403` capability/role failures, `404` for resources the caller cannot see, `422` for hard
  validation failures, `429` with `Retry-After` when rate limited.
- Uploads: size limited before storage, magic-byte sniffed, SHA-256 content-addressed, stored
  with server-generated keys; SVG responses carry a `sandbox` CSP and
  `X-Content-Type-Options: nosniff`; published HTML is served with a restrictive CSP.
- Errors never include SQL or internal error strings; `500` responses are logged server-side
  with the underlying error.

## Verified by

Environment: Windows 10, Rust 1.98.0 (`cargo 1.98.0`), workspace `C:\Users\w4ua\Documents\Icon Crafter`.
Verification runs use `--offline` because the crate cache is local; Docker is not installed.

| Command | Result |
| --- | --- |
| `cargo check -p iconcrafter-server --offline` | `Finished dev profile ... in 8.20s`, no warnings (default features) |
| `cargo test -p iconcrafter-server --offline` | lib tests: 132 passed, 0 failed; `db_integration`: 5 ignored; 0 failed |
| `cargo check -p iconcrafter-server --features s3 --offline` | `Finished dev profile ... in 55.72s`, exit 0 |
| `cargo check -p iconcrafter-server --features otel --offline` | `Finished dev profile ... in 1m 15s`, exit 0 (dependency wiring only) |
| `cargo clippy -p iconcrafter-server --no-deps --offline -- -D warnings` | `Finished`, exit 0 |
| `cargo fmt -p iconcrafter-server -- --check` | no output, exit 0 |

`cargo clippy -p iconcrafter-server -- -D warnings` (without `--no-deps`) fails inside the
`dp-geom` dependency; `--no-deps` limits lints to this crate.

PostgreSQL integration tests are `#[ignore]`d and were not run here (no database available):

```powershell
$env:TEST_DATABASE_URL = "postgres://iconcrafter:iconcrafter@127.0.0.1:5432/iconcrafter_test"
cargo test -p iconcrafter-server -- --ignored --test-threads=1
```

They cover migration replay, session round trips, role resolution, op-journal idempotency, and
audit-event insertion.

## Honest limitations

- No e-mail verification or mailer, no password reset, no OAuth/SSO. Accounts are created with
  e-mail + password only.
- S3 support is compile-checked and wired behind a feature flag but has not been exercised
  against a real bucket; MinIO integration is unverified (Docker unavailable).
- Redis is only present as an optional compose profile. It is not used for presence, caching,
  or rate limiting; rate limiting is in-process and per instance.
- `dp-collab` is not wired. The WebSocket layer relays, journals, and orders batches, but
  does not merge concurrent operations (no CRDT convergence/repair yet). Clients receive
  accepted batches with server sequences; conflict resolution is future work.
- Publishing renders a static self-contained HTML export (canonical document JSON embedded)
  and serves it from object storage; there is no CDN integration, custom domain, or
  invalidation pipeline.
- No pagination cursor tokens; `page`/`limit` offsets only, capped at 200 per page.
- `otel` feature builds the dependencies but no exporter/pipeline is configured.
- The `s3` and `otel` feature variants, the axum router wiring, and all handlers are checked
  at compile time; end-to-end HTTP tests were not run (they require PostgreSQL).
