use glam::{Vec2, Vec3};
use std::f64::consts::PI;

use super::half_edge::{FaceId, HalfEdgeId, HalfEdgeMesh, VertexId};
use super::uv::{normalize_uv, unwrap_angle};

const EPSILON: f64 = 1e-8;
/// Larger epsilon for classifying vertices as "on the cut line".
/// Needed because vertices from previous edge splits may have phi values
/// slightly above EPSILON due to floating point in unwrap+wrap round-trip.
/// Also used for skipping intersections that coincide with OnLine vertices.
const MERGE_EPS: f64 = 1e-4;

/// Clamp UV to [min, max] instead of wrapping.
/// In UV space, periodic boundaries (u=0, u=2π) are separate — no wrapping.
fn clamp_uv(val: f64, min: f64, max: f64) -> f64 {
    val.clamp(min, max)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Less,
    Greater,
    OnLine,
}

#[derive(Debug)]
pub struct TriangleIntersection {
    pub edge_points: Vec<(usize, usize, Vec3, Vec2)>,
    pub on_line_vertices: Vec<usize>,
}

/// Intersect a polygon face with a U-constant grid line.
///
/// Key idea: unwrap all vertex u-coordinates relative to the first vertex
/// so they form a continuous sequence, then do simple linear interpolation.
pub fn intersect_face_u(
    face_verts: &[(usize, Vec2, Vec3)],
    u_const: f64,
    uv_range: (f64, f64, f64, f64),
) -> Option<TriangleIntersection> {
    let n = face_verts.len();
    if n < 3 {
        return None;
    }

    let (min_u, max_u, min_v, max_v) = uv_range;

    // Normalize UVs to [0, 2π) so unwrap_angle (period 2π) works correctly
    let u0_norm = normalize_uv(face_verts[0].1.x as f64, min_u, max_u);
    let v0_norm = normalize_uv(face_verts[0].1.y as f64, min_v, max_v);

    let mut u_unwrap: Vec<f64> = Vec::with_capacity(n);
    let mut v_unwrap: Vec<f64> = Vec::with_capacity(n);
    for fv in face_verts {
        let u_norm = normalize_uv(fv.1.x as f64, min_u, max_u);
        let v_norm = normalize_uv(fv.1.y as f64, min_v, max_v);
        u_unwrap.push(unwrap_angle(u_norm, u0_norm));
        v_unwrap.push(unwrap_angle(v_norm, v0_norm));
    }

    let uc_norm = normalize_uv(u_const, min_u, max_u);
    let uc_unwrap = unwrap_angle(uc_norm, u0_norm);

    // Classify each vertex
    let mut sides: Vec<Side> = Vec::with_capacity(n);
    for &uu in &u_unwrap {
        let diff = uu - uc_unwrap;
        if diff.abs() < MERGE_EPS {
            sides.push(Side::OnLine);
        } else if diff < 0.0 {
            sides.push(Side::Less);
        } else {
            sides.push(Side::Greater);
        }
    }

    let has_less = sides.contains(&Side::Less);
    let has_greater = sides.contains(&Side::Greater);
    let has_on_line = sides.contains(&Side::OnLine);

    if !has_less && !has_greater {
        return Some(TriangleIntersection {
            edge_points: Vec::new(),
            on_line_vertices: (0..n).map(|i| face_verts[i].0).collect(),
        });
    }

    if !((has_less && has_greater) || has_on_line) {
        return None;
    }

    let mut edge_points = Vec::new();
    let mut on_line_vertices = Vec::new();

    for i in 0..n {
        if sides[i] == Side::OnLine {
            on_line_vertices.push(face_verts[i].0);
        }
    }

    for e in 0..n {
        let (i0, i1) = (e, (e + 1) % n);

        if sides[i0] == Side::OnLine && sides[i1] == Side::OnLine {
            continue;
        }
        if sides[i0] != Side::OnLine && sides[i1] != Side::OnLine && sides[i0] == sides[i1] {
            continue;
        }

        let du = u_unwrap[i1] - u_unwrap[i0];
        if du.abs() <= EPSILON {
            continue;
        }

        let t = (uc_unwrap - u_unwrap[i0]) / du;
        if !(-EPSILON..=1.0 + EPSILON).contains(&t) {
            continue;
        }
        let t = t.clamp(0.0, 1.0);

        if sides[i0] == Side::OnLine && t < MERGE_EPS {
            continue;
        }
        if sides[i1] == Side::OnLine && t > 1.0 - MERGE_EPS {
            continue;
        }

        let p0 = face_verts[i0].2;
        let p1 = face_verts[i1].2;

        let t32 = t as f32;
        let pos = p0 + t32 * (p1 - p0);

        let u_new = clamp_uv(
            u_unwrap[i0] + t * (u_unwrap[i1] - u_unwrap[i0]),
            min_u,
            max_u,
        );
        let v_new = clamp_uv(
            v_unwrap[i0] + t * (v_unwrap[i1] - v_unwrap[i0]),
            min_v,
            max_v,
        );

        edge_points.push((
            face_verts[i0].0,
            face_verts[i1].0,
            pos,
            Vec2::new(u_new as f32, v_new as f32),
        ));
    }

    on_line_vertices.sort();
    on_line_vertices.dedup();

    Some(TriangleIntersection {
        edge_points,
        on_line_vertices,
    })
}

/// Intersect a polygon face with a V-constant grid line.
pub fn intersect_face_v(
    face_verts: &[(usize, Vec2, Vec3)],
    v_const: f64,
    uv_range: (f64, f64, f64, f64),
) -> Option<TriangleIntersection> {
    let n = face_verts.len();
    if n < 3 {
        return None;
    }

    let (min_u, max_u, min_v, max_v) = uv_range;

    // Normalize UVs to [0, 2π) so unwrap_angle (period 2π) works correctly
    let u0_norm = normalize_uv(face_verts[0].1.x as f64, min_u, max_u);
    let v0_norm = normalize_uv(face_verts[0].1.y as f64, min_v, max_v);

    let mut u_unwrap: Vec<f64> = Vec::with_capacity(n);
    let mut v_unwrap: Vec<f64> = Vec::with_capacity(n);
    for fv in face_verts {
        let u_norm = normalize_uv(fv.1.x as f64, min_u, max_u);
        let v_norm = normalize_uv(fv.1.y as f64, min_v, max_v);
        u_unwrap.push(unwrap_angle(u_norm, u0_norm));
        v_unwrap.push(unwrap_angle(v_norm, v0_norm));
    }

    let vc_norm = normalize_uv(v_const, min_v, max_v);
    let vc_unwrap = unwrap_angle(vc_norm, v0_norm);

    let mut sides: Vec<Side> = Vec::with_capacity(n);
    for &vv in &v_unwrap {
        let diff = vv - vc_unwrap;
        if diff.abs() < MERGE_EPS {
            sides.push(Side::OnLine);
        } else if diff < 0.0 {
            sides.push(Side::Less);
        } else {
            sides.push(Side::Greater);
        }
    }

    let has_less = sides.contains(&Side::Less);
    let has_greater = sides.contains(&Side::Greater);
    let has_on_line = sides.contains(&Side::OnLine);

    if !has_less && !has_greater {
        return Some(TriangleIntersection {
            edge_points: Vec::new(),
            on_line_vertices: (0..n).map(|i| face_verts[i].0).collect(),
        });
    }

    if !((has_less && has_greater) || has_on_line) {
        return None;
    }

    let mut edge_points = Vec::new();
    let mut on_line_vertices = Vec::new();

    for i in 0..n {
        if sides[i] == Side::OnLine {
            on_line_vertices.push(face_verts[i].0);
        }
    }

    for e in 0..n {
        let (i0, i1) = (e, (e + 1) % n);

        if sides[i0] == Side::OnLine && sides[i1] == Side::OnLine {
            continue;
        }
        if sides[i0] != Side::OnLine && sides[i1] != Side::OnLine && sides[i0] == sides[i1] {
            continue;
        }

        let dv = v_unwrap[i1] - v_unwrap[i0];
        if dv.abs() <= EPSILON {
            continue;
        }

        let t = (vc_unwrap - v_unwrap[i0]) / dv;
        if !(-EPSILON..=1.0 + EPSILON).contains(&t) {
            continue;
        }
        let t = t.clamp(0.0, 1.0);

        if sides[i0] == Side::OnLine && t < MERGE_EPS {
            continue;
        }
        if sides[i1] == Side::OnLine && t > 1.0 - MERGE_EPS {
            continue;
        }

        let p0 = face_verts[i0].2;
        let p1 = face_verts[i1].2;

        let t32 = t as f32;
        let pos = p0 + t32 * (p1 - p0);

        let u_new = clamp_uv(
            u_unwrap[i0] + t * (u_unwrap[i1] - u_unwrap[i0]),
            min_u,
            max_u,
        );
        let v_new = clamp_uv(
            v_unwrap[i0] + t * (v_unwrap[i1] - v_unwrap[i0]),
            min_v,
            max_v,
        );

        edge_points.push((
            face_verts[i0].0,
            face_verts[i1].0,
            pos,
            Vec2::new(u_new as f32, v_new as f32),
        ));
    }

    on_line_vertices.sort();
    on_line_vertices.dedup();

    Some(TriangleIntersection {
        edge_points,
        on_line_vertices,
    })
}

pub fn get_face_uvs(mesh: &HalfEdgeMesh, face: FaceId) -> Vec<(usize, Vec2, Vec3)> {
    let hes = mesh.face_half_edges(face);
    hes.iter()
        .map(|he| {
            let v = mesh.half_edges[he.0].origin.0;
            (v, mesh.vertices[v].uv, mesh.vertices[v].position)
        })
        .collect()
}

pub fn cut_mesh_by_grid(
    mesh: &mut HalfEdgeMesh,
    u_values: &[f64],
    v_values: &[f64],
    uv_range: (f64, f64, f64, f64),
    finalize_triangles: bool,
) {
    log::info!(
        "Cutting: {} U-lines, {} V-lines",
        u_values.len(),
        v_values.len()
    );
    log::info!(
        "UV range for cutting: u=[{:.4}, {:.4}], v=[{:.4}, {:.4}]",
        uv_range.0,
        uv_range.1,
        uv_range.2,
        uv_range.3
    );

    // 交替迭代直到收敛：一条切割线切出的新面可能跨越另一条切割线
    // （如 U 线切完 → V 线切分产生的 4 边形仍跨 U 线），
    // 需反复执行全部切割线，直到面数不再变化（通常 2-3 轮收敛）。
    // 面数硬上限：极端参数（细网格 + 大量切割线）下交叉点处可能
    // 反复切分导致指数增长，超过上限即停止，防止内存/GPU 缓冲超限。
    const MAX_FACES: usize = 150_000;
    for _round in 0..6 {
        let faces_before = mesh.num_valid_faces();
        for &u in u_values {
            cut_mesh_by_u_line(mesh, u, uv_range);
        }
        for &v in v_values {
            cut_mesh_by_v_line(mesh, v, uv_range);
        }
        if mesh.num_valid_faces() == faces_before {
            break;
        }
        if mesh.num_valid_faces() > MAX_FACES {
            log::warn!(
                "Cut mesh exploded to {} faces (limit {}), stopping iteration",
                mesh.num_valid_faces(),
                MAX_FACES
            );
            break;
        }
    }

    // 三角网格（Delaunay）语义：切割后所有面收尾为三角形——
    // 覆盖"被相邻面 split_edge 增边但自身未切分"的残留多边形。
    // Quad 网格保持四边形，不调用。
    if finalize_triangles {
        for fi in 0..mesh.faces.len() {
            if mesh.faces[fi].valid && mesh.face_half_edges(FaceId(fi)).len() > 3 {
                triangulate_face(mesh, FaceId(fi));
            }
        }
    }

    let mut non_tri_count = 0;
    for fi in 0..mesh.faces.len() {
        if !mesh.faces[fi].valid {
            continue;
        }
        let hes = mesh.face_half_edges(FaceId(fi));
        if hes.len() != 3 {
            non_tri_count += 1;
            if non_tri_count <= 5 {
                log::warn!(
                    "Face {} has {} edges (not triangle), patch={:?}",
                    fi,
                    hes.len(),
                    mesh.faces[fi].patch_index
                );
            }
        }
    }
    if non_tri_count > 0 {
        log::warn!("{} non-triangular faces after cutting!", non_tri_count);
    }

    if !mesh.validate() {
        log::error!("Mesh validation FAILED after cutting!");
    }

    log::info!(
        "Cut complete: {} faces, {} vertices",
        mesh.num_valid_faces(),
        mesh.vertices.len()
    );
}

pub fn cut_mesh_by_knots(
    mesh: &mut HalfEdgeMesh,
    k_values: &[usize],
    uv_range: (f64, f64, f64, f64),
) {
    log::info!("Cutting by {} torus knots", k_values.len());

    for &k in k_values {
        cut_mesh_by_knot_line(mesh, k as f64, uv_range);
    }

    // Merge duplicate vertices created by independent face processing

    if !mesh.validate() {
        log::error!("Mesh validation FAILED after knot cutting!");
    }
    log::info!(
        "Knot cut complete: {} faces, {} vertices",
        mesh.num_valid_faces(),
        mesh.vertices.len()
    );
}

/// Compute the circular mean centroid of a face's UV coordinates.
/// Handles periodic boundaries (u=0 / u=2π seam) by unwrapping all vertex
/// UVs relative to the first vertex before averaging.
/// Returns values in the original [min, max] UV space.
fn circular_centroid(
    mesh: &HalfEdgeMesh,
    hes: &[HalfEdgeId],
    min_u: f64,
    max_u: f64,
    min_v: f64,
    max_v: f64,
) -> (f64, f64) {
    let n = hes.len() as f64;
    if n < 1.0 {
        return (0.0, 0.0);
    }

    // Use first vertex as unwrap reference
    let uv0 = mesh.vertices[mesh.half_edges[hes[0].0].origin.0].uv;
    let u0_norm = normalize_uv(uv0.x as f64, min_u, max_u);
    let v0_norm = normalize_uv(uv0.y as f64, min_v, max_v);

    let mut sum_u = 0.0f64;
    let mut sum_v = 0.0f64;
    for &he_id in hes {
        let uv = mesh.vertices[mesh.half_edges[he_id.0].origin.0].uv;
        let u_norm = normalize_uv(uv.x as f64, min_u, max_u);
        let v_norm = normalize_uv(uv.y as f64, min_v, max_v);
        // Unwrap relative to first vertex so seam-crossing faces are handled correctly
        let u_unwrap = unwrap_angle(u_norm, u0_norm);
        let v_unwrap = unwrap_angle(v_norm, v0_norm);
        sum_u += u_unwrap;
        sum_v += v_unwrap;
    }

    let avg_u_norm = sum_u / n;
    let avg_v_norm = sum_v / n;

    // Map from normalized [0, 2π) space back to original [min, max] space
    let two_pi = 2.0 * PI;
    let u_range = max_u - min_u;
    let v_range = max_v - min_v;

    // Wrap average into [0, 2π) and convert back to raw UV
    let mut avg_u_raw = (avg_u_norm % two_pi + two_pi) % two_pi;
    let mut avg_v_raw = (avg_v_norm % two_pi + two_pi) % two_pi;
    avg_u_raw = min_u + avg_u_raw / two_pi * u_range;
    avg_v_raw = min_v + avg_v_raw / two_pi * v_range;

    (avg_u_raw, avg_v_raw)
}

fn cut_mesh_by_u_line(mesh: &mut HalfEdgeMesh, u_const: f64, uv_range: (f64, f64, f64, f64)) {
    let mut face_idx = 0;
    let mut cut_count = 0;
    let initial_face_count = mesh.faces.len();
    while face_idx < initial_face_count {
        if !mesh.faces[face_idx].valid {
            face_idx += 1;
            continue;
        }
        if cut_face_local(mesh, FaceId(face_idx), |fv| {
            intersect_face_u(fv, u_const, uv_range)
        }) {
            cut_count += 1;
        }
        face_idx += 1;
    }
    log::info!("U-line {:.3}: made {} cuts", u_const, cut_count);
}

fn cut_mesh_by_v_line(mesh: &mut HalfEdgeMesh, v_const: f64, uv_range: (f64, f64, f64, f64)) {
    let mut face_idx = 0;
    let mut cut_count = 0;
    let initial_face_count = mesh.faces.len();
    while face_idx < initial_face_count {
        if !mesh.faces[face_idx].valid {
            face_idx += 1;
            continue;
        }
        if cut_face_local(mesh, FaceId(face_idx), |fv| {
            intersect_face_v(fv, v_const, uv_range)
        }) {
            cut_count += 1;
        }
        face_idx += 1;
    }
    log::info!("V-line {:.3}: made {} cuts", v_const, cut_count);
}

pub fn cut_mesh_by_knot_line(mesh: &mut HalfEdgeMesh, k: f64, uv_range: (f64, f64, f64, f64)) {
    let mut face_idx = 0;
    let mut cut_count = 0;
    let initial_face_count = mesh.faces.len();
    while face_idx < initial_face_count {
        if !mesh.faces[face_idx].valid {
            face_idx += 1;
            continue;
        }
        let _face_verts = get_face_uvs(mesh, FaceId(face_idx));
        if cut_face_local_knot(mesh, FaceId(face_idx), k, uv_range) {
            cut_count += 1;
        }
        face_idx += 1;
    }

    // --- Verification & repair: re-scan all faces for straddling ---
    // Some faces may be missed by the single-pass approach (e.g., when the
    // cut line passes exactly through a vertex, adjacent faces might not be
    // detected).  Repeat until no straddling faces remain.
    let two_pi = 2.0 * PI;
    let (min_u, max_u, min_v, max_v) = uv_range;
    // Use a generous tolerance: if a vertex is within this distance of a
    // branch, treat it as ON the branch (not straddling).
    let straddle_tol = 1e-4;
    for pass in 0..5 {
        let mut repair_count = 0;
        let n_faces = mesh.faces.len();
        for fi in 0..n_faces {
            if !mesh.faces[fi].valid {
                continue;
            }
            let face_verts = get_face_uvs(mesh, FaceId(fi));
            if face_verts.len() < 3 {
                continue;
            }

            // Check if face straddles any branch of the knot line
            let u0_n = normalize_uv(face_verts[0].1.x as f64, min_u, max_u);
            let v0_n = normalize_uv(face_verts[0].1.y as f64, min_v, max_v);
            let mut phi_unwrap: Vec<f64> = Vec::new();
            for fv in &face_verts {
                let u_norm = normalize_uv(fv.1.x as f64, min_u, max_u);
                let v_norm = normalize_uv(fv.1.y as f64, min_v, max_v);
                let u_uw = unwrap_angle(u_norm, u0_n);
                let v_uw = unwrap_angle(v_norm, v0_n);
                phi_unwrap.push(v_uw - k * u_uw);
            }

            let phi_min = phi_unwrap.iter().cloned().fold(f64::MAX, f64::min);
            let phi_max = phi_unwrap.iter().cloned().fold(f64::MIN, f64::max);

            // Check each branch that falls within [phi_min, phi_max]
            let n_lo = (phi_min / two_pi).ceil() as isize;
            let n_hi = (phi_max / two_pi).floor() as isize;
            let mut truly_straddles = false;
            for n in n_lo..=n_hi {
                let target = two_pi * n as f64;
                let mut has_below = false;
                let mut has_above = false;
                for &phi in &phi_unwrap {
                    let d = phi - target;
                    if d < -straddle_tol {
                        has_below = true;
                    }
                    if d > straddle_tol {
                        has_above = true;
                    }
                }
                if has_below && has_above {
                    truly_straddles = true;
                    break;
                }
            }
            if !truly_straddles {
                continue;
            }

            // Face straddles a branch — try to cut it (local triangulation only)
            if cut_face_local_knot(mesh, FaceId(fi), k, uv_range) {
                repair_count += 1;
            }
        }
        if repair_count == 0 {
            break;
        }
        log::info!(
            "Knot k={}: repair pass {} fixed {} faces",
            k,
            pass + 1,
            repair_count
        );
        cut_count += repair_count;
    }

    log::info!("Knot k={}: made {} cuts (total)", k, cut_count);
}

/// 在指定面的边界中查找 va→vb 半边。
/// 用面内遍历而非绕顶点（绕顶点在边界顶点/接缝副本处会因 twin=MAX
/// 提前退出而漏找目标边）；va→vb 一定是该面的边，面内查找必然命中。
fn find_half_edge_on_face(
    mesh: &HalfEdgeMesh,
    face: FaceId,
    va: VertexId,
    vb: VertexId,
) -> Option<HalfEdgeId> {
    for he in mesh.face_half_edges(face) {
        let e = &mesh.half_edges[he.0];
        if e.origin == va && mesh.half_edges[e.next.0].origin == vb {
            return Some(he);
        }
    }
    None
}

/// 判断两顶点是否由一条边直接相连（同一面内相邻）。
fn are_adjacent(mesh: &HalfEdgeMesh, a: VertexId, b: VertexId) -> bool {
    let start = mesh.vertices[a.0].outgoing;
    let mut he = start;
    loop {
        let e = &mesh.half_edges[he.0];
        if e.next.0 != usize::MAX && mesh.half_edges[e.next.0].origin == b {
            return true;
        }
        let twin = e.twin;
        if twin.0 == usize::MAX {
            return false;
        }
        he = mesh.half_edges[twin.0].next;
        if he == start {
            return false;
        }
    }
}

/// 把单个面（n>3 边形）fan 三角化：从第一个顶点向非相邻顶点连线切分。
/// **只影响该面**，其他面完全不动。
/// fan 三角化：循环切分直到所有**产生的面**都 ≤3 边。
/// 注意每次 split_face 会同时产生一个新面（如 5 边形切出 4 边形），
/// 新面也必须继续三角化——否则残留多边形面。
fn triangulate_face(mesh: &mut HalfEdgeMesh, face: FaceId) {
    let mut queue: Vec<FaceId> = vec![face];
    while let Some(f) = queue.pop() {
        while mesh.face_half_edges(f).len() > 3 {
            let hes = mesh.face_half_edges(f);
            let v0 = mesh.half_edges[hes[0].0].origin;
            let v2 = mesh.half_edges[hes[2].0].origin;
            if v0 == v2 {
                break;
            }
            let nf = mesh.split_face(f, v0, v2);
            if nf != f && mesh.face_half_edges(nf).len() > 3 {
                queue.push(nf);
            }
        }
    }
}

/// 按面边界顺序排列顶点（沿 face 边界的出现顺序）。
fn order_verts_on_face(mesh: &HalfEdgeMesh, face: FaceId, verts: &[VertexId]) -> Vec<VertexId> {
    let hes = mesh.face_half_edges(face);
    let mut ordered: Vec<VertexId> = Vec::new();
    for he in hes {
        let o = mesh.half_edges[he.0].origin;
        if verts.contains(&o) {
            ordered.push(o);
        }
    }
    ordered
}

/// 用求交结果切分单个面：在边上插入交点，再沿面边界按顺序连接，
/// 把面切成多个部分。只影响该面。
/// 若原面为三角形，切割产生的 >3 边新面会收尾三角化——
/// Delaunay 网格切割后保持全三角（一条直线切三角形必然产生四边形+三角形）。
fn cut_face_by_intersection(mesh: &mut HalfEdgeMesh, face: FaceId, isect: &TriangleIntersection) {
    // 1. 在边上插入交点（面内查找——边界顶点/接缝处也可靠）
    let mut cut_verts: Vec<VertexId> = Vec::new();
    for &(va, vb, pos, uv) in &isect.edge_points {
        if let Some(he) = find_half_edge_on_face(mesh, face, VertexId(va), VertexId(vb)) {
            cut_verts.push(mesh.split_edge(he, pos, uv));
        }
    }
    // 2. 恰在曲线上的面顶点（不插点，配合边交点参与切分）
    for &vi in &isect.on_line_vertices {
        cut_verts.push(VertexId(vi));
    }
    if cut_verts.len() < 2 {
        return;
    }
    // 3. 沿面边界排序
    let ordered = order_verts_on_face(mesh, face, &cut_verts);
    if ordered.len() < 2 {
        return;
    }
    // 4. 相邻点连接（不相邻才切分）
    let n = ordered.len();
    let mut produced: Vec<FaceId> = Vec::new();
    let mut split_happened = false;
    for i in 0..n {
        let a = ordered[i];
        let b = ordered[(i + 1) % n];
        if a == b || are_adjacent(mesh, a, b) {
            continue;
        }
        let nf = mesh.split_face(face, a, b);
        if nf != face {
            produced.push(nf);
            split_happened = true;
        }
    }
    // 5. 切分产生的多边形收尾三角化（仅真正切分过才收尾——未切分的
    // 面保持原面型，Quad 网格的"沿网格线切割"不会把四边形拆掉）。
    // 被切面可能是三角形、被相邻面 split_edge 增边的面（4 边）、或 fan 后的面——
    // 一律把切分产物 >3 边的收尾三角化，保证切割后网格为三角网格。
    if split_happened {
        produced.push(face);
        for f in produced {
            if mesh.face_half_edges(f).len() > 3 {
                triangulate_face(mesh, f);
            }
        }
    }
}

/// 面在归一化 UV 下的 φ = v − k·u 值（用于分支范围计算）
fn face_phi_values(
    face_verts: &[(usize, Vec2, Vec3)],
    k: f64,
    uv_range: (f64, f64, f64, f64),
) -> Vec<f64> {
    let (min_u, max_u, min_v, max_v) = uv_range;
    let u0_norm = normalize_uv(face_verts[0].1.x as f64, min_u, max_u);
    let v0_norm = normalize_uv(face_verts[0].1.y as f64, min_v, max_v);

    let mut u_unwrap: Vec<f64> = Vec::with_capacity(face_verts.len());
    let mut v_unwrap: Vec<f64> = Vec::with_capacity(face_verts.len());
    for fv in face_verts {
        let u_norm = normalize_uv(fv.1.x as f64, min_u, max_u);
        let v_norm = normalize_uv(fv.1.y as f64, min_v, max_v);
        u_unwrap.push(unwrap_angle(u_norm, u0_norm));
        v_unwrap.push(unwrap_angle(v_norm, v0_norm));
    }

    let mut phi: Vec<f64> = Vec::with_capacity(face_verts.len());
    for i in 0..face_verts.len() {
        phi.push(v_unwrap[i] - k * u_unwrap[i]);
    }
    phi
}

/// 面跨越的 knot 分支范围：φ = v − k·u ∈ [2π·n_lo, 2π·n_hi]。
/// knot 曲线 v = k·u 在 UV 方域内有多个分支（每 2π 一条），
/// 面较大或 k 较大时可能同时跨越多条分支——必须全部切割。
fn knot_branch_range(
    face_verts: &[(usize, Vec2, Vec3)],
    k: f64,
    uv_range: (f64, f64, f64, f64),
) -> (isize, isize) {
    let two_pi = 2.0 * PI;
    let phi = face_phi_values(face_verts, k, uv_range);
    let phi_min = phi.iter().cloned().fold(f64::MAX, f64::min);
    let phi_max = phi.iter().cloned().fold(f64::MIN, f64::max);
    let n_lo = (phi_min / two_pi).ceil() as isize;
    let n_hi = (phi_max / two_pi).floor() as isize;
    (n_lo, n_hi)
}

/// 按指定分支（n）求 knot 曲线 v = k·u + 2πn 与面的交点。
/// 旧版 intersect_face_knot 只取 n_lo（最低分支）——面跨越多条分支时
/// 会漏切，导致实际切割边与渲染的完整曲线不一致。
pub fn intersect_face_knot_branch(
    face_verts: &[(usize, Vec2, Vec3)],
    k: f64,
    branch: isize,
    uv_range: (f64, f64, f64, f64),
) -> Option<TriangleIntersection> {
    let n = face_verts.len();
    if n < 3 {
        return None;
    }

    let (min_u, max_u, min_v, max_v) = uv_range;
    let two_pi = 2.0 * PI;

    let u0_norm = normalize_uv(face_verts[0].1.x as f64, min_u, max_u);
    let v0_norm = normalize_uv(face_verts[0].1.y as f64, min_v, max_v);

    let mut u_unwrap: Vec<f64> = Vec::with_capacity(n);
    let mut v_unwrap: Vec<f64> = Vec::with_capacity(n);
    for fv in face_verts {
        let u_norm = normalize_uv(fv.1.x as f64, min_u, max_u);
        let v_norm = normalize_uv(fv.1.y as f64, min_v, max_v);
        u_unwrap.push(unwrap_angle(u_norm, u0_norm));
        v_unwrap.push(unwrap_angle(v_norm, v0_norm));
    }

    let mut phi: Vec<f64> = Vec::with_capacity(n);
    for (&vv, &uu) in v_unwrap.iter().zip(u_unwrap.iter()) {
        phi.push(vv - k * uu);
    }

    let target = two_pi * branch as f64;

    let mut sides: Vec<Side> = Vec::with_capacity(n);
    for &p in &phi {
        let diff = p - target;
        if diff.abs() < MERGE_EPS {
            sides.push(Side::OnLine);
        } else if diff < 0.0 {
            sides.push(Side::Less);
        } else {
            sides.push(Side::Greater);
        }
    }

    let has_less = sides.contains(&Side::Less);
    let has_greater = sides.contains(&Side::Greater);
    let has_on_line = sides.contains(&Side::OnLine);

    if !has_less && !has_greater {
        return Some(TriangleIntersection {
            edge_points: Vec::new(),
            on_line_vertices: (0..n).map(|i| face_verts[i].0).collect(),
        });
    }

    if !((has_less && has_greater) || has_on_line) {
        return None;
    }

    let mut edge_points = Vec::new();
    let mut on_line_vertices = Vec::new();

    for i in 0..n {
        if sides[i] == Side::OnLine {
            on_line_vertices.push(face_verts[i].0);
        }
    }

    for e in 0..n {
        let (i0, i1) = (e, (e + 1) % n);

        if sides[i0] == Side::OnLine && sides[i1] == Side::OnLine {
            continue;
        }
        if sides[i0] != Side::OnLine && sides[i1] != Side::OnLine && sides[i0] == sides[i1] {
            continue;
        }

        let d0 = phi[i0] - target;
        let d1 = phi[i1] - target;
        let denom = d1 - d0;
        if denom.abs() <= EPSILON {
            continue;
        }

        let t = -d0 / denom;
        if !(-EPSILON..=1.0 + EPSILON).contains(&t) {
            continue;
        }
        let t = t.clamp(0.0, 1.0);

        if sides[i0] == Side::OnLine && t < MERGE_EPS {
            continue;
        }
        if sides[i1] == Side::OnLine && t > 1.0 - MERGE_EPS {
            continue;
        }

        let p0 = face_verts[i0].2;
        let p1 = face_verts[i1].2;

        let t32 = t as f32;
        let pos = p0 + t32 * (p1 - p0);

        let u_new = clamp_uv(
            u_unwrap[i0] + t * (u_unwrap[i1] - u_unwrap[i0]),
            min_u,
            max_u,
        );
        let v_new = clamp_uv(
            v_unwrap[i0] + t * (v_unwrap[i1] - v_unwrap[i0]),
            min_v,
            max_v,
        );

        edge_points.push((
            face_verts[i0].0,
            face_verts[i1].0,
            pos,
            Vec2::new(u_new as f32, v_new as f32),
        ));
    }

    on_line_vertices.sort();
    on_line_vertices.dedup();

    Some(TriangleIntersection {
        edge_points,
        on_line_vertices,
    })
}

/// 对与切割线相交的面执行 knot 切割：遍历面跨越的**所有分支**
/// （v = k·u + 2πn，n ∈ [n_lo, n_hi]），每个分支局部三角化并切割。
fn cut_face_local_knot(
    mesh: &mut HalfEdgeMesh,
    face: FaceId,
    k: f64,
    uv_range: (f64, f64, f64, f64),
) -> bool {
    let (n_lo, n_hi) = knot_branch_range(&get_face_uvs(mesh, face), k, uv_range);
    if n_lo > n_hi {
        return false;
    }
    let mut cut_any = false;
    for branch in n_lo..=n_hi {
        let face_verts = get_face_uvs(mesh, face);
        let Some(isect) = intersect_face_knot_branch(&face_verts, k, branch, uv_range) else {
            continue;
        };
        if !isect.edge_points.is_empty() {
            let hes = mesh.face_half_edges(face);
            if hes.len() > 3 {
                triangulate_face(mesh, face);
            }
        }
        let face_verts = get_face_uvs(mesh, face);
        if let Some(isect) = intersect_face_knot_branch(&face_verts, k, branch, uv_range) {
            cut_face_by_intersection(mesh, face, &isect);
            cut_any = true;
        }
    }
    cut_any
}

/// 对与切割线相交的面执行切割：**仅局部处理**——
/// 1. 切割线有边交点（真正穿过面）时，先把该非三角形面三角化
///    （其他面保持原面型，quad 网格的四边形不受影响）；
/// 2. 三角化后重新求交并按三角形切割逻辑处理。
fn cut_face_local(
    mesh: &mut HalfEdgeMesh,
    face: FaceId,
    intersects: impl Fn(&[(usize, Vec2, Vec3)]) -> Option<TriangleIntersection>,
) -> bool {
    let face_verts = get_face_uvs(mesh, face);
    let Some(isect) = intersects(&face_verts) else {
        return false;
    };
    if !isect.edge_points.is_empty() {
        let hes = mesh.face_half_edges(face);
        if hes.len() > 3 {
            triangulate_face(mesh, face);
        }
    }
    let face_verts = get_face_uvs(mesh, face);
    if let Some(isect) = intersects(&face_verts) {
        cut_face_by_intersection(mesh, face, &isect);
        true
    } else {
        false
    }
}

fn find_region_index(value: f64, sorted_cuts: &[f64]) -> usize {
    if sorted_cuts.is_empty() {
        return 0;
    }
    if value < sorted_cuts[0] {
        return sorted_cuts.len() - 1;
    }
    for i in 0..sorted_cuts.len() - 1 {
        if value < sorted_cuts[i + 1] {
            return i;
        }
    }
    sorted_cuts.len() - 1
}

pub fn assign_multi_knot_patch_indices(
    mesh: &mut HalfEdgeMesh,
    k_values: &[usize],
    uv_range: (f64, f64, f64, f64),
) {
    let two_pi = 2.0 * PI;
    let (min_u, max_u, min_v, max_v) = uv_range;
    let nk = k_values.len();

    let mut band_vecs: Vec<Option<Vec<isize>>> = vec![None; mesh.faces.len()];
    let mut distinct: std::collections::BTreeSet<Vec<isize>> = std::collections::BTreeSet::new();

    for (fi, face) in mesh.faces.iter().enumerate() {
        if !face.valid {
            continue;
        }
        let hes = mesh.face_half_edges(FaceId(fi));
        if hes.is_empty() {
            continue;
        }

        // Compute band for a single knot at a single (u,v)
        // Invariant under u→u+2π, v→v+2π (since k is integer),
        // so raw UV values can be used directly.
        let knot_band =
            |u: f64, v: f64, k: f64| -> isize { (v - k * u).div_euclid(two_pi) as isize };

        // For each vertex, compute its signed distance to the nearest
        // branch of each knot line.  Pick the vertex farthest from ALL
        // knot lines — this is the most unambiguous classifier.
        let verts: Vec<(f64, f64)> = hes
            .iter()
            .map(|he_id| {
                let uv = mesh.vertices[mesh.half_edges[he_id.0].origin.0].uv;
                (uv.x as f64, uv.y as f64)
            })
            .collect();

        let mut best_vertex: Option<usize> = None;
        let mut best_min_dist: f64 = -1.0;

        for (vi, &(u, v)) in verts.iter().enumerate() {
            let mut min_dist = f64::MAX;
            for &k in k_values {
                let phi = v - k as f64 * u;
                let n = (phi / two_pi).round();
                let dist = (phi - n * two_pi).abs();
                if dist < min_dist {
                    min_dist = dist;
                }
            }
            if min_dist > best_min_dist {
                best_min_dist = min_dist;
                best_vertex = Some(vi);
            }
        }

        // Hybrid strategy:
        // - If the best vertex is clearly far from all knot lines (> 0.1),
        //   use it for classification (robust and precise).
        // - Otherwise (face is near a knot intersection), use the circular
        //   centroid which averages all vertices and gives a more stable
        //   directional signal away from the intersection.
        let (cu, cv) = if best_min_dist > 0.1 {
            let vi = best_vertex.unwrap_or(0);
            verts[vi]
        } else {
            circular_centroid(mesh, &hes, min_u, max_u, min_v, max_v)
        };

        let bands: Vec<isize> = k_values
            .iter()
            .map(|&k| knot_band(cu, cv, k as f64))
            .collect();

        distinct.insert(bands.clone());
        band_vecs[fi] = Some(bands);
    }

    // --- UV 平面区域着色（不做 torus-wrap 合并）---
    // knot 曲线 v = k·u + 2πn 的 n 条分支把 UV 平面分成 k+1 条带，
    // 每条带一个区域颜色（ByRegion 着色需求）。
    // 注：环面拓扑上单条闭曲线不分割环面（所有带经 wrap 连通），
    // 但 UV 展开平面的视觉区域应独立着色——用户可见的"区域"。
    let distinct_vec: Vec<Vec<isize>> = distinct.iter().cloned().collect();
    let pair_index: std::collections::BTreeMap<Vec<isize>, usize> = distinct_vec
        .iter()
        .enumerate()
        .map(|(i, p)| (p.clone(), i))
        .collect();
    let n = distinct_vec.len();

    log::info!(
        "Knot bands: {} raw band-vectors ({} k-values) → {} UV-plane regions",
        n,
        nk,
        n
    );

    for (fi, bands_opt) in band_vecs.iter().enumerate() {
        if let Some(ref bands) = bands_opt {
            if let Some(&color) = pair_index.get(bands) {
                mesh.faces[fi].patch_index = Some((color, 0));
            }
        }
    }
}
pub fn assign_patch_indices(
    mesh: &mut HalfEdgeMesh,
    u_values: &[f64],
    v_values: &[f64],
    uv_range: (f64, f64, f64, f64),
) {
    let mut sorted_u: Vec<f64> = u_values.to_vec();
    sorted_u.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mut sorted_v: Vec<f64> = v_values.to_vec();
    sorted_v.sort_by(|a, b| a.partial_cmp(b).unwrap());

    let (min_u, max_u, min_v, max_v) = uv_range;

    // Normalize cut values once outside the loop
    let sorted_u_norm: Vec<f64> = sorted_u
        .iter()
        .map(|&u| normalize_uv(u, min_u, max_u))
        .collect();
    let sorted_v_norm: Vec<f64> = sorted_v
        .iter()
        .map(|&v| normalize_uv(v, min_v, max_v))
        .collect();

    for fi in 0..mesh.faces.len() {
        if !mesh.faces[fi].valid {
            continue;
        }

        let hes = mesh.face_half_edges(FaceId(fi));
        if hes.is_empty() {
            continue;
        }

        let (avg_u, avg_v) = circular_centroid(mesh, &hes, min_u, max_u, min_v, max_v);

        // Normalize centroid to [0, 2π) to match sorted_u_norm / sorted_v_norm
        let avg_u_norm = normalize_uv(avg_u, min_u, max_u);
        let avg_v_norm = normalize_uv(avg_v, min_v, max_v);

        let pu = find_region_index(avg_u_norm, &sorted_u_norm);
        let pv = find_region_index(avg_v_norm, &sorted_v_norm);

        mesh.faces[fi].patch_index = Some((pu, pv));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Standard [0, 2π) UV range for tests using radian-space coordinates.
    fn uv_2pi() -> (f64, f64, f64, f64) {
        (0.0, 2.0 * PI, 0.0, 2.0 * PI)
    }

    /// Quad 网格的 Grid 切割必须保持四边形面型：切割线对齐网格顶点线
    /// （u = 2π·k/res_u），只沿面边界走、不穿过面内部，面不应被切分。
    /// Delaunay（三角）网格切割后保持全三角：一条线切三角形产生
    /// 四边形+三角形，收尾三角化保证网格面型一致。
    #[test]
    fn test_triangle_cut_keeps_all_triangles() {
        use crate::mesh::delaunay::generate_delaunay_mesh;

        let (positions, uvs, triangles) = generate_delaunay_mesh(2.0, 0.6, 300, 42);
        let mut mesh = HalfEdgeMesh::from_triangles(&positions, &uvs, &triangles);
        for fi in 0..mesh.faces.len() {
            if mesh.faces[fi].valid {
                assert_eq!(
                    mesh.face_half_edges(FaceId(fi)).len(),
                    3,
                    "生成网格应全为三角形"
                );
            }
        }

        // 4 条 U 线 + 2 条 V 线（任意位置，穿过面）
        let u_vals: Vec<f64> = vec![0.9, 2.1, 3.6, 5.0];
        let v_vals: Vec<f64> = vec![1.3, 4.2];
        cut_mesh_by_grid(&mut mesh, &u_vals, &v_vals, uv_2pi(), true);
        assert!(mesh.validate(), "网格应保持有效半边结构");
        assert!(mesh.num_valid_faces() > triangles.len(), "切割应增加面数");

        // 切割后所有面仍为三角形
        for fi in 0..mesh.faces.len() {
            if mesh.faces[fi].valid {
                assert_eq!(
                    mesh.face_half_edges(FaceId(fi)).len(),
                    3,
                    "切割后应保持全三角形（面 {fi}）"
                );
            }
        }
    }

    #[test]
    fn test_grid_cut_keeps_quad_faces() {
        use crate::mesh::torus::generate_unfolded_quad_mesh;

        let (positions, uvs, quads) = generate_unfolded_quad_mesh(2.0, 0.6, 12, 10);
        let mut mesh = HalfEdgeMesh::from_quads(&positions, &uvs, &quads);
        let face_count_before = mesh.num_valid_faces();
        let vert_count_before = mesh.vertices.len();
        assert_eq!(face_count_before, 12 * 10);

        // 切割线对齐网格线：u = 2π·k/12（k=2,5）、v = 2π·k/10（k=3）
        let u_vals: Vec<f64> = vec![2.0 * PI * 2.0 / 12.0, 2.0 * PI * 5.0 / 12.0];
        let v_vals: Vec<f64> = vec![2.0 * PI * 3.0 / 10.0];
        cut_mesh_by_grid(&mut mesh, &u_vals, &v_vals, uv_2pi(), false);
        assert!(mesh.validate(), "网格应保持有效半边结构");

        // 面型不变：所有面仍为四边形，面数/顶点数不变
        assert_eq!(mesh.num_valid_faces(), face_count_before, "面数不应变化");
        assert_eq!(mesh.vertices.len(), vert_count_before, "顶点数不应变化");
        let mut non_quad = 0;
        for fi in 0..mesh.faces.len() {
            if !mesh.faces[fi].valid {
                continue;
            }
            let hes = mesh.face_half_edges(FaceId(fi));
            if hes.len() != 4 {
                non_quad += 1;
            }
        }
        assert_eq!(
            non_quad, 0,
            "切割后所有面应保持四边形，{} 个面被切分",
            non_quad
        );
    }

    /// Knot 切割只影响与曲线相交的面：远处区域保持原面型
    /// （Quad 网格的大部分四边形不被三角化）。
    #[test]
    fn test_knot_cut_only_affects_intersecting_faces() {
        use crate::mesh::torus::generate_unfolded_quad_mesh;

        let (positions, uvs, quads) = generate_unfolded_quad_mesh(2.0, 0.6, 24, 20);
        let mut mesh = HalfEdgeMesh::from_quads(&positions, &uvs, &quads);
        let faces_before = mesh.num_valid_faces();

        cut_mesh_by_knots(&mut mesh, &[2], uv_2pi());
        assert!(mesh.validate(), "knot 切割后网格应保持有效半边结构");

        let total = mesh.num_valid_faces();
        let mut non_quad = 0;
        for fi in 0..mesh.faces.len() {
            if !mesh.faces[fi].valid {
                continue;
            }
            let hes = mesh.face_half_edges(FaceId(fi));
            if hes.len() != 4 {
                non_quad += 1;
            }
        }
        let quad = total - non_quad;

        assert!(
            total > faces_before,
            "切割应增加面数（got {} → {}）",
            faces_before,
            total
        );
        assert!(non_quad > 0, "与曲线相交的面应被切分");
        assert!(
            quad > total / 2,
            "大多数面应保持四边形（quad={} non_quad={} total={}）",
            quad,
            non_quad,
            total
        );
    }

    #[test]
    fn test_intersect_face_u_simple_cross() {
        let tri: Vec<(usize, Vec2, Vec3)> = vec![
            (0, Vec2::new(0.0, 0.0), Vec3::ZERO),
            (1, Vec2::new(2.5, 0.0), Vec3::ZERO),
            (2, Vec2::new(0.8, 1.5), Vec3::ZERO),
        ];
        let result = intersect_face_u(&tri, 1.0, uv_2pi());
        assert!(result.is_some());
        let isect = result.unwrap();
        assert!(!isect.edge_points.is_empty(), "Should have edge crossings");
    }

    #[test]
    fn test_intersect_face_u_no_cross() {
        let tri: Vec<(usize, Vec2, Vec3)> = vec![
            (0, Vec2::new(0.0, 0.0), Vec3::ZERO),
            (1, Vec2::new(0.5, 0.0), Vec3::ZERO),
            (2, Vec2::new(0.3, 0.8), Vec3::ZERO),
        ];
        let result = intersect_face_u(&tri, 1.0, uv_2pi());
        assert!(result.is_none());
    }

    #[test]
    fn test_intersect_face_u_vertex_on_line() {
        let tri: Vec<(usize, Vec2, Vec3)> = vec![
            (0, Vec2::new(0.0, 0.0), Vec3::ZERO),
            (1, Vec2::new(1.0, 0.0), Vec3::ZERO),
            (2, Vec2::new(0.0, 1.0), Vec3::ZERO),
        ];
        let result = intersect_face_u(&tri, 1.0, uv_2pi());
        assert!(result.is_some());
        let isect = result.unwrap();
        assert!(isect.on_line_vertices.contains(&1));
    }

    #[test]
    fn test_intersect_face_u_wrapping_no_false_positive() {
        // Edge from 0.1 to 0.2, grid line at π.
        // Short path doesn't cross π, and unwrapping keeps it continuous.
        let tri: Vec<(usize, Vec2, Vec3)> = vec![
            (0, Vec2::new(0.1, 0.0), Vec3::ZERO),
            (1, Vec2::new(0.2, 0.0), Vec3::ZERO),
            (2, Vec2::new(0.15, 0.8), Vec3::ZERO),
        ];
        let result = intersect_face_u(&tri, PI, uv_2pi());
        assert!(result.is_none(), "Short edge near 0 should NOT cross π");
    }

    #[test]
    fn test_intersect_face_u_wrapping_crosses_seam() {
        // Face that wraps around u=0: vertices at u=6.2 and u=0.1
        let tri: Vec<(usize, Vec2, Vec3)> = vec![
            (0, Vec2::new(6.2, 1.0), Vec3::ZERO),
            (1, Vec2::new(0.1, 0.5), Vec3::ZERO),
            (2, Vec2::new(0.15, 1.5), Vec3::ZERO),
        ];
        // After unwrapping relative to v0 (u=6.2):
        // v0: 6.2, v1: 0.1+2π≈6.38, v2: 0.15+2π≈6.43
        // All on same side of uc=3.0, so no intersection
        let result = intersect_face_u(&tri, 3.0, uv_2pi());
        assert!(
            result.is_none(),
            "Face with short-path edges near 2π should not cross π"
        );
    }

    #[test]
    fn test_intersect_face_u_wrapping_crosses_zero() {
        // Face that spans the u=0 seam
        let tri: Vec<(usize, Vec2, Vec3)> = vec![
            (0, Vec2::new(5.5, 1.0), Vec3::ZERO),
            (1, Vec2::new(0.5, 0.5), Vec3::ZERO),
            (2, Vec2::new(0.3, 1.5), Vec3::ZERO),
        ];
        // After unwrapping relative to v0 (u=5.5):
        // v0: 5.5, v1: 0.5+2π≈6.78, v2: 0.3+2π≈6.58
        // uc=0.0 unwrapped: 0.0+2π≈6.28
        // v0=5.5 < 6.28, v1=6.78 > 6.28 → edge 0→1 crosses!
        let result = intersect_face_u(&tri, 0.0, uv_2pi());
        assert!(
            result.is_some(),
            "Face spanning u=0 seam should be detected"
        );
    }

    #[test]
    fn test_intersect_face_u_obj_seam_no_false_positive() {
        // OBJ-style UVs in [0, 1] range. Edge from u=0.95 to u=0.05 is a seam edge.
        // Cut line at u=0.5 should NOT be crossed by this seam edge.
        let tri: Vec<(usize, Vec2, Vec3)> = vec![
            (0, Vec2::new(0.95, 0.3), Vec3::ZERO),
            (1, Vec2::new(0.05, 0.3), Vec3::ZERO),
            (2, Vec2::new(0.98, 0.6), Vec3::ZERO),
        ];
        let uv_range = (0.0, 1.0, 0.0, 1.0);
        let result = intersect_face_u(&tri, 0.5, uv_range);
        assert!(result.is_none(), "OBJ seam edge should NOT cross u=0.5");
    }

    #[test]
    fn test_intersect_face_u_obj_normal_cross() {
        // OBJ-style UVs in [0, 1]. Normal edge from u=0.2 to u=0.7 should cross u=0.5.
        let tri: Vec<(usize, Vec2, Vec3)> = vec![
            (0, Vec2::new(0.2, 0.0), Vec3::ZERO),
            (1, Vec2::new(0.7, 0.0), Vec3::ZERO),
            (2, Vec2::new(0.3, 0.8), Vec3::ZERO),
        ];
        let uv_range = (0.0, 1.0, 0.0, 1.0);
        let result = intersect_face_u(&tri, 0.5, uv_range);
        assert!(result.is_some(), "Normal OBJ edge should cross u=0.5");
    }

    #[test]
    fn test_intersect_face_u_seam_uv_interpolation() {
        // Triangle spanning the u=0 seam: v0 at u=5.5, v1 at u=0.5, v2 at u=0.3
        // Cut line at u=0.0 should cross edge v0→v1.
        // The intersection point's u should be near 0.0 (not ~2.5 from naive interpolation).
        let tri: Vec<(usize, Vec2, Vec3)> = vec![
            (0, Vec2::new(5.5, 1.0), Vec3::ZERO),
            (1, Vec2::new(0.5, 0.5), Vec3::ZERO),
            (2, Vec2::new(0.3, 1.5), Vec3::ZERO),
        ];
        let result = intersect_face_u(&tri, 0.0, uv_2pi());
        assert!(result.is_some());
        let isect = result.unwrap();
        assert!(!isect.edge_points.is_empty(), "Should have edge crossings");
        for ep in &isect.edge_points {
            let u = ep.3.x as f64;
            let diff = if u > PI {
                (u - 2.0 * PI).abs()
            } else {
                u.abs()
            };
            assert!(
                diff < 0.1,
                "Intersection u={:.4} should be near 0.0 or 2π",
                u
            );
        }
    }

    #[test]
    fn test_intersect_face_v_seam_uv_interpolation() {
        // Triangle spanning the v=0 seam: v0 at v=5.5, v1 at v=0.5, v2 at v=0.3
        // Cut line at v=0.0 should cross edge v0→v1.
        let tri: Vec<(usize, Vec2, Vec3)> = vec![
            (0, Vec2::new(1.0, 5.5), Vec3::ZERO),
            (1, Vec2::new(0.5, 0.5), Vec3::ZERO),
            (2, Vec2::new(1.5, 0.3), Vec3::ZERO),
        ];
        let result = intersect_face_v(&tri, 0.0, uv_2pi());
        assert!(result.is_some());
        let isect = result.unwrap();
        assert!(!isect.edge_points.is_empty(), "Should have edge crossings");
        for ep in &isect.edge_points {
            let v = ep.3.y as f64;
            let diff = if v > PI {
                (v - 2.0 * PI).abs()
            } else {
                v.abs()
            };
            assert!(
                diff < 0.1,
                "Intersection v={:.4} should be near 0.0 or 2π",
                v
            );
        }
    }

    // ── Knot intersection tests ──────────────────────────────────────

    #[test]
    fn test_intersect_face_knot_simple_cross() {
        // Triangle with v = k*u cut line (k=2) crossing through it.
        // v0: (1.0, 1.0) → v - 2u = -1.0 (Less)
        // v1: (2.0, 1.0) → v - 2u = -3.0 (Less)
        // v2: (1.0, 3.0) → v - 2u =  1.0 (Greater)
        let tri: Vec<(usize, Vec2, Vec3)> = vec![
            (0, Vec2::new(1.0, 1.0), Vec3::ZERO),
            (1, Vec2::new(2.0, 1.0), Vec3::ZERO),
            (2, Vec2::new(1.0, 3.0), Vec3::ZERO),
        ];
        let result = intersect_face_knot_branch(&tri, 2.0, 0, uv_2pi());
        assert!(result.is_some(), "Knot k=2 should cross this triangle");
        let isect = result.unwrap();
        assert!(!isect.edge_points.is_empty(), "Should have edge crossings");
    }

    #[test]
    fn test_intersect_face_knot_no_cross() {
        // All vertices must be strictly on the same side of v = 2*u
        // with none on the line and none wrapping across.
        // v0: (1.0, 0.5) → v - 2u = -1.5
        // v1: (1.5, 0.8) → v - 2u = -2.2
        // v2: (1.0, 0.8) → v - 2u = -1.2
        let tri: Vec<(usize, Vec2, Vec3)> = vec![
            (0, Vec2::new(1.0, 0.5), Vec3::ZERO),
            (1, Vec2::new(1.5, 0.8), Vec3::ZERO),
            (2, Vec2::new(1.0, 0.8), Vec3::ZERO),
        ];
        let result = intersect_face_knot_branch(&tri, 2.0, 0, uv_2pi());
        assert!(result.is_none(), "All vertices on Less side, no crossing");
    }

    #[test]
    fn test_intersect_face_knot_vertex_on_line() {
        // v1 is exactly on the knot line v = 2*u
        // v0: (1.0, 1.0) → v - 2u = -1.0 (Less)
        // v1: (1.5, 3.0) → v - 2u =  0.0 (OnLine)
        // v2: (2.0, 5.0) → v - 2u =  1.0 (Greater)
        let tri: Vec<(usize, Vec2, Vec3)> = vec![
            (0, Vec2::new(1.0, 1.0), Vec3::ZERO),
            (1, Vec2::new(1.5, 3.0), Vec3::ZERO),
            (2, Vec2::new(2.0, 5.0), Vec3::ZERO),
        ];
        let result = intersect_face_knot_branch(&tri, 2.0, 0, uv_2pi());
        assert!(result.is_some());
        let isect = result.unwrap();
        assert!(
            isect.on_line_vertices.contains(&1),
            "Vertex 1 should be on the knot line"
        );
    }

    #[test]
    fn test_intersect_face_knot_large_k_no_false_negative() {
        // Regression test: k=5, face with moderate edge spans (< π) whose
        // vertices genuinely straddle the knot line v = 5*u.  The old
        // normalize_diff_to_pi approach wrapped large k*u products to [-π, π],
        // causing false classifications.
        //
        // v0: (0.5, 3.5) → v - 5u = +1.0   (Greater)
        // v1: (0.5, 2.0) → v - 5u = -0.5   (Less)
        // v2: (1.5, 4.5) → v - 5u = -3.0   (Less)
        let tri: Vec<(usize, Vec2, Vec3)> = vec![
            (0, Vec2::new(0.5, 3.5), Vec3::ZERO),
            (1, Vec2::new(0.5, 2.0), Vec3::ZERO),
            (2, Vec2::new(1.5, 4.5), Vec3::ZERO),
        ];
        let k = 5.0;
        let uv_range = uv_2pi();

        let result = intersect_face_knot_branch(&tri, k, 0, uv_range);
        assert!(
            result.is_some(),
            "k=5 face with vertices on both sides MUST be detected as crossing"
        );
        let isect = result.unwrap();
        assert!(
            !isect.edge_points.is_empty(),
            "Should have edge crossings for k=5 face"
        );
    }

    #[test]
    fn test_intersect_face_knot_k1_diagonal() {
        // k=1 knot line: v = u (diagonal in parameter space)
        // v0: (0.5, 1.5) → v - u =  1.0 (Greater)
        // v1: (2.0, 0.5) → v - u = -1.5 (Less)
        // v2: (3.0, 4.0) → v - u =  1.0 (Greater)
        let tri: Vec<(usize, Vec2, Vec3)> = vec![
            (0, Vec2::new(0.5, 1.5), Vec3::ZERO),
            (1, Vec2::new(2.0, 0.5), Vec3::ZERO),
            (2, Vec2::new(3.0, 4.0), Vec3::ZERO),
        ];
        let result = intersect_face_knot_branch(&tri, 1.0, 0, uv_2pi());
        assert!(result.is_some());
        let isect = result.unwrap();
        assert!(!isect.edge_points.is_empty());
    }

    #[test]
    fn test_intersect_face_knot_all_on_line() {
        // All three vertices satisfy v = u (k=1), all within [0, 2π)
        let tri: Vec<(usize, Vec2, Vec3)> = vec![
            (0, Vec2::new(1.0, 1.0), Vec3::ZERO),
            (1, Vec2::new(1.5, 1.5), Vec3::ZERO),
            (2, Vec2::new(2.0, 2.0), Vec3::ZERO),
        ];
        let result = intersect_face_knot_branch(&tri, 1.0, 0, uv_2pi());
        assert!(result.is_some());
        let isect = result.unwrap();
        assert!(
            isect.edge_points.is_empty(),
            "No edge crossings when all on line"
        );
        assert_eq!(isect.on_line_vertices.len(), 3, "All 3 vertices on line");
    }

    #[test]
    fn test_intersect_face_knot_non_primary_branch() {
        // k=2, face crossing Branch -1 (v = 2u - 2π) but NOT Branch 0 (v = 2u).
        // Branch -1 passes through (π, 0).
        // v0: (π, 0)   → θ = 0 - 2π = -2π  (OnLine for branch -1)
        // v1: (3.5, 0.5) → θ = 0.5 - 7 = -6.5  (Less for branch -1)
        // v2: (3.0, 1.5) → θ = 1.5 - 6 = -4.5  (Greater for branch -1)
        let tri: Vec<(usize, Vec2, Vec3)> = vec![
            (0, Vec2::new(PI as f32, 0.0), Vec3::ZERO),
            (1, Vec2::new(3.5, 0.5), Vec3::ZERO),
            (2, Vec2::new(3.0, 1.5), Vec3::ZERO),
        ];
        // 分支范围计算必须覆盖 branch -1
        let (n_lo, n_hi) = knot_branch_range(&tri, 2.0, uv_2pi());
        assert!(
            n_lo <= -1 && n_hi >= -1,
            "branch range {:?} should cover -1",
            (n_lo, n_hi)
        );
        // 对 branch 0 无交点（该面只跨 branch -1）
        assert!(intersect_face_knot_branch(&tri, 2.0, 0, uv_2pi()).is_none());
        // 对 branch -1 有交点
        let result = intersect_face_knot_branch(&tri, 2.0, -1, uv_2pi());
        assert!(
            result.is_some(),
            "Face crossing branch -1 should be detected"
        );
        let isect = result.unwrap();
        assert!(
            !isect.edge_points.is_empty(),
            "Should have edge crossings on branch -1"
        );
    }

    #[test]
    fn test_knot_cut_full_mesh() {
        // Integration test: generate a torus mesh, cut with k=2 knot,
        // verify all branches are detected and mesh is valid.
        use crate::mesh::half_edge::HalfEdgeMesh;
        use crate::mesh::torus::generate_unfolded_quad_mesh;

        let (positions, uvs, quads) = generate_unfolded_quad_mesh(2.0, 0.6, 24, 20);
        let mut mesh = HalfEdgeMesh::from_quads(&positions, &uvs, &quads);
        let uv_range = uv_2pi();

        let initial_faces = mesh.num_valid_faces();
        cut_mesh_by_knot_line(&mut mesh, 2.0, uv_range);
        let final_faces = mesh.num_valid_faces();

        // k=2 has 2 branches. Each branch should cross roughly res_u faces.
        // With 24 u-resolution, expect at least 24 cuts (old code got ~24 for branch 0 only).
        // With both branches: expect significantly more.
        let cuts = final_faces - initial_faces;
        eprintln!(
            "k=2 knot on 24x20 mesh: {} initial faces, {} final faces, {} new faces from cuts",
            initial_faces, final_faces, cuts
        );
        assert!(
            cuts >= 40,
            "k=2 should produce at least 40 cuts (2 branches × ~24), got {}",
            cuts
        );

        // Verify mesh is valid
        assert!(mesh.validate(), "Mesh should be valid after knot cutting");

        // 分配补片：k=2 的 knot 在 UV 平面分成 k+1 = 3 条带（区域着色）
        assign_multi_knot_patch_indices(&mut mesh, &[2], uv_range);
        let mut patch_values = std::collections::HashSet::new();
        for f in &mesh.faces {
            if f.valid {
                if let Some((pu, _pv)) = f.patch_index {
                    patch_values.insert(pu);
                }
            }
        }
        eprintln!("Distinct patch values for k=2: {:?}", patch_values);
        assert!(
            patch_values.len() >= 2,
            "k=2 knot 在 UV 平面应产生 ≥2 个着色区域，got {}",
            patch_values.len()
        );
    }

    #[test]
    fn test_knot_cut_k3_full_mesh() {
        // Same integration test but with k=3 (3 branches)
        use crate::mesh::half_edge::HalfEdgeMesh;
        use crate::mesh::torus::generate_unfolded_quad_mesh;

        let (positions, uvs, quads) = generate_unfolded_quad_mesh(2.0, 0.6, 24, 20);
        let mut mesh = HalfEdgeMesh::from_quads(&positions, &uvs, &quads);
        let uv_range = uv_2pi();

        let initial_faces = mesh.num_valid_faces();
        cut_mesh_by_knot_line(&mut mesh, 3.0, uv_range);
        let final_faces = mesh.num_valid_faces();

        let cuts = final_faces - initial_faces;
        eprintln!(
            "k=3 knot on 24x20 mesh: {} initial faces, {} final faces, {} new faces from cuts",
            initial_faces, final_faces, cuts
        );
        assert!(
            cuts >= 50,
            "k=3 should produce at least 50 cuts (3 branches × ~24), got {}",
            cuts
        );
        assert!(mesh.validate(), "Mesh should be valid after knot cutting");
    }

    #[test]
    fn test_knot_cut_multiple_branches_coarse_grid() {
        // 粗网格 + 大 k：单个面跨越多条 knot 分支（v = k·u + 2πn），
        // 必须全部切割——旧实现只切 n_lo（最低分支）会大量漏切。
        // 8×6 quad 网格，k=6：曲线在 [0,2π]² 内有 6 条分支。
        use crate::mesh::half_edge::HalfEdgeMesh;
        use crate::mesh::torus::generate_unfolded_quad_mesh;

        let (positions, uvs, quads) = generate_unfolded_quad_mesh(2.0, 0.6, 8, 6);
        let mut mesh = HalfEdgeMesh::from_quads(&positions, &uvs, &quads);
        let initial_faces = mesh.num_valid_faces();

        cut_mesh_by_knot_line(&mut mesh, 6.0, uv_2pi());
        let final_faces = mesh.num_valid_faces();
        let cuts = final_faces - initial_faces;
        eprintln!(
            "k=6 on 8x6 mesh: {} -> {} faces ({} cuts)",
            initial_faces, final_faces, cuts
        );

        assert!(
            mesh.validate(),
            "Mesh should be valid after multi-branch knot cutting"
        );
        // 每条分支应穿过约 8 个面；6 条分支全部切割 → 至少 30 个新面。
        // 若只切 n_lo 分支 → 仅 ~8 个新面，测试必失败。
        assert!(
            cuts >= 30,
            "coarse mesh + k=6 must cut all 6 branches, got only {} cuts",
            cuts
        );
    }

    /// knot 切割 + 补片分配：验证 ByRegion 着色的数据基础——
    /// 所有面都有 patch_index、颜色编号连续（0..=max 无空洞）、至少 2 个区域。
    #[test]
    fn test_knot_patch_assignment_for_coloring() {
        use crate::mesh::half_edge::HalfEdgeMesh;
        use crate::mesh::torus::generate_unfolded_quad_mesh;

        let (positions, uvs, quads) = generate_unfolded_quad_mesh(2.0, 0.6, 24, 20);
        let mut mesh = HalfEdgeMesh::from_quads(&positions, &uvs, &quads);
        let uv_range = uv_2pi();
        cut_mesh_by_knot_line(&mut mesh, 2.0, uv_range);
        assign_multi_knot_patch_indices(&mut mesh, &[2], uv_range);

        for (fi, f) in mesh.faces.iter().enumerate() {
            if f.valid {
                assert!(f.patch_index.is_some(), "面 {fi} 应有 patch_index");
            }
        }
        let colors: Vec<usize> = mesh
            .faces
            .iter()
            .filter(|f| f.valid)
            .filter_map(|f| f.patch_index.map(|p| p.0))
            .collect();
        let max = *colors.iter().max().unwrap();
        for c in 0..=max {
            assert!(colors.contains(&c), "颜色编号 {c} 缺失（有空洞）");
        }
        let distinct: std::collections::BTreeSet<usize> = colors.iter().copied().collect();
        assert!(
            distinct.len() >= 2,
            "k=2 knot 应产生至少 2 个着色区域，got {}",
            distinct.len()
        );
        eprintln!("knot k=2: {} 个区域, max color = {}", distinct.len(), max);
    }

    /// 区域划分一致性：每个面的 patch_index 必须等于其质心所在区域
    /// （find_region_index 语义）——边界处的面不得落入相邻区域。
    #[test]
    fn test_patch_index_matches_face_centroid() {
        use crate::mesh::half_edge::HalfEdgeMesh;
        use crate::mesh::torus::generate_unfolded_quad_mesh;

        let (positions, uvs, quads) = generate_unfolded_quad_mesh(2.0, 0.6, 24, 20);
        let mut mesh = HalfEdgeMesh::from_quads(&positions, &uvs, &quads);
        let uv_range = uv_2pi();
        let two_pi = 2.0 * PI;

        // UI 等分位置（Delaunay 分支公式，测试 Quad 亦用此位置以穿过面）
        let u_vals: Vec<f64> = (0..4).map(|i| two_pi / 4.0 * (i as f64 + 0.5)).collect();
        let v_vals: Vec<f64> = (0..6).map(|i| two_pi / 6.0 * (i as f64 + 0.5)).collect();

        cut_mesh_by_grid(&mut mesh, &u_vals, &v_vals, uv_range, false);
        assign_patch_indices(&mut mesh, &u_vals, &v_vals, uv_range);

        let mut sorted_u = u_vals.clone();
        sorted_u.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let mut sorted_v = v_vals.clone();
        sorted_v.sort_by(|a, b| a.partial_cmp(b).unwrap());

        let mut mismatches = 0;
        for (fi, face) in mesh.faces.iter().enumerate() {
            if !face.valid {
                continue;
            }
            let hes = mesh.face_half_edges(FaceId(fi));
            if hes.len() < 3 {
                continue;
            }
            let (avg_u, avg_v) = circular_centroid(&mesh, &hes, 0.0, two_pi, 0.0, two_pi);
            let (avg_u, avg_v) = (
                normalize_uv(avg_u, 0.0, two_pi),
                normalize_uv(avg_v, 0.0, two_pi),
            );
            let exp = (
                find_region_index(avg_u, &sorted_u),
                find_region_index(avg_v, &sorted_v),
            );
            let got = face.patch_index.unwrap_or((usize::MAX, usize::MAX));
            if exp != got {
                mismatches += 1;
                if mismatches <= 5 {
                    eprintln!(
                        "面 {fi} 质心 ({avg_u:.4},{avg_v:.4}) → 期望区域 {exp:?}，实际 {got:?}"
                    );
                }
            }
        }
        eprintln!("质心-区域不一致面数: {mismatches}");
        assert_eq!(
            mismatches, 0,
            "{mismatches} 个面的 patch_index 与质心区域不一致"
        );
    }

    /// Delaunay 三角网格：每个面的 patch_index 必须等于其质心所在区域——
    /// 边界处的面不得落入相邻区域。
    #[test]
    fn test_delaunay_patch_index_matches_centroid() {
        use crate::mesh::delaunay::generate_delaunay_mesh;
        use crate::mesh::half_edge::HalfEdgeMesh;

        let (positions, uvs, triangles) = generate_delaunay_mesh(2.0, 0.5, 400, 1);
        let mut mesh = HalfEdgeMesh::from_triangles(&positions, &uvs, &triangles);
        let uv_range = uv_2pi();
        let two_pi = 2.0 * PI;

        let u_vals: Vec<f64> = (0..4).map(|i| two_pi / 4.0 * (i as f64 + 0.5)).collect();
        let v_vals: Vec<f64> = (0..6).map(|i| two_pi / 6.0 * (i as f64 + 0.5)).collect();

        cut_mesh_by_grid(&mut mesh, &u_vals, &v_vals, uv_range, false);
        assign_patch_indices(&mut mesh, &u_vals, &v_vals, uv_range);

        let mut sorted_u = u_vals.clone();
        sorted_u.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let mut sorted_v = v_vals.clone();
        sorted_v.sort_by(|a, b| a.partial_cmp(b).unwrap());

        let mut mismatches = 0;
        for (fi, face) in mesh.faces.iter().enumerate() {
            if !face.valid {
                continue;
            }
            let hes = mesh.face_half_edges(FaceId(fi));
            if hes.len() < 3 {
                continue;
            }
            let (avg_u, avg_v) = circular_centroid(&mesh, &hes, 0.0, two_pi, 0.0, two_pi);
            let (avg_u, avg_v) = (
                normalize_uv(avg_u, 0.0, two_pi),
                normalize_uv(avg_v, 0.0, two_pi),
            );
            let exp = (
                find_region_index(avg_u, &sorted_u),
                find_region_index(avg_v, &sorted_v),
            );
            let got = face.patch_index.unwrap_or((usize::MAX, usize::MAX));
            if exp != got {
                mismatches += 1;
                if mismatches <= 5 {
                    eprintln!("面 {fi} 质心 ({avg_u:.4},{avg_v:.4}) → 期望 {exp:?}，实际 {got:?}");
                }
            }
        }
        eprintln!("Delaunay 质心-区域不一致面数: {mismatches}");
        assert_eq!(mismatches, 0, "{mismatches} 个面落入相邻区域");
    }

    /// Quad 实际 UI 位置（网格顶点线：k·range/res）：质心分类一致。
    #[test]
    fn test_quad_gridline_patch_index_matches_centroid() {
        use crate::mesh::half_edge::HalfEdgeMesh;
        use crate::mesh::torus::generate_unfolded_quad_mesh;

        let (positions, uvs, quads) = generate_unfolded_quad_mesh(2.0, 0.6, 24, 20);
        let mut mesh = HalfEdgeMesh::from_quads(&positions, &uvs, &quads);
        let uv_range = uv_2pi();
        let two_pi = 2.0 * PI;

        // Quad 分支：range/res_u·(index+1)
        let u_vals: Vec<f64> = (0..4).map(|i| two_pi / 24.0 * (i as f64 + 1.0)).collect();
        let v_vals: Vec<f64> = (0..6).map(|i| two_pi / 20.0 * (i as f64 + 1.0)).collect();

        cut_mesh_by_grid(&mut mesh, &u_vals, &v_vals, uv_range, false);
        assign_patch_indices(&mut mesh, &u_vals, &v_vals, uv_range);

        let mut sorted_u = u_vals.clone();
        sorted_u.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let mut sorted_v = v_vals.clone();
        sorted_v.sort_by(|a, b| a.partial_cmp(b).unwrap());

        let mut mismatches = 0;
        for (fi, face) in mesh.faces.iter().enumerate() {
            if !face.valid {
                continue;
            }
            let hes = mesh.face_half_edges(FaceId(fi));
            if hes.len() < 3 {
                continue;
            }
            let (avg_u, avg_v) = circular_centroid(&mesh, &hes, 0.0, two_pi, 0.0, two_pi);
            let (avg_u, avg_v) = (
                normalize_uv(avg_u, 0.0, two_pi),
                normalize_uv(avg_v, 0.0, two_pi),
            );
            let exp = (
                find_region_index(avg_u, &sorted_u),
                find_region_index(avg_v, &sorted_v),
            );
            let got = face.patch_index.unwrap_or((usize::MAX, usize::MAX));
            if exp != got {
                mismatches += 1;
                if mismatches <= 5 {
                    eprintln!("面 {fi} 质心 ({avg_u:.4},{avg_v:.4}) → 期望 {exp:?}，实际 {got:?}");
                }
            }
        }
        eprintln!("Quad 网格线切割质心-区域不一致面数: {mismatches}");
        assert_eq!(mismatches, 0, "{mismatches} 个面落入相邻区域");
    }

    /// 漏切检测：切割完成后，任何面都不得跨越切割线
    /// （面顶点在线的两侧即视为跨线，必须已被切分）。
    #[test]
    fn test_no_face_straddles_cut_line() {
        use crate::mesh::delaunay::generate_delaunay_mesh;
        use crate::mesh::half_edge::HalfEdgeMesh;

        let (positions, uvs, triangles) = generate_delaunay_mesh(2.0, 0.5, 400, 1);
        let mut mesh = HalfEdgeMesh::from_triangles(&positions, &uvs, &triangles);
        let uv_range = uv_2pi();
        let two_pi = 2.0 * PI;

        let u_vals: Vec<f64> = (0..4).map(|i| two_pi / 4.0 * (i as f64 + 0.5)).collect();
        let v_vals: Vec<f64> = (0..6).map(|i| two_pi / 6.0 * (i as f64 + 0.5)).collect();

        cut_mesh_by_grid(&mut mesh, &u_vals, &v_vals, uv_range, false);

        let mut straddles = 0;
        let eps = 1e-4;
        for (fi, face) in mesh.faces.iter().enumerate() {
            if !face.valid {
                continue;
            }
            let hes = mesh.face_half_edges(FaceId(fi));
            if hes.len() < 3 {
                continue;
            }
            let us: Vec<f64> = hes
                .iter()
                .map(|he| {
                    normalize_uv(
                        mesh.vertices[mesh.half_edges[he.0].origin.0].uv.x as f64,
                        0.0,
                        two_pi,
                    )
                })
                .collect();
            let vs: Vec<f64> = hes
                .iter()
                .map(|he| {
                    normalize_uv(
                        mesh.vertices[mesh.half_edges[he.0].origin.0].uv.y as f64,
                        0.0,
                        two_pi,
                    )
                })
                .collect();
            // 周期最短弧：相对第一个顶点 unwrap，避免接缝（0↔2π）误报
            let (umin, umax) = {
                let u0 = us[0];
                let uw: Vec<f64> = us.iter().map(|&u| unwrap_angle(u, u0)).collect();
                (
                    uw.iter().cloned().fold(f64::MAX, f64::min),
                    uw.iter().cloned().fold(f64::MIN, f64::max),
                )
            };
            let (vmin, vmax) = {
                let v0 = vs[0];
                let vw: Vec<f64> = vs.iter().map(|&v| unwrap_angle(v, v0)).collect();
                (
                    vw.iter().cloned().fold(f64::MAX, f64::min),
                    vw.iter().cloned().fold(f64::MIN, f64::max),
                )
            };
            for &uc in &u_vals {
                let uc_n = unwrap_angle(normalize_uv(uc, 0.0, two_pi), us[0]);
                if umin < uc_n - eps && umax > uc_n + eps {
                    straddles += 1;
                    if straddles <= 5 {
                        eprintln!("面 {fi} 跨 U 线 {uc:.4}（u∈[{umin:.4},{umax:.4}]）未切开");
                    }
                }
            }
            for &vc in &v_vals {
                let vc_n = unwrap_angle(normalize_uv(vc, 0.0, two_pi), vs[0]);
                if vmin < vc_n - eps && vmax > vc_n + eps {
                    straddles += 1;
                    if straddles <= 5 {
                        eprintln!("面 {fi} 跨 V 线 {vc:.4}（v∈[{vmin:.4},{vmax:.4}]）未切开");
                    }
                }
            }
        }
        eprintln!("跨线未切开的面数: {straddles}");

        assert_eq!(straddles, 0, "{straddles} 个面跨越切割线但未被切开");
    }
}
