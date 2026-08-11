use crate::mesh::half_edge::HalfEdgeMesh;
use glam::{Vec2, Vec3};

/// OBJ 解析结果：顶点、UV、三角形索引。
pub type LoadedMesh = (Vec<Vec3>, Vec<Vec2>, Vec<crate::mesh::TriIndex>);

pub fn load_obj(path: &str) -> Result<LoadedMesh, String> {
    let content =
        std::fs::read_to_string(path).map_err(|e| format!("Failed to read {}: {}", path, e))?;

    let mut positions: Vec<Vec3> = Vec::new();
    let mut uvs: Vec<Vec2> = Vec::new();
    let mut faces: Vec<ObjFace> = Vec::new();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.is_empty() {
            continue;
        }

        match parts[0] {
            "v" => {
                if parts.len() < 4 {
                    continue;
                }
                let x: f32 = parts[1]
                    .parse()
                    .map_err(|_| format!("Bad v x: {}", parts[1]))?;
                let y: f32 = parts[2]
                    .parse()
                    .map_err(|_| format!("Bad v y: {}", parts[2]))?;
                let z: f32 = parts[3]
                    .parse()
                    .map_err(|_| format!("Bad v z: {}", parts[3]))?;
                positions.push(Vec3::new(x, y, z));
            }
            "vt" => {
                if parts.len() < 3 {
                    continue;
                }
                let u: f32 = parts[1]
                    .parse()
                    .map_err(|_| format!("Bad vt u: {}", parts[1]))?;
                let v: f32 = parts[2]
                    .parse()
                    .map_err(|_| format!("Bad vt v: {}", parts[2]))?;
                uvs.push(Vec2::new(u, v));
            }
            "f" => {
                let verts: Vec<FaceVert> = parts[1..]
                    .iter()
                    .filter_map(|s| parse_face_vert(s))
                    .collect();
                if verts.len() >= 3 {
                    faces.push(ObjFace { verts });
                }
            }
            _ => {}
        }
    }

    if uvs.is_empty() {
        uvs = vec![Vec2::ZERO; positions.len()];
    }

    let mut triangles: Vec<(usize, usize, usize)> = Vec::new();
    let mut final_positions: Vec<Vec3> = Vec::new();
    let mut final_uvs: Vec<Vec2> = Vec::new();
    let mut vertex_map: std::collections::HashMap<(usize, Option<usize>), usize> =
        std::collections::HashMap::new();

    for face in &faces {
        let base_idx: Vec<usize> = face
            .verts
            .iter()
            .map(|fv| {
                let key = (fv.pos, fv.uv);
                if let Some(&idx) = vertex_map.get(&key) {
                    idx
                } else {
                    let idx = final_positions.len();
                    let pos = positions.get(fv.pos).copied().unwrap_or(Vec3::ZERO);
                    let uv = fv
                        .uv
                        .and_then(|i| uvs.get(i).copied())
                        .unwrap_or(Vec2::ZERO);
                    final_positions.push(pos);
                    final_uvs.push(uv);
                    vertex_map.insert(key, idx);
                    idx
                }
            })
            .collect();

        for i in 1..base_idx.len() - 1 {
            triangles.push((base_idx[0], base_idx[i], base_idx[i + 1]));
        }
    }

    log::info!(
        "Loaded OBJ {}: {} vertices, {} triangles",
        path,
        final_positions.len(),
        triangles.len()
    );

    Ok((final_positions, final_uvs, triangles))
}

#[derive(Debug)]
struct FaceVert {
    pos: usize,
    uv: Option<usize>,
}

#[derive(Debug)]
struct ObjFace {
    verts: Vec<FaceVert>,
}

fn parse_face_vert(s: &str) -> Option<FaceVert> {
    let parts: Vec<&str> = s.split('/').collect();
    let pos: usize = parts.first()?.parse().ok()?;
    if pos == 0 {
        return None;
    }
    let pos = pos - 1;

    let uv = if parts.len() >= 2 && !parts[1].is_empty() {
        let idx: usize = parts[1].parse().ok()?;
        if idx == 0 {
            None
        } else {
            Some(idx - 1)
        }
    } else {
        None
    };

    Some(FaceVert { pos, uv })
}

pub fn load_obj_as_half_edge(path: &str) -> Result<HalfEdgeMesh, String> {
    let (positions, uvs, triangles) = load_obj(path)?;
    Ok(HalfEdgeMesh::from_triangles(&positions, &uvs, &triangles))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_face_vert() {
        let fv = parse_face_vert("1/2").unwrap();
        assert_eq!(fv.pos, 0);
        assert_eq!(fv.uv, Some(1));

        let fv = parse_face_vert("5").unwrap();
        assert_eq!(fv.pos, 4);
        assert_eq!(fv.uv, None);

        let fv = parse_face_vert("3/4/5").unwrap();
        assert_eq!(fv.pos, 2);
        assert_eq!(fv.uv, Some(3));
    }
}
