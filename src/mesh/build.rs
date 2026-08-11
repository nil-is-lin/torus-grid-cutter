//! OBJ 导入的后台构建逻辑。
//!
//! 把"解析 OBJ → 半边网格 → torus 拟合 → UV 重映射"这套重活从 UI 主线程挪到
//! worker 线程，并通过 `report` 回调回报阶段进度（供状态栏显示），避免导入大
//! 批量 patch 时主线程被阻塞导致窗口"未响应"。

use crate::mesh::half_edge::HalfEdgeMesh;
use crate::mesh::obj_loader::load_objs_as_half_edge;
use crate::mesh::surface::SurfaceModel;
use glam::{Vec2, Vec3};
use std::collections::HashSet;

/// 后台构建产物（纯 CPU 数据，可跨线程发送）。
pub struct BuildOutput {
    pub mesh: HalfEdgeMesh,
    pub uv_range: (f64, f64, f64, f64),
    pub surface_model: SurfaceModel,
}

/// 在 worker 线程中构建 OBJ 导入网格。
///
/// - `report(stage, done, total)` 在各阶段回报进度；`done/total` 用于状态栏进度条。
/// - 顶点去重改用量化坐标 `HashSet`，复杂度 O(n)（原 `app.rs` 的 O(n²) 遍历是卡顿主因之一）。
pub fn build_objfile_mesh(
    paths: &[String],
    major: f64,
    minor: f64,
    report: &mut dyn FnMut(&str, usize, usize),
) -> Result<BuildOutput, String> {
    report("Loading OBJ files", 0, paths.len());
    // 进度事件节流：最快每 80ms 发一次，避免小文件瞬间产生大量事件。
    let mut last_report = std::time::Instant::now();
    let mut last_done = 0usize;
    let mut last_total = paths.len();
    let mut throttled_report = |done: usize, total: usize| {
        last_done = done;
        last_total = total;
        if last_report.elapsed().as_millis() >= 80 {
            report("Loading OBJ files", done, total);
            last_report = std::time::Instant::now();
        }
    };
    let mut mesh = load_objs_as_half_edge(paths, &mut throttled_report)?;
    // 加载阶段结束：强制补发最终进度
    report("Loading OBJ files", last_done, last_total);

    report("Computing UV range", paths.len(), paths.len());
    let mut uv_range = compute_uv_range(&mesh);

    report("Welding vertices", paths.len(), paths.len());
    let unique = collect_unique_positions(&mesh);

    report("Fitting torus surface", paths.len(), paths.len());
    let model = SurfaceModel::fit_from_mesh(&unique, major, minor);

    if model.is_torus() {
        report("Remapping UVs", paths.len(), paths.len());
        remap_uvs(&mut mesh, &model);
        // 解析环面重映射后 UV 落在 [0, 2π)，与程序生成网格保持一致
        uv_range = (0.0, 2.0 * std::f64::consts::PI, 0.0, 2.0 * std::f64::consts::PI);
    }

    Ok(BuildOutput {
        mesh,
        uv_range,
        surface_model: model,
    })
}

/// 由顶点 UV 推算 uv_range（与原 `build_torus_mesh` 的 ObjFile 分支一致）。
fn compute_uv_range(mesh: &HalfEdgeMesh) -> (f64, f64, f64, f64) {
    if mesh.vertices.is_empty() {
        return (0.0, 1.0, 0.0, 1.0);
    }
    let mut min_u = f64::MAX;
    let mut max_u = f64::MIN;
    let mut min_v = f64::MAX;
    let mut max_v = f64::MIN;
    for v in &mesh.vertices {
        let u = v.uv.x as f64;
        let vv = v.uv.y as f64;
        if u < min_u {
            min_u = u;
        }
        if u > max_u {
            max_u = u;
        }
        if vv < min_v {
            min_v = vv;
        }
        if vv > max_v {
            max_v = vv;
        }
    }
    if max_u - min_u > 1e-6 && max_v - min_v > 1e-6 {
        (min_u, max_u, min_v, max_v)
    } else {
        (0.0, 1.0, 0.0, 1.0)
    }
}

/// O(n) 顶点去重：按量化坐标入 `HashSet`，替代原 O(n²) 的 `unique_positions` 遍历。
fn collect_unique_positions(mesh: &HalfEdgeMesh) -> Vec<Vec3> {
    let scale = 1e4_f32; // 1e-4 分辨率，对应原去重阈值 eps_sq = 1e-8
    let mut seen: HashSet<(i64, i64, i64)> = HashSet::new();
    let mut out = Vec::with_capacity(mesh.vertices.len());
    for v in &mesh.vertices {
        let key = (
            (v.position.x * scale).round() as i64,
            (v.position.y * scale).round() as i64,
            (v.position.z * scale).round() as i64,
        );
        if seen.insert(key) {
            out.push(v.position);
        }
    }
    out
}

/// 用解析环面模型把顶点 3D 位置重映射为 (u, v) 参数坐标（与原 `remap_uvs_from_analytical_model` 等价）。
fn remap_uvs(mesh: &mut HalfEdgeMesh, model: &SurfaceModel) {
    let two_pi = 2.0 * std::f64::consts::PI;
    for v in &mut mesh.vertices {
        if let Some((u, v_param)) = model.compute_uv(v.position) {
            let mut u = u;
            let mut v_param = v_param;
            while u < 0.0 {
                u += two_pi;
            }
            while u >= two_pi {
                u -= two_pi;
            }
            while v_param < 0.0 {
                v_param += two_pi;
            }
            while v_param >= two_pi {
                v_param -= two_pi;
            }
            v.uv = Vec2::new(u as f32, v_param as f32);
        }
    }
}
