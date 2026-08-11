use crate::export::PatchSubMesh;
use crate::mesh::half_edge::{FaceId, HalfEdgeMesh};
use std::io::Write;

/// 导出 ASCII PLY（顶点 + 三角面，保留原始顶点索引）。
pub fn export_ply(mesh: &HalfEdgeMesh, path: &str) -> std::io::Result<()> {
    let mut tris = Vec::new();
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
            tris.push((v0, v1, v2));
        }
    }
    let sub = PatchSubMesh {
        positions: mesh
            .vertices
            .iter()
            .map(|v| v.position.to_array())
            .collect(),
        uvs: mesh.vertices.iter().map(|v| v.uv.to_array()).collect(),
        tris,
    };
    write_ply_submesh(&sub, path)
}

/// 写出单个子网格为 ASCII PLY。
pub fn write_ply_submesh(sub: &PatchSubMesh, path: &str) -> std::io::Result<()> {
    if let Some(parent) = std::path::Path::new(path).parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = std::fs::File::create(path)?;
    writeln!(file, "ply")?;
    writeln!(file, "format ascii 1.0")?;
    writeln!(file, "comment Torus Grid Cutter")?;
    writeln!(file, "element vertex {}", sub.positions.len())?;
    writeln!(file, "property float x")?;
    writeln!(file, "property float y")?;
    writeln!(file, "property float z")?;
    writeln!(file, "element face {}", sub.tris.len())?;
    writeln!(file, "property list uchar int vertex_indices")?;
    writeln!(file, "end_header")?;

    for p in &sub.positions {
        writeln!(file, "{:.6e} {:.6e} {:.6e}", p[0], p[1], p[2])?;
    }
    for &(i0, i1, i2) in &sub.tris {
        writeln!(file, "3 {} {} {}", i0, i1, i2)?;
    }
    Ok(())
}
