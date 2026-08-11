use crate::export::PatchSubMesh;
use crate::mesh::half_edge::{FaceId, HalfEdgeMesh};
use crate::mesh::PatchTriMap;
use std::io::Write;

/// 按 patch_index 收集面片三角形（fan 三角化，无 patch 时归入 (0,0)）。
fn collect_patch_groups(mesh: &HalfEdgeMesh) -> PatchTriMap {
    let mut patch_groups: PatchTriMap = std::collections::BTreeMap::new();

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
            patch_groups.entry(pi).or_default().push((v0, v1, v2));
        }
    }

    patch_groups
}

pub fn export_obj(mesh: &HalfEdgeMesh, path: &str) -> std::io::Result<()> {
    if let Some(parent) = std::path::Path::new(path).parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = std::fs::File::create(path)?;

    writeln!(file, "# Torus Grid Cutter - OBJ Export")?;
    writeln!(file, "# Vertices: {}", mesh.vertices.len())?;
    writeln!(file, "# Faces: {}", mesh.num_valid_faces())?;
    writeln!(file)?;

    for v in &mesh.vertices {
        writeln!(file, "v {} {} {}", v.position.x, v.position.y, v.position.z)?;
    }
    for v in &mesh.vertices {
        writeln!(file, "vt {} {}", v.uv.x, v.uv.y)?;
    }

    let normals = mesh.compute_vertex_normals();
    for n in &normals {
        writeln!(file, "vn {} {} {}", n.x, n.y, n.z)?;
    }

    let patch_groups = collect_patch_groups(mesh);

    for ((pi, pj), tris) in &patch_groups {
        writeln!(file, "g patch_{}_{}", pi, pj)?;
        for &(v0, v1, v2) in tris {
            writeln!(
                file,
                "f {}/{}//{} {}/{}//{} {}/{}//{}",
                v0 + 1,
                v0 + 1,
                v0 + 1,
                v1 + 1,
                v1 + 1,
                v1 + 1,
                v2 + 1,
                v2 + 1,
                v2 + 1
            )?;
        }
    }

    Ok(())
}

/// 写出单个补片子网格为 OBJ（顶点 + 面，1-based 索引）。
pub fn write_obj_submesh(
    sub: &PatchSubMesh,
    path: &str,
    pi: usize,
    pj: usize,
) -> std::io::Result<()> {
    if let Some(parent) = std::path::Path::new(path).parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = std::fs::File::create(path)?;
    writeln!(file, "# Patch ({}, {}) - OBJ Export", pi, pj)?;
    writeln!(
        file,
        "# Vertices: {}, Faces: {}",
        sub.positions.len(),
        sub.tris.len()
    )?;
    writeln!(file)?;
    for p in &sub.positions {
        writeln!(file, "v {:.6e} {:.6e} {:.6e}", p[0], p[1], p[2])?;
    }
    for uv in &sub.uvs {
        writeln!(file, "vt {:.6e} {:.6e}", uv[0], uv[1])?;
    }
    for &(a, b, c) in &sub.tris {
        writeln!(
            file,
            "f {}/{} {}/{} {}/{}",
            a + 1,
            a + 1,
            b + 1,
            b + 1,
            c + 1,
            c + 1
        )?;
    }
    Ok(())
}
