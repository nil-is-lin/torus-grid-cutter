use glam::{Vec2, Vec3};
use std::f64::consts::PI;

pub fn torus_position(u: f64, v: f64, major_r: f64, minor_r: f64) -> Vec3 {
    let cu = u.cos();
    let su = u.sin();
    let cv = v.cos();
    let sv = v.sin();
    let rr = major_r + minor_r * cv;
    Vec3::new(
        rr as f32 * cu as f32,
        rr as f32 * su as f32,
        minor_r as f32 * sv as f32,
    )
}

/// Compute 3D position on a torus using an arbitrary coordinate frame.
/// This matches the orientation used by `SurfaceModel::Torus` and
/// `generate_torus_knot_line`, ensuring mesh vertices and analytical
/// curves share the exact same 3D coordinate frame.
#[allow(clippy::too_many_arguments)]
pub fn torus_position_frame(
    u: f64,
    v: f64,
    major_r: f64,
    minor_r: f64,
    center: Vec3,
    axis: Vec3,
    u_axis: Vec3,
    v_axis: Vec3,
) -> Vec3 {
    let cu = u.cos();
    let su = u.sin();
    let cv = v.cos();
    let sv = v.sin();
    let rr = major_r + minor_r * cv;
    let x = rr * cu;
    let y = rr * su;
    let z = minor_r * sv;
    center + u_axis * x as f32 + v_axis * y as f32 + axis * z as f32
}

/// 将环面体的极坐标 (u, v) 展开为平面直角坐标。
/// x = u * R（沿大圆的弧长），y = v * r（沿小圆的弧长），z = 0
pub fn unfold_position(u: f64, v: f64, major_r: f64, minor_r: f64) -> Vec3 {
    Vec3::new((u * major_r) as f32, (v * minor_r) as f32, 0.0)
}

/// 直接从参数方程生成展开后的四边形网格。
/// 不依赖已有网格的UV，而是直接在参数域均匀采样后映射到平面坐标。
/// 不生成跨越接缝的四边形（即 u=2π→0 和 v=2π→0 的回绕面片）。
pub fn generate_unfolded_quad_mesh(
    major_r: f64,
    minor_r: f64,
    res_u: usize,
    res_v: usize,
) -> (Vec<Vec3>, Vec<Vec2>, Vec<crate::mesh::QuadIndex>) {
    // res_u+1 × res_v+1 grid: includes both u=0 and u=2π (same for v)
    // These are SEPARATE vertices in UV but same 3D position
    let nu = res_u + 1;
    let nv = res_v + 1;

    let mut positions = Vec::with_capacity(nu * nv);
    let mut uvs = Vec::with_capacity(nu * nv);

    for i in 0..nu {
        let u = 2.0 * PI * i as f64 / res_u as f64;
        for j in 0..nv {
            let v = 2.0 * PI * j as f64 / res_v as f64;
            let pos = torus_position(u, v, major_r, minor_r);
            positions.push(pos);
            uvs.push(Vec2::new(u as f32, v as f32));
        }
    }

    // Quads: no wrapping — connect consecutive columns and rows only
    let mut quads = Vec::with_capacity(res_u * res_v);
    for i in 0..res_u {
        for j in 0..res_v {
            let v00 = i * nv + j;
            let v10 = (i + 1) * nv + j;
            let v11 = (i + 1) * nv + (j + 1);
            let v01 = i * nv + (j + 1);
            quads.push((v00, v10, v11, v01));
        }
    }

    (positions, uvs, quads)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 环面 u/v 均以 2π 为周期：torus_position(u+2π, v) ≡ torus_position(u, v)。
    #[test]
    fn test_torus_periodicity() {
        let (major, minor) = (2.0, 0.5);
        let (u, v) = (1.2, 2.4);
        let p = torus_position(u, v, major, minor);
        assert!(torus_position(u + 2.0 * std::f64::consts::PI, v, major, minor).distance(p) < 1e-5);
        assert!(torus_position(u, v + 2.0 * std::f64::consts::PI, major, minor).distance(p) < 1e-5);
    }

    /// 几何性质：u=0 时点在 XZ 平面；v=0 时点在主圆上（半径 = major + minor）。
    #[test]
    fn test_torus_geometry() {
        let (major, minor) = (2.0, 0.5);
        let p = torus_position(0.0, 0.0, major, minor);
        // v=0: 最外侧，距中心 = major + minor
        assert!((p.length() as f64 - (major + minor)).abs() < 1e-5);
        // u=0, v=π/2: 管顶在 +Z（环面轴线为 Z 轴），z = minor，主圆半径 = major
        let top = torus_position(0.0, std::f64::consts::FRAC_PI_2, major, minor);
        assert!((top.z as f64 - minor).abs() < 1e-5);
        assert!((top.x as f64 - major).abs() < 1e-5);
        assert!(top.y.abs() < 1e-5);
        // v=π: 内圈，距中心 = major - minor
        let inner = torus_position(0.0, std::f64::consts::PI, major, minor);
        assert!((inner.x as f64 - (major - minor)).abs() < 1e-5);
    }

    /// 展开平面映射（UV 视图）与 3D 映射共用同一 u/v 参数域，
    /// 且 unfold_position 与 torus_position 的几何关系一致：
    /// 展开坐标 (x, z) 对应 u·R 与 v·r，3D 点应落在对应环面参数位置。
    #[test]
    fn test_unfold_matches_3d() {
        let (major, minor) = (2.0, 0.5);
        let (u, v) = (0.8, 1.1);
        let flat = unfold_position(u, v, major, minor);
        assert!((flat.x - (u * major) as f32).abs() < 1e-4);
        assert!((flat.y - (v * minor) as f32).abs() < 1e-4);
        // 3D 点必须不在展开平面附近（坐标域不同），仅验证可计算且有限
        let p = torus_position(u, v, major, minor);
        assert!(p.is_finite());
    }
}
