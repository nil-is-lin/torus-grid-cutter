//! 导出格式集成测试：完整 pipeline（生成 → 切割 → 导出 OBJ/STL/PLY），
//! 验证文件可生成、内容结构合法、按补片导出数量正确。

use torus_grid_cutter::export::{export_by_patch, export_mesh, ExportFormat};
use torus_grid_cutter::mesh::cut::cut_mesh_by_grid;
use torus_grid_cutter::mesh::half_edge::HalfEdgeMesh;
use torus_grid_cutter::mesh::torus::generate_unfolded_quad_mesh;

fn build_cut_mesh() -> HalfEdgeMesh {
    let (positions, uvs, quads) = generate_unfolded_quad_mesh(2.0, 0.6, 24, 20);
    let mut mesh = HalfEdgeMesh::from_quads(&positions, &uvs, &quads);
    cut_mesh_by_grid(
        &mut mesh,
        &[1.0, 3.0],
        &[1.0, 4.0],
        (
            0.0,
            2.0 * std::f64::consts::PI,
            0.0,
            2.0 * std::f64::consts::PI,
        ),
        false,
    );
    torus_grid_cutter::mesh::cut::assign_patch_indices(
        &mut mesh,
        &[1.0, 3.0],
        &[1.0, 4.0],
        (
            0.0,
            2.0 * std::f64::consts::PI,
            0.0,
            2.0 * std::f64::consts::PI,
        ),
    );
    mesh
}

/// 临时目录（测试结束后自动清理）。
struct TempDir(std::path::PathBuf);
impl TempDir {
    fn new(tag: &str) -> Self {
        let dir =
            std::env::temp_dir().join(format!("tgc_export_test_{}_{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        TempDir(dir)
    }
}
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn test_export_obj() {
    let mesh = build_cut_mesh();
    let tmp = TempDir::new("obj");
    let path = tmp.0.join("out.obj");
    export_mesh(&mesh, path.to_str().unwrap(), ExportFormat::Obj).unwrap();

    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.starts_with("# Torus Grid Cutter"), "OBJ 头注释");
    let v_count = text.lines().filter(|l| l.starts_with("v ")).count();
    let f_count = text.lines().filter(|l| l.starts_with("f ")).count();
    assert_eq!(v_count, mesh.vertices.len());
    assert!(f_count > 0, "OBJ 应有面");
    assert!(text.contains("g patch_"), "OBJ 应按补片分组");
}

#[test]
fn test_export_stl() {
    let mesh = build_cut_mesh();
    let tmp = TempDir::new("stl");
    let path = tmp.0.join("out.stl");
    export_mesh(&mesh, path.to_str().unwrap(), ExportFormat::Stl).unwrap();

    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.starts_with("solid torus_grid_cutter"));
    assert!(text.trim_end().ends_with("endsolid torus_grid_cutter"));
    let facets = text.lines().filter(|l| l.trim() == "endfacet").count();
    let vertices = text
        .lines()
        .filter(|l| l.trim().starts_with("vertex "))
        .count();
    assert_eq!(vertices, facets * 3, "STL 每个 facet 恰好 3 个 vertex");
    assert!(facets > 0);
}

#[test]
fn test_export_ply() {
    let mesh = build_cut_mesh();
    let tmp = TempDir::new("ply");
    let path = tmp.0.join("out.ply");
    export_mesh(&mesh, path.to_str().unwrap(), ExportFormat::Ply).unwrap();

    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.starts_with("ply\n"));
    assert!(text.contains("element vertex"));
    assert!(text.contains("element face"));
    assert!(text.contains("end_header"));
    let body = text.split("end_header").nth(1).unwrap();
    let lines: Vec<&str> = body.lines().filter(|l| !l.trim().is_empty()).collect();
    let v_count = lines
        .iter()
        .filter(|l| l.split_whitespace().count() == 3)
        .count();
    let f_count = lines.iter().filter(|l| l.starts_with("3")).count();
    assert_eq!(v_count, mesh.vertices.len());
    let faces = mesh.num_valid_faces();
    assert!(
        f_count >= faces && f_count <= faces * 2,
        "PLY 三角面数 {f_count} 应在 [{faces}, {}]（quad fan 三角化）",
        faces * 2
    );
}

#[test]
fn test_export_by_patch_all_formats() {
    let mesh = build_cut_mesh();
    for (fi, fmt) in ExportFormat::all().iter().enumerate() {
        let tmp = TempDir::new(&format!("bypatch_{}", fi));
        export_by_patch(&mesh, tmp.0.to_str().unwrap(), *fmt).unwrap();
        // 2×2 切割线（周期语义）→ 2×2 = 4 个补片文件
        let files: Vec<_> = std::fs::read_dir(&tmp.0)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        assert_eq!(files.len(), 4, "{} 按补片应生成 4 个文件: {:?}", fmt, files);
        for f in &files {
            assert!(
                f.ends_with(&format!(".{}", fmt.extension())),
                "文件后缀应为 .{}: {}",
                fmt.extension(),
                f
            );
            let content = std::fs::read_to_string(tmp.0.join(f)).unwrap();
            assert!(!content.is_empty());
        }
    }
}
