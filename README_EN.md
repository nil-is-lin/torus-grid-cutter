<p align="right">
  <a href="./README.md">中文</a> | <strong>English</strong>
</p>

# Torus Grid Cutter

[![CI](https://github.com/nil-is-lin/torus-grid-cutter/actions/workflows/ci.yml/badge.svg)](https://github.com/nil-is-lin/torus-grid-cutter/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/nil-is-lin/torus-grid-cutter)](https://github.com/nil-is-lin/torus-grid-cutter/releases)
[![Documentation](https://img.shields.io/badge/docs-LaTeX-blue)](doc/)

A **wgpu + egui** based torus mesh cutting and UV-unwrapping tool: parametrically generate torus meshes (quad / periodic Delaunay / OBJ), cut them into patches along U/V grid lines or Torus Knot curves, with 42 shader rendering modes, a UV-plane unwrap view, and per-patch OBJ export.

## Features

- **Mesh generation**: quad mesh (adjustable U/V resolution), periodic Delaunay triangulation (Poisson-disk sampling + edge flips), OBJ model loading
- **Cutting**:
  - U/V Grid cutting: in Quad mode the cut lines align to the mesh vertex lines, **preserving quad faces** (no triangulation)
  - Torus Knot curve cutting (`v = k·u + 2πn` for all branches), affecting only faces that intersect the curve; two `(1,k₁)`, `(1,k₂)` curves split the torus into exactly `|k₂−k₁|` topological regions (region ids come from an analytic invariant, no flood-fill)
- **Patch management**: per-patch visibility, materials, and per-patch shader override (42 modes: PBR / Glass / X-Ray / Toon / Holographic…)
- **Dual views**: 3D torus view (orbit camera, lighting presets, smooth/flat shading) and UV-plane unwrap view (`(u·R, v·r)` mapping), switchable from the right-docked panel
- **Region rendering**: per-patch coloring (Rainbow / Checkerboard / Heatmap / Grayscale)
- **Mesh statistics**: closedness / boundary loop count / Euler characteristic / surface area / volume / average face degree
- **Export**: OBJ / STL / PLY (ASCII), for the whole mesh or batch-per-patch

## Demo

![demo](assets/demo.gif)

> The demo video ships with the repo: the full mp4 lives in `docs/demo.mp4`, hosted via **GitHub Pages**, so it plays in the browser on click (no forced download). In the repo **Settings → Pages**, set Source to the `/docs` directory of the `main` branch.

[▶ Watch the full video (15 MB)](https://nil-is-lin.github.io/torus-grid-cutter/demo.mp4)

Recording tips: window 1280×720, OBS / Windows Game Bar both work; recommended length 1–3 minutes.
Upload steps: drop `demo.mp4` into this repo's `docs/` directory and commit & push.

## Build

```bash
# Debug
cargo run

# Release
cargo build --release
```

Requires Rust 1.85+ (edition 2021). Depends on wgpu 24 / egui 0.31 / winit 0.30.

## Workflow

The UI is organized into 4 step pages following the algorithm pipeline:

1. **Mesh** — choose mesh type (Quad / Delaunay / OBJ) and set resolution and torus parameters
2. **Cut** — choose cut mode (Grid / Knot), configure cut lines and execute
3. **Shader** — patch-level appearance: visibility, material, per-patch shader override
4. **Export** — export whole mesh or per patch (OBJ/STL/PLY)

Right panel (persistent): view mode, background, lighting, edge lines, global shader & params, face color, smooth/flat shading, mesh statistics.

## Architecture

```
src/
├── main.rs / lib.rs     entry
├── app.rs               app state machine: mesh build / cut / render-state rebuild / input handling
├── mesh/                core algorithms (pure data, no rendering deps)
│   ├── mod.rs           module re-exports
│   ├── half_edge.rs     half-edge mesh (vertex / half-edge / face / edge flip / triangulation)
│   ├── build.rs         mesh constructors (quad / Delaunay / OBJ)
│   ├── torus.rs         torus parameterization & unfold (unfold_position)
│   ├── cut.rs           U/V/Knot cutting, patch index assignment
│   ├── delaunay.rs      periodic Delaunay generation
│   ├── surface.rs       torus surface fitting
│   ├── stats.rs         mesh statistics
│   ├── uv.rs            UV mapping helpers
│   ├── vertex.rs        vertex types / helpers
│   └── obj_loader.rs    OBJ parser
├── render/              wgpu rendering: pipelines / vertex buffers / wireframe (mesh edges, cut lines, Knot curves)
├── ui/panel.rs          egui panel (4 workflow pages)
├── color_scheme.rs      patch color schemes
├── camera.rs            orbit camera
└── export/              OBJ / STL / PLY export (obj.rs / stl.rs / ply.rs)
```

Design notes:

- **Cutting algorithm independent of UI**: the `mesh/` module is pure data and independently testable (110+ unit tests + export integration tests)
- **Single source of truth for cut & render position**: `UiState::loop_u_position` drives both the cut line and the render line, keeping the display consistent
- **Docked UI**: the egui panel docks to the right of the render window (resizable); global display settings persist while workflow ops switch by step
- **UV as primary domain, 3D as mapping**: mesh generation, cut intersection, and region division all happen in the UV plane; the 3D view is just a mapping of `torus_position`, and both views show the same UV geometry
- **In-face localization of cut intersections**: when inserting an intersection, search for the target half-edge **within the face boundary** (instead of walking around a vertex) — seam-copy vertices in the unfolded mesh have a boundary outgoing edge with `twin = MAX`; walking around a vertex exits early and misses cuts, whereas in-face search always hits and is reliable at seams/boundaries
- **Alternating U/V cut iteration**: a face cut by one line may still cross another (e.g., a quad produced by a V-cut still spans a U line) — `cut_mesh_by_grid` repeatedly applies all cut lines until the face count stops changing (usually converges in 2–3 rounds), guaranteeing **no face crosses any cut line**
- **All-triangle after Delaunay cut**: a cut line through a triangle necessarily yields a quad + triangle; after cutting, a unified finalize-triangulation pass (fan triangulation recursing on newly produced faces) cleans up — Quad-mesh grid cuts don't cross faces so the face type is unaffected: Delaunay meshes are all triangles after any cut, Quad meshes stay quads
- **Delaunay topology cache**: Poisson sampling + triangulation depend only on the point count (independent of R/r); dragging R/r only remaps vertex 3D positions (`torus_position`), skipping repeated sampling and triangulation — at 2000 points the rebuild drops from ~4.2ms to 0.38ms
- **Analytic model direct construction**: Quad/Delaunay meshes are generated from parametric equations, so their `SurfaceModel` is built directly from (R, r) with no numeric fitting (fitting is only used for OBJ import) — this avoids the numeric-fitting path misclassifying a definite torus as Unknown
- **Full OBJ import/export UV chain**: export writes `vt` (`f v/vt//vn`); on import, if UV is degenerate (old files without vt) the original 3D positions are kept and UV→3D remap is skipped — avoiding vertex collapse to a single point and invisibility
- **GPU buffer cap protection**: when mesh-edge/cut-line buffers exceed `device.limits().max_buffer_size` (usually 256MB), degrade by skipping edge-line rendering with a warning — under extreme params (fine mesh + many cut lines producing 80k+ faces) the program doesn't crash and face rendering continues
- **Cut convergence protection**: at U/V line crossings (a line coinciding with a Delaunay vertex) repeated cutting can cause exponential face-count growth — during iteration, stop with a warning if the face count exceeds 150k

## Testing

```bash
cargo test        # 110+ unit tests + export integration tests (incl. missed-cut detection, centroid-region consistency, all-triangle invariants)
cargo clippy      # static analysis (CI enforces -D warnings)
```

Algorithm details are in [`doc/`](doc/) (LaTeX docs: torus.tex / knot_algorithm.tex / algorithm.tex / flowchart.tex).

## License

[MIT](LICENSE)
