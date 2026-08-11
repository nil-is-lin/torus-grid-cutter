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

// 保留给 delaunay 集成测试使用（非测试构建下无其它调用方，故允许 dead_code）。
#[allow(dead_code)]
pub fn load_obj_as_half_edge(path: &str) -> Result<HalfEdgeMesh, String> {
    let (positions, uvs, triangles) = load_obj(path)?;
    Ok(HalfEdgeMesh::from_triangles(&positions, &uvs, &triangles))
}

/// 加载多个 OBJ 文件并合并为一个半边网格。
///
/// 每个文件成为一个独立补片：`patch_index = (file_idx, 0)`。
/// - 单文件内部的 (pos, uv) 去重已在 [`load_obj`] 中完成；
/// - 跨文件**不做顶点焊接**（各 OBJ 本就是彼此独立的补片）；
/// - [`HalfEdgeMesh::from_triangles`] 的 twin 配对按顶点索引 key，
///   不同文件的顶点区间互不相交，天然不会跨文件配对。
///
/// 任一个文件加载失败时跳过并告警，其余文件继续；若全部失败则返回错误。
pub fn load_objs_as_half_edge(
    paths: &[String],
    report: &mut dyn FnMut(usize, usize),
) -> Result<HalfEdgeMesh, String> {
    if paths.is_empty() {
        return Err("No OBJ files selected".into());
    }
    let mut positions: Vec<Vec3> = Vec::new();
    let mut uvs: Vec<Vec2> = Vec::new();
    let mut triangles: Vec<(usize, usize, usize)> = Vec::new();
    let mut tri_counts: Vec<usize> = Vec::with_capacity(paths.len());
    let mut loaded = 0usize;
    for path in paths {
        match load_obj(path) {
            Ok((p, u, t)) => {
                let offset = positions.len();
                let count = t.len();
                positions.extend(p);
                uvs.extend(u);
                for (a, b, c) in t {
                    triangles.push((a + offset, b + offset, c + offset));
                }
                tri_counts.push(count);
                loaded += 1;
                report(loaded, paths.len());
            }
            Err(e) => {
                log::warn!("跳过无法加载的 OBJ '{}': {}", path, e);
                report(loaded, paths.len());
            }
        }
    }
    if positions.is_empty() {
        return Err("所有选中的 OBJ 均无法加载".into());
    }
    let mut mesh = HalfEdgeMesh::from_triangles(&positions, &uvs, &triangles);
    // 按三角形连续区间给每个文件分配 patch_index = (file_idx, 0)。
    // 注意：from_triangles 按 face_triplets 的顺序建面，face i 对应 triangle i。
    let mut face_idx = 0usize;
    for (file_idx, &count) in tri_counts.iter().enumerate() {
        for _ in 0..count {
            if let Some(f) = mesh.faces.get_mut(face_idx) {
                if f.valid {
                    f.patch_index = Some((file_idx, 0));
                }
            }
            face_idx += 1;
        }
    }
    Ok(mesh)
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

    #[test]
    fn test_load_objs_as_half_edge_multi_patch() {
        let path = "models/torus.obj".to_string();
        let single = match load_obj_as_half_edge(&path) {
            Ok(m) => m,
            Err(e) => {
                eprintln!("Skipping test: could not load {}: {}", path, e);
                return;
            }
        };
        let single_faces = single.faces.len();
        assert!(single_faces > 0, "单文件应至少有 1 个面");

        let merged = match load_objs_as_half_edge(&[path.clone(), path.clone()], &mut |_, _| {}) {
            Ok(m) => m,
            Err(e) => {
                eprintln!("Skipping test: could not load {}: {}", path, e);
                return;
            }
        };
        assert_eq!(merged.faces.len(), single_faces * 2, "面数应为单文件的两倍");

        // patch_index 应分两段：(0,0) 与 (1,0)
        let mut seen0 = 0usize;
        let mut seen1 = 0usize;
        for f in &merged.faces {
            match f.patch_index {
                Some((0, 0)) => seen0 += 1,
                Some((1, 0)) => seen1 += 1,
                other => panic!("意外的 patch_index {:?}", other),
            }
        }
        assert_eq!(seen0, single_faces);
        assert_eq!(seen1, single_faces);
    }
}
