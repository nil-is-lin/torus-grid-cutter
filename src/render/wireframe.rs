use crate::color_scheme;
use crate::mesh::half_edge::HalfEdgeMesh;
use crate::mesh::surface::SurfaceModel;
use crate::mesh::torus;
use crate::mesh::uv::{normalize_uv, unwrap_angle};
use crate::ui::panel::{CutDirection, CutMode, UiState, ViewMode};
use std::f64::consts::PI;

pub struct ColoredSegment {
    pub positions: [[f32; 3]; 2],
    pub color: [f32; 4],
    pub width: f32,
}

/// Order crossing points by 3D nearest-neighbor traversal to form a closed loop.
/// This avoids UV-periodic-boundary sorting issues entirely.
fn order_crossings_nn(crossings: &[(f64, glam::Vec3)]) -> Vec<usize> {
    let n = crossings.len();
    if n <= 2 {
        return (0..n).collect();
    }

    let mut visited = vec![false; n];
    let mut order = Vec::with_capacity(n);

    // Start from the first crossing
    let mut current = 0;
    visited[current] = true;
    order.push(current);

    for _ in 1..n {
        let cur_pos = crossings[current].1;
        let mut best = usize::MAX;
        let mut best_dist = f32::MAX;
        for j in 0..n {
            if visited[j] {
                continue;
            }
            let d = (crossings[j].1 - cur_pos).length_squared();
            if d < best_dist {
                best_dist = d;
                best = j;
            }
        }
        if best == usize::MAX {
            break;
        }
        visited[best] = true;
        order.push(best);
        current = best;
    }

    order
}

pub fn classify_mesh_edges(
    mesh: &HalfEdgeMesh,
    skip_seams: bool,
    major_r: f64,
    minor_r: f64,
) -> Vec<[[f32; 3]; 2]> {
    let mut mesh_edges = Vec::new();
    let mut visited = vec![false; mesh.half_edges.len()];

    for (i, he) in mesh.half_edges.iter().enumerate() {
        if visited[i] {
            continue;
        }
        let v0_idx = he.origin.0;
        let v1_idx = mesh.half_edges[he.next.0].origin.0;

        if !mesh.vertices[v0_idx].position.is_finite() {
            continue;
        }
        if !mesh.vertices[v1_idx].position.is_finite() {
            continue;
        }

        // UV视图：直接使用 UV 坐标
        if skip_seams {
            let uv0 = mesh.vertices[v0_idx].uv;
            let uv1 = mesh.vertices[v1_idx].uv;
            let p0 =
                crate::mesh::torus::unfold_position(uv0.x as f64, uv0.y as f64, major_r, minor_r);
            let p1 =
                crate::mesh::torus::unfold_position(uv1.x as f64, uv1.y as f64, major_r, minor_r);
            mesh_edges.push([p0.to_array(), p1.to_array()]);
        } else {
            // 3D视图：直接用 mesh.position
            let p0 = mesh.vertices[v0_idx].position.to_array();
            let p1 = mesh.vertices[v1_idx].position.to_array();
            mesh_edges.push([p0, p1]);
        }

        visited[i] = true;
        if he.twin.0 < mesh.half_edges.len() {
            visited[he.twin.0] = true;
        }
    }

    mesh_edges
}

/// Generate cut loop visualization segments.
/// - 3D Grid 切割线：**沿网格边求交**（与实际切割产生的边完全重合）——
///   解析曲线在三角网格（Delaunay/OBJ）上会与实际切割边分离
///   （光滑曲线 vs 锯齿状切割边），造成"没沿切割线切开"的观感。
/// - Knot 曲线：保留解析几何曲线（knot 是设计曲线，非网格边）。
/// - Unfolded 视图：平面直线（unfolded 分支）。
pub fn generate_loop_segments(mesh: &HalfEdgeMesh, ui_state: &UiState) -> Vec<ColoredSegment> {
    if !ui_state.show_patch_edges {
        return Vec::new();
    }
    if ui_state.view_mode == ViewMode::Unfolded {
        return generate_unfolded_loop_segments(ui_state);
    }
    match ui_state.cut_mode {
        CutMode::Grid => generate_mesh_loop_segments(mesh, ui_state),
        CutMode::Knot => generate_analytical_loop_segments(ui_state),
    }
}

fn generate_analytical_loop_segments(ui_state: &UiState) -> Vec<ColoredSegment> {
    let model = &ui_state.surface_model;
    let n_points = 512;
    let mut segments = Vec::new();

    let knot_info = [
        (ui_state.knot_k1, ui_state.knot_show_1, 0.6_f32),
        (
            ui_state.knot_k2,
            ui_state.knot_show_2,
            0.236_067_98_f32 + 0.6,
        ),
    ];
    for &(k_val, show, hue) in &knot_info {
        if !show {
            continue;
        }
        let color = color_scheme::hsl_to_rgb(hue % 1.0, 0.9, 0.55);
        let width = ui_state.loop_line_width * line_width_scale(ui_state);
        let pts = model.generate_torus_knot_line(k_val as f64, n_points);
        for k in 0..pts.len() {
            let next = (k + 1) % pts.len();
            segments.push(ColoredSegment {
                positions: [pts[k].to_array(), pts[next].to_array()],
                color,
                width,
            });
        }
    }

    segments
}

/// 生成展开视图中的切割线段。
/// U-loop 在展开平面上为竖直线，V-loop 为水平线，纽结线为斜线。
fn generate_unfolded_loop_segments(ui_state: &UiState) -> Vec<ColoredSegment> {
    let (major_r, minor_r) = match &ui_state.surface_model {
        SurfaceModel::Torus {
            major_radius,
            minor_radius,
            ..
        } => (*major_radius, *minor_radius),
        SurfaceModel::Unknown => return Vec::new(),
    };
    let (min_u, max_u, min_v, max_v) = ui_state.uv_range;
    let u_range = max_u - min_u;
    let v_range = max_v - min_v;

    let mut segments = Vec::new();

    match ui_state.cut_mode {
        CutMode::Grid => {
            for l in &ui_state.cut_loops {
                if !l.active {
                    continue;
                }
                let color = l.color;
                let width = ui_state.loop_line_width * line_width_scale(ui_state);

                match l.direction {
                    CutDirection::U if u_range > 1e-6 => {
                        // U-loop: 在展开平面上为竖直线 x = u_val * R
                        let u_val = ui_state.loop_u_position(l.index);
                        let x = (u_val * major_r) as f32;
                        let y0 = (min_v * minor_r) as f32;
                        let y1 = (max_v * minor_r) as f32;
                        segments.push(ColoredSegment {
                            positions: [[x, y0, 0.0], [x, y1, 0.0]],
                            color,
                            width,
                        });
                    }
                    CutDirection::V if v_range > 1e-6 => {
                        // V-loop: 在展开平面上为水平线 y = v_val * r
                        let v_val = ui_state.loop_v_position(l.index);
                        let y = (v_val * minor_r) as f32;
                        let x0 = (min_u * major_r) as f32;
                        let x1 = (max_u * major_r) as f32;
                        segments.push(ColoredSegment {
                            positions: [[x0, y, 0.0], [x1, y, 0.0]],
                            color,
                            width,
                        });
                    }
                    _ => {}
                }
            }
        }
        CutMode::Knot => {
            // 纽结线 v = k*u: wrap v 回 uv_range，跳过跳跃段
            let n_points = 512;
            let knot_info = [
                (ui_state.knot_k1, ui_state.knot_show_1, 0.6_f32),
                (
                    ui_state.knot_k2,
                    ui_state.knot_show_2,
                    0.236_067_98_f32 + 0.6,
                ),
            ];
            for &(k_val, show, hue) in &knot_info {
                if !show {
                    continue;
                }
                let color = color_scheme::hsl_to_rgb(hue % 1.0, 0.9, 0.55);
                let width = ui_state.loop_line_width * line_width_scale(ui_state);
                let k = k_val as f64;
                let mut prev_pos: Option<[f32; 3]> = None;
                for j in 0..=n_points {
                    let t = j as f64 / n_points as f64;
                    let u = min_u + t * u_range;
                    let mut v = k * u;
                    // Wrap v into [min_v, max_v]
                    while v < min_v {
                        v += v_range;
                    }
                    while v > max_v {
                        v -= v_range;
                    }
                    let pos = torus::unfold_position(u, v, major_r, minor_r);
                    let curr = pos.to_array();
                    if let Some(prev) = prev_pos {
                        let dy = (curr[1] - prev[1]).abs();
                        if dy < (minor_r * std::f64::consts::PI) as f32 {
                            segments.push(ColoredSegment {
                                positions: [prev, curr],
                                color,
                                width,
                            });
                        }
                    }
                    prev_pos = Some(curr);
                }
            }
        }
    }

    segments
}

fn generate_mesh_loop_segments(mesh: &HalfEdgeMesh, ui_state: &UiState) -> Vec<ColoredSegment> {
    let (major_r, minor_r) = ui_state
        .surface_model
        .radii(ui_state.major_radius, ui_state.minor_radius);
    let (min_u, max_u, min_v, max_v) = ui_state.uv_range;
    let u_range = max_u - min_u;
    let v_range = max_v - min_v;

    // Collect unique edges (skip twin duplicates)
    let mut visited = vec![false; mesh.half_edges.len()];
    let mut edges: Vec<(usize, usize)> = Vec::new();
    for (i, he) in mesh.half_edges.iter().enumerate() {
        if visited[i] {
            continue;
        }
        let v0 = he.origin.0;
        let v1 = mesh.half_edges[he.next.0].origin.0;
        edges.push((v0, v1));
        visited[i] = true;
        if he.twin.0 < mesh.half_edges.len() {
            visited[he.twin.0] = true;
        }
    }

    let mut segments = Vec::new();

    for l in &ui_state.cut_loops {
        if !l.active {
            continue;
        }
        let color = l.color;
        let width = ui_state.loop_line_width * line_width_scale(ui_state);

        match l.direction {
            CutDirection::U if u_range > 1e-6 => {
                let u_const = ui_state.loop_u_position(l.index);
                let uc_norm = normalize_uv(u_const, min_u, max_u);
                let mut crossings: Vec<(f64, glam::Vec3)> = Vec::new();

                for &(v0, v1) in &edges {
                    let uv0 = mesh.vertices[v0].uv;
                    let uv1 = mesh.vertices[v1].uv;
                    let p0 = mesh.vertices[v0].position;

                    // Normalize UVs to [0, 2π) for correct unwrap_angle behavior
                    let u0 = normalize_uv(uv0.x as f64, min_u, max_u);
                    let u1_raw = normalize_uv(uv1.x as f64, min_u, max_u);
                    let u1 = unwrap_angle(u1_raw, u0);
                    let uc = unwrap_angle(uc_norm, u0);

                    let du = u1 - u0;

                    // Detect seam edge using RAW normalized values (before unwrap).
                    // After unwrap_angle, du is always in [-π, π], so we can't use it.
                    // Seam edges connect duplicate vertices: same 3D position but UV
                    // values at opposite ends of the [0, 2π) range.
                    let raw_du = (u1_raw - u0).abs();
                    let is_seam = raw_du > PI;

                    if !is_seam {
                        if du.abs() < 1e-12 {
                            continue;
                        }
                        let t = (uc - u0) / du;
                        if !(-1e-8..=1.0 + 1e-8).contains(&t) {
                            continue;
                        }
                        let t = t.clamp(0.0, 1.0);

                        let vv0 = normalize_uv(uv0.y as f64, min_v, max_v);
                        let vv1_raw = normalize_uv(uv1.y as f64, min_v, max_v);
                        let vv1 = unwrap_angle(vv1_raw, vv0);
                        let v_interp = vv0 + t * (vv1 - vv0);

                        // UV 域求交 → 环面映射：交点位于环面表面（等价于
                        // UV 上的直线段映射；3D 弦插值会偏离环面表面）
                        let pos =
                            crate::mesh::torus::torus_position(u_const, v_interp, major_r, minor_r);
                        crossings.push((v_interp, pos));
                    } else {
                        // Seam edge: both vertices share the same 3D position.
                        // Check if the cut line crosses the seam (u_const is within
                        // the small arc between the two seam UV values).
                        // The seam arc is the complement of the large du gap.
                        let u_small = u0.min(u1_raw);
                        let u_large = u0.max(u1_raw);
                        // Cut line is "at the seam" if it's NOT in the interior arc
                        let in_interior = uc_norm > u_small && uc_norm < u_large;
                        if !in_interior {
                            // Crossing at seam — both endpoints have same 3D position
                            let pos = p0;
                            let vv0 = normalize_uv(uv0.y as f64, min_v, max_v);
                            crossings.push((vv0, pos));
                        }
                    }
                }

                // Connect using 3D nearest-neighbor to avoid UV periodic sort issues
                if crossings.len() >= 2 {
                    log::debug!(
                        "U-loop {} (u={:.3}): {} crossings",
                        l.index,
                        u_const,
                        crossings.len()
                    );
                    let order = order_crossings_nn(&crossings);
                    for k in 0..order.len() {
                        let next = (k + 1) % order.len();
                        segments.push(ColoredSegment {
                            positions: [
                                crossings[order[k]].1.to_array(),
                                crossings[order[next]].1.to_array(),
                            ],
                            color,
                            width,
                        });
                    }
                }
            }
            CutDirection::V if v_range > 1e-6 => {
                let v_const = ui_state.loop_v_position(l.index);
                let vc_norm = normalize_uv(v_const, min_v, max_v);
                let mut crossings: Vec<(f64, glam::Vec3)> = Vec::new();

                for &(v0, v1) in &edges {
                    let uv0 = mesh.vertices[v0].uv;
                    let uv1 = mesh.vertices[v1].uv;
                    let p0 = mesh.vertices[v0].position;

                    // Normalize UVs to [0, 2π) for correct unwrap_angle behavior
                    let vv0 = normalize_uv(uv0.y as f64, min_v, max_v);
                    let vv1_raw = normalize_uv(uv1.y as f64, min_v, max_v);
                    let vv1 = unwrap_angle(vv1_raw, vv0);
                    let vc = unwrap_angle(vc_norm, vv0);

                    let dv = vv1 - vv0;

                    // Use RAW normalized values for seam detection (same as U-direction)
                    let raw_dv = (vv1_raw - vv0).abs();
                    let is_seam = raw_dv > PI;

                    if !is_seam {
                        if dv.abs() < 1e-12 {
                            continue;
                        }
                        let t = (vc - vv0) / dv;
                        if !(-1e-8..=1.0 + 1e-8).contains(&t) {
                            continue;
                        }
                        let t = t.clamp(0.0, 1.0);

                        let u0_val = normalize_uv(uv0.x as f64, min_u, max_u);
                        let u1_raw = normalize_uv(uv1.x as f64, min_u, max_u);
                        let u1_val = unwrap_angle(u1_raw, u0_val);
                        let u_interp = u0_val + t * (u1_val - u0_val);

                        // UV 域求交 → 环面映射（同 U 线）
                        let pos =
                            crate::mesh::torus::torus_position(u_interp, v_const, major_r, minor_r);
                        crossings.push((u_interp, pos));
                    } else {
                        // V-seam edge: check if v_const is at the seam
                        let v_small = vv0.min(vv1_raw);
                        let v_large = vv0.max(vv1_raw);
                        let in_interior = vc_norm > v_small && vc_norm < v_large;
                        if !in_interior {
                            let pos = p0;
                            let u0_val = normalize_uv(uv0.x as f64, min_u, max_u);
                            crossings.push((u0_val, pos));
                        }
                    }
                }

                // Connect using 3D nearest-neighbor to avoid UV periodic sort issues
                if crossings.len() >= 2 {
                    log::debug!(
                        "V-loop {} (v={:.3}): {} crossings",
                        l.index,
                        v_const,
                        crossings.len()
                    );
                    let order = order_crossings_nn(&crossings);
                    for k in 0..order.len() {
                        let next = (k + 1) % order.len();
                        segments.push(ColoredSegment {
                            positions: [
                                crossings[order[k]].1.to_array(),
                                crossings[order[next]].1.to_array(),
                            ],
                            color,
                            width,
                        });
                    }
                }
            }
            _ => {}
        }
    }

    segments
}

pub fn create_patch_edge_pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    surface_format: wgpu::TextureFormat,
    depth_bias: i32,
) -> (wgpu::RenderPipeline, wgpu::BindGroupLayout) {
    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Patch Edge Bind Group Layout"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }],
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Patch Edge Pipeline Layout"),
        bind_group_layouts: &[&bind_group_layout],
        push_constant_ranges: &[],
    });

    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Patch Edge Pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_main"),
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: (std::mem::size_of::<f32>() * 10) as wgpu::BufferAddress,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &[
                    wgpu::VertexAttribute {
                        offset: 0,
                        shader_location: 0,
                        format: wgpu::VertexFormat::Float32x3,
                    },
                    wgpu::VertexAttribute {
                        offset: std::mem::size_of::<f32>() as u64 * 3,
                        shader_location: 1,
                        format: wgpu::VertexFormat::Float32x4,
                    },
                    wgpu::VertexAttribute {
                        offset: std::mem::size_of::<f32>() as u64 * 7,
                        shader_location: 2,
                        format: wgpu::VertexFormat::Float32x3,
                    },
                ],
            }],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format: surface_format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: None,
            polygon_mode: wgpu::PolygonMode::Fill,
            unclipped_depth: false,
            conservative: false,
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            // 线框不写深度：避免线-线互相遮挡（U/V 线交叉点断裂）
            depth_write_enabled: false,
            depth_compare: wgpu::CompareFunction::LessEqual,
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState {
                constant: depth_bias,
                // 按表面斜率偏移：线条贴合曲面（共面），仅靠 constant 无法抵消
                // 切线方向的深度变化（z-fighting → 线条断断续续）
                slope_scale: -1.5,
                clamp: 0.0,
            },
        }),
        multisample: wgpu::MultisampleState {
            count: crate::render::state::MSAA_SAMPLES,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
        multiview: None,
        cache: None,
    });

    (pipeline, bind_group_layout)
}

const CYLINDER_SEGS: usize = 20;
/// 线宽→世界半径系数（width 1.0 ≈ 直径 0.016 单位）
const RADIUS_PER_WIDTH: f32 = 0.008;

/// Unfolded 视图的 UV 平面尺寸大（约 2πR × 2πr）、初始相机距离远（~15.7），
/// 世界空间线宽在屏幕上会缩成亚像素（线条显示不全/断断续续，放大才正常）。
/// 该系数在 Unfolded 模式下放大线宽，保证默认视图下线条可见。
pub fn line_width_scale(ui_state: &UiState) -> f32 {
    if ui_state.view_mode == ViewMode::Unfolded {
        2.5
    } else {
        1.0
    }
}

pub fn colored_segments_to_vertex_data(segments: &[ColoredSegment]) -> Vec<[f32; 10]> {
    let mut data = Vec::with_capacity(segments.len() * CYLINDER_SEGS * 6);
    for seg in segments {
        let c = seg.color;
        let p0: glam::Vec3 = glam::Vec3::from_array(seg.positions[0]);
        let p1: glam::Vec3 = glam::Vec3::from_array(seg.positions[1]);
        let len = (p1 - p0).length();
        if len < 1e-8 {
            continue;
        }
        let dir = (p1 - p0) / len;
        let radius = seg.width * RADIUS_PER_WIDTH;

        let ref_up = if dir.y.abs() < 0.9 {
            glam::Vec3::Y
        } else {
            glam::Vec3::X
        };
        let perp1 = ref_up.cross(dir).normalize();
        let perp2 = dir.cross(perp1);

        let v = |pos: glam::Vec3, n: glam::Vec3| -> [f32; 10] {
            [pos.x, pos.y, pos.z, c[0], c[1], c[2], c[3], n.x, n.y, n.z]
        };

        for i in 0..CYLINDER_SEGS {
            let a0 = 2.0 * std::f32::consts::PI * i as f32 / CYLINDER_SEGS as f32;
            let a1 = 2.0 * std::f32::consts::PI * (i + 1) as f32 / CYLINDER_SEGS as f32;

            // Ring at p0
            let n0_a0 = perp1 * a0.cos() + perp2 * a0.sin();
            let n0_a1 = perp1 * a1.cos() + perp2 * a1.sin();
            let o0_a0 = n0_a0 * radius;
            let o0_a1 = n0_a1 * radius;

            // Two triangles per quad face
            data.push(v(p0 + o0_a0, n0_a0));
            data.push(v(p0 + o0_a1, n0_a1));
            data.push(v(p1 + o0_a0, n0_a0));

            data.push(v(p0 + o0_a1, n0_a1));
            data.push(v(p1 + o0_a1, n0_a1));
            data.push(v(p1 + o0_a0, n0_a0));
        }
    }
    data
}
