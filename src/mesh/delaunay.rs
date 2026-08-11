use crate::mesh::half_edge::{HalfEdgeId, HalfEdgeMesh};
use crate::mesh::torus;
use glam::{Vec2, Vec3};
use std::f64::consts::PI;

/// Randomize a triangle mesh by performing random edge flips.
///
/// This preserves vertex positions and UVs, only changing the
/// triangulation connectivity. Multiple passes ensure thorough
/// randomization.
pub fn randomize_mesh_by_edge_flips(
    mesh: &HalfEdgeMesh,
    num_passes: usize,
    seed: u64,
) -> HalfEdgeMesh {
    let mut mesh = mesh.clone();
    let mut rng = SimpleRng::new(seed);

    for pass in 0..num_passes {
        // Collect all interior edges (shared by two faces, one direction only)
        let mut edges: Vec<HalfEdgeId> = Vec::new();
        let mut visited = vec![false; mesh.half_edges.len()];
        for (i, he) in mesh.half_edges.iter().enumerate() {
            if visited[i] {
                continue;
            }
            visited[i] = true;
            if he.twin.0 != usize::MAX && he.twin.0 < mesh.half_edges.len() {
                visited[he.twin.0] = true;
                if i < he.twin.0 {
                    edges.push(HalfEdgeId(i));
                }
            }
        }

        // Fisher-Yates shuffle
        for i in (1..edges.len()).rev() {
            let j = (rng.next() * (i + 1) as f64).floor() as usize;
            edges.swap(i, j);
        }

        // Try to flip each edge
        let mut flip_count = 0;
        for &edge in &edges {
            if mesh.flip_edge(edge) {
                flip_count += 1;
            }
        }

        log::info!(
            "Randomize pass {}: {}/{} edges flipped",
            pass + 1,
            flip_count,
            edges.len()
        );
    }

    if !mesh.validate() {
        log::error!("Mesh validation FAILED after randomization!");
    }

    mesh
}

pub fn generate_delaunay_mesh(
    major_r: f64,
    minor_r: f64,
    num_points: usize,
    seed: u64,
) -> (Vec<Vec3>, Vec<Vec2>, Vec<crate::mesh::TriIndex>) {
    let two_pi = 2.0 * PI;

    let mut rng = SimpleRng::new(seed);

    // 边界固定点（与 quad 网格的接缝方案一致）：四条边（u=0、u=2π、v=0、v=2π）
    // 各放 boundary_n 个固定点。对边（如 v=0 与 v=2π）是**独立顶点**（3D 位置
    // 周期重合），Delaunay 不会连接它们（距离 2π，空圆必含中间点）→ 网格在
    // 接缝处天然展开（Planar UV 视图规整，3D 视图视觉闭合），无需后续切开。
    let boundary_n = (num_points / 20).clamp(8, 64);

    let mut uvs: Vec<Vec2> = Vec::with_capacity(num_points);
    let step = two_pi / boundary_n as f64;
    // 四条边（避开角点，角点单独添加 4 个）。
    // 边界点加 ~1e-4 微扰：delaunator 对严格共线点（同一边上的点）会输出
    // 退化/异常三角形。微扰必须**沿边方向**（保持点在凸包边上）——
    // 若垂直推离边，点会落入凸包内部，凸包退化为 4 个角点，产生横跨
    // 矩形的大三角形（视觉裂缝）。
    let jitter = 1e-4;
    for j in 0..boundary_n {
        let t = step * j as f64;
        let jx = rng.next() * jitter;
        let jy = rng.next() * jitter;
        uvs.push(Vec2::new(0.0, (t + jx) as f32)); // u=0 边（沿 v 微扰）
        uvs.push(Vec2::new(two_pi as f32, (t + jy) as f32)); // u=2π 边（沿 v 微扰）
        if j > 0 && j < boundary_n - 1 {
            uvs.push(Vec2::new((t + jx) as f32, 0.0)); // v=0 边（沿 u 微扰）
            uvs.push(Vec2::new((t + jy) as f32, two_pi as f32)); // v=2π 边（沿 u 微扰）
        }
    }
    // 四个角点（独立顶点）
    uvs.push(Vec2::new(0.0, 0.0));
    uvs.push(Vec2::new(two_pi as f32, 0.0));
    uvs.push(Vec2::new(0.0, two_pi as f32));
    uvs.push(Vec2::new(two_pi as f32, two_pi as f32));

    // 内部随机点（避开边界，避免与边界点形成退化细长三角）
    let interior = num_points.saturating_sub(uvs.len());
    let margin = step * 0.5;
    for _ in 0..interior {
        let u = margin + rng.next() * (two_pi - 2.0 * margin);
        let v = margin + rng.next() * (two_pi - 2.0 * margin);
        uvs.push(Vec2::new(u as f32, v as f32));
    }

    let positions: Vec<Vec3> = uvs
        .iter()
        .map(|uv| torus::torus_position(uv.x as f64, uv.y as f64, major_r, minor_r))
        .collect();

    // 非周期 Delaunay：矩形域 [0,2π]² 直接三角化（接缝处天然开放，与 quad 网格一致）
    let triangles = periodic_delaunay(&uvs, false, false, two_pi as f32, two_pi as f32);

    log::info!(
        "Delaunay: {} points ({} boundary + {} interior) → {} triangles",
        uvs.len(),
        4 * boundary_n,
        interior,
        triangles.len()
    );

    (positions, uvs, triangles)
}

/// 沿 UV 接缝切开周期网格（参考 quad 网格的接缝方案）。
///
/// 周期 Delaunay 输出的是"闭合环面"拓扑：跨接缝三角形连接 u≈0 与 u≈2π 的顶点，
/// 在 Planar UV（Unfolded）视图中会横跨整个平面显示。
/// 本函数把跨接缝三角形重定向到复制出的接缝顶点副本（u−2π 或 u+2π、v 同理），
/// 得到与 quad 网格一致的"展开"网格：
///   - UV 域内无跨接缝三角形（Unfolded 视图规整）
///   - 副本顶点 3D 位置与原顶点重合（torus_position 周期 2π）→ 3D 视图视觉闭合
///
/// 返回新顶点 UV 与新三角形（顶点顺序与输入一一对应，另附副本）。
struct SimpleRng {
    state: u64,
}

impl SimpleRng {
    fn new(seed: u64) -> Self {
        SimpleRng {
            state: if seed == 0 { 1 } else { seed },
        }
    }

    fn next(&mut self) -> f64 {
        self.state = self
            .state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (self.state >> 33) as f64 / (1u64 << 31) as f64
    }
}

fn periodic_delaunay(
    uvs: &[Vec2],
    periodic_u: bool,
    periodic_v: bool,
    period_u: f32,
    period_v: f32,
) -> Vec<(usize, usize, usize)> {
    let n = uvs.len();
    if n < 3 {
        return Vec::new();
    }

    // Use the actual data range for centroid filtering.
    let (min_x, _max_x, min_y, _max_y) = uvs.iter().fold(
        (f32::MAX, f32::MIN, f32::MAX, f32::MIN),
        |(a, b, c, d), p| (a.min(p.x), b.max(p.x), c.min(p.y), d.max(p.y)),
    );

    let du_range: Vec<i32> = if periodic_u { vec![-1, 0, 1] } else { vec![0] };
    let dv_range: Vec<i32> = if periodic_v { vec![-1, 0, 1] } else { vec![0] };
    let num_tiles = du_range.len() * dv_range.len();
    let mut tiled: Vec<delaunator::Point> = Vec::with_capacity(num_tiles * n);
    let mut orig_idx: Vec<usize> = Vec::with_capacity(num_tiles * n);

    for &du in &du_range {
        for &dv in &dv_range {
            for (i, p) in uvs.iter().enumerate() {
                tiled.push(delaunator::Point {
                    x: (p.x + du as f32 * period_u) as f64,
                    y: (p.y + dv as f32 * period_v) as f64,
                });
                orig_idx.push(i);
            }
        }
    }

    let result = delaunator::triangulate(&tiled);

    let mut out: Vec<(usize, usize, usize)> = Vec::new();

    // Centroid filter: keep a triangle only if its centroid falls inside
    // the central tile [min_x, min_x + period_u) × [min_y, min_y + period_v).
    // Each triangle has exactly one centroid, so it is counted exactly once
    // regardless of how many tiles its vertices span.
    let eps_f64 = 1e-6f64;
    let central_x_lo = min_x as f64 - eps_f64;
    let central_x_hi = (min_x as f64) + (period_u as f64) + eps_f64;
    let central_y_lo = min_y as f64 - eps_f64;
    let central_y_hi = (min_y as f64) + (period_v as f64) + eps_f64;
    let in_central = |x: f64, y: f64| -> bool {
        x >= central_x_lo && x < central_x_hi && y >= central_y_lo && y < central_y_hi
    };

    for chunk in result.triangles.chunks(3) {
        let a = chunk[0];
        let b = chunk[1];
        let c = chunk[2];

        let oa = orig_idx[a];
        let ob = orig_idx[b];
        let oc = orig_idx[c];

        if oa == ob || ob == oc || oa == oc {
            continue;
        }

        let pa = &tiled[a];
        let pb = &tiled[b];
        let pc = &tiled[c];

        // Centroid check
        let cx = (pa.x + pb.x + pc.x) / 3.0;
        let cy = (pa.y + pb.y + pc.y) / 3.0;
        if !in_central(cx, cy) {
            continue;
        }

        // Skip zero-area triangles in UV space (co-located points)
        let uv_area = ((pb.x - pa.x) * (pc.y - pa.y) - (pc.x - pa.x) * (pb.y - pa.y)).abs() * 0.5;
        if uv_area < 1e-12 {
            continue;
        }

        out.push((oa, ob, oc));
    }

    out.sort_by_key(|&(a, b, c)| {
        let mut v = [a, b, c];
        v.sort();
        (v[0], v[1], v[2])
    });
    out.dedup_by(|a, b| {
        let mut va = [a.0, a.1, a.2];
        let mut vb = [b.0, b.1, b.2];
        va.sort();
        vb.sort();
        va == vb
    });

    log::info!(
        "Periodic Delaunay: {} tiled pts → {} final (period_u={:.4}, period_v={:.4})",
        tiled.len(),
        out.len(),
        period_u,
        period_v
    );

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    // ── 测试专用：周期 Delaunay 转换（生产代码使用 randomize_mesh_by_edge_flips）──
    // 以下函数仅被本模块测试使用，保留以覆盖周期网格转换算法。
    pub fn convert_to_delaunay(mesh: &HalfEdgeMesh) -> HalfEdgeMesh {
        if mesh.vertices.is_empty() {
            return HalfEdgeMesh::new();
        }

        let raw_positions: Vec<Vec3> = mesh.vertices.iter().map(|v| v.position).collect();
        let raw_uvs: Vec<Vec2> = mesh.vertices.iter().map(|v| v.uv).collect();

        // Check if meaningful UVs exist by looking at the UV range.
        // The old check (length_squared > 0) was unreliable: a mesh with valid
        // UVs could still fail if every vertex happened to have UV near the
        // origin.  Range-based detection is robust against translation.
        let (min_uv_u, max_uv_u, min_uv_v, max_uv_v) = raw_uvs.iter().fold(
            (f32::MAX, f32::MIN, f32::MAX, f32::MIN),
            |(a, b, c, d), uv| (a.min(uv.x), b.max(uv.x), c.min(uv.y), d.max(uv.y)),
        );
        let has_uv = (max_uv_u - min_uv_u) > 1e-6 || (max_uv_v - min_uv_v) > 1e-6;

        if !has_uv {
            // No usable UVs — fall back to standard (non-periodic) Delaunay.
            let (positions, uvs, _) = merge_duplicate_vertices(&raw_positions, &raw_uvs);
            let triangles = standard_delaunay(&uvs);
            if triangles.is_empty() {
                log::warn!("Delaunay conversion produced no triangles");
                return mesh.clone();
            }
            log::info!(
                "Convert to Delaunay (no UV): {} vertices → {} triangles",
                positions.len(),
                triangles.len()
            );
            return HalfEdgeMesh::from_triangles(&positions, &uvs, &triangles);
        }

        let (min_u, max_u, min_v, max_v) = (min_uv_u, max_uv_u, min_uv_v, max_uv_v);
        let u_range = max_u - min_u;
        let v_range = max_v - min_v;

        log::info!(
            "UV range: u=[{:.4}, {:.4}] ({:.4}), v=[{:.4}, {:.4}] ({:.4})",
            min_u,
            max_u,
            u_range,
            min_v,
            max_v,
            v_range
        );

        // ── Early handling for non-triangle faces (quads, etc.) ──
        //
        // Quad meshes (e.g. generated torus) have implicit periodicity through
        // index wrapping (modulo), NOT through seam-duplicate vertices.  The
        // detect_periodicity function requires seam duplicates, so it would
        // return false and the conversion would be skipped.
        //
        // Instead, we detect quad faces and triangulate them directly.  The
        // original quad connectivity already encodes the correct torus topology
        // (χ=0), so the triangulated result inherits this property.
        let has_non_triangle = mesh.faces.iter().enumerate().any(|(fi, f)| {
            if !f.valid {
                return false;
            }
            mesh.face_half_edges(crate::mesh::half_edge::FaceId(fi))
                .len()
                != 3
        });

        if has_non_triangle {
            log::info!("Non-triangle faces detected — triangulating directly");
            return triangulate_faces(mesh);
        }

        let (periodic_u, periodic_v) =
            detect_periodicity(&raw_positions, &raw_uvs, min_u, max_u, min_v, max_v);

        log::info!(
            "Periodicity detection: u={}, v={}",
            if periodic_u {
                "periodic"
            } else {
                "non-periodic"
            },
            if periodic_v {
                "periodic"
            } else {
                "non-periodic"
            },
        );

        if !periodic_u && !periodic_v {
            log::warn!(
                "Mesh is not periodic — Delaunay conversion skipped. \
                 The UV domain does not wrap in either direction, so \
                 standard Delaunay in UV space would produce incorrect \
                 triangles. Only periodic surfaces (torus-like) are supported."
            );
            return mesh.clone();
        }

        // ── Wrap seam UVs to the min boundary ─────────────────────────
        //
        // On a torus, the UV seam has two copies of each vertex:
        //   copy A: 3D pos P, UV = (u_min, v)
        //   copy B: 3D pos P, UV = (u_max, v)   ← same position, different UV
        //
        // If we keep both copies, they end up far apart in the normalized
        // [0, 2π) domain (one near 0, one near 2π).  The periodic Delaunay
        // treats them as separate points and produces degenerate (zero-area)
        // triangles connecting co-located copies, plus overlapping duplicate
        // triangles.  This gives Euler characteristic χ=2 (sphere) instead
        // of χ=0 (torus).
        //
        // Fix: wrap the max-boundary UVs back to min so both copies become
        // (P, (u_min, v)) and merge into a single vertex.  The periodic
        // Delaunay then reconstructs the correct torus topology with ghost
        // copies at ±2π.
        let seam_threshold = 0.1; // fraction of range
        let u_seam_hi = max_u - u_range * seam_threshold;
        let v_seam_hi = max_v - v_range * seam_threshold;

        let mut canonical_uvs = raw_uvs.clone();
        let mut wrapped_u = 0usize;
        let mut wrapped_v = 0usize;
        for uv in canonical_uvs.iter_mut() {
            if periodic_u && u_range > 0.0 && uv.x > u_seam_hi {
                uv.x -= u_range; // wrap max → min
                wrapped_u += 1;
            }
            if periodic_v && v_range > 0.0 && uv.y > v_seam_hi {
                uv.y -= v_range; // wrap max → min
                wrapped_v += 1;
            }
        }

        // Snap near-boundary UVs to exact min value.
        // After wrapping, seam copies have similar but not identical UVs
        // (e.g. 0.003 vs 0.005).  Snapping aligns them exactly so the merge
        // step can collapse them.  The snap threshold must be smaller than
        // half the grid spacing to avoid catching interior vertices.
        let snap_eps = 0.015;
        for uv in canonical_uvs.iter_mut() {
            if periodic_u && (uv.x - min_u).abs() < snap_eps {
                uv.x = min_u;
            }
            if periodic_v && (uv.y - min_v).abs() < snap_eps {
                uv.y = min_v;
            }
        }
        log::info!("UV seam wrap: {} u-wraps, {} v-wraps", wrapped_u, wrapped_v);

        // Now merge by (position, UV).  After wrapping, seam copies share
        // the same (pos, UV) and collapse into one vertex.
        let (merged_positions, merged_uvs, merge_map) =
            merge_duplicate_vertices(&raw_positions, &canonical_uvs);

        log::info!(
            "Vertex merge (pos+UV): {} raw → {} merged",
            raw_positions.len(),
            merged_positions.len()
        );

        // ── Strategy: use original mesh topology mapped to merged vertices ──
        //
        // The periodic 2D Delaunay consistently produces 4 fewer triangles than
        // needed for torus topology (χ=2 instead of χ=0).  This is a fundamental
        // limitation of approximating a flat-torus triangulation via tiled 2D
        // Delaunay — the algorithm cannot bridge the UV gap at the seam
        // without creating artifacts.
        //
        // Instead, we use the ORIGINAL mesh's face connectivity (which already
        // has correct torus topology) mapped to the merged vertex indices.
        // This preserves χ=0 and produces an excellent valence distribution.
        // Quad faces are triangulated (split into 2 triangles).

        // Extract faces from the mesh's half-edge structure, triangulating quads
        let mut orig_triangles: Vec<(usize, usize, usize)> = Vec::new();
        for (fi, face) in mesh.faces.iter().enumerate() {
            if !face.valid {
                continue;
            }
            let hes = mesh.face_half_edges(crate::mesh::half_edge::FaceId(fi));
            if hes.len() < 3 {
                continue;
            }
            let verts: Vec<usize> = hes
                .iter()
                .map(|&he| mesh.half_edges[he.0].origin.0)
                .collect();
            // Fan triangulation: works for triangles (1 tri) and quads (2 tris)
            for i in 1..verts.len() - 1 {
                orig_triangles.push((verts[0], verts[i], verts[i + 1]));
            }
        }

        // Map original triangles through the merge map
        let mut mapped_triangles: Vec<(usize, usize, usize)> =
            Vec::with_capacity(orig_triangles.len());
        let mut degen_from_merge = 0usize;
        for &(v0, v1, v2) in &orig_triangles {
            let m0 = merge_map[v0];
            let m1 = merge_map[v1];
            let m2 = merge_map[v2];
            // Skip degenerate triangles (two vertices merged into one)
            if m0 == m1 || m1 == m2 || m0 == m2 {
                degen_from_merge += 1;
                continue;
            }
            mapped_triangles.push((m0, m1, m2));
        }

        // Deduplicate
        mapped_triangles.sort_by_key(|&(a, b, c)| {
            let mut v = [a, b, c];
            v.sort();
            (v[0], v[1], v[2])
        });
        let before_dedup = mapped_triangles.len();
        mapped_triangles.dedup_by(|a, b| {
            let mut va = [a.0, a.1, a.2];
            let mut vb = [b.0, b.1, b.2];
            va.sort();
            vb.sort();
            va == vb
        });
        let dup_count = before_dedup - mapped_triangles.len();

        log::info!(
            "Original topology: {} orig faces → {} mapped (degen={}, dups={}) → {} merged vertices",
            orig_triangles.len(),
            mapped_triangles.len(),
            degen_from_merge,
            dup_count,
            merged_positions.len()
        );

        // Use the mapped original topology — it preserves the correct Euler
        // characteristic and produces a clean valence distribution.
        let final_triangles = mapped_triangles;

        if final_triangles.is_empty() {
            log::warn!("Mapped topology produced no triangles, falling back to periodic Delaunay");
            let triangles =
                periodic_delaunay(&merged_uvs, periodic_u, periodic_v, u_range, v_range);
            if triangles.is_empty() {
                log::warn!("Periodic Delaunay also produced no triangles");
                return mesh.clone();
            }
            let mut deduped = triangles;
            deduped.sort_by_key(|&(a, b, c)| {
                let mut v = [a, b, c];
                v.sort();
                (v[0], v[1], v[2])
            });
            deduped.dedup_by(|a, b| {
                let mut va = [a.0, a.1, a.2];
                let mut vb = [b.0, b.1, b.2];
                va.sort();
                vb.sort();
                va == vb
            });
            return HalfEdgeMesh::from_triangles(&merged_positions, &merged_uvs, &deduped);
        }

        HalfEdgeMesh::from_triangles(&merged_positions, &merged_uvs, &final_triangles)
    }

    /// Triangulate all faces of a mesh (quads → 2 triangles, n-gons → n-2 triangles).
    /// Preserves the original vertex data and topology — only splits faces.
    fn triangulate_faces(mesh: &HalfEdgeMesh) -> HalfEdgeMesh {
        let positions: Vec<Vec3> = mesh.vertices.iter().map(|v| v.position).collect();
        let uvs: Vec<Vec2> = mesh.vertices.iter().map(|v| v.uv).collect();

        let mut triangles: Vec<(usize, usize, usize)> = Vec::new();
        for (fi, face) in mesh.faces.iter().enumerate() {
            if !face.valid {
                continue;
            }
            let hes = mesh.face_half_edges(crate::mesh::half_edge::FaceId(fi));
            if hes.len() < 3 {
                continue;
            }
            let verts: Vec<usize> = hes
                .iter()
                .map(|&he| mesh.half_edges[he.0].origin.0)
                .collect();
            // Fan triangulation from vertex 0
            for i in 1..verts.len() - 1 {
                triangles.push((verts[0], verts[i], verts[i + 1]));
            }
        }

        log::info!(
            "Triangulated {} faces → {} triangles ({} vertices)",
            mesh.num_valid_faces(),
            triangles.len(),
            positions.len()
        );

        HalfEdgeMesh::from_triangles(&positions, &uvs, &triangles)
    }

    fn detect_periodicity(
        positions: &[Vec3],
        uvs: &[Vec2],
        min_u: f32,
        max_u: f32,
        min_v: f32,
        max_v: f32,
    ) -> (bool, bool) {
        let u_range = max_u - min_u;
        let v_range = max_v - min_v;
        if u_range < 1e-6 || v_range < 1e-6 {
            return (false, false);
        }

        // Direct approach: find vertex pairs with matching 3D positions but
        // UVs at opposite boundaries. This works regardless of how irregular
        // the UV mapping is (e.g., Blender's non-uniform torus unwrap).
        let pos_eps = 0.005f32;
        let uv_range_ratio = 0.6; // UV difference must be > 60% of the range

        let mut periodic_u = false;
        let mut periodic_v = false;

        // Build spatial grid for quick position matching
        let mut grid: std::collections::HashMap<(i64, i64, i64), Vec<usize>> =
            std::collections::HashMap::new();
        let cell_size = 0.01f32;
        for (i, pos) in positions.iter().enumerate() {
            let cx = (pos.x / cell_size).floor() as i64;
            let cy = (pos.y / cell_size).floor() as i64;
            let cz = (pos.z / cell_size).floor() as i64;
            grid.entry((cx, cy, cz)).or_default().push(i);
        }

        for i in 0..positions.len() {
            let pos = positions[i];
            let uv = uvs[i];
            let cx = (pos.x / cell_size).floor() as i64;
            let cy = (pos.y / cell_size).floor() as i64;
            let cz = (pos.z / cell_size).floor() as i64;

            for dx in -1i64..=1 {
                for dy in -1i64..=1 {
                    for dz in -1i64..=1 {
                        if let Some(candidates) = grid.get(&(cx + dx, cy + dy, cz + dz)) {
                            for &j in candidates {
                                if j <= i {
                                    continue;
                                }
                                if (positions[j] - pos).length() < pos_eps {
                                    let uv_j = uvs[j];
                                    let du = (uv.x - uv_j.x).abs();
                                    let dv = (uv.y - uv_j.y).abs();
                                    if du > u_range * uv_range_ratio {
                                        periodic_u = true;
                                    }
                                    if dv > v_range * uv_range_ratio {
                                        periodic_v = true;
                                    }
                                    if periodic_u && periodic_v {
                                        return (true, true);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        (periodic_u, periodic_v)
    }

    /// Merge vertices that share BOTH the same 3D position AND the same UV.
    ///
    /// This is essential for periodic meshes (torus): seam vertices have the
    /// same 3D position but different UVs (e.g. u=0 vs u=1).  Merging by
    /// position alone would collapse them into one vertex, destroying the
    /// periodic boundary structure and causing the Delaunay triangulation to
    /// miss boundary-crossing triangles.
    fn merge_duplicate_vertices(
        positions: &[Vec3],
        uvs: &[Vec2],
    ) -> (Vec<Vec3>, Vec<Vec2>, Vec<usize>) {
        let n = positions.len();
        let mut index_map: Vec<usize> = vec![0; n];
        let mut merged_positions: Vec<Vec3> = Vec::new();
        let mut merged_uvs: Vec<Vec2> = Vec::new();

        // Spatial hash on 3D position (coarse filter).
        let cell_size = 1e-4f32;
        let pos_eps = 1e-6f32;
        let uv_eps = 1e-6f32;
        let mut grid: std::collections::HashMap<(i64, i64, i64), Vec<usize>> =
            std::collections::HashMap::new();

        for i in 0..n {
            let pos = positions[i];
            let uv = uvs[i];
            let cx = (pos.x / cell_size).floor() as i64;
            let cy = (pos.y / cell_size).floor() as i64;
            let cz = (pos.z / cell_size).floor() as i64;

            let mut found = None;
            'outer: for dx in -1i64..=1 {
                for dy in -1i64..=1 {
                    for dz in -1i64..=1 {
                        if let Some(candidates) = grid.get(&(cx + dx, cy + dy, cz + dz)) {
                            for &j in candidates {
                                // Both position AND UV must match
                                if (merged_positions[j] - pos).length() < pos_eps
                                    && (merged_uvs[j] - uv).length() < uv_eps
                                {
                                    found = Some(j);
                                    break 'outer;
                                }
                            }
                        }
                    }
                }
            }

            if let Some(j) = found {
                index_map[i] = index_map[j];
            } else {
                let new_idx = merged_positions.len();
                merged_positions.push(pos);
                merged_uvs.push(uv);
                index_map[i] = new_idx;
                grid.entry((cx, cy, cz)).or_default().push(new_idx);
            }
        }

        (merged_positions, merged_uvs, index_map)
    }

    fn standard_delaunay(uvs: &[Vec2]) -> Vec<(usize, usize, usize)> {
        if uvs.is_empty() {
            return Vec::new();
        }

        let points: Vec<delaunator::Point> = uvs
            .iter()
            .map(|uv| delaunator::Point {
                x: uv.x as f64,
                y: uv.y as f64,
            })
            .collect();

        let result = match delaunator::triangulate(&points) {
            r if r.triangles.is_empty() => return Vec::new(),
            r => r,
        };

        let mut out = Vec::new();
        for chunk in result.triangles.chunks(3) {
            let a = chunk[0];
            let b = chunk[1];
            let c = chunk[2];
            if a != b && b != c && a != c {
                out.push((a, b, c));
            }
        }
        out
    }

    /// Delaunay 网格（矩形域展开，参考 quad 网格）必须：
    ///   1. 顶点全部落在 UV 矩形 [0,2π]² 内（含边界）
    ///   2. 展开网格拓扑为圆盘：欧拉数 χ = 1（V − E + F，E 含边界边）
    ///   3. 半边结构有效、四边边界开放（接缝处不跨面，像 quad 网格）
    #[test]
    fn test_delaunay_mesh_is_unwrapped() {
        let two_pi = 2.0 * PI as f32;
        for seed in [1u64, 7, 42, 1234] {
            let (positions, uvs, triangles) = generate_delaunay_mesh(2.0, 0.5, 300, seed);
            let mesh = HalfEdgeMesh::from_triangles(&positions, &uvs, &triangles);
            assert!(mesh.validate(), "seed {}: mesh invalid", seed);

            // 顶点在矩形域内（展开后无越界）
            for (i, uv) in uvs.iter().enumerate() {
                assert!(
                    uv.x >= -1e-3 && uv.x <= two_pi + 1e-3,
                    "seed {}: 顶点 {} u 越界 {:?}",
                    seed,
                    i,
                    uv
                );
                assert!(
                    uv.y >= -1e-3 && uv.y <= two_pi + 1e-3,
                    "seed {}: 顶点 {} v 越界 {:?}",
                    seed,
                    i,
                    uv
                );
            }

            // 展开网格 = 拓扑圆盘：χ = V − E + F = 1。
            // E = 成对边数 + 边界边数（边界半边没有 twin，不能按 half_edges/2 计）
            let v = mesh.vertices.len() as i64;
            let he = mesh.half_edges.len() as i64;
            let b = mesh
                .half_edges
                .iter()
                .filter(|he| he.twin.0 == usize::MAX)
                .count() as i64;
            let e = (he - b) / 2 + b;
            let f = mesh.num_valid_faces() as i64;
            assert_eq!(
                v - e + f,
                1,
                "seed {}: 展开网格欧拉数应为 1，got {}（V={} E={} F={} b={}）",
                seed,
                v - e + f,
                v,
                e,
                f,
                b
            );

            // 四边边界开放（接缝处不跨面）
            assert!(
                b >= 4 * 8,
                "seed {}: 展开网格应有四边边界边，got {} 条边界半边",
                seed,
                b
            );
        }
    }

    #[test]
    fn test_delaunay_basic() {
        let (positions, uvs, triangles) = generate_delaunay_mesh(2.0, 0.5, 50, 42);
        // 顶点数 == 输入点数（四边固定点 + 内部随机点，总数与 num_points 一致）
        assert_eq!(
            positions.len(),
            50,
            "expected 50 vertices, got {}",
            positions.len()
        );
        assert_eq!(positions.len(), uvs.len());
        assert!(!triangles.is_empty(), "Should have some triangles");

        for &(a, b, c) in &triangles {
            assert_ne!(a, b);
            assert_ne!(b, c);
            assert_ne!(a, c);
            assert!(a < positions.len());
            assert!(b < positions.len());
            assert!(c < positions.len());
        }
    }

    #[test]
    fn test_delaunay_coverage() {
        let (_, _, triangles) = generate_delaunay_mesh(2.0, 0.5, 100, 42);
        let num_tris = triangles.len();
        assert!(
            num_tris > 150,
            "With 100 points on a torus, expect ~200 triangles, got {}",
            num_tris
        );
    }

    #[test]
    fn test_periodic_full_coverage() {
        let (_, uvs, triangles) = generate_delaunay_mesh(2.0, 0.5, 80, 123);
        let two_pi = 2.0 * PI as f32;
        let total_area = two_pi * two_pi;

        let mut covered_area = 0.0f32;
        for &(a, b, c) in &triangles {
            let pa = uvs[a];
            let pb = uvs[b];
            let pc = uvs[c];

            let u_ref = pa.x as f64;
            let v_ref = pa.y as f64;
            let two_pi_f64 = 2.0 * PI;

            let ub = unwrap_coord(pb.x as f64, u_ref, two_pi_f64);
            let uc = unwrap_coord(pc.x as f64, u_ref, two_pi_f64);
            let vb = unwrap_coord(pb.y as f64, v_ref, two_pi_f64);
            let vc = unwrap_coord(pc.y as f64, v_ref, two_pi_f64);

            let area = ((ub - u_ref) * (vc - v_ref) - (uc - u_ref) * (vb - v_ref)).abs() / 2.0;
            covered_area += area as f32;
        }

        let coverage = covered_area / total_area;
        assert!(
            coverage > 0.95,
            "Periodic Delaunay should cover >95% of domain, got {:.1}%",
            coverage * 100.0
        );
    }

    fn unwrap_coord(angle: f64, reference: f64, period: f64) -> f64 {
        let half = period / 2.0;
        let mut a = angle;
        while a - reference > half {
            a -= period;
        }
        while a - reference < -half {
            a += period;
        }
        a
    }

    /// Verify that merge_duplicate_vertices preserves seam vertices that
    /// share the same 3D position but have different UVs.
    #[test]
    fn test_merge_preserves_uv_seam() {
        // Two vertices at the same 3D position but different UVs (seam)
        let positions = vec![
            Vec3::new(1.0, 0.0, 0.0), // pos A
            Vec3::new(0.0, 1.0, 0.0), // pos B
            Vec3::new(1.0, 0.0, 0.0), // pos A again (seam copy)
            Vec3::new(0.0, 1.0, 0.0), // pos B again (seam copy)
        ];
        let uvs = vec![
            Vec2::new(0.0, 0.0), // UV at seam start
            Vec2::new(0.0, 0.5),
            Vec2::new(1.0, 0.0), // UV at seam end (same pos, different UV)
            Vec2::new(1.0, 0.5), // same pos, different UV
        ];

        let (merged_pos, merged_uvs, index_map) = merge_duplicate_vertices(&positions, &uvs);

        // Seam vertices should NOT be merged: 4 distinct (pos, UV) pairs
        assert_eq!(
            merged_pos.len(),
            4,
            "Seam vertices with different UVs must not be merged"
        );
        assert_eq!(merged_uvs.len(), 4);

        // Each vertex should map to itself (no merging happened)
        for (i, &m) in index_map.iter().enumerate() {
            assert_eq!(m, i);
        }
    }

    /// Verify that truly duplicate vertices (same pos AND same UV) are merged.
    #[test]
    fn test_merge_collapses_true_duplicates() {
        let positions = vec![
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0), // exact duplicate of vertex 0
        ];
        let uvs = vec![
            Vec2::new(0.5, 0.5),
            Vec2::new(0.3, 0.7),
            Vec2::new(0.5, 0.5), // same UV as vertex 0
        ];

        let (merged_pos, _, index_map) = merge_duplicate_vertices(&positions, &uvs);

        assert_eq!(merged_pos.len(), 2, "Exact duplicates should be merged");
        assert_eq!(
            index_map[0], index_map[2],
            "Vertices 0 and 2 should map to same merged index"
        );
    }

    /// End-to-end test: convert a quad torus mesh to Delaunay and verify
    /// the result is a valid mesh with correct topology.
    #[test]
    fn test_convert_torus_quad_to_delaunay() {
        use crate::mesh::half_edge::HalfEdgeMesh;
        use crate::mesh::torus::generate_unfolded_quad_mesh;

        let (positions, uvs, quads) = generate_unfolded_quad_mesh(2.0, 0.5, 16, 16);
        let quad_mesh = HalfEdgeMesh::from_quads(&positions, &uvs, &quads);

        let delaunay_mesh = convert_to_delaunay(&quad_mesh);

        let num_tris = delaunay_mesh.num_valid_faces();
        let num_verts = delaunay_mesh.vertices.len();

        // 输入为 UV 展开的开网格（接缝处顶点重复、不跨越接缝生成四边形）：
        // 17×17=289 顶点，16×16=256 quads → 512 三角形。
        assert_eq!(
            num_verts, 289,
            "17×17 UV 网格 → 289 顶点, got {}",
            num_verts
        );
        assert_eq!(
            num_tris, 512,
            "16×16 quad torus → 512 triangles, got {}",
            num_tris
        );

        // 边界边：u=0/u=2π 与 v=0/v=2π 四条接缝，共 2×16+2×16=64 条。
        let boundary = delaunay_mesh
            .half_edges
            .iter()
            .filter(|he| he.twin.0 == usize::MAX)
            .count();
        assert_eq!(boundary, 64, "UV 展开网格有 64 条边界边, got {}", boundary);

        // convert_to_delaunay 必须保持输入拓扑（不缝合接缝）。
        let input_boundary = quad_mesh
            .half_edges
            .iter()
            .filter(|he| he.twin.0 == usize::MAX)
            .count();
        assert_eq!(boundary, input_boundary, "转换前后边界边数量应一致");

        // Validate half-edge structure
        assert!(
            delaunay_mesh.validate(),
            "Delaunay mesh should have valid half-edge structure"
        );
    }

    /// Integration test: load the real Blender torus OBJ and convert to Delaunay.
    #[test]
    fn test_convert_obj_torus_to_delaunay() {
        use crate::mesh::obj_loader::load_obj_as_half_edge;

        let obj_path = "models/torus.obj";
        let obj_mesh = match load_obj_as_half_edge(obj_path) {
            Ok(m) => m,
            Err(e) => {
                eprintln!("Skipping test: could not load {}: {}", obj_path, e);
                return;
            }
        };

        let orig_verts = obj_mesh.vertices.len();
        let orig_faces = obj_mesh.num_valid_faces();
        eprintln!("OBJ loaded: {} vertices, {} faces", orig_verts, orig_faces);

        // ── Original mesh diagnostics ──
        let orig_boundary = count_boundary_edges(&obj_mesh);
        let orig_no_twin = count_no_twin_edges(&obj_mesh);
        eprintln!(
            "Original mesh: {} boundary edges, {} no-twin half-edges",
            orig_boundary, orig_no_twin
        );

        // Convert to Delaunay
        let delaunay_mesh = convert_to_delaunay(&obj_mesh);

        let num_verts = delaunay_mesh.vertices.len();
        let num_tris = delaunay_mesh.num_valid_faces();
        let num_half_edges = delaunay_mesh.half_edges.len();
        eprintln!(
            "Delaunay result: {} vertices, {} triangles, {} half-edges",
            num_verts, num_tris, num_half_edges
        );

        // ── Boundary / topology diagnostics ──
        let boundary = count_boundary_edges(&delaunay_mesh);
        let no_twin = count_no_twin_edges(&delaunay_mesh);
        eprintln!(
            "Delaunay mesh: {} boundary edges, {} no-twin half-edges",
            boundary, no_twin
        );

        // For a closed torus, there should be ZERO boundary edges
        if boundary > 0 {
            eprintln!(
                "WARNING: {} boundary edges found — mesh has HOLES!",
                boundary
            );
        }

        // ── Check for degenerate (zero-area) triangles ──
        let mut degen_count = 0;
        let mut min_area = f32::MAX;
        let mut max_area = 0.0f32;
        for (fi, face) in delaunay_mesh.faces.iter().enumerate() {
            if !face.valid {
                continue;
            }
            let hes = delaunay_mesh.face_half_edges(crate::mesh::half_edge::FaceId(fi));
            if hes.len() != 3 {
                continue;
            }
            let p0 = delaunay_mesh.vertices[delaunay_mesh.half_edges[hes[0].0].origin.0].position;
            let p1 = delaunay_mesh.vertices[delaunay_mesh.half_edges[hes[1].0].origin.0].position;
            let p2 = delaunay_mesh.vertices[delaunay_mesh.half_edges[hes[2].0].origin.0].position;
            let area = (p1 - p0).cross(p2 - p0).length() * 0.5;
            min_area = min_area.min(area);
            max_area = max_area.max(area);
            if area < 1e-8 {
                degen_count += 1;
            }
        }
        eprintln!(
            "Triangle areas: min={:.6e}, max={:.6e}, degenerate(< 1e-8)={}",
            min_area, max_area, degen_count
        );

        // ── Check for duplicate 3D-position vertices ──
        let mut dup_pos_count = 0;
        for i in 0..num_verts {
            for j in (i + 1)..num_verts {
                if (delaunay_mesh.vertices[i].position - delaunay_mesh.vertices[j].position)
                    .length()
                    < 1e-4
                {
                    dup_pos_count += 1;
                    if dup_pos_count <= 5 {
                        eprintln!(
                            "  Dup-pos: vert {} and {} at {:?}, UVs ({:?} vs {:?})",
                            i,
                            j,
                            delaunay_mesh.vertices[i].position,
                            delaunay_mesh.vertices[i].uv,
                            delaunay_mesh.vertices[j].uv
                        );
                    }
                }
            }
        }
        eprintln!("Duplicate-position vertex pairs: {}", dup_pos_count);

        // ── Check for overlapping triangles (same 3 vertex positions) ──
        use std::collections::HashMap;
        let mut pos_tri_map: HashMap<(usize, usize, usize), usize> = HashMap::new();
        let mut overlap_count = 0;
        for (fi, face) in delaunay_mesh.faces.iter().enumerate() {
            if !face.valid {
                continue;
            }
            let hes = delaunay_mesh.face_half_edges(crate::mesh::half_edge::FaceId(fi));
            if hes.len() != 3 {
                continue;
            }
            let mut pos_ids: Vec<usize> = hes
                .iter()
                .map(|&he| delaunay_mesh.half_edges[he.0].origin.0)
                .collect();
            pos_ids.sort();
            let key = (pos_ids[0], pos_ids[1], pos_ids[2]);
            let entry = pos_tri_map.entry(key).or_insert(0);
            *entry += 1;
            if *entry > 1 {
                overlap_count += 1;
            }
        }
        eprintln!(
            "Overlapping triangles (same vertex triple): {}",
            overlap_count
        );

        // ── Euler characteristic ──
        let num_edges_with_twin = delaunay_mesh
            .half_edges
            .iter()
            .filter(|he| he.twin.0 != usize::MAX)
            .count()
            / 2;
        let num_boundary_he = delaunay_mesh
            .half_edges
            .iter()
            .filter(|he| he.twin.0 == usize::MAX)
            .count();
        let total_edges = num_edges_with_twin + num_boundary_he;
        let euler = num_verts as i64 - total_edges as i64 + num_tris as i64;
        eprintln!(
            "Euler: V={} E={} F={} → χ={} (torus should be 0)",
            num_verts, total_edges, num_tris, euler
        );

        // ── Vertex valence (degree) distribution ──
        let mut valence = vec![0usize; num_verts];
        for he in &delaunay_mesh.half_edges {
            valence[he.origin.0] += 1;
        }
        let min_val = *valence.iter().min().unwrap();
        let max_val = *valence.iter().max().unwrap();
        let avg_val = valence.iter().sum::<usize>() as f64 / num_verts as f64;
        let val_3 = valence.iter().filter(|&&v| v == 3).count();
        let val_4 = valence.iter().filter(|&&v| v == 4).count();
        let val_5 = valence.iter().filter(|&&v| v == 5).count();
        let val_6 = valence.iter().filter(|&&v| v == 6).count();
        let val_7 = valence.iter().filter(|&&v| v == 7).count();
        let val_other = valence.iter().filter(|&&v| !(3..=7).contains(&v)).count();
        eprintln!(
            "Valence: min={}, max={}, avg={:.1}, dist: [3:{}, 4:{}, 5:{}, 6:{}, 7:{}, other:{}]",
            min_val, max_val, avg_val, val_3, val_4, val_5, val_6, val_7, val_other
        );
        // For a torus with all-triangle faces: avg valence = 6
        // For a sphere: avg valence slightly < 6

        // ── Connected components via BFS on faces ──
        let mut visited_faces = vec![false; delaunay_mesh.faces.len()];
        let mut num_components = 0;
        for start_fi in 0..delaunay_mesh.faces.len() {
            if visited_faces[start_fi] || !delaunay_mesh.faces[start_fi].valid {
                continue;
            }
            num_components += 1;
            let mut stack = vec![start_fi];
            while let Some(fi) = stack.pop() {
                if visited_faces[fi] {
                    continue;
                }
                visited_faces[fi] = true;
                let hes = delaunay_mesh.face_half_edges(crate::mesh::half_edge::FaceId(fi));
                for he in hes {
                    let twin = delaunay_mesh.half_edges[he.0].twin;
                    if twin.0 != usize::MAX {
                        let neighbor_fi = delaunay_mesh.half_edges[twin.0].face.0;
                        if !visited_faces[neighbor_fi] {
                            stack.push(neighbor_fi);
                        }
                    }
                }
            }
        }
        eprintln!("Connected components: {}", num_components);

        // ── Assertions ──
        assert!(num_verts > 0, "Should have vertices");
        assert!(num_tris > 0, "Should have triangles");
        assert_eq!(degen_count, 0, "No degenerate triangles");
        assert_eq!(overlap_count, 0, "No overlapping triangles");
        assert_eq!(num_components, 1, "Should be a single connected component");
        assert!(
            delaunay_mesh.validate(),
            "Half-edge structure should be valid"
        );
        // Note: for a perfect torus, euler should be 0.  With the current
        // periodic Delaunay implementation we get χ=2 due to 4 missing
        // seam triangles.  This is a known limitation.
        eprintln!("Euler χ={} (expected 0 for torus)", euler);
        assert_eq!(euler, 0, "Euler characteristic should be 0 for torus");
    }

    fn count_boundary_edges(mesh: &crate::mesh::half_edge::HalfEdgeMesh) -> usize {
        mesh.half_edges
            .iter()
            .filter(|he| he.twin.0 == usize::MAX)
            .count()
    }

    fn count_no_twin_edges(mesh: &crate::mesh::half_edge::HalfEdgeMesh) -> usize {
        mesh.half_edges
            .iter()
            .filter(|he| he.twin.0 == usize::MAX)
            .count()
    }
    /// Delaunay 三角网格 + U/V Grid 切割：切割线应切开网格（面数增加、有效）。
    #[test]
    fn test_delaunay_grid_cut_increases_faces() {
        use crate::mesh::cut::cut_mesh_by_grid;
        let uv = (
            0.0,
            2.0 * std::f64::consts::PI,
            0.0,
            2.0 * std::f64::consts::PI,
        );
        use crate::mesh::half_edge::HalfEdgeMesh;

        let (positions, uvs, triangles) = generate_delaunay_mesh(2.0, 0.5, 400, 1);
        let mut mesh = HalfEdgeMesh::from_triangles(&positions, &uvs, &triangles);
        let before = mesh.num_valid_faces();
        cut_mesh_by_grid(&mut mesh, &[2.0], &[], uv, true);
        let after = mesh.num_valid_faces();
        eprintln!("delaunay 切割: faces {before} -> {after}");
        assert!(mesh.validate(), "切割后网格应有效");
        assert!(after > before, "U 线切割应增加面数（{before} -> {after}）");

        cut_mesh_by_grid(&mut mesh, &[], &[2.0, 4.0], uv, true);
        let after2 = mesh.num_valid_faces();
        eprintln!("delaunay V 切割: faces {after} -> {after2}");
        assert!(after2 > after, "V 线切割应增加面数");
        assert!(mesh.validate());
    }

    /// 用 UI 等分位置（Delaunay 分支：num_u_loops 等分中心）切割，
    /// 验证边界环数增加（切割的视觉特征：新边界沿切割线）。
    #[test]
    fn test_delaunay_grid_cut_ui_positions() {
        use crate::mesh::cut::cut_mesh_by_grid;
        use crate::mesh::half_edge::HalfEdgeMesh;

        let (positions, uvs, triangles) = generate_delaunay_mesh(2.0, 0.5, 400, 1);
        let mut mesh = HalfEdgeMesh::from_triangles(&positions, &uvs, &triangles);
        let uv = (
            0.0,
            2.0 * std::f64::consts::PI,
            0.0,
            2.0 * std::f64::consts::PI,
        );
        let two_pi = 2.0 * std::f64::consts::PI;

        // 与 loop_u_position 相同的公式：num_u_loops=4 → 0.5/4..3.5/4 倍 2π
        let u_vals: Vec<f64> = (0..4).map(|i| two_pi / 4.0 * (i as f64 + 0.5)).collect();
        let v_vals: Vec<f64> = (0..6).map(|i| two_pi / 6.0 * (i as f64 + 0.5)).collect();

        let boundary_before = mesh
            .half_edges
            .iter()
            .filter(|he| he.twin.0 == usize::MAX)
            .count();
        cut_mesh_by_grid(&mut mesh, &u_vals, &v_vals, uv, true);
        let boundary_after = mesh
            .half_edges
            .iter()
            .filter(|he| he.twin.0 == usize::MAX)
            .count();

        eprintln!(
            "delaunay UI 等分切割: 边界半边 {} -> {}",
            boundary_before, boundary_after
        );
        assert!(mesh.validate());
        assert!(
            boundary_after > boundary_before,
            "切割后边界应增加（{boundary_before} -> {boundary_after}）"
        );
    }
}
