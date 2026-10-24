# Performance

Measured on the development machine (Windows 11, AMD Ryzen-class CPU, debug
profile with `opt-level = 1` for our crates and `opt-level = 2` for
dependencies, `debug = 0`, `incremental = false`). Reproduce with:

```powershell
cargo run -p dp-editor --example bench_editor
```

The benchmark builds a 500-frame grid with 20 children per frame = **10,500
nodes**, then measures the operations the UI performs per interaction.

## Measured (2026-10-24, session 12; machine idle)

| Operation | Result | Notes |
|---|---|---|
| Cold layout, whole page | 10.3 ms | first full auto-layout pass **and** spatial-index build |
| Layout x10 (invalidated each time) | 134.0 ms total (≈ 13.4 ms each) | index rebuild included |
| Node bounds x1000 | 0.1 ms | cached `LayoutMap` lookup |
| Render 1440×900, 3 frames | 125.8 ms total (≈ 42 ms/frame) | CPU tiny-skia, 5,184,000 bytes/frame (RGBA8) |
| Pointer click x200 (down+up each) | 196.1 ms (≈ 0.98 ms/click) | spatial-index hit test; includes selection change and overlay rebuild |
| Serialize document (JSON) | 111.7 ms, 6.9 MiB | canonical JSON |
| Deserialize + validate | 92.8 ms | 10,500 nodes |
| Delete 50 nodes + undo | 47.4 ms | undoable transaction |

Two real fixes came out of these measurements:

1. **Hit testing / marquee** now use a `dp_geom::SpatialGrid` built with the
   layout cache (invalidated on document change): ≈2.0 ms → ≈1.0 ms per
   click update (inside the 1 ms target), independent of total node count for
   small click areas.
2. **Render no longer invalidates layout per frame.** It previously rebuilt
   layout for the whole document on every call (≈10 ms for 10,500 nodes); that
   was a correctness-neutral bug that made every presented frame O(nodes).

## Honest assessment against the targets in docs/ARCHITECTURE.md

- Layout, bounds lookup and hit testing are inside targets on this document.
- Full-page serialization is fast enough for autosave on a background thread.
- **Render is still above the 16 ms interaction target** (~42 ms/frame) when the
  viewport contains many visible nodes. The remaining cost is scene building +
  per-item tiny-skia rasterization + the straight-alpha conversion for IPC; next
  optimizations: static subtree culling via the spatial index, batching by paint
  and avoiding the full-frame alpha conversion for the GPU-ready path.
- **Pointer click at ~1 ms** still rebuilds overlays for every event; overlay
  construction is the next target.
- These numbers are *dev-profile* numbers. A release build is expected to be
  faster (LTO), but we only quote measured numbers, so no release claims are
  made here until the release benchmark is run.

## Release-build interaction benchmark (2026-10-24, session 21)

Protocol: installed standalone release binary (`cargo build -p iconcrafter-desktop
--release --features custom-protocol`), embedded UI at `http://tauri.localhost/`,
WebView2 (Chromium 155), window 1440×900 with panels; engine viewport 1200×800
dpr 1 unless noted. Each `render_frame` timed from the page with
`window.__TAURI_INTERNALS__.invoke` (includes CPU raster + IPC transfer). The
"before" column is the same protocol after the lag report (user: "It's lagging a
ton") and before the session-21 fixes.

| Operation | Before | After |
|---|---|---|
| `bootstrap` | 2.28 ms | 1.91 ms |
| `engine_state` | 4.9–54 ms (first-run noise) | 2.11 ms |
| Full render 1200×800 dpr 1 | 77.28 ms | **46.6 ms** |
| Draft render 1200×800 (quality 0.4) | — | **6.4–8.8 ms** |
| Full render dpr 2 | 337.23 ms | **137.1 ms** |
| Draft render dpr 2 (quality 0.4) | — | **39.0 ms** |
| Pointer move → engine | — | 2.58 ms |
| Synthetic 90-step drag | ≈13 fps | **≈85 fps painted** (46 frames / 542 ms) |

Session-21 fixes (all verified by the numbers above):

1. **Clip-mask cache in the CPU rasterizer.** Every child of a clipping frame
   used to allocate and fill a full-target `Mask` per item (300 hidden vector
   nodes ⇒ 300 × full-window masks). Masks are now cached per distinct clip rect
   for the whole frame, and clips covering the target skip masking entirely.
2. **Scaled rendering (`RenderTarget::render_scaled`).** The renderer accepts a
   quality scale (0.1–2.0), so gestures paint at 40 % resolution and the browser
   upscales; full quality is drawn on interaction end. IPC `render_frame`
   accepts numeric quality 0.25–1.0 (strings `draft`/`low` = 0.5).
3. **Demultiplied IPC conversion.** Frames cross the IPC boundary through
   tiny-skia's `take_demultiplied()` instead of a hand-written per-pixel loop.
4. **UI commit/pointer coalescing.** Engine snapshots commit once per animation
   frame and pointer/wheel flush on rAF; painting is skipped while the engine
   viewport disagrees with the measured CSS size (that mismatch used to stretch
   the frame canvas and made selection outlines look "wrong").

Remaining, honest gaps: a full-quality frame at dpr 2 is still 137 ms (above the
16 ms interaction target) and every interaction ends with one full-quality
render; the GPU path and a tile/scene cache remain the next step, so this table
does not claim 60 fps at full quality on retina displays.

## Reproducible protocol

1. `git checkout <release commit>` and confirm a clean tree.
2. Run the command above twice; use the second run (warm file cache).
3. Record the machine CPU and OS version next to the numbers.
4. Compare only same-protocol runs; do not extrapolate to larger documents
   without also running `BENCH_FRAMES` (e.g. `$env:BENCH_FRAMES=2000`).
