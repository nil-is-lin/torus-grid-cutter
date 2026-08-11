//! 网格质量统计（对标 trimesh 的一等待查属性：watertight / euler / area / volume）。
//!
//! 全部基于半边结构在 CPU 端计算，不依赖渲染。

use crate::mesh::half_edge::{FaceId, HalfEdgeMesh};

/// 网格统计信息。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MeshStats {
    pub vertices: usize,
    pub faces: usize,
    pub edges: usize,
    /// 边界半边数（0 = 闭合流形，watertight）。
    pub boundary_half_edges: usize,
    /// 边界环数（闭合网格为 0）。
    pub boundary_loops: usize,
    pub is_closed: bool,
    /// 欧拉示性数 χ = V − E + F（闭合环面 = 0）。
    pub euler_characteristic: i64,
    /// 表面积（fan 三角化后按三角形求和）。
    pub surface_area: f64,
    /// 体积（散度定理，闭合网格有效；开放网格给出带符号值）。
    pub volume: f64,
    /// 每个面平均顶点数（四边形 = 4，三角形 = 3）。
    pub avg_face_degree: f64,
}

impl HalfEdgeMesh {
    /// 计算网格统计。
    pub fn compute_stats(&self) -> MeshStats {
        let vertices = self.vertices.len();
        let faces = self.num_valid_faces();

        let boundary_half_edges = self
            .half_edges
            .iter()
            .filter(|he| he.twin.0 == usize::MAX)
            .count();
        // 成对半边 = 内部边；边界半边各代表一条边界边
        let edges = (self.half_edges.len() - boundary_half_edges) / 2 + boundary_half_edges;
        let is_closed = boundary_half_edges == 0;
        let euler = vertices as i64 - edges as i64 + faces as i64;

        // 边界环数：沿边界走（绕顶点找下一条边界出边），统计闭合环。
        //
        // 注意：导入的 OBJ（尤其是 CAD 导出的 patch）可能是非流形拓扑——
        // 某个边界顶点挂了 ≥2 条出边界半边、或 twin 链因 T 型连接而错配。
        // 这类退化拓扑会让"沿边界走"陷入永不回到起点 `i` 的环，旧实现会
        // 死循环导致主线程"未响应"。这里加两道护栏：
        //   1) 外层：若下一步要走的半边已访问过，立即终止（打破环）；
        //   2) 内层：绕顶点转圈的迭代次数封顶为半边数，避免错配 twin 造成的死循环。
        let mut visited = vec![false; self.half_edges.len()];
        let mut boundary_loops = 0usize;
        let hlen = self.half_edges.len();
        for (i, he) in self.half_edges.iter().enumerate() {
            if he.twin.0 != usize::MAX || visited[i] {
                continue;
            }
            boundary_loops += 1;
            let mut cur = i;
            loop {
                visited[cur] = true;
                // 当前半边 cur: A→B；绕 B 转圈找下一条边界出边
                let first = self.half_edges[cur].next;
                let mut cand = first;
                let mut found = None;
                let mut inner_iter = 0usize;
                loop {
                    if self.half_edges[cand.0].twin.0 == usize::MAX {
                        found = Some(cand);
                        break;
                    }
                    let twin = self.half_edges[cand.0].twin;
                    cand = self.half_edges[twin.0].next;
                    if cand == first {
                        break;
                    }
                    inner_iter += 1;
                    if inner_iter > hlen {
                        // 退化/坏拓扑：绕顶点循环未闭合，强制终止以免死循环
                        break;
                    }
                }
                let Some(nxt) = found else { break };
                if nxt.0 == i {
                    break; // 回到起点，环闭合
                }
                if visited[nxt.0] {
                    // 退化拓扑（非流形边界顶点）导致的环；停止以免死循环
                    break;
                }
                cur = nxt.0;
            }
        }

        // 面积 / 体积 / 面度数（fan 三角化）
        let mut surface_area = 0.0f64;
        let mut volume = 0.0f64;
        let mut degree_sum = 0usize;
        let mut face_count = 0usize;
        for (fi, face) in self.faces.iter().enumerate() {
            if !face.valid {
                continue;
            }
            let hes = self.face_half_edges(FaceId(fi));
            if hes.len() < 3 {
                continue;
            }
            degree_sum += hes.len();
            face_count += 1;
            let v0 = self.vertices[self.half_edges[hes[0].0].origin.0].position;
            for w in hes.windows(2).skip(1) {
                let v1 = self.vertices[self.half_edges[w[0].0].origin.0].position;
                let v2 = self.vertices[self.half_edges[w[1].0].origin.0].position;
                // 面积：|cross(b-a, c-a)| / 2
                let ab = v1 - v0;
                let ac = v2 - v0;
                let cross = ab.cross(ac);
                surface_area += (cross.length() as f64) * 0.5;
                // 体积：散度定理 Σ dot(a, cross(b, c)) / 6
                volume += (v0.dot(v1.cross(v2)) as f64) / 6.0;
            }
        }

        MeshStats {
            vertices,
            faces,
            edges,
            boundary_half_edges,
            boundary_loops,
            is_closed,
            euler_characteristic: euler,
            surface_area,
            volume,
            avg_face_degree: if face_count > 0 {
                degree_sum as f64 / face_count as f64
            } else {
                0.0
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::torus;

    /// 构造拓扑闭合的环面 quad 网格（顶点不重复，边 wrap 缝合）。
    fn closed_torus_mesh(res_u: usize, res_v: usize, r: f64, r_minor: f64) -> HalfEdgeMesh {
        let mut positions = Vec::new();
        let mut uvs = Vec::new();
        for i in 0..res_u {
            for j in 0..res_v {
                let u = 2.0 * std::f64::consts::PI * i as f64 / res_u as f64;
                let v = 2.0 * std::f64::consts::PI * j as f64 / res_v as f64;
                positions.push(torus::torus_position(u, v, r, r_minor));
                uvs.push(glam::Vec2::new(u as f32, v as f32));
            }
        }
        let mut quads = Vec::new();
        for i in 0..res_u {
            for j in 0..res_v {
                let a = i * res_v + j;
                let b = i * res_v + (j + 1) % res_v;
                let c = ((i + 1) % res_u) * res_v + (j + 1) % res_v;
                let d = ((i + 1) % res_u) * res_v + j;
                quads.push((a, b, c, d));
            }
        }
        HalfEdgeMesh::from_quads(&positions, &uvs, &quads)
    }

    /// 闭合环面：χ = 0、watertight、体积 ≈ 2π²Rr²、面平均度数 = 4。
    #[test]
    fn test_closed_torus_stats() {
        let mesh = closed_torus_mesh(48, 40, 2.0, 0.6);
        let s = mesh.compute_stats();

        assert!(s.is_closed, "闭合环面网格应 watertight");
        assert_eq!(s.boundary_half_edges, 0);
        assert_eq!(s.euler_characteristic, 0, "闭合环面 χ 应为 0");
        assert_eq!(s.avg_face_degree, 4.0);
        assert!(s.surface_area > 0.0);
        // 环面体积公式 V = 2π² R r²，R=2, r=0.6 → ≈ 14.2。
        // 手工网格绕向可能使符号为负，取绝对值验证量级。
        let expect = 2.0 * std::f64::consts::PI * std::f64::consts::PI * 2.0 * 0.6 * 0.6;
        assert!(
            (s.volume.abs() - expect).abs() / expect < 0.02,
            "环面体积 {:.4} 应接近 {:.4}",
            s.volume,
            expect
        );
    }

    /// 展开网格（接缝开放）：χ = 1（圆盘）、1 条边界环。
    #[test]
    fn test_unfolded_mesh_stats() {
        let (positions, uvs, quads) = torus::generate_unfolded_quad_mesh(2.0, 0.6, 24, 20);
        let mesh = HalfEdgeMesh::from_quads(&positions, &uvs, &quads);
        let s = mesh.compute_stats();
        assert!(!s.is_closed, "展开网格在接缝处开放");
        assert_eq!(s.euler_characteristic, 1, "展开网格拓扑为圆盘");
        assert_eq!(s.boundary_loops, 1, "展开网格只有一条外边界环");
    }

    /// 切割后的闭合环面仍闭合（切割线不破坏流形）。
    #[test]
    fn test_cut_mesh_stays_closed() {
        let mut mesh = closed_torus_mesh(48, 40, 2.0, 0.6);
        let uv_range = (
            0.0,
            2.0 * std::f64::consts::PI,
            0.0,
            2.0 * std::f64::consts::PI,
        );
        crate::mesh::cut::cut_mesh_by_grid(&mut mesh, &[1.0, 3.0], &[1.0, 4.0], uv_range, false);
        let s = mesh.compute_stats();
        assert!(mesh.validate());
        assert!(s.is_closed, "切割不应产生开放边界");
        assert_eq!(s.euler_characteristic, 0);
    }

    /// 回归测试：非流形 / 退化拓扑不应让 `compute_stats` 死循环。
    ///
    /// 旧实现的 `boundary_loops` 遍历缺少 `visited` 护栏，一旦边界顶点挂了
    /// ≥2 条出边界半边（CAD 导出的 patch 常见），"沿边界走"会陷入永不回到
    /// 起点的环，主线程卡死 → 窗口"未响应"。这里用一个会触发该拓扑的输入
    /// 验证 `compute_stats` 能在有限步内返回。
    #[test]
    fn test_compute_stats_terminates_on_nonmanifold_boundary() {
        // 三个三角形拼成"棒棒糖"拓扑：顶点 3 是非流形边界顶点（2 条出边界
        // 半边），且 from_triangles 会为 (1,2) 这类同向边链出坏 twin。
        let positions = vec![
            glam::Vec3::new(0.0, 0.0, 0.0), // 0
            glam::Vec3::new(1.0, 0.0, 0.0), // 1
            glam::Vec3::new(1.0, 1.0, 0.0), // 2
            glam::Vec3::new(0.0, 1.0, 0.0), // 3
        ];
        let uvs = vec![glam::Vec2::ZERO; 4];
        let tris = vec![(0usize, 1, 2), (0usize, 2, 3), (1usize, 2, 3)];
        let mesh = HalfEdgeMesh::from_triangles(&positions, &uvs, &tris);
        // 若 compute_stats 死循环，本条测试会挂死（被 cargo test 超时捕获）。
        let s = mesh.compute_stats();
        // 非闭合网格应统计到若干边界半边。
        assert!(s.boundary_half_edges > 0, "退化网格应存在边界半边");
    }
}
