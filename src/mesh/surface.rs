use glam::Vec3;
use std::f64::consts::PI;

#[derive(Debug, Clone)]
pub enum SurfaceModel {
    Torus {
        center: Vec3,
        axis: Vec3,
        major_radius: f64,
        minor_radius: f64,
        u_axis: Vec3,
        v_axis: Vec3,
    },
    Unknown,
}

impl SurfaceModel {
    pub fn fit_from_mesh(positions: &[Vec3], major_hint: f64, minor_hint: f64) -> SurfaceModel {
        if positions.len() < 10 {
            return SurfaceModel::Unknown;
        }
        fit_torus(positions, major_hint, minor_hint)
    }

    pub fn torus_from_params(major_r: f64, minor_r: f64) -> SurfaceModel {
        SurfaceModel::Torus {
            center: Vec3::ZERO,
            axis: Vec3::new(0.0, 0.0, 1.0),
            major_radius: major_r,
            minor_radius: minor_r,
            u_axis: Vec3::new(1.0, 0.0, 0.0),
            v_axis: Vec3::new(0.0, 1.0, 0.0),
        }
    }

    /// Returns (major_radius, minor_radius), using hints for `Unknown`.
    pub fn radii(&self, major_hint: f64, minor_hint: f64) -> (f64, f64) {
        match self {
            SurfaceModel::Torus {
                major_radius,
                minor_radius,
                ..
            } => (*major_radius, *minor_radius),
            SurfaceModel::Unknown => (major_hint, minor_hint),
        }
    }

    /// Returns (center, axis, u_axis, v_axis), using defaults for `Unknown`.
    pub fn frame(&self) -> (Vec3, Vec3, Vec3, Vec3) {
        match self {
            SurfaceModel::Torus {
                center,
                axis,
                u_axis,
                v_axis,
                ..
            } => (*center, *axis, *u_axis, *v_axis),
            SurfaceModel::Unknown => (Vec3::ZERO, Vec3::Z, Vec3::X, Vec3::Y),
        }
    }

    /// Returns true if this is a Torus model.
    pub fn is_torus(&self) -> bool {
        matches!(self, SurfaceModel::Torus { .. })
    }

    /// U 等参线（3D 解析曲线；供测试/几何查询，切割线渲染已改用网格边求交）。
    #[allow(dead_code)]
    pub fn generate_u_line(&self, u_param: f64, n_points: usize) -> Vec<Vec3> {
        match self {
            SurfaceModel::Torus {
                center,
                axis,
                major_radius,
                minor_radius,
                u_axis,
                v_axis,
            } => {
                let r = *minor_radius;
                let big_r = *major_radius;
                let circle_center = *center
                    + *u_axis * (big_r * u_param.cos()) as f32
                    + *v_axis * (big_r * u_param.sin()) as f32;
                let radial = *u_axis * u_param.cos() as f32 + *v_axis * u_param.sin() as f32;

                let mut pts = Vec::with_capacity(n_points);
                for i in 0..n_points {
                    let v = 2.0 * PI * i as f64 / n_points as f64;
                    let p = circle_center
                        + radial * (r * v.cos()) as f32
                        + *axis * (r * v.sin()) as f32;
                    pts.push(p);
                }
                pts
            }
            SurfaceModel::Unknown => Vec::new(),
        }
    }

    /// V 等参线（3D 解析曲线；供测试/几何查询）。
    #[allow(dead_code)]
    pub fn generate_v_line(&self, v_param: f64, n_points: usize) -> Vec<Vec3> {
        match self {
            SurfaceModel::Torus {
                center,
                axis,
                major_radius,
                minor_radius,
                u_axis,
                v_axis,
            } => {
                let r = *minor_radius;
                let big_r = *major_radius;
                let tube_r = big_r + r * v_param.cos();
                let height = r * v_param.sin();
                let ring_center = *center + *axis * height as f32;

                let mut pts = Vec::with_capacity(n_points);
                for i in 0..n_points {
                    let u = 2.0 * PI * i as f64 / n_points as f64;
                    let p = ring_center
                        + *u_axis * (tube_r * u.cos()) as f32
                        + *v_axis * (tube_r * u.sin()) as f32;
                    pts.push(p);
                }
                pts
            }
            SurfaceModel::Unknown => Vec::new(),
        }
    }

    pub fn generate_torus_knot_line(&self, k: f64, n_points: usize) -> Vec<Vec3> {
        match self {
            SurfaceModel::Torus {
                center,
                axis,
                major_radius,
                minor_radius,
                u_axis,
                v_axis,
            } => {
                let r = *minor_radius;
                let big_r = *major_radius;
                let mut pts = Vec::with_capacity(n_points);
                for i in 0..n_points {
                    let theta = 2.0 * PI * i as f64 / n_points as f64;
                    let x = (big_r + r * (k * theta).cos()) * theta.cos();
                    let y = (big_r + r * (k * theta).cos()) * theta.sin();
                    let z = r * (k * theta).sin();
                    let p = *center + *u_axis * x as f32 + *v_axis * y as f32 + *axis * z as f32;
                    pts.push(p);
                }
                pts
            }
            SurfaceModel::Unknown => Vec::new(),
        }
    }

    pub fn compute_uv(&self, point: Vec3) -> Option<(f64, f64)> {
        match self {
            SurfaceModel::Torus {
                center,
                axis,
                major_radius,
                minor_radius: _,
                u_axis,
                v_axis,
            } => {
                let dp = point - *center;
                let h = dp.dot(*axis) as f64;
                let dp_perp = dp - *axis * h as f32;

                let u = (dp_perp.dot(*v_axis) as f64).atan2(dp_perp.dot(*u_axis) as f64);

                let d = dp_perp.length() as f64;
                let dr = d - *major_radius;
                let v = h.atan2(dr);

                Some((u, v))
            }
            SurfaceModel::Unknown => None,
        }
    }
}

fn fit_torus(positions: &[Vec3], major_hint: f64, minor_hint: f64) -> SurfaceModel {
    let n = positions.len();
    let center = positions.iter().fold(Vec3::ZERO, |acc, &p| acc + p) / n as f32;

    let mut cov = [[0.0f64; 3]; 3];
    for &p in positions {
        let d = (p - center).as_dvec3();
        for i in 0..3 {
            for j in 0..3 {
                cov[i][j] += d[i] * d[j];
            }
        }
    }
    for row in cov.iter_mut() {
        for v in row.iter_mut() {
            *v /= n as f64;
        }
    }

    let axis_d = smallest_eigenvector(&cov);
    let axis = Vec3::new(axis_d[0] as f32, axis_d[1] as f32, axis_d[2] as f32).normalize();

    let mut u_axis = if axis.x.abs() < 0.9 {
        axis.cross(Vec3::X).normalize()
    } else {
        axis.cross(Vec3::Y).normalize()
    };
    let v_axis = axis.cross(u_axis).normalize();
    u_axis = v_axis.cross(axis).normalize();

    let mut ds = Vec::with_capacity(n);
    let mut hs = Vec::with_capacity(n);
    for &p in positions {
        let dp = p - center;
        let h = dp.dot(axis) as f64;
        let d = (dp - axis * h as f32).length() as f64;
        ds.push(d);
        hs.push(h);
    }

    let big_r = fit_circle_major_r(&ds, &hs, major_hint);

    let mut sum_r2 = 0.0f64;
    for i in 0..n {
        let dr = ds[i] - big_r;
        sum_r2 += dr * dr + hs[i] * hs[i];
    }
    let small_r = (sum_r2 / n as f64).sqrt();

    let small_r = if small_r < 0.01 { minor_hint } else { small_r };
    let big_r = if big_r < small_r { major_hint } else { big_r };

    let residual = compute_torus_residual(positions, &center, &axis, big_r, small_r);
    if residual > 0.1 {
        log::warn!(
            "Torus fit residual too high ({:.4}), treating as unknown surface",
            residual
        );
        return SurfaceModel::Unknown;
    }

    log::info!("Fitted torus: R={:.4}, r={:.4}, center=({:.3},{:.3},{:.3}), axis=({:.3},{:.3},{:.3}), residual={:.6}",
        big_r, small_r, center.x, center.y, center.z, axis.x, axis.y, axis.z, residual);

    SurfaceModel::Torus {
        center,
        axis,
        major_radius: big_r,
        minor_radius: small_r,
        u_axis,
        v_axis,
    }
}

fn fit_circle_major_r(ds: &[f64], hs: &[f64], hint: f64) -> f64 {
    let n = ds.len() as f64;
    let mut sum_d = 0.0;
    let mut sum_d2 = 0.0;

    for &d in ds.iter() {
        sum_d += d;
        sum_d2 += d * d;
    }

    let a11 = 4.0 * sum_d2;
    let a12 = 2.0 * sum_d;
    let a21 = 2.0 * sum_d;
    let a22 = n;

    let mut xty1 = 0.0f64;
    let mut xty2 = 0.0f64;
    for i in 0..ds.len() {
        let y = ds[i] * ds[i] + hs[i] * hs[i];
        xty1 += 2.0 * ds[i] * y;
        xty2 += y;
    }

    let det = a11 * a22 - a12 * a21;
    if det.abs() < 1e-20 {
        return hint;
    }

    let major_r = (a22 * xty1 - a12 * xty2) / det;

    if major_r <= 0.0 || !major_r.is_finite() {
        hint
    } else {
        major_r
    }
}

fn compute_torus_residual(
    positions: &[Vec3],
    center: &Vec3,
    axis: &Vec3,
    major_r: f64,
    minor_r: f64,
) -> f64 {
    let mut total = 0.0f64;
    for &p in positions {
        let dp = p - *center;
        let h = dp.dot(*axis) as f64;
        let d = (dp - *axis * h as f32).length() as f64;
        let dist_to_circle = ((d - major_r) * (d - major_r) + h * h).sqrt();
        let diff = dist_to_circle - minor_r;
        total += diff * diff;
    }
    (total / positions.len() as f64).sqrt() / minor_r.max(0.001)
}

/// 求 3×3 对称矩阵最小特征值对应的单位特征向量。
///
/// 方法：两次幂迭代 + 投影（deflation）：
///   1. 幂迭代求主特征向量 v1（对应最大特征值）；
///   2. 从矩阵中投影掉 v1 后再幂迭代，得垂直于 v1 的次主特征向量 v2；
///   3. 最小特征向量 = v1 × v2（正交补）。
///
/// 注意：单次幂迭代必然收敛到最大特征向量（主方向），不能直接用于
/// 求最小特征向量。
fn smallest_eigenvector(cov: &[[f64; 3]; 3]) -> [f64; 3] {
    fn power_iteration(m: &[[f64; 3]; 3]) -> ([f64; 3], f64) {
        // 起始向量含全部分量，避免恰好落在某个特征方向上
        let mut v = [1.0, 0.5, 0.25];
        for _ in 0..200 {
            let mut new_v = [0.0; 3];
            for i in 0..3 {
                for j in 0..3 {
                    new_v[i] += m[i][j] * v[j];
                }
            }
            let norm = (new_v[0] * new_v[0] + new_v[1] * new_v[1] + new_v[2] * new_v[2]).sqrt();
            if norm < 1e-30 {
                break;
            }
            v = [new_v[0] / norm, new_v[1] / norm, new_v[2] / norm];
        }
        let lambda = v[0] * (m[0][0] * v[0] + m[0][1] * v[1] + m[0][2] * v[2])
            + v[1] * (m[1][0] * v[0] + m[1][1] * v[1] + m[1][2] * v[2])
            + v[2] * (m[2][0] * v[0] + m[2][1] * v[1] + m[2][2] * v[2]);
        (v, lambda)
    }

    let (v1, l1) = power_iteration(cov);
    let mut deflated = [[0.0; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            deflated[i][j] = cov[i][j] - l1 * v1[i] * v1[j];
        }
    }
    let (v2, _) = power_iteration(&deflated);

    // v3 = v1 × v2（单位化）
    let v3 = [
        v1[1] * v2[2] - v1[2] * v2[1],
        v1[2] * v2[0] - v1[0] * v2[2],
        v1[0] * v2[1] - v1[1] * v2[0],
    ];
    let norm = (v3[0] * v3[0] + v3[1] * v3[1] + v3[2] * v3[2]).sqrt();
    if norm < 1e-12 {
        [0.0, 0.0, 1.0] // 退化情形：退化为平面时无法确定，取 z 轴
    } else {
        [v3[0] / norm, v3[1] / norm, v3[2] / norm]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fit_torus_from_generated() {
        let (positions, _, _) = crate::mesh::torus::generate_unfolded_quad_mesh(2.0, 0.6, 40, 24);
        let model = SurfaceModel::fit_from_mesh(&positions, 2.0, 0.6);
        match model {
            SurfaceModel::Torus {
                major_radius,
                minor_radius,
                ..
            } => {
                assert!((major_radius - 2.0).abs() < 0.1, "R={major_radius}");
                assert!((minor_radius - 0.6).abs() < 0.1, "r={minor_radius}");
            }
            SurfaceModel::Unknown => panic!("Should detect torus"),
        }
    }

    #[test]
    fn test_analytical_u_line_is_circle() {
        let model = SurfaceModel::torus_from_params(2.0, 0.6);
        let pts = model.generate_u_line(0.0, 100);
        assert_eq!(pts.len(), 100);
        let center = pts.iter().fold(Vec3::ZERO, |a, &p| a + p) / 100.0;
        for &p in &pts {
            let d = (p - center).length();
            assert!((d - 0.6).abs() < 0.01, "distance={d}");
        }
    }

    #[test]
    fn test_analytical_v_line_is_circle() {
        let model = SurfaceModel::torus_from_params(2.0, 0.6);
        let pts = model.generate_v_line(0.0, 100);
        assert_eq!(pts.len(), 100);
        let center = pts.iter().fold(Vec3::ZERO, |a, &p| a + p) / 100.0;
        for &p in &pts {
            let d = (p - center).length();
            assert!((d - 2.6).abs() < 0.01, "distance={d}");
        }
    }
}
