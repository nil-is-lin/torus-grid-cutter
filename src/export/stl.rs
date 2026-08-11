use crate::export::PatchSubMesh;
use crate::mesh::half_edge::{FaceId, HalfEdgeMesh};
use std::io::Write;

/// 三角形面法线（右手定则，归一化；退化时返回零向量）。
fn face_normal(a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> [f32; 3] {
    let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let ac = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    let n = [
        ab[1] * ac[2] - ab[2] * ac[1],
        ab[2] * ac[0] - ab[0] * ac[2],
        ab[0] * ac[1] - ab[1] * ac[0],
    ];
    let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    if len < 1e-12 {
        [0.0, 0.0, 0.0]
    } else {
        [n[0] / len, n[1] / len, n[2] / len]
    }
}

/// 导出 ASCII STL（三角形网格的通用交换格式）。
pub fn export_stl(mesh: &HalfEdgeMesh, path: &str) -> std::io::Result<()> {
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
    write_stl_submesh(&sub, path)
}

/// 写出单个子网格为 ASCII STL。
pub fn write_stl_submesh(sub: &PatchSubMesh, path: &str) -> std::io::Result<()> {
    if let Some(parent) = std::path::Path::new(path).parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = std::fs::File::create(path)?;

    writeln!(file, "solid torus_grid_cutter")?;
    for &(i0, i1, i2) in &sub.tris {
        let (a, b, c) = (sub.positions[i0], sub.positions[i1], sub.positions[i2]);
        let n = face_normal(a, b, c);
        writeln!(
            file,
            "  facet normal {:.6e} {:.6e} {:.6e}",
            n[0], n[1], n[2]
        )?;
        writeln!(file, "    outer loop")?;
        writeln!(file, "      vertex {:.6e} {:.6e} {:.6e}", a[0], a[1], a[2])?;
        writeln!(file, "      vertex {:.6e} {:.6e} {:.6e}", b[0], b[1], b[2])?;
        writeln!(file, "      vertex {:.6e} {:.6e} {:.6e}", c[0], c[1], c[2])?;
        writeln!(file, "    endloop")?;
        writeln!(file, "  endfacet")?;
    }
    writeln!(file, "endsolid torus_grid_cutter")?;
    Ok(())
}
