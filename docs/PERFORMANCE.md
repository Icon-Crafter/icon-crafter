# Performance

Measured on the development machine (Windows 11, AMD Ryzen-class CPU, debug
profile with `opt-level = 1` for our crates and `opt-level = 2` for
dependencies, `debug = 0`, `incremental = false`). Reproduce with:

```powershell
cargo run -p dp-editor --example bench_editor
```

The benchmark builds a 500-frame grid with 20 children per frame = **10,500
nodes**, then measures the operations the UI performs per interaction.

## Measured (2026-10-24, session 11)

| Operation | Result | Notes |
|---|---|---|
| Cold layout, whole page | 11.7 ms | first full auto-layout pass **and** spatial-index build |
| Layout x10 (invalidated each time) | 122.4 ms total (≈ 12.2 ms each) | index rebuild included |
| Node bounds x1000 | 0.1 ms | cached `LayoutMap` lookup |
| Render 1440×900, 3 frames | 161.5 ms total (≈ 54 ms/frame) | CPU tiny-skia, 5,184,000 bytes/frame (RGBA8) |
| Pointer click x200 (down+up each) | 241.4 ms (≈ 1.2 ms/click) | spatial-index hit test; includes selection change and overlay rebuild |
| Serialize document (JSON) | 101.1 ms, 6.9 MiB | canonical JSON |
| Deserialize + validate | 90.4 ms | 10,500 nodes |
| Delete 50 nodes + undo | 42.8 ms | undoable transaction |

Hit testing now uses a `dp_geom::SpatialGrid` built with the layout cache: click
cost dropped from ≈2.0 ms to ≈1.2 ms per click (≈39%) and no longer scales with
total node count as long as the click area is small. The index costs ≈5 ms to
build on a cold layout for 10,500 nodes.

## Honest assessment against the targets in docs/ARCHITECTURE.md

- Layout, bounds lookup and hit testing are inside targets on this document.
- Full-page serialization is fast enough for autosave on a background thread.
- **Render is above the 16 ms interaction target** for this document when the
  viewport contains many visible nodes; culling currently iterates the whole
  display list. Next optimization: back the scene cull with the same spatial
  index and cache the static subtree order, then re-measure.
- **Pointer click at ~1.2 ms** still rebuilds overlays for every event; the
  overlay construction is the next target (rebuild overlays only on state
  change / visible-world changes).
- These numbers are *dev-profile* numbers. A release build is expected to be
  faster (LTO), but we only quote measured numbers, so no release claims are
  made here until the release benchmark is run.

## Reproducible protocol

1. `git checkout <release commit>` and confirm a clean tree.
2. Run the command above twice; use the second run (warm file cache).
3. Record the machine CPU and OS version next to the numbers.
4. Compare only same-protocol runs; do not extrapolate to larger documents
   without also running `BENCH_FRAMES` (e.g. `$env:BENCH_FRAMES=2000`).
