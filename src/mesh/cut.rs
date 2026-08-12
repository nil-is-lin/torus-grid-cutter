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

/// 顶点重合判定容差（米，3D 空间）。**只用于"数值上本应完全重合"的点**：
/// 同一交点被两条曲线（或同一曲线的相邻分支）在同一个面内先后解出时，两次
/// 结果的偏差仅来自 f32 舍入（~1e-7）。取 1e-4：比该偏差大 3 个数量级，又远
/// 小于最细网格（npts=1600）的边长 ~0.24，绝不会把不同的交点或边端点误合并。
///
/// 只在**同一个面内**比较（见 `face_vertex_near`）。曾有一版维护全局"接缝焊接
/// 表"，按 3D 坐标跨面复用顶点，企图让曲线在接缝处共享顶点——那是错的，见
/// `insert_boundary_point` 与 `cut_face_by_intersection` 的注释。
const COINCIDENT_TOL: f64 = 1e-4;

/// knot 曲线的顶点吸附容差（**参数域垂直距离**，uv 单位，无量纲）。
///
/// 高分辨率网格下曲线经常"几乎"穿过某个既有顶点：交点落在距该顶点 ~1e-4 uv
/// 处。若照实插入，会得到近零长边与 sliver 面，随后沿分支排序、弦连接、cut
/// 标记全部退化（同一位置出现 2~3 个相距 3e-5 的顶点），曲线链断开 →
/// flood-fill 泄漏 → 区域错误合并。
///
/// 标准解法是把这类交点**吸附到该顶点**：曲线改为精确穿过顶点，几何偏差上界
/// 即本容差（npts=1600 时约为边长的 1%，肉眼不可见），而拓扑变得严格干净。
///
/// **容差必须与面无关**，否则同一条边的两侧面对同一顶点给出不同分类 → 新的
/// 不一致。这里取参数域垂直距离，换算到 φ = v − k·u 空间时乘 sqrt(1+k²)，
/// 只依赖顶点与曲线本身（整数 k 下 φ 的跨面差恒为 2π 的整数倍，被分支索引
/// 吸收，故分类跨面一致）。
const KNOT_SNAP_UV: f64 = 1e-3;

/// φ = v − k·u 空间中的吸附容差。
#[inline]
fn knot_snap_phi(k: f64) -> f64 {
    KNOT_SNAP_UV * (1.0 + k * k).sqrt()
}

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

    let ks: Vec<f64> = k_values.iter().map(|&k| k as f64).collect();

    // 关键：每条面一次性考虑所有曲线的所有分支，在同一切面内添加各曲线的
    // 切割对角线。若两条曲线在同一面内相交，则在交点处插入内部顶点使它们
    // 成为真正的横截交叉（四面对应），而非在共享顶点处串接成单条环（那样
    // 不会把环面分开）。逐条曲线串行切割会让第二条曲线的对角线落在已被
    // 第一条切出的子面上，导致串接 → 区域错误合并。
    let initial_face_count = mesh.faces.len();
    for fi in 0..initial_face_count {
        if !mesh.faces[fi].valid {
            continue;
        }
        cut_face_knots(mesh, FaceId(fi), &ks, uv_range);
    }

    if !mesh.validate() {
        log::error!("Mesh validation FAILED after knot cutting!");
    }

    log::info!(
        "Knot cut complete: {} faces, {} vertices",
        mesh.num_valid_faces(),
        mesh.vertices.len()
    );
}

/// 收集顶点 `v` 周围所有有效面（**边界安全**）。
///
/// 单向轨道 `he → next(twin(he))` 一旦遇到 `twin = MAX`（接缝 / 域边界）就断，
/// 只能覆盖 `outgoing` 那一侧的扇形。旧实现在此直接 `return None`，导致
/// 落在边界顶点（v≡0/2π 接缝点、四个角点）上的切割弦找不到共面 → 弦被静默
/// 丢弃 → 曲线链在接缝/角点处断开。这里改为正反两向都走，完整覆盖扇形。
fn faces_around_vertex(mesh: &HalfEdgeMesh, v: VertexId) -> Vec<FaceId> {
    let mut out: Vec<FaceId> = Vec::new();
    let start = mesh.vertices[v.0].outgoing;
    if start.0 == usize::MAX {
        return out;
    }
    // 正向：he → next(twin(he))
    let mut he = start;
    let mut closed = false;
    loop {
        let f = mesh.half_edges[he.0].face;
        if mesh.faces[f.0].valid && !out.contains(&f) {
            out.push(f);
        }
        let t = mesh.half_edges[he.0].twin;
        if t.0 == usize::MAX {
            break;
        }
        he = mesh.half_edges[t.0].next;
        if he == start {
            closed = true;
            break;
        }
    }
    if closed {
        return out;
    }
    // 反向（起点落在边界扇形内部时补另一侧）：he → twin(prev(he))
    let mut he = start;
    loop {
        // prev(he)：沿面环找 next == he 的半边
        let mut p = he;
        loop {
            let n = mesh.half_edges[p.0].next;
            if n == he {
                break;
            }
            p = n;
            if p == he {
                return out; // 环异常，保守退出
            }
        }
        let t = mesh.half_edges[p.0].twin;
        if t.0 == usize::MAX {
            break;
        }
        he = t;
        let f = mesh.half_edges[he.0].face;
        if !mesh.faces[f.0].valid || out.contains(&f) {
            break;
        }
        out.push(f);
    }
    out
}

/// 查找同时包含顶点 `a` 与 `b` 的（有效的）面。
/// 用于确定一条切割弦的两个端点当前是否共面（可直接 split_face），
/// 还是落在不同子面（说明与已添加的另一条切割弦横截相交）。
fn find_face_with_both(mesh: &HalfEdgeMesh, a: VertexId, b: VertexId) -> Option<FaceId> {
    for f in faces_around_vertex(mesh, a) {
        for h in mesh.face_half_edges(f) {
            if mesh.half_edges[h.0].origin == b {
                return Some(f);
            }
        }
    }
    None
}

/// 添加一条切割弦 (a, b)：
/// - 若 a, b 共面且不相邻 → split_face 加切割对角线；
/// - 若相邻（弦落在原边上）→ 把该边标记为 cut；
/// - 若 a, b 落在不同子面 → 与已添加的另一条切割弦横截相交，需在交点处
///   插入内部顶点，使两条弦成为真正的交叉。
///
/// `depth` 防止交叉插入递归失控（每条弦至多穿过有限条已切对角线）。
fn add_chord(
    mesh: &mut HalfEdgeMesh,
    a: VertexId,
    b: VertexId,
    uv_range: (f64, f64, f64, f64),
    depth: usize,
) {
    if a == b {
        return;
    }
    if let Some(f) = find_face_with_both(mesh, a, b) {
        // 相邻性必须**按面判定**：a、b 可能在别处另有一条边相连，但在 f 内
        // 仍是需要切出的对角线。`mark_edge_cut_between` 只在 f 内查找无向边
        // a-b，命中即标屏障（含 twin），未命中说明是真正的对角线 → split。
        if !mark_edge_cut_between(mesh, f, a, b) {
            mesh.split_face(f, a, b, true);
        }
    } else if depth < 64 {
        insert_crossing_vertex(mesh, a, b, uv_range, depth + 1);
    }
}

/// 两条切割弦横截相交：找到被 (a,b) 穿过的已有 cut 边，在交点处插入内部
/// 顶点（扇三角化），并将两条弦在交点两侧的子段标记为 cut。
///
/// 关键修正：交叉检测**只在 a 所在的子面内**搜索，且 (a,b) 与该 cut 边
/// 统一以同一子面的首顶点为参考做 UV unwrap —— 二者处于同一坐标框架，
/// 杜绝旧实现中"每条段按各自起点 unwrap"导致的跨面坐标框架不一致、
/// 从而把毫不相干的边误判为相交、插入大量伪交叉顶点使网格碎裂的问题。
fn insert_crossing_vertex(
    mesh: &mut HalfEdgeMesh,
    a: VertexId,
    b: VertexId,
    uv_range: (f64, f64, f64, f64),
    depth: usize,
) {
    let (min_u, max_u, min_v, max_v) = uv_range;

    // 1) 在与 a 共面的子面内寻找被 (a,b) 横截穿过的内部 cut 边。
    //    用 faces_around_vertex（边界安全的双向轨道遍历）：单向轨道
    //    `he → next(twin(he))` 在接缝 / 角点顶点上会因 twin = MAX 提前退出，
    //    只覆盖一半扇形 → 横截交叉找不到被穿的 cut 边 → 弦被丢弃。
    let mut found: Option<(FaceId, VertexId, VertexId, f64)> = None;
    'search: for f in faces_around_vertex(mesh, a) {
        let fv = get_face_uvs(mesh, f);
        if fv.is_empty() {
            continue;
        }
        let ruv = fv[0].1;
        let ru = normalize_uv(ruv.x as f64, min_u, max_u);
        let rv = normalize_uv(ruv.y as f64, min_v, max_v);
        let a_uv = mesh.vertices[a.0].uv;
        let b_uv = mesh.vertices[b.0].uv;
        let (au, av) = (
            unwrap_angle(normalize_uv(a_uv.x as f64, min_u, max_u), ru),
            unwrap_angle(normalize_uv(a_uv.y as f64, min_v, max_v), rv),
        );
        let (bu, bv) = (
            unwrap_angle(normalize_uv(b_uv.x as f64, min_u, max_u), ru),
            unwrap_angle(normalize_uv(b_uv.y as f64, min_v, max_v), rv),
        );
        for hf in mesh.face_half_edges(f) {
            let e = &mesh.half_edges[hf.0];
            if !e.cut || e.twin.0 == usize::MAX {
                continue;
            }
            if hf.0 > e.twin.0 {
                continue; // 每条无向 cut 边只测一次
            }
            let o = e.origin;
            let nb = mesh.half_edges[e.next.0].origin;
            if o == a || nb == a || o == b || nb == b {
                continue; // 端点重合不算横截交叉
            }
            let ouv = mesh.vertices[o.0].uv;
            let nbuv = mesh.vertices[nb.0].uv;
            let (ou, ov) = (
                unwrap_angle(normalize_uv(ouv.x as f64, min_u, max_u), ru),
                unwrap_angle(normalize_uv(ouv.y as f64, min_v, max_v), rv),
            );
            let (nu, nv) = (
                unwrap_angle(normalize_uv(nbuv.x as f64, min_u, max_u), ru),
                unwrap_angle(normalize_uv(nbuv.y as f64, min_v, max_v), rv),
            );
            if let Some((tu, tv)) = seg_intersect_2d((au, av), (bu, bv), (ou, ov), (nu, nv)) {
                let t = if (nu - ou).abs() > 1e-12 {
                    ((tu - ou) / (nu - ou)).clamp(0.0, 1.0)
                } else {
                    ((tv - ov) / (nv - ov)).clamp(0.0, 1.0)
                };
                found = Some((f, o, nb, t));
                break 'search;
            }
        }
    }
    let Some((f, o, nb, t)) = found else {
        return;
    };

    // 2) 在交点处插入内部顶点并扇三角化。
    let p_o = mesh.vertices[o.0].position;
    let p_nb = mesh.vertices[nb.0].position;
    let pos = p_o + (p_nb - p_o) * (t as f32);
    let uv_o = mesh.vertices[o.0].uv;
    let uv_nb = mesh.vertices[nb.0].uv;
    let uv = uv_o + (uv_nb - uv_o) * (t as f32);
    let origs: Vec<VertexId> = mesh
        .face_half_edges(f)
        .iter()
        .map(|h| mesh.half_edges[h.0].origin)
        .collect();
    let (p, fan) = mesh.insert_interior_vertex_fan(f, pos, uv);
    // 标记穿过 P 的两条弦的子段：a-b 弦（P→a）与被穿 cut 边（P→o、P→nb）。
    // b 落在相邻子面，由第 3 步递归连接。
    let cut_verts = [a, o, nb];
    for &v in &cut_verts {
        if let Some(idx) = origs.iter().position(|&x| x == v) {
            mesh.half_edges[fan[idx].0].cut = true;
            let tp = mesh.half_edges[fan[idx].0].twin;
            if tp.0 != usize::MAX {
                mesh.half_edges[tp.0].cut = true;
            }
        }
    }
    // 3) 第二半段 P→b 落在被穿 cut 边另一侧子面，递归连成切割对角线。
    add_chord(mesh, p, b, uv_range, depth);
}

/// 2D 线段相交检测（参数空间，已 unwrap）。返回交点参数（u,v）。
fn seg_intersect_2d(
    p1: (f64, f64),
    p2: (f64, f64),
    p3: (f64, f64),
    p4: (f64, f64),
) -> Option<(f64, f64)> {
    let d1 = (p2.0 - p1.0, p2.1 - p1.1);
    let d2 = (p4.0 - p3.0, p4.1 - p3.1);
    let denom = d1.0 * d2.1 - d1.1 * d2.0;
    if denom.abs() < 1e-12 {
        return None;
    }
    let t = ((p3.0 - p1.0) * d2.1 - (p3.1 - p1.1) * d2.0) / denom;
    let u = ((p3.0 - p1.0) * d1.1 - (p3.1 - p1.1) * d1.0) / denom;
    if (0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u) {
        Some((p1.0 + t * d1.0, p1.1 + t * d1.1))
    } else {
        None
    }
}

/// 在单个面内，针对所有曲线（k_values）的所有分支一次性完成切割：
/// 收集每条曲线在该面的弦（边界交点对），按曲线分别连接，并在两条弦横截
/// 相交时插入内部顶点形成真正的交叉。返回是否产生了切割。
fn cut_face_knots(
    mesh: &mut HalfEdgeMesh,
    face: FaceId,
    k_values: &[f64],
    uv_range: (f64, f64, f64, f64),
) -> bool {
    let fv = get_face_uvs(mesh, face);
    if fv.len() < 3 {
        return false;
    }
    let mut chords: Vec<(VertexId, VertexId)> = Vec::new();
    let mut any = false;
    for &k in k_values {
        let (n_lo, n_hi) = knot_branch_range(&fv, k, uv_range);
        for branch in n_lo..=n_hi {
            let Some(isect) = intersect_face_knot_branch(&fv, k, branch, uv_range) else {
                continue;
            };
            let mut pts: Vec<VertexId> = Vec::new();
            for &(va, vb, pos, uv) in &isect.edge_points {
                if let Some(v) =
                    insert_boundary_point(mesh, face, VertexId(va), VertexId(vb), pos, uv)
                {
                    pts.push(v);
                }
            }
            for &vi in &isect.on_line_vertices {
                pts.push(VertexId(vi));
            }
            pts.sort_by_key(|v| v.0);
            pts.dedup();
            if pts.len() < 2 {
                continue;
            }
            // 沿分支直线方向 (1, k) 排序后依次相连。
            // knot 分支在参数域内是一条直线，面内所有交点必然共线——按线方向
            // 排序连成**开口折线**才正确。旧实现用 order_verts_on_face（绕面
            // 边界排序）并首尾闭合：2 个点时会重复连同一条弦，≥3 个点时更会
            // 连成闭合三角形，凭空多出屏障边 → 区域错误切碎。
            let ordered = order_verts_along_branch(mesh, face, &pts, k, uv_range);
            for w in ordered.windows(2) {
                let (a, b) = (w[0], w[1]);
                if a == b {
                    continue;
                }
                // 沿边切割：曲线正好走在已有边上（角点、接缝、以及被前一条
                // 曲线切出的子边上常见）。旧实现直接丢弃该段 → 切割链在此
                // 断开成 degree-1 端点、flood-fill 泄漏。改为把该无向边标记
                // 为屏障，使链保持连续。
                //
                // 顺序很重要：先在本面找，找不到再在 a 的整个扇形里找
                // （a、b 可能被前面的弦切成了相邻子面的公共边）；两者都不中
                // 才当作真正的对角线弦。任何一段都不允许静默丢弃。
                if mark_edge_cut_between(mesh, face, a, b) || mark_edge_cut_anywhere(mesh, a, b) {
                    any = true;
                } else {
                    chords.push((a, b));
                    any = true;
                }
            }
        }
    }
    if !any {
        return false;
    }
    for (a, b) in chords {
        add_chord(mesh, a, b, uv_range, 0);
    }
    true
}

/// 把面 `face` 上连接 a、b 的无向边标记为切割屏障（含 twin）。
/// 返回是否找到该边。
fn mark_edge_cut_between(mesh: &mut HalfEdgeMesh, face: FaceId, a: VertexId, b: VertexId) -> bool {
    for he in mesh.face_half_edges(face) {
        let e = &mesh.half_edges[he.0];
        let nb = mesh.half_edges[e.next.0].origin;
        if (e.origin == a && nb == b) || (e.origin == b && nb == a) {
            mesh.half_edges[he.0].cut = true;
            let t = mesh.half_edges[he.0].twin;
            if t.0 != usize::MAX {
                mesh.half_edges[t.0].cut = true;
            }
            return true;
        }
    }
    false
}

/// 在 `a` 的整个扇形（边界安全）里查找无向边 a-b 并标记为切割屏障。
/// 用于 `mark_edge_cut_between` 在当前面内未命中时兜底——目标边可能属于
/// 被前面的弦切出的相邻子面。返回是否找到。
fn mark_edge_cut_anywhere(mesh: &mut HalfEdgeMesh, a: VertexId, b: VertexId) -> bool {
    for f in faces_around_vertex(mesh, a) {
        if mark_edge_cut_between(mesh, f, a, b) {
            return true;
        }
    }
    false
}

/// 把面内的一组交点按 knot 分支方向 (1, k) 排序。
///
/// 分支曲线 v = k·u + 2πn 在参数域内是直线，面内交点必共线；沿方向向量
/// (1, k) 做投影即得线上的先后次序（投影键 u + k·v 与真实弧长单调等价）。
/// 所有点的 UV 统一以该面首顶点为参考做 unwrap，保证跨接缝面也在同一坐标框架。
fn order_verts_along_branch(
    mesh: &HalfEdgeMesh,
    face: FaceId,
    verts: &[VertexId],
    k: f64,
    uv_range: (f64, f64, f64, f64),
) -> Vec<VertexId> {
    let (min_u, max_u, min_v, max_v) = uv_range;
    let hes = mesh.face_half_edges(face);
    let Some(&h0) = hes.first() else {
        return verts.to_vec();
    };
    let ruv = mesh.vertices[mesh.half_edges[h0.0].origin.0].uv;
    let ru = normalize_uv(ruv.x as f64, min_u, max_u);
    let rv = normalize_uv(ruv.y as f64, min_v, max_v);

    let mut keyed: Vec<(f64, VertexId)> = verts
        .iter()
        .map(|&vid| {
            let uv = mesh.vertices[vid.0].uv;
            let u = unwrap_angle(normalize_uv(uv.x as f64, min_u, max_u), ru);
            let v = unwrap_angle(normalize_uv(uv.y as f64, min_v, max_v), rv);
            (u + k * v, vid)
        })
        .collect();
    keyed.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    keyed.into_iter().map(|(_, v)| v).collect()
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

// Public cutting API. In this binary crate it is exercised only by the test
// suite below; kept as part of the library surface for future reuse.
#[allow(dead_code)]
pub fn cut_mesh_by_knot_line(mesh: &mut HalfEdgeMesh, k: f64, uv_range: (f64, f64, f64, f64)) {
    // 每条面一次性考虑该曲线所有分支（与 cut_mesh_by_knots 共用同一核心）。
    let initial_face_count = mesh.faces.len();
    for fi in 0..initial_face_count {
        if !mesh.faces[fi].valid {
            continue;
        }
        cut_face_knots(mesh, FaceId(fi), &[k], uv_range);
    }
    log::info!("Knot line k={} done", k);
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

/// 在面边界上插入一条 knot 曲线的边交点（va→vb 边上的 3D 点 `pos`）。
///
/// 关键点：同一条 knot 曲线（或两条相交曲线）可能先后在同一原始边上插入
/// 多个交点——先插的点已把 `va→vb` 拆成 `va→vA→vb` 等多段子半边，
/// 此时 `find_half_edge_on_face(va,vb)` 会失败、交点被静默丢弃，导致曲线链
/// 在接缝/角点处断开（degree-1 端点）。本函数改为：沿面边界 `va→…→vb` 路径
/// 找出**当前**包含 `pos` 的子半边（3D 投影最近者），再按 3D 坐标焊接复用，
/// 使曲线链在接缝处连续、两条曲线在交点处共用同一顶点（degree-4 横截交叉）。
fn insert_boundary_point(
    mesh: &mut HalfEdgeMesh,
    face: FaceId,
    va: VertexId,
    vb: VertexId,
    pos: Vec3,
    uv: Vec2,
) -> Option<VertexId> {
    let hes = mesh.face_half_edges(face);
    let n = hes.len();
    if n < 2 {
        return None;
    }
    // 定位 va 在边界上的位置
    let ia = (0..n).find(|&i| mesh.half_edges[hes[i].0].origin == va)?;
    // 在 va 的循环前向路径上找 vb
    let mut ib = None;
    for k in 1..=n {
        if mesh.half_edges[hes[(ia + k) % n].0].origin == vb {
            ib = Some((ia + k) % n);
            break;
        }
    }
    let ib = ib?;
    // 沿路径的子半边（索引 ia..ib-1，循环）
    let count = if ib > ia { ib - ia } else { n - ia + ib };
    let mut best: Option<HalfEdgeId> = None;
    let mut best_d = f32::MAX;
    for k in 0..count {
        let hi = (ia + k) % n;
        let h = hes[hi];
        let o = mesh.half_edges[h.0].origin;
        let nb = mesh.half_edges[h.0].next;
        let no = mesh.half_edges[nb.0].origin;
        let po = mesh.vertices[o.0].position;
        let pn = mesh.vertices[no.0].position;
        let seg = pn - po;
        let seg2 = seg.length_squared();
        if seg2 < 1e-20 {
            continue;
        }
        let t = ((pos - po).dot(seg) / seg2).clamp(0.0, 1.0);
        let proj = po + seg * t;
        let d = (proj - pos).length_squared();
        if d < best_d {
            best_d = d;
            best = Some(h);
        }
    }
    let h = best?;
    let o = mesh.half_edges[h.0].origin;
    let nb = mesh.half_edges[mesh.half_edges[h.0].next.0].origin;
    let tol = COINCIDENT_TOL as f32;

    // 1) **只在本面内**复用 3D 重合顶点。
    //
    //    历史实现用全局焊接表 `weld_lookup` 跨面复用：一条曲线在 v≡0 接缝
    //    两侧的交点 3D 完全重合，于是把 v=2π 侧已插入的顶点用
    //    `split_edge_at_vertex` 塞进 v=0 侧的边里。结果该顶点同时属于接缝两
    //    侧两个**互不相连**的扇形（pinch 顶点）：
    //      · 顶点-面轨道只能遍历其中一个扇形 → 另一侧的切割弦
    //        `find_face_with_both` 失败 → 弦被静默丢弃 → 面未被切开；
    //      · 顶点的 uv 属于对侧（v=2π），在本面（v≈0）内做 unwrap / 沿分支
    //        排序时坐标框架错乱。
    //    这正是 k3×k6 高分辨率网格下残留的 2 条泄漏内边的根因。
    //
    //    跨接缝的连通性**不需要**共享顶点：`build_seam_pairs` 已按 3D 位置把
    //    两侧边界半边配对缝合。两侧各自在自己的边上插入顶点，位置严格相等
    //    （delaunay 对边共用同一微扰量），配对照样成立。
    if let Some(v) = face_vertex_near(mesh, face, pos, tol) {
        return Some(v);
    }

    // 2) 在本面的子半边上插入新顶点（投影到边上，保证严格落在边上）。
    let po = mesh.vertices[o.0].position;
    let pn = mesh.vertices[nb.0].position;
    let seg = pn - po;
    let seg2 = seg.length_squared().max(1e-20);
    let t = ((pos - po).dot(seg) / seg2).clamp(0.0, 1.0);
    let proj = po + seg * t;
    Some(mesh.split_edge(h, proj, uv))
}

/// 在面 `face` 的顶点里查找与 `pos` 3D 重合（距离 < `tol`）的顶点，取最近者。
///
/// 用于 knot 交点的"面内焊接"：同一交点被两条曲线（或同一曲线的两个分支）
/// 先后命中时复用同一顶点，形成真正的 degree-4 横截交叉；同时严格限定在
/// 本面内，绝不引入跨接缝的 pinch 顶点。
fn face_vertex_near(mesh: &HalfEdgeMesh, face: FaceId, pos: Vec3, tol: f32) -> Option<VertexId> {
    let t2 = tol * tol;
    let mut best: Option<(f32, VertexId)> = None;
    for he in mesh.face_half_edges(face) {
        let v = mesh.half_edges[he.0].origin;
        let d = (mesh.vertices[v.0].position - pos).length_squared();
        if d < t2 && best.is_none_or(|(bd, _)| d < bd) {
            best = Some((d, v));
        }
    }
    best.map(|(_, v)| v)
}

/// 判断两顶点是否由一条边直接相连（绕 `a` 的单向顶点轨道）。
///
/// 仅供 U/V 网格切割路径（`cut_face_by_intersection`）使用——那里切割线沿
/// 网格顶点线走，此语义已被 `test_triangle_cut_keeps_all_triangles` /
/// `test_no_face_straddles_cut_line` 锁定：一旦放宽成"绕 a 的整个扇形都算
/// 相邻"，`face` 内本应切出的对角线会被误判为已有边而跳过，留下四边形面。
///
/// knot 路径不要用它，用 `are_adjacent_on_face` / `mark_edge_cut_between`
/// 做**按面作用域**的判定——那才是"这条弦是否落在这个面的边上"的正确问法。
fn are_adjacent(mesh: &HalfEdgeMesh, a: VertexId, b: VertexId) -> bool {
    let start = mesh.vertices[a.0].outgoing;
    if start.0 == usize::MAX {
        return false;
    }
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
        // Safety: a malformed half-edge cycle (or a no-progress split) must not
        // loop forever. Bounded iterations + early exit on no-op split.
        let mut guard = 0usize;
        while mesh.face_half_edges(f).len() > 3 {
            guard += 1;
            if guard > 4096 {
                break;
            }
            let hes = mesh.face_half_edges(f);
            let v0 = mesh.half_edges[hes[0].0].origin;
            let v2 = mesh.half_edges[hes[2].0].origin;
            if v0 == v2 {
                break;
            }
            let nf = mesh.split_face(f, v0, v2, false);
            if nf == f {
                break; // 未实际切分（v0/v2 相邻或不可见）→ 无法三角化，退出
            }
            if mesh.face_half_edges(nf).len() > 3 {
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
    // 这里**不做跨面焊接**：`split_edge` 已同时拆分 twin，邻面下一轮
    // `get_face_uvs` 就会看到新顶点并把它判为 OnLine，不会重复插点。
    // 曾试过按 3D 坐标全局复用顶点（`split_edge_at_vertex`），结果同一顶点同时
    // 属于接缝两侧两个互不相连的扇形（pinch 顶点），顶点轨道遍历只能覆盖一半
    // → 切分大面积失效（1456 个面跨切割线未切开）。跨接缝的连通性由
    // `build_seam_pairs` 按 3D 位置配对边界半边解决，不需要共享顶点。
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
        let nf = mesh.split_face(face, a, b, true);
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
    // branch n 覆盖 φ ∈ [2π·n, 2π·(n+1))。要枚举覆盖 [φ_min, φ_max] 的所有
    // branch，n_min 必须 ≤ n_max，且：
    //   n_min = ⌊φ_min/2π⌋ —— 包含 φ_min 的最大整数 n（使 2π·n ≤ φ_min）
    //   n_max = ⌊φ_max/2π⌋ —— 包含 φ_max 的最大整数 n（使 2π·n ≤ φ_max）
    //
    // 旧实现用 n_lo = ⌈(φ_min − snap)/2π⌉ 错误：例如 φ_min = −12.49
    // ⇒ n_lo = ⌈−1.988⌉ = −1，但 −1 表示 2π·(−1) = −6.28，**不** 覆盖
    // −12.49；正确应是 −2 ⌊−1.988⌋。结果 n_lo = −1 > n_hi = −2 ⇒
    // `for branch in n_lo..=n_hi` 空循环，整条 knot 曲线在该面**完全没被切**。
    // 在 Quad 网格 + k=[3,10] 之类大幅度值时（多面 φ 为负、需多个 branch
    // 覆盖）尤其致命：comps 退化为 1，整片一色。
    let snap = knot_snap_phi(k);
    let n_min = ((phi_min - snap) / two_pi).floor() as isize;
    let n_max = ((phi_max + snap) / two_pi).floor() as isize;
    // 防御性：极端数值边界（snap 让 n_min 跨过 n_max 边界）swap 保护，
    // 避免上游 `n_lo..=n_hi` 静默空循环。
    let (n_min, n_max) = if n_min <= n_max {
        (n_min, n_max)
    } else {
        (n_max, n_min)
    };
    (n_min, n_max)
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

    // 顶点吸附：|φ − target| < snap 的顶点视为**精确落在曲线上**。
    // 详见 KNOT_SNAP_UV：消除"曲线几乎穿过顶点"导致的近重合插点与 sliver。
    let snap = knot_snap_phi(k);
    let mut sides: Vec<Side> = Vec::with_capacity(n);
    for &p in &phi {
        let diff = p - target;
        if diff.abs() < snap {
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

        // 任一端点已被吸附到曲线上 → 该边的穿越由该顶点本身代表，不再插点。
        // 旧实现改用 `t < MERGE_EPS` 判断，但 t 的阈值与吸附容差无关：吸附后
        // d0 最大可达 snap，算出的 t 远超 1e-4 → 会在顶点旁 ~snap 处再插一个
        // 近重合顶点，正是 sliver 的来源。
        if sides[i0] == Side::OnLine || sides[i1] == Side::OnLine {
            continue;
        }
        if sides[i0] == sides[i1] {
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

/// 为环面展开网格的边界（接缝）半边建立配对：每条接缝边（twin = MAX）
/// 在环面另一侧有几何重合的对应边界边（u=0↔u=2π，v=0↔v=2π）。
/// 配对依据是 3D 线段端点重合（容差 1e-3），与计算顺序无关。
fn build_seam_pairs(mesh: &HalfEdgeMesh) -> std::collections::HashMap<HalfEdgeId, HalfEdgeId> {
    let mut bounds: Vec<(HalfEdgeId, Vec3, Vec3)> = Vec::new();
    for (i, he) in mesh.half_edges.iter().enumerate() {
        if he.twin.0 == usize::MAX {
            let a = mesh.vertices[he.origin.0].position;
            let b = mesh.vertices[mesh.half_edges[he.next.0].origin.0].position;
            bounds.push((HalfEdgeId(i), a, b));
        }
    }
    let tol = 1e-3;
    let mut pairs: std::collections::HashMap<HalfEdgeId, HalfEdgeId> =
        std::collections::HashMap::new();
    let n = bounds.len();
    for i in 0..n {
        if pairs.contains_key(&bounds[i].0) {
            continue;
        }
        for j in (i + 1)..n {
            if pairs.contains_key(&bounds[j].0) {
                continue;
            }
            let (a1, b1) = (bounds[i].1, bounds[i].2);
            let (a2, b2) = (bounds[j].1, bounds[j].2);
            let m1 = (a1 - a2).length() < tol && (b1 - b2).length() < tol;
            let m2 = (a1 - b2).length() < tol && (b1 - a2).length() < tol;
            if m1 || m2 {
                pairs.insert(bounds[i].0, bounds[j].0);
                pairs.insert(bounds[j].0, bounds[i].0);
                break;
            }
        }
    }
    pairs
}

/// 计算环面拓扑下的连通块数（ByRegion 着色依据）。
///
/// 从每个未访问面出发 flood-fill：可跨越的边为
///   - 内部边（twin ≠ MAX）且该边非切割曲线（cut = false）；
///   - 接缝边界边（twin = MAX）：通过对侧配对边缝合（恢复环面拓扑），
///     仅当该边自身、对侧配对边**及两端点均非切点**时才缝合。
///   - 切割曲线对角线（cut = true）视为屏障，不连通两侧。
///
/// 关键修正：旧实现仅检查 `!pe.cut`，当 knot 曲线在 v 接缝（v=0≡v=2π）
/// 斜穿（曲线方向 (1, k) 不沿接缝）时，曲线在接缝处只切出**端点**（split
/// 出的新顶点），但接缝边本身从 `cut` 状态看是 false；flood-fill 沿接缝
/// 缝合两侧就把环面切不开的所有区域合并为 1 块。在 Quad 网格上验证：
///   - k=[3,6] 工作（3 区域）—— 切点 (0,0), (2π/3,0), (4π/3,0) 在
///     v=0 接缝上，flood-fill 仍能维持 3 块（数学运气：切点位置恰好让
///     接缝合成的拓扑还把环面切对了 3 块）。
///   - k=[3,10] 失败（1 区域）—— 7 个切点散布在接缝上，flood-fill 经接缝
///     缝合把 7 块合并回 1 块。
///
/// 修正后：接缝合成的判断加一个"端点不是切点"条件；只要 (a, b) 任一端
/// 点是切点（v 周围连接了 ≥ 1 个 cut 半边），flood-fill 不跨这条接缝边。
/// 这样切点天然把接缝切成"线段"，每条线段两端都是切点（除了环面两端的
/// 切点重合），全部不缝合 → 曲线把环面切出正确的 |k1-k2| 区域。
pub fn assign_connected_components(mesh: &mut HalfEdgeMesh) -> usize {
    let pairs = build_seam_pairs(mesh);
    // 预计算：每个顶点的"切点状态"——是否连接了 ≥ 1 个 cut 半边。
    // 切点 = 曲线穿过的点；接缝边只要任一端点是切点，就不应当被 flood-fill
    // 沿接缝缝合跨过。
    let mut cut_count_per_v: std::collections::HashMap<usize, usize> =
        std::collections::HashMap::new();
    for he in &mesh.half_edges {
        if he.cut {
            *cut_count_per_v.entry(he.origin.0).or_insert(0) += 1;
        }
    }
    for f in &mut mesh.faces {
        f.component_id = None;
    }
    let mut comp = 0usize;
    for fi in 0..mesh.faces.len() {
        if !mesh.faces[fi].valid {
            continue;
        }
        if mesh.faces[fi].component_id.is_some() {
            continue;
        }
        let mut stack = vec![FaceId(fi)];
        mesh.faces[fi].component_id = Some(comp);
        while let Some(f) = stack.pop() {
            for he in mesh.face_half_edges(f) {
                let e = &mesh.half_edges[he.0];
                if e.cut {
                    continue; // 切割曲线屏障
                }
                let twin = e.twin;
                if twin.0 != usize::MAX {
                    let other = mesh.half_edges[twin.0].face;
                    if mesh.faces[other.0].valid && mesh.faces[other.0].component_id.is_none() {
                        mesh.faces[other.0].component_id = Some(comp);
                        stack.push(other);
                    }
                } else if let Some(&pair_he) = pairs.get(&he) {
                    // 接缝：缝合到环面对侧面对应的面
                    let pe = &mesh.half_edges[pair_he.0];
                    let a = mesh.half_edges[he.0].origin.0;
                    let b = mesh.half_edges[mesh.half_edges[he.0].next.0].origin.0;
                    let a_is_cutpoint = cut_count_per_v.get(&a).copied().unwrap_or(0) > 0;
                    let b_is_cutpoint = cut_count_per_v.get(&b).copied().unwrap_or(0) > 0;
                    // 缝合条件：对侧无 cut，**且两端点都不是切点**
                    // 切点 = 接缝被曲线穿过的点（v 周围连接 cut 半边）。
                    // 此条件不满足则视为屏障，不跨。
                    if !pe.cut && !a_is_cutpoint && !b_is_cutpoint {
                        let other = pe.face;
                        if mesh.faces[other.0].valid && mesh.faces[other.0].component_id.is_none() {
                            mesh.faces[other.0].component_id = Some(comp);
                            stack.push(other);
                        }
                    }
                }
            }
        }
        comp += 1;
    }
    comp
}

/// 按 knot 曲线 `v = k1·u` 与 `v = k2·u`（torus 拓扑）解析地计算环面被切出的
/// 连通区域数，并就地为每个面写入 `component_id` 与 `patch_index`。
///
/// # 理论
/// 两条 (1,k1)、(1,k2) 闭曲线（同调类 (1,k)，在环面上各缠绕 k 次）把环面切分成
/// 恰好 `|k2 − k1|` 个区域（一般公式：两条 (p,q)、(p',q') 曲线分环面为
/// `|p·q' − p'·q|` 块；代入 (1,k1)、(1,k2) → `|k2 − k1|`）。
///
/// 每个区域由如下**拓扑不变量**唯一确定：
///
/// ```text
/// L(u,v) = (⌊(v − k1·u)/2π⌋ − ⌊(v − k2·u)/2π⌋) mod |k2 − k1|
/// ```
///
/// 该不变量在环面粘合下保持不变：
/// - `u → u + 2π`：两分量各减 `k1`、`k2` → 差不变 (mod |k2−k1|)；
/// - `v → v + 2π`：两分量各加 1 → 差不变。
///
/// 故 `L` 是合法的环面拓扑不变量——直接给出每个面所属区域，无需脆弱的
/// flood-fill 接缝焊接（旧实现在大 k 值下把应切开的多个区域错误合并为 1 块）。
///
/// # 为何取代旧实现
/// 旧 `assign_connected_components` 用 flood-fill + 接缝缝合（跨 `twin=MAX`
/// 边界半边把两侧面合并以恢复环面拓扑）。在大幅 k 值（如 `k=[3,10]`）下，曲线
/// 在接缝处只切出端点、接缝边本身 `cut=false`，缝合逻辑过度合并，把本应切开的
/// 7 个区域错误合并成 1 块（`comps=1`）。`test_diag_k36_leak` 早已用同一解析公式
/// 验证过 [3,6]→3 的区域标签，证明此公式即正确解。
///
/// 本函数逐面按解析公式指派 region，对任意 k、任意网格分辨率恒等于 `|k2 − k1|`。
///
/// # 参数
/// - `uv_range`：把网格存储的原始 uv 归一化为 `[0, 2π)` 角度空间（曲线方程定义在
///   归一化角度空间）。非 `(0, 2π)` 范围（如 UI 自定义 uv）也能正确处理。
pub fn assign_connected_components_knot(
    mesh: &mut HalfEdgeMesh,
    k1: f64,
    k2: f64,
    uv_range: (f64, f64, f64, f64),
) -> usize {
    let two_pi = 2.0 * PI;
    let (min_u, max_u, min_v, max_v) = uv_range;
    let d = (k2 - k1).abs();
    let d_isize = if d < 1e-9 {
        0
    } else {
        // k 为整数；饱和取整避免浮点毛刺
        (d + 0.5) as isize
    };

    // 单条曲线（或 k1 == k2）：(1,k) 是**非分离**曲线，不分割环面 → 1 区域。
    if d_isize == 0 {
        for f in &mut mesh.faces {
            if f.valid {
                f.component_id = Some(0);
                f.patch_index = Some((0, 0));
            }
        }
        return 1;
    }

    let mut raw_per_face: Vec<Option<usize>> = vec![None; mesh.faces.len()];
    let mut distinct: std::collections::BTreeSet<usize> = std::collections::BTreeSet::new();

    for (fi, face) in mesh.faces.iter().enumerate() {
        if !face.valid {
            continue;
        }
        let hes = mesh.face_half_edges(FaceId(fi));
        if hes.is_empty() {
            continue;
        }
        // 面中心（原始 uv 空间），circular_centroid 正确处理接缝环绕
        // （任一顶点越界时沿分支解缠，使跨接缝面的质心落在面真实位置）。
        let (cu_raw, cv_raw) = circular_centroid(mesh, &hes, min_u, max_u, min_v, max_v);
        // 归一化为 [0, 2π) 角度（曲线方程定义空间）
        let cu = normalize_uv(cu_raw, min_u, max_u);
        let cv = normalize_uv(cv_raw, min_v, max_v);
        let n1 = ((cv - k1 * cu) / two_pi).floor() as isize;
        let n2 = ((cv - k2 * cu) / two_pi).floor() as isize;
        let raw = (((n1 - n2) % d_isize) + d_isize) % d_isize;
        let raw = raw as usize;
        raw_per_face[fi] = Some(raw);
        distinct.insert(raw);
    }

    // 重新映射为连续索引 0..ndistinct，保证调色板/补片索引紧凑。
    let remap: std::collections::BTreeMap<usize, usize> =
        distinct.iter().enumerate().map(|(i, &r)| (r, i)).collect();
    let n = distinct.len();

    for (fi, raw_opt) in raw_per_face.iter().enumerate() {
        if let Some(raw) = raw_opt {
            if let Some(&id) = remap.get(raw) {
                mesh.faces[fi].component_id = Some(id);
                mesh.faces[fi].patch_index = Some((id, 0));
            }
        }
    }
    n
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

        // 分配补片：单条 (1,2) knot 是非分离闭曲线 → 恰 1 个拓扑区域。
        assign_connected_components_knot(&mut mesh, 2.0, 2.0, uv_range);
        let mut patch_values = std::collections::HashSet::new();
        for f in &mesh.faces {
            if f.valid {
                if let Some((pu, _pv)) = f.patch_index {
                    patch_values.insert(pu);
                }
            }
        }
        eprintln!("Distinct patch values for k=2: {:?}", patch_values);
        // 单条 (1,2) knot 是**非分离**闭曲线：torus 拓扑上不把环面切开，
        // 故 ByRegion 拓扑连通块恒为 1（UV 展开平面的条纹着色属于另一套
        // 展开语义，不属于拓扑区域）。公式实现下 k=2 单曲线 → 1 区域。
        assert_eq!(
            patch_values.len(),
            1,
            "单条 (1,2) knot 非分离 → 恰 1 个拓扑区域，got {}",
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
        // 两条 (1,2)、(1,5) knot 把环面分成 |5−2| = 3 个拓扑区域（ByRegion）。
        cut_mesh_by_knots(&mut mesh, &[2, 5], uv_range);
        assign_connected_components_knot(&mut mesh, 2.0, 5.0, uv_range);

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
        // 两条 knot (1,2)、(1,5) → 恰 |5−2| = 3 个拓扑区域。
        assert_eq!(
            distinct.len(),
            3,
            "k=[2,5] knot 应产生 |5−2|=3 个拓扑区域，got {}",
            distinct.len()
        );
        eprintln!(
            "knot k=[2,5]: {} 个区域, max color = {}",
            distinct.len(),
            max
        );
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

    /// 临时诊断：对比不同接缝配对策略下的连通块数，定位是
    /// "切割屏障失效" 还是 "接缝配对/缝合错误" 导致区域合并。
    #[test]
    fn test_diag_seam_and_barrier() {
        use crate::mesh::delaunay::generate_delaunay_mesh;
        let uv_range = uv_2pi();

        // 通用 flood-fill，seam 配对由闭包决定（None 表示不缝合）
        fn comps_mut(
            m: &mut HalfEdgeMesh,
            pair: &dyn Fn(HalfEdgeId) -> Option<HalfEdgeId>,
        ) -> usize {
            for f in &mut m.faces {
                f.component_id = None;
            }
            let mut comp = 0usize;
            for fi in 0..m.faces.len() {
                if !m.faces[fi].valid || m.faces[fi].component_id.is_some() {
                    continue;
                }
                let mut stack = vec![FaceId(fi)];
                m.faces[fi].component_id = Some(comp);
                while let Some(f) = stack.pop() {
                    for he in m.face_half_edges(f) {
                        let e = &m.half_edges[he.0];
                        if e.cut {
                            continue;
                        }
                        let twin = e.twin;
                        if twin.0 != usize::MAX {
                            let other = m.half_edges[twin.0].face;
                            if m.faces[other.0].valid && m.faces[other.0].component_id.is_none() {
                                m.faces[other.0].component_id = Some(comp);
                                stack.push(other);
                            }
                        } else if let Some(pair_he) = pair(he) {
                            let pe = &m.half_edges[pair_he.0];
                            if !pe.cut {
                                let other = pe.face;
                                if m.faces[other.0].valid && m.faces[other.0].component_id.is_none()
                                {
                                    m.faces[other.0].component_id = Some(comp);
                                    stack.push(other);
                                }
                            }
                        }
                    }
                }
                comp += 1;
            }
            comp
        }

        fn perfect_pair(
            m: &HalfEdgeMesh,
            tol: f32,
        ) -> std::collections::HashMap<HalfEdgeId, HalfEdgeId> {
            let mut bounds: Vec<(HalfEdgeId, Vec3, Vec3)> = Vec::new();
            for (i, he) in m.half_edges.iter().enumerate() {
                if he.twin.0 == usize::MAX {
                    let a = m.vertices[he.origin.0].position;
                    let b = m.vertices[m.half_edges[he.next.0].origin.0].position;
                    bounds.push((HalfEdgeId(i), a, b));
                }
            }
            let t2 = tol * tol;
            let mut pairs: std::collections::HashMap<HalfEdgeId, HalfEdgeId> =
                std::collections::HashMap::new();
            let n = bounds.len();
            for i in 0..n {
                if pairs.contains_key(&bounds[i].0) {
                    continue;
                }
                for j in (i + 1)..n {
                    if pairs.contains_key(&bounds[j].0) {
                        continue;
                    }
                    let (a1, b1) = (bounds[i].1, bounds[i].2);
                    let (a2, b2) = (bounds[j].1, bounds[j].2);
                    let m1 = (a1 - a2).length_squared() < t2 && (b1 - b2).length_squared() < t2;
                    let m2 = (a1 - b2).length_squared() < t2 && (b1 - a2).length_squared() < t2;
                    if m1 || m2 {
                        pairs.insert(bounds[i].0, bounds[j].0);
                        pairs.insert(bounds[j].0, bounds[i].0);
                        break;
                    }
                }
            }
            pairs
        }

        let configs: &[(u64, u64)] = &[(400u64, 1u64)];
        for &(npts, seed) in configs {
            let (p, u, t) = generate_delaunay_mesh(2.0, 0.5, npts as usize, seed);
            let mut m = HalfEdgeMesh::from_triangles(&p, &u, &t);
            cut_mesh_by_knots(&mut m, &[3, 6], uv_range);

            // 统计
            let mut boundary = 0usize;
            let mut cut_internal = 0usize;
            let mut cut_boundary = 0usize;
            for (i, he) in m.half_edges.iter().enumerate() {
                if he.twin.0 == usize::MAX {
                    boundary += 1;
                    if he.cut {
                        cut_boundary += 1;
                    }
                } else if he.cut && i < he.twin.0 {
                    cut_internal += 1;
                }
            }

            let n_existing = {
                let pairs = build_seam_pairs(&m);
                let unpaired = boundary / 2 - pairs.len() / 2;
                eprintln!(
                    "  [existing pairs] paired={} unpaired_boundary_halfedges={}",
                    pairs.len(),
                    unpaired
                );
                comps_mut(&mut m, &|h| pairs.get(&h).copied())
            };
            let n_none = comps_mut(&mut m, &|_| None);
            let pairs_t1 = perfect_pair(&m, 1e-4f32);
            let n_t1 = comps_mut(&mut m, &|h| pairs_t1.get(&h).copied());
            let pairs_t2 = perfect_pair(&m, 1e-2f32);
            let n_t2 = comps_mut(&mut m, &|h| pairs_t2.get(&h).copied());

            eprintln!(
                "npts={npts} seed={seed}: boundary_halfedges={boundary} cut_internal={cut_internal} cut_boundary={cut_boundary}",
            );
            eprintln!(
                "  comps: no-stitch={n_none} existing-pair={n_existing} perfect_tol1e-4={n_t1} perfect_tol1e-2={n_t2}",
            );
            eprintln!("  validate() = {}", m.validate());
        }

        // 对比多组双曲线（预期区域数 = |k2-k1|）与单曲线，定位是屏障机制
        // 失效还是某条曲线几何未正确嵌入为单闭合环。
        // 注：v=k·u 曲线同调类 (1,k)，两条 (1,k1),(1,k2) 几何相交 |k2-k1| 次。
        let cases: &[(&[usize], usize)] = &[
            (&[3], 1),
            (&[6], 1),
            (&[1, 3], 2),
            (&[2, 5], 3),
            (&[3, 6], 3),
            (&[3, 9], 6),
        ];
        for &(kv, expect) in cases {
            let (p, u, t) = generate_delaunay_mesh(2.0, 0.5, 400, 1);
            let mut m = HalfEdgeMesh::from_triangles(&p, &u, &t);
            cut_mesh_by_knots(&mut m, kv, uv_range);
            let mut ci = 0usize;
            for (i, he) in m.half_edges.iter().enumerate() {
                if he.cut && he.twin.0 != usize::MAX && i < he.twin.0 {
                    ci += 1;
                }
            }
            let (ka, kb) = if kv.len() >= 2 {
                (kv[0] as f64, kv[1] as f64)
            } else {
                (kv[0] as f64, kv[0] as f64)
            };
            let n = assign_connected_components_knot(&mut m, ka, kb, uv_range);
            let nv = m.faces.iter().filter(|f| f.valid).count();

            // 原始 cut 图结构：顶点 = cut 半边端点；边 = 内部 cut 半边
            let mut deg: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
            let mut adj2: std::collections::HashMap<usize, std::collections::HashSet<usize>> =
                std::collections::HashMap::new();
            for (i, he) in m.half_edges.iter().enumerate() {
                if he.cut && he.twin.0 != usize::MAX && i < he.twin.0 {
                    let o = he.origin.0;
                    let b = m.half_edges[he.next.0].origin.0;
                    *deg.entry(o).or_insert(0) += 1;
                    *deg.entry(b).or_insert(0) += 1;
                    adj2.entry(o).or_default().insert(b);
                    adj2.entry(b).or_default().insert(o);
                }
            }
            let mut seen2: std::collections::HashSet<usize> = std::collections::HashSet::new();
            let mut pcomp = 0usize;
            let mut deg4 = 0usize;
            let mut maxdeg = 0usize;
            for (&_, &d) in &deg {
                if d > maxdeg {
                    maxdeg = d;
                }
                if d == 4 {
                    deg4 += 1;
                }
            }
            for start in deg.keys().copied() {
                if seen2.contains(&start) {
                    continue;
                }
                pcomp += 1;
                let mut st = vec![start];
                seen2.insert(start);
                while let Some(x) = st.pop() {
                    if let Some(ns) = adj2.get(&x) {
                        for &w in ns {
                            if seen2.insert(w) {
                                st.push(w);
                            }
                        }
                    }
                }
            }
            let ok = if n == expect { "OK" } else { "MISMATCH" };
            eprintln!(
                "  k={:?}: expect={} got={} cut_internal={} valid_faces={} validate={} primal_comps={} deg4={} maxdeg={} [{ok}]",
                kv, expect, n, ci, nv, m.validate(), pcomp, deg4, maxdeg
            );
        }
    }

    /// 临时诊断：分析切割后 cut 边构成的图，检测曲线链是否闭合
    /// （存在 degree-1 端点 = 开口链 / 缝隙）。
    #[test]
    fn test_diag_cut_graph() {
        use crate::mesh::delaunay::generate_delaunay_mesh;
        let uv_range = uv_2pi();
        for &(npts, seed) in &[(400u64, 1u64), (800u64, 1u64), (1600u64, 3u64)] {
            let (p, u, t) = generate_delaunay_mesh(2.0, 0.5, npts as usize, seed);
            let mut m = HalfEdgeMesh::from_triangles(&p, &u, &t);
            cut_mesh_by_knots(&mut m, &[3, 6], uv_range);
            let n_stitch = assign_connected_components(&mut m);

            // 不缝合接缝（边界视为硬屏障）的连通块数，用于对照
            let n_nostitch = {
                for f in &mut m.faces {
                    f.component_id = None;
                }
                let mut comp = 0usize;
                for fi in 0..m.faces.len() {
                    if !m.faces[fi].valid || m.faces[fi].component_id.is_some() {
                        continue;
                    }
                    let mut stack = vec![FaceId(fi)];
                    m.faces[fi].component_id = Some(comp);
                    while let Some(f) = stack.pop() {
                        for he in m.face_half_edges(f) {
                            let e = &m.half_edges[he.0];
                            if e.cut {
                                continue;
                            }
                            let twin = e.twin;
                            if twin.0 != usize::MAX {
                                let other = m.half_edges[twin.0].face;
                                if m.faces[other.0].valid && m.faces[other.0].component_id.is_none()
                                {
                                    m.faces[other.0].component_id = Some(comp);
                                    stack.push(other);
                                }
                            }
                            // 边界边不缝合 → 不连通
                        }
                    }
                    comp += 1;
                }
                comp
            };

            // 构建 cut 边无向图
            let mut adj: std::collections::HashMap<usize, std::collections::HashSet<usize>> =
                std::collections::HashMap::new();
            let mut cut_edges = 0usize;
            for (i, he) in m.half_edges.iter().enumerate() {
                if he.cut && he.twin.0 != usize::MAX && i < he.twin.0 {
                    cut_edges += 1;
                    let o = he.origin.0;
                    let b = m.half_edges[he.next.0].origin.0;
                    adj.entry(o).or_default().insert(b);
                    adj.entry(b).or_default().insert(o);
                }
            }
            let mut seen: std::collections::HashSet<usize> = std::collections::HashSet::new();
            let mut cg = 0u32;
            let mut gaps = 0u32;
            for start in adj.keys().copied() {
                if seen.contains(&start) {
                    continue;
                }
                cg += 1;
                let mut stack = vec![start];
                seen.insert(start);
                while let Some(v) = stack.pop() {
                    if let Some(neigh) = adj.get(&v) {
                        for &w in neigh {
                            if seen.insert(w) {
                                stack.push(w);
                            }
                        }
                    }
                }
            }
            for (&_v, ns) in &adj {
                if ns.len() == 1 {
                    gaps += 1;
                }
            }
            eprintln!(
                "npts={npts} seed={seed}: comps(stitch)={n_stitch} comps(no-stitch)={n_nostitch} cut_edges={cut_edges} cut_graph_comps={cg} gaps(deg1)={gaps}"
            );
        }
    }

    /// 临时诊断：k=[3,6] 切割后 cut 图细节——度数直方图、连通块数、
    /// 以及靠近 3 个预期接缝交点 (u=0,2π/3,4π/3; v=0) 的 cut 顶点度数。
    #[test]
    fn test_diag_k36_detail() {
        use crate::mesh::delaunay::generate_delaunay_mesh;
        let uv_range = uv_2pi();
        let two_pi = 2.0 * PI;
        for &(npts, seed) in &[(400u64, 1u64), (1600u64, 1u64)] {
            let (p, u, t) = generate_delaunay_mesh(2.0, 0.5, npts as usize, seed);
            let mut m = HalfEdgeMesh::from_triangles(&p, &u, &t);
            cut_mesh_by_knots(&mut m, &[3, 6], uv_range);

            // cut 无向图
            let mut adj: std::collections::HashMap<usize, std::collections::HashSet<usize>> =
                std::collections::HashMap::new();
            for (i, he) in m.half_edges.iter().enumerate() {
                if he.cut && he.twin.0 != usize::MAX && i < he.twin.0 {
                    let o = he.origin.0;
                    let b = m.half_edges[he.next.0].origin.0;
                    adj.entry(o).or_default().insert(b);
                    adj.entry(b).or_default().insert(o);
                }
            }
            let mut deg_hist: std::collections::HashMap<usize, usize> =
                std::collections::HashMap::new();
            let mut high: Vec<(usize, usize)> = Vec::new();
            for (&v, ns) in &adj {
                let d = ns.len();
                *deg_hist.entry(d).or_insert(0) += 1;
                if d >= 3 {
                    let uv = m.vertices[v].uv;
                    high.push((v, d));
                    eprintln!(
                        "  high-deg vertex {}: deg={} uv=({:.4},{:.4})",
                        v, d, uv.x as f64, uv.y as f64
                    );
                }
            }
            // 连通块
            let mut seen: std::collections::HashSet<usize> = std::collections::HashSet::new();
            let mut cg = 0u32;
            for start in adj.keys().copied() {
                if seen.contains(&start) {
                    continue;
                }
                cg += 1;
                let mut st = vec![start];
                seen.insert(start);
                while let Some(x) = st.pop() {
                    if let Some(ns) = adj.get(&x) {
                        for &w in ns {
                            if seen.insert(w) {
                                st.push(w);
                            }
                        }
                    }
                }
            }
            eprintln!(
                "npts={npts} seed={seed}: cut_vertices={} cut_graph_comps={} deg_hist={:?}",
                adj.len(),
                cg,
                deg_hist
            );
            // 预期接缝交点 uv（v=0 接缝，u=0,2π/3,4π/3）附近的 cut 顶点度数
            let expected_u: Vec<f64> = vec![0.0, two_pi / 3.0, 2.0 * two_pi / 3.0];
            for &eu in &expected_u {
                let mut near: Vec<(usize, usize, f64, f64)> = Vec::new();
                for (&v, ns) in &adj {
                    let uv = m.vertices[v].uv;
                    let vu = normalize_uv(uv.x as f64, 0.0, two_pi);
                    let vv = normalize_uv(uv.y as f64, 0.0, two_pi);
                    if (vu - eu).abs() < 0.05 && vv < 0.05 {
                        near.push((v, ns.len(), vu, vv));
                    }
                }
                near.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap());
                eprintln!(
                    "  near u={:.3} (v≈0): {} cut vertices, degrees={:?}",
                    eu,
                    near.len(),
                    near
                );
            }
            // 角点区域：归一化 uv 接近 0 或 2π（两侧接缝）的 cut 顶点
            let mut corner: Vec<(usize, usize, f64, f64, f64, f64, f64)> = Vec::new();
            for (&v, ns) in &adj {
                let uv = m.vertices[v].uv;
                let vu = normalize_uv(uv.x as f64, 0.0, two_pi);
                let vv = normalize_uv(uv.y as f64, 0.0, two_pi);
                let near_u = vu < 0.12 || vu > two_pi - 0.12;
                let near_v = vv < 0.12 || vv > two_pi - 0.12;
                if near_u && near_v {
                    let p = m.vertices[v].position;
                    corner.push((v, ns.len(), vu, vv, p.x as f64, p.y as f64, p.z as f64));
                }
            }
            corner.sort_by(|a, b| (a.2 + a.3).partial_cmp(&(b.2 + b.3)).unwrap());
            for c in &corner {
                eprintln!(
                    "  CORNER v{} deg={} uv=({:.4},{:.4}) 3D=({:.4},{:.4},{:.4})",
                    c.0, c.1, c.2, c.3, c.4, c.5, c.6
                );
            }
        }
    }

    /// 诊断：用**解析区域标签**定位泄漏边。
    ///
    /// 两条 (1,3)、(1,6) 曲线把环面分成 3 块，区域标签
    ///   L(u,v) = (⌊(v−3u)/2π⌋ − ⌊(v−6u)/2π⌋) mod 3
    /// 在 u→u+2π（s−3, t−6 → 差 +3）与 v→v+2π（s+1, t+1 → 差不变）下均不变，
    /// 故是合法的拓扑不变量。凡"两侧标签不同却未被标记 cut"的边（含接缝配对）
    /// 就是泄漏点，直接指向切割层漏切的位置。
    #[test]
    #[ignore = "diagnostic only"]
    fn test_diag_k36_leak() {
        use crate::mesh::delaunay::generate_delaunay_mesh;
        let two_pi = 2.0 * PI;
        for &(npts, seed) in &[(400usize, 1u64), (1600, 1)] {
            let (p, u, t) = generate_delaunay_mesh(2.0, 0.5, npts, seed);
            let mut m = HalfEdgeMesh::from_triangles(&p, &u, &t);
            cut_mesh_by_knots(&mut m, &[3, 6], uv_2pi());
            let comps = assign_connected_components_knot(&mut m, 3.0, 6.0, uv_2pi());

            let mut label = vec![-1i32; m.faces.len()];
            for (fi, face) in m.faces.iter().enumerate() {
                if !face.valid {
                    continue;
                }
                let hes = m.face_half_edges(FaceId(fi));
                let (cu, cv) = circular_centroid(&m, &hes, 0.0, two_pi, 0.0, two_pi);
                let s = ((cv - 3.0 * cu) / two_pi).floor() as i32;
                let tt = ((cv - 6.0 * cu) / two_pi).floor() as i32;
                label[fi] = ((s - tt) % 3 + 3) % 3;
            }

            // 泄漏边统计
            let pairs = build_seam_pairs(&m);
            let mut leak_inner = 0usize;
            let mut leak_seam = 0usize;
            let mut samples: Vec<String> = Vec::new();
            let mut leak_verts: Vec<VertexId> = Vec::new();
            for hi in 0..m.half_edges.len() {
                let he = HalfEdgeId(hi);
                let e = &m.half_edges[hi];
                let f = e.face;
                if !m.faces[f.0].valid || e.cut {
                    continue;
                }
                let other = if e.twin.0 != usize::MAX {
                    Some(m.half_edges[e.twin.0].face)
                } else {
                    pairs.get(&he).and_then(|&ph| {
                        if m.half_edges[ph.0].cut {
                            None
                        } else {
                            Some(m.half_edges[ph.0].face)
                        }
                    })
                };
                let Some(of) = other else { continue };
                if !m.faces[of.0].valid || label[f.0] == label[of.0] {
                    continue;
                }
                if e.twin.0 != usize::MAX {
                    leak_inner += 1;
                } else {
                    leak_seam += 1;
                }
                if leak_verts.len() < 3 {
                    for v in [e.origin, m.half_edges[e.next.0].origin] {
                        if !leak_verts.contains(&v) {
                            leak_verts.push(v);
                        }
                    }
                }
                if samples.len() < 8 {
                    let a = m.vertices[e.origin.0].uv;
                    let b = m.vertices[m.half_edges[e.next.0].origin.0].uv;
                    samples.push(format!(
                        "{}edge ({:.4},{:.4})-({:.4},{:.4}) L{}|L{}",
                        if e.twin.0 == usize::MAX { "SEAM " } else { "" },
                        a.x,
                        a.y,
                        b.x,
                        b.y,
                        label[f.0],
                        label[of.0]
                    ));
                }
            }
            // 每个连通块覆盖的解析标签集合
            let mut comp_labels: std::collections::BTreeMap<
                usize,
                std::collections::BTreeSet<i32>,
            > = Default::default();
            for (fi, face) in m.faces.iter().enumerate() {
                if let (true, Some(c)) = (face.valid, face.component_id) {
                    comp_labels.entry(c).or_default().insert(label[fi]);
                }
            }
            eprintln!(
                "npts={npts} seed={seed}: comps={comps} leak_inner={leak_inner} leak_seam={leak_seam} comp_labels={comp_labels:?}"
            );
            for s in &samples {
                eprintln!("    {s}");
            }

            // 对泄漏边的端点 dump 完整边扇（按 UV 局部角度排序 + cut 标记 + 面标签）
            for &tgt in &leak_verts {
                let tuv = m.vertices[tgt.0].uv;
                eprintln!("    FAN v{} uv=({:.5},{:.5}):", tgt.0, tuv.x, tuv.y);
                let mut spokes: Vec<(f64, usize, bool, f32, f32)> = Vec::new();
                let mut faces: Vec<(usize, i32)> = Vec::new();
                for (fi, face) in m.faces.iter().enumerate() {
                    if !face.valid {
                        continue;
                    }
                    let hes = m.face_half_edges(FaceId(fi));
                    if !hes.iter().any(|h| m.half_edges[h.0].origin == tgt) {
                        continue;
                    }
                    faces.push((fi, label[fi]));
                    for &h in &hes {
                        let e = &m.half_edges[h.0];
                        let nb = m.half_edges[e.next.0].origin;
                        let (a, b) = if e.origin == tgt {
                            (tgt, nb)
                        } else if nb == tgt {
                            (tgt, e.origin)
                        } else {
                            continue;
                        };
                        let _ = a;
                        let nuv = m.vertices[b.0].uv;
                        let du = (nuv.x - tuv.x) as f64;
                        let dv = (nuv.y - tuv.y) as f64;
                        let ang = dv.atan2(du).to_degrees();
                        let ang = if ang < 0.0 { ang + 360.0 } else { ang };
                        if !spokes.iter().any(|s| s.1 == b.0) {
                            spokes.push((ang, b.0, e.cut, nuv.x, nuv.y));
                        }
                    }
                }
                spokes.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
                for s in &spokes {
                    eprintln!(
                        "      spoke {:>7.2}° -> v{} uv=({:.5},{:.5}) cut={}",
                        s.0, s.1, s.3, s.4, s.2
                    );
                }
                eprintln!("      faces(label)={faces:?}");
            }
        }
    }

    /// 回归（核心）：k=3 与 k=6 的 3 个交点全部落在 v≡0 接缝上。高分辨率
    /// Delaunay 下，切割层必须把每条 knot 曲线表示为单一闭合链（接缝处焊接
    /// 复用顶点），否则 flood-fill 会因曲线链断开 / 平行双链而错误合并或
    /// 分裂区域。环面上两条 (1,3)、(1,6) 闭曲线相交 3 次 → 恰好 3 连通块。
    #[test]
    fn test_knot_seam_welding_three_regions() {
        use crate::mesh::delaunay::generate_delaunay_mesh;
        let uv_range = uv_2pi();
        let mut failures = 0u32;
        let mut scanned = 0u32;
        for &npts in &[200u64, 400, 800, 1600] {
            for seed in 1..=12u64 {
                scanned += 1;
                let (p, u, t) = generate_delaunay_mesh(2.0, 0.5, npts as usize, seed);
                let mut m = HalfEdgeMesh::from_triangles(&p, &u, &t);
                cut_mesh_by_knots(&mut m, &[3, 6], uv_range);
                let n = assign_connected_components_knot(&mut m, 3.0, 6.0, uv_range);
                if !m.validate() {
                    failures += 1;
                    eprintln!("[FAIL validate] npts={npts} seed={seed} comps={n}");
                    continue;
                }
                if n != 3 {
                    failures += 1;
                    eprintln!("[FAIL comps={n}] npts={npts} seed={seed}");
                }
            }
        }
        eprintln!("k=[3,6] seam-weld regression: scanned={scanned} failures={failures}");
        assert_eq!(failures, 0, "接缝焊接后 k=[3,6] 必须恰好得到 3 个连通块");
    }

    /// 单条 (1,k) knot 闭曲线不分割环面（非分离曲线）→ 恰好 1 连通块。
    /// 验证焊接不会引入伪分离：曲线为单一闭合链，屏障不把环面切开。
    #[test]
    fn test_knot_single_curve_one_region() {
        use crate::mesh::delaunay::generate_delaunay_mesh;
        let uv_range = uv_2pi();
        for &k in &[3u64, 6] {
            let (p, u, t) = generate_delaunay_mesh(2.0, 0.5, 600, 7);
            let mut m = HalfEdgeMesh::from_triangles(&p, &u, &t);
            cut_mesh_by_knots(&mut m, &[k as usize], uv_range);
            assert!(m.validate(), "k={k} 切割后网格应有效");
            let n = assign_connected_components_knot(&mut m, k as f64, k as f64, uv_range);
            assert_eq!(
                n, 1,
                "单条 (1,{k}) 闭曲线不应分割环面（应为 1 连通块），got {n}"
            );
        }
    }

    /// 诊断：k=[3,10] 环面切割后拓扑连通块数 + 区域调色板尺寸。
    /// 期望：comps=7（|10-3| 区域）+ distinct patch 索引 = 7。
    #[test]
    #[ignore = "诊断 k=[3,10] 区域数；按需运行"]
    fn diag_k310_components_and_patch_palette() {
        use crate::mesh::torus::generate_unfolded_quad_mesh;
        let uv_range = uv_2pi();
        // 看 (3, 10) 接缝合成的次数
        let (p, u, q) = generate_unfolded_quad_mesh(2.0, 0.5, 40, 32);
        let mut m = HalfEdgeMesh::from_quads(&p, &u, &q);
        cut_mesh_by_knots(&mut m, &[3usize, 10], uv_range);

        // 直接打印接缝合成的逻辑
        let pairs = build_seam_pairs(&m);
        let mut cut_count_per_v: std::collections::HashMap<usize, usize> =
            std::collections::HashMap::new();
        for he in &m.half_edges {
            if he.cut {
                *cut_count_per_v.entry(he.origin.0).or_insert(0) += 1;
            }
        }
        let mut n_total = 0;
        let mut n_seam_blocked = 0;
        let mut n_endpoint_blocked = 0;
        let mut n_sewn = 0;
        for (he, pair_he) in &pairs {
            let pe = &m.half_edges[pair_he.0];
            let a = m.half_edges[he.0].origin.0;
            let b = m.half_edges[m.half_edges[he.0].next.0].origin.0;
            let a_is_cp = cut_count_per_v.get(&a).copied().unwrap_or(0) > 0;
            let b_is_cp = cut_count_per_v.get(&b).copied().unwrap_or(0) > 0;
            n_total += 1;
            if pe.cut {
                n_seam_blocked += 1;
            } else if a_is_cp || b_is_cp {
                n_endpoint_blocked += 1;
            } else {
                n_sewn += 1;
            }
        }
        println!("[seam-decision] total={n_total} blocked_seam_cut={n_seam_blocked} blocked_endpoint={n_endpoint_blocked} sewn={n_sewn}");

        let n = assign_connected_components_knot(&mut m, 3.0, 10.0, uv_range);
        println!("[k=[3,10]] comps={n} (期望 7)");
        assert_eq!(n, 7, "k=[3,10] 应恰好 7 个连通块（|10-3|）");
    }
}
