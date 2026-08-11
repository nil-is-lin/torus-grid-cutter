pub mod obj;
pub mod ply;
pub mod stl;

use crate::mesh::half_edge::{FaceId, HalfEdgeMesh};

/// 导出格式（与同类工具 trimesh/pmp 对齐的最小子集：OBJ/STL/PLY）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Obj,
    Stl,
    Ply,
}

impl ExportFormat {
    pub fn all() -> &'static [ExportFormat] {
        &[ExportFormat::Obj, ExportFormat::Stl, ExportFormat::Ply]
    }

    pub fn label(self) -> &'static str {
        match self {
            ExportFormat::Obj => "OBJ (.obj)",
            ExportFormat::Stl => "STL (.stl)",
            ExportFormat::Ply => "PLY (.ply)",
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            ExportFormat::Obj => "obj",
            ExportFormat::Stl => "stl",
            ExportFormat::Ply => "ply",
        }
    }
}

impl std::fmt::Display for ExportFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

/// 单个补片的子网格（顶点已重映射为本地索引）。
pub struct PatchSubMesh {
    pub positions: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    pub tris: Vec<(usize, usize, usize)>,
}

/// 按 patch_index 收集子网格（fan 三角化；无 patch 时归入 (0,0)）。
pub fn collect_patch_submeshes(mesh: &HalfEdgeMesh) -> Vec<((usize, usize), PatchSubMesh)> {
    // 1. 按 patch 收集三角形（网格顶点索引）
    let mut patch_tris: crate::mesh::PatchTriMap = std::collections::BTreeMap::new();
    for (fi, face) in mesh.faces.iter().enumerate() {
        if !face.valid {
            continue;
        }
        let hes = mesh.face_half_edges(FaceId(fi));
        if hes.len() < 3 {
            continue;
        }
        let v0 = mesh.half_edges[hes[0].0].origin.0;
        for w in hes.windows(2).skip(1) {
            let v1 = mesh.half_edges[w[0].0].origin.0;
            let v2 = mesh.half_edges[w[1].0].origin.0;
            let pi = face.patch_index.unwrap_or((0, 0));
            patch_tris.entry(pi).or_default().push((v0, v1, v2));
        }
    }

    // 2. 每个 patch 重映射为本地顶点
    let mut result = Vec::new();
    for ((pi, pj), tris) in patch_tris {
        let mut vert_set = std::collections::BTreeSet::new();
        for &(v0, v1, v2) in &tris {
            vert_set.insert(v0);
            vert_set.insert(v1);
            vert_set.insert(v2);
        }
        let vert_list: Vec<usize> = vert_set.into_iter().collect();
        let mut old_to_new = vec![0usize; mesh.vertices.len()];
        for (new_idx, &old_idx) in vert_list.iter().enumerate() {
            old_to_new[old_idx] = new_idx;
        }
        let positions: Vec<[f32; 3]> = vert_list
            .iter()
            .map(|&vi| mesh.vertices[vi].position.to_array())
            .collect();
        let uvs: Vec<[f32; 2]> = vert_list
            .iter()
            .map(|&vi| mesh.vertices[vi].uv.to_array())
            .collect();
        let local_tris: Vec<(usize, usize, usize)> = tris
            .iter()
            .map(|&(a, b, c)| (old_to_new[a], old_to_new[b], old_to_new[c]))
            .collect();
        result.push((
            (pi, pj),
            PatchSubMesh {
                positions,
                uvs,
                tris: local_tris,
            },
        ));
    }
    result
}

/// 按格式导出整个网格。
pub fn export_mesh(mesh: &HalfEdgeMesh, path: &str, format: ExportFormat) -> std::io::Result<()> {
    match format {
        ExportFormat::Obj => obj::export_obj(mesh, path),
        ExportFormat::Stl => stl::export_stl(mesh, path),
        ExportFormat::Ply => ply::export_ply(mesh, path),
    }
}

/// 按补片批量导出（每补片一个文件：`<dir>/patch_<i>_<j>.<ext>`）。
pub fn export_by_patch(
    mesh: &HalfEdgeMesh,
    dir: &str,
    format: ExportFormat,
) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    for ((pi, pj), sub) in collect_patch_submeshes(mesh) {
        let filename = format!("{}/patch_{}_{}.{}", dir, pi, pj, format.extension());
        match format {
            ExportFormat::Obj => obj::write_obj_submesh(&sub, &filename, pi, pj)?,
            ExportFormat::Stl => stl::write_stl_submesh(&sub, &filename)?,
            ExportFormat::Ply => ply::write_ply_submesh(&sub, &filename)?,
        }
    }
    Ok(())
}
