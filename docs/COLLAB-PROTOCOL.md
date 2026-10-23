# Collaboration protocol (draft → implementation incrementally)

Status: **design** for `dp-collab`; implemented in phases 6+. Transport-independent: the
same types and convergence rules are used by the desktop editor (local undo/sync) and the
server (relay/sequencer).

## Goals

1. Concurrent edits converge without whole-document last-write-wins.
2. A lost connection never loses local edits; reconnect replays and reconciles.
3. Server authorizes each op batch (object-level permission) and persists an ordered journal.
4. The protocol is deterministic and testable in Rust without a browser or database.

Non-goals: offline multi-branch merging semantics beyond documented rules; peer-to-peer
sync (server relay only).

## 1. Actors and clocks

* Each editing session has an `ActorId` = random UUID v4, registered in Postgres for the
  user/session (`doc_session`), so two tabs of the same user are distinct actors with
  distinct presence.
* Every op batch gets a `stamp`: `(lamport: u64, actor: Uuid, wall: i64 millis)`.
  Lamport clock is per-actor monotonic: `next = max(seen) + 1`. Ties broken by actor id
  (lexicographic), then wall clock. This is a hybrid logical clock (HLC) with
  `(lamport, actor)` doing the ordering and `wall` retained for diagnostics and
  human-scale conflict messages.
* Vector clocks are tracked per *document*, not per op, only to summarize "who has seen
  what" for presence and initial sync range negotiation; they are not used for conflict
  resolution (the HLC ordering is total and deterministic).

## 2. Replicated data types

`dp-collab` applies `DocOp`s to a `ReplicatedDoc`:

1. **Node map** — LWW-map at the field granularity: each node property (name, transform,
   style.*, text.*, layout.*, ...) is an LWW register with stamp. Concurrent writes to
   different fields of the same node both apply. Writes to the same field resolve by stamp.
2. **Children lists (RGA)** — each child entry has `(stamp, node_id, insertion_id)`. An
   insert carries `after = Option<insertion_id>` meaning "place after this entry" (or at
   the head when `None`). Deletions add tombstones. Concurrent inserts at the same anchor
   are ordered by stamp. Reorders are expressed as move ops: remove from old position
   (tombstone the old entry) + insert at new anchor. This is a standard RGA-with-moves that
   converges deterministically; the sequence is materialized by skipping tombstones.
   Rationale: array CRDTs with move support are exactly this class; an LWW children array
   would lose concurrent inserts.
3. **Styles/variables/assets** — LWW-maps at entry granularity (whole style overwritten by
   stamp; one style edited concurrently in two places resolves to one winner, which the UI
   reports as a conflict for review rather than a silent overwrite notification-less).
4. **Deletes** — tombstones with `deleted_at` stamp; restored nodes are a new insert with
   the old node id but a newer stamp (resurrection). Tombstones are compacted only by a
   checkpoint op that is itself a snapshot boundary (server-side, after retention).
5. **Presence** — ephemeral, not CRDT: `PresenceState { actor, user, cursor: Point,
   viewport, selection: Vec<NodeId>, tool, updated_at }` broadcast at ≤ 20 Hz, dropped
   after 30 s of silence. Not persisted; reconnect re-announces.

## 3. Op batches (wire messages)

```jsonc
{ "type": "hello",   "doc_id": "uuid", "actor": "uuid", "token": "...",
  "last_server_seq": 1234, "schema": 1 }
{ "type": "welcome", "doc_id": "uuid", "actor": "uuid",
  "server_seq": 1300, "snapshot_key": "s3/...", "snapshot_seq": 900,
  "permissions": { "role": "edit", "capabilities": ["view","comment","edit"] },
  "peers": [ PresenceState, ... ] }
{ "type": "ops",     "doc_id": "uuid", "actor": "uuid",
  "base_seq": 1298, "batch": { "batch_id": "uuid", "ops": [ DocOp, ... ],
                               "stamp": { "lamport": 17, "actor": "uuid", "wall": 1700000000000 } } }
{ "type": "ack",     "batch_id": "uuid", "server_seq": 1301, "accepted": true }
{ "type": "reject",  "batch_id": "uuid", "reason": "permission|validation|stale_schema",
  "message": "..." }
{ "type": "rebroadcast", "server_seq": 1302, "actor": "uuid", "batch": { ... } }
{ "type": "presence", ... } { "type": "cursor", ... } { "type": "selection", ... }
{ "type": "sync_from", "from_seq": 1200 }   // client requests missing ops (replay)
{ "type": "ping"/"pong" }
```

Delivery is ordered per client by `server_seq`; clients must apply in `server_seq` order
and may apply local optimistic batches before their ack (rebase on server order when the
ack arrives; ops are commutative by construction, so rebase only needs reordering local
echoes for undo bookkeeping — UI undo remains local-per-actor).

## 4. Local persistence and recovery

* Every local batch is written to the SQLite `op_journal` (doc_id, batch_id, base_seq,
  payload, acked) **before** being sent (write-ahead). On ack it is marked acked and
  pruned after the snapshot covering it.
* On reconnect the client sends `hello.last_server_seq`; if the server has no longer
  retained ops from that point it responds with a fresh snapshot key + `snapshot_seq`, and
  the client rebases unacked local batches on the snapshot.
* Server keeps an op journal per document (Postgres table partitioned by `doc_id`) plus
  periodic snapshots in object storage; retention ≥ 30 days configurable.

## 5. Authorization

* Ops are authorized per batch at the object level: the server resolves `doc_id` →
  project → team and checks the actor's capability (`edit` required for ops, `comment` for
  comment ops, `view` for presence). A batch applying to a document the actor cannot edit
  is rejected as a whole (no partial application).
* The op applier on the server is the same `dp-collab` code as clients, so validation is
  identical; the server is the source of `server_seq` and snapshotting only.
* Uploads (images/fonts) referenced by ops must be owned by the document/project; the op
  validator checks asset ownership when it applies `SetAsset`/image fill ops.

## 6. Conflict policy (explicit, user-visible)

The following conflicts are **reported** (activity feed + optional toast), never silently
resolved:

* Concurrent style/variable entry overwrites (whole-entry LWW): report winner + loser
  values with actor/time.
* Concurrent delete vs edit of the same node: edit wins (node resurrected), reported.
* Component/library updates that conflict with local overrides: apply propagation, keep
  overrides, report "N overrides kept".
* Branch merge conflicts: block merge until resolved (per-node/property choice UI); the
  merge operation is itself an op batch so it is auditable.

## 7. Testing strategy for `dp-collab`

* Convergence property tests: generate random op sequences from N actors with a seeded
  RNG, apply in different delivery orders to independent replicas, assert identical
  materialized state and identical children order.
* Fuzz the op decoder with `arbitrary`-style generators (hand-rolled, no extra deps).
* Reconnect tests: apply batches, drop the journal tail, resync from snapshot, assert state.
* Server integration tests against a real Postgres (testcontainers or a dedicated test DB in
  `docker compose`) for authorization + journal ordering; unit tests use in-memory stores.
