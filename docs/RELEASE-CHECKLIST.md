# Release checklist

This checklist is run before calling a build release-ready. Items are only ticked with
evidence (command output, test name, or a manual verification step recorded in
`DEVELOPMENT-LOG.md`). The platform is developed incrementally; use the milestone table
to see which rows apply to the current build.

## Build

- [ ] `cargo fmt --all -- --check` passes.
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` passes (documented allow-list
      only for third-party code).
- [ ] `cargo test --workspace` passes without a database or Docker installed.
- [ ] `cargo build --release -p iconcrafter-desktop -p iconcrafter-server` passes.
- [ ] `cargo check -p iconcrafter-server --features s3,otel` passes.
- [ ] `pnpm --dir apps/ui typecheck && pnpm --dir apps/ui test && pnpm --dir apps/ui build`
      passes.
- [ ] `cargo tauri build` produces an installer (NSIS/MSI) on the target OS, and the
      installed app launches, opens a document, edits, saves, reopens and exports.

## Functional smoke test (vertical-slice build)

- [ ] New document opens with the editor shell and an empty/example page.
- [ ] Create frame, rectangle, ellipse, line, polygon, star, text; all render crisply.
- [ ] Pan (wheel/space+drag/middle), zoom (ctrl+wheel, buttons, fit/100%/selection),
      rulers and guides update correctly.
- [ ] Select by click, shift-click and marquee; move, resize, rotate; snapping and
      alignment guides appear and release cleanly.
- [ ] Layers panel shows the real tree, rename/reorder/hide/lock round-trip.
- [ ] Inspector edits transform, fill (solid + gradient), stroke, corner radius, opacity;
      canvas reflects changes immediately.
- [ ] Undo/redo across create, move, style, group, delete; coalesced nudges undo in one step.
- [ ] Save `.icraft`; reopen in a fresh process; document is identical (node ids, styles,
      text content, undo history reset, revision preserved).
- [ ] Export PNG at 1x/2x and SVG; SVG re-imports as editable geometry.
- [ ] Command palette executes real commands; shortcuts are customizable and persist.
- [ ] Closing with unsaved changes prompts; crash/autosave journal recovers the last edits.

## Backend smoke test (requires Postgres; Docker optional)

- [ ] Migrations apply to an empty database (`sqlx migrate run` or server startup).
- [ ] Register/login/logout; session cookie is HttpOnly and expires; password hashes are
      Argon2id and never logged.
- [ ] Unauthorized access to another team's file returns 403; viewer cannot edit; editor
      cannot manage members; owner/admin can.
- [ ] Comment create/edit/resolve/delete; audit events recorded.
- [ ] Asset upload enforces size and magic-byte type; content-addressed dedupe works;
      traversal attempts rejected.
- [ ] Version snapshot/named/restore/diff endpoints return correct node-level changes.
- [ ] Share link with view permission reads; password-protected share rejects wrong
      password; revoked/expired share returns 404/403.
- [ ] WS relay: two clients join one file, ops batches are journaled once (idempotent),
      presence leaves on disconnect, oversized frames are rejected.
- [ ] `/healthz` and `/readyz` behave (readyz fails when DB is down).

## Security & reliability

- [ ] No hardcoded secrets; `.env.example` only. Secrets loaded from env and redacted in logs.
- [ ] Database migrations are additive; destructive changes documented with backup steps.
- [ ] Upload limits, request body limits, pagination caps and rate limits verified.
- [ ] Importers treat SVG/images/fonts as untrusted input (size/type validation).
- [ ] Plugin runtime (when enabled) runs sandboxed with declared permissions only.
- [ ] Logs contain no passwords, tokens or document contents.
- [ ] Dependency audit run (`cargo audit` if available; record advisories and decisions).
- [ ] Backups/restore procedure documented for PostgreSQL and object storage.

## Performance

- [ ] Benchmarks exist for document ops, hit testing, rendering and save/reopen, and match
      the budgets in `docs/ARCHITECTURE.md` on a representative large document.
- [ ] Editor stays interactive while dragging in a 10k-node document (measured frame times).
- [ ] No unbounded memory growth while panning/zooming for several minutes.

## Documentation

- [ ] `docs/FEATURE-CHECKLIST.md` matches reality (implemented/verified/limited/planned).
- [ ] `docs/DEVELOPMENT-LOG.md` has an entry for the build.
- [ ] `README.md` quick start works from a clean checkout.
- [ ] KNOWN LIMITATIONS section current (no invented compatibility claims).
- [ ] `apps/ui/README.md`, `apps/backend/README.md` describe how to run each app.

## Milestone gates

| Milestone | Gate |
|---|---|
| Phase 3 vertical slice | Functional smoke test rows 1–13 fully pass in the running desktop app |
| Phase 4 editor depth | Vector/typography/auto-layout/components rows verified with tests + app |
| Phase 5–7 | Prototype, Dev Mode, backend, workspaces verified per checklist |
| Phase 8 release | Security, performance, packaging and docs items all ticked |


## Status (2026-10-24, session 13)

Verified with evidence (see DEVELOPMENT-LOG / TESTING):

- [x] cargo test -p ... full engine suite: 266 tests green (13 crates; dp-ai/dp-plugins are stubs with 0 tests until Phase 7).
- [x] Backend unit suite: 132 tests green (cargo test -p iconcrafter-server --offline --lib).
- [x] Strict clippy clean for dp-devmode/dp-editor/dp-collab/dp-model/dp-render (others pass default clippy; workspace-wide -D warnings gate is not yet free of third-party noise).
- [x] Production desktop binary builds and runs standalone: cargo build -p iconcrafter-desktop --release --features custom-protocol — embedded UI at http://tauri.localhost/, save/reopen/export verified with the dev server stopped.
- [x] Benchmarks exist with reported numbers (docs/PERFORMANCE.md) and one optimization landed (spatial-index hit testing; per-frame layout rebuild removed).

Still open for a release candidate: NSIS/MSI installer (cargo tauri build — needs the Tauri CLI), backend integration tests with a live Postgres (Docker not installed on this machine), dependency audit (cargo audit), accessibility pass, and workspace modules (whiteboard/slides/website) which are planned not implemented.
