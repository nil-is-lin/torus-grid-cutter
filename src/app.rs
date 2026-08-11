use std::sync::Arc;
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalSize,
    event::WindowEvent,
    event_loop::ActiveEventLoop,
    keyboard::PhysicalKey,
    window::{Window, WindowAttributes, WindowId},
};

use crate::camera::OrbitCamera;
use crate::color_scheme;
use crate::mesh::half_edge::{FaceId, HalfEdgeMesh};
use crate::mesh::surface::SurfaceModel;
use crate::mesh::torus;
use crate::mesh::vertex::GpuVertex;
use crate::render::pipeline::SceneUniform;
use crate::render::state::{ColoredEdgeData, RenderState};
use crate::ui::panel::{self, ColorMode, MeshType, UiState, ViewMode};

/// 标记 per-patch shader 模式的哨兵值（写入 camera_position.w 传给 WGSL）。
const PER_PATCH_SHADER_FLAG: f32 = 999.0;

/// 安装中文字体：egui 默认字体（Ubuntu-Light）不含 CJK 字形，
/// 中文字符会渲染为方框（tofu）。从 Windows 系统字体目录加载中文字体
/// 追加到 Proportional/Monospace 家族末尾作为 fallback。
fn install_cjk_fonts(ctx: &egui::Context) {
    // 常见中文 Windows 字体（按优先级：纯 TTF 优先于 TTC 集合）
    const CANDIDATES: &[&str] = &[
        "C:\\Windows\\Fonts\\msyh.ttc",   // 微软雅黑
        "C:\\Windows\\Fonts\\simhei.ttf", // 黑体
        "C:\\Windows\\Fonts\\simsun.ttc", // 宋体
        "C:\\Windows\\Fonts\\Deng.ttf",   // 等线
        "C:\\Windows\\Fonts\\msyhbd.ttc", // 微软雅黑 Bold
    ];

    for path in CANDIDATES {
        let Ok(bytes) = std::fs::read(path) else {
            continue;
        };
        let mut fonts = egui::FontDefinitions::default();
        fonts.font_data.insert(
            "cjk".to_owned(),
            Arc::new(egui::FontData::from_owned(bytes)),
        );
        for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
            fonts
                .families
                .get_mut(&family)
                .unwrap()
                .push("cjk".to_owned());
        }
        ctx.set_fonts(fonts);
        log::info!("Loaded CJK font from {}", path);
        return;
    }

    log::warn!("No CJK font found — Chinese text will render as boxes");
}

pub struct App {
    // Render window (3D scene)
    render_window: Option<Arc<Window>>,
    render_window_id: Option<WindowId>,
    render_surface: Option<wgpu::Surface<'static>>,
    render_surface_config: Option<wgpu::SurfaceConfiguration>,
    render_size: PhysicalSize<u32>,
    /// 中央 3D 视口区域（egui 逻辑坐标，每帧由布局更新）
    scene_rect: egui::Rect,
    /// 最近一次窗口指针位置（物理像素，CursorMoved 维护）——不依赖 egui 内部状态
    last_pointer: Option<(f32, f32)>,
    /// Delaunay 网格缓存：拓扑（UV/三角形/半边结构）与 R/r 无关，
    /// 拖动 R/r 时只需重映射顶点 3D 位置，避免重复泊松采样+三角化。
    delaunay_base: Option<HalfEdgeMesh>,
    /// 缓存对应的采样点数（变化时重新生成）。
    delaunay_base_points: usize,

    // Shared GPU resources
    device: Option<wgpu::Device>,
    queue: Option<wgpu::Queue>,

    // Rendering
    render_state: Option<RenderState>,
    camera: OrbitCamera,

    // Mesh data
    torus_mesh: Option<HalfEdgeMesh>,
    base_mesh: Option<HalfEdgeMesh>,
    patch_colors: Vec<[f32; 4]>,

    // Shader modules
    shader: Option<wgpu::ShaderModule>,
    wireframe_shader: Option<wgpu::ShaderModule>,

    // Egui (runs on UI window)
    egui_ctx: egui::Context,
    egui_winit_state: Option<egui_winit::State>,
    egui_renderer: Option<egui_wgpu::Renderer>,

    // UI state
    pub ui_state: UiState,

    // Click detection

    // Timing
    last_time: std::time::Instant,

    // 每帧只渲染一次：render() 同时绘制两个窗口，由 RedrawRequested 驱动
    //（render 窗口优先；其被最小化/抑制时由 UI 窗口兜底）
    rendered_this_frame: bool,

    // 后台构建（OBJ 导入）状态：worker 线程回报，主线程每帧轮询并镜像到 UI 状态栏
    build_rx: Option<std::sync::mpsc::Receiver<BuildEvent>>,
    is_building: bool,
    build_status: String,
    build_progress: f32,
    // 文件对话框（OBJ 多选）在后台线程打开，主线程保持响应，避免 rfd 模态
    // 对话框阻塞事件循环导致"未响应"。线程把结果通过此通道回传。
    dialog_rx: Option<std::sync::mpsc::Receiver<Option<Vec<String>>>>,
    // 网格统计缓存：仅在网格变化时重算（apply_build_output / build_torus_mesh /
    // reapply_cuts），避免每帧 compute_stats 占用主线程。
    cached_stats: Option<crate::mesh::stats::MeshStats>,
}

/// 后台 OBJ 构建线程回报的事件。
enum BuildEvent {
    Progress {
        stage: String,
        done: usize,
        total: usize,
    },
    Done(crate::mesh::build::BuildOutput),
    Error(String),
}

/// 在后台线程打开 rfd 文件对话框，避免阻塞主事件循环（Windows 下可跨线程调用；
/// macOS/Linux 的 rfd 同步 API 必须在主线程调用，故回退为同步执行）。
fn spawn_obj_dialog(tx: std::sync::mpsc::Sender<Option<Vec<String>>>) {
    let run = || {
        let files = rfd::FileDialog::new()
            .add_filter("OBJ Files", &["obj"])
            .add_filter("All Files", &["*"])
            .set_title("Open OBJ Files")
            .pick_files();
        files.map(|v| {
            v.into_iter()
                .map(|p| p.to_string_lossy().to_string())
                .collect()
        })
    };
    #[cfg(target_os = "windows")]
    {
        std::thread::spawn(move || {
            let _ = tx.send(run());
        });
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = tx.send(run());
    }
}

impl App {
    pub async fn new() -> Self {
        let egui_ctx = egui::Context::default();
        install_cjk_fonts(&egui_ctx);
        App {
            render_window: None,
            render_window_id: None,
            render_surface: None,
            render_surface_config: None,
            render_size: PhysicalSize::new(1280, 720),
            scene_rect: egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1280.0, 720.0)),
            last_pointer: None,
            delaunay_base: None,
            delaunay_base_points: 0,
            device: None,
            queue: None,
            render_state: None,
            camera: OrbitCamera::new(),
            torus_mesh: None,
            base_mesh: None,
            patch_colors: Vec::new(),
            shader: None,
            wireframe_shader: None,
            egui_ctx,
            egui_winit_state: None,
            egui_renderer: None,
            ui_state: UiState::default(),
            last_time: std::time::Instant::now(),
            rendered_this_frame: false,
            build_rx: None,
            is_building: false,
            build_status: "Ready".to_string(),
            build_progress: 0.0,
            dialog_rx: None,
            cached_stats: None,
        }
    }

    async fn init_gpu(
        render_window: &Arc<Window>,
    ) -> (
        wgpu::Device,
        wgpu::Queue,
        wgpu::Surface<'static>,
        wgpu::SurfaceConfiguration,
    ) {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        let render_surface = instance.create_surface(render_window.clone()).unwrap();

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&render_surface),
                force_fallback_adapter: false,
            })
            .await
            .unwrap();

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                    ..Default::default()
                },
                None,
            )
            .await
            .unwrap();

        // Configure render surface
        let r_config = Self::configure_surface(
            &render_surface,
            &adapter,
            &device,
            render_window.inner_size(),
        );

        (device, queue, render_surface, r_config)
    }

    /// 创建 surface 配置并应用（两个窗口共用同一套参数）。
    fn configure_surface(
        surface: &wgpu::Surface<'_>,
        adapter: &wgpu::Adapter,
        device: &wgpu::Device,
        size: PhysicalSize<u32>,
    ) -> wgpu::SurfaceConfiguration {
        let caps = surface.get_capabilities(adapter);
        let format = caps
            .formats
            .iter()
            .find(|f| f.is_srgb())
            .copied()
            .unwrap_or(caps.formats[0]);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width,
            height: size.height,
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(device, &config);
        config
    }

    /// 窗口尺寸变化时重新配置 surface（render 窗口额外重建深度纹理）。
    fn resize_surface(&mut self, new_size: PhysicalSize<u32>) {
        if new_size.width == 0 || new_size.height == 0 {
            return;
        }
        let (size_slot, config, surface) = (
            &mut self.render_size,
            &mut self.render_surface_config,
            &self.render_surface,
        );
        let (Some(config), Some(surface), Some(device)) =
            (config.as_mut(), surface.as_ref(), self.device.as_ref())
        else {
            return;
        };
        *size_slot = new_size;
        config.width = new_size.width;
        config.height = new_size.height;
        surface.configure(device, config);
        if let Some(rs) = self.render_state.as_mut() {
            rs.update_depth_texture(device, new_size.width, new_size.height);
        }
    }

    fn build_torus_mesh(&mut self) {
        let mut mesh = match &self.ui_state.mesh_type {
            MeshType::Quad => {
                let (positions, uvs, quads) = torus::generate_unfolded_quad_mesh(
                    self.ui_state.major_radius,
                    self.ui_state.minor_radius,
                    self.ui_state.resolution_u,
                    self.ui_state.resolution_v,
                );
                HalfEdgeMesh::from_quads(&positions, &uvs, &quads)
            }
            MeshType::Delaunay => {
                // 拓扑缓存：Delaunay 采样+三角化只依赖点数（不依赖 R/r），
                // 拖动 R/r 时仅重映射顶点 3D 位置（torus_position），
                // 显著降低 rebuild 延迟。
                let pts = self.ui_state.delaunay_points;
                let needs_regenerate =
                    self.delaunay_base.is_none() || self.delaunay_base_points != pts;
                if needs_regenerate {
                    let (positions, uvs, triangles) = crate::mesh::delaunay::generate_delaunay_mesh(
                        self.ui_state.major_radius,
                        self.ui_state.minor_radius,
                        pts,
                        42,
                    );
                    self.delaunay_base =
                        Some(HalfEdgeMesh::from_triangles(&positions, &uvs, &triangles));
                    self.delaunay_base_points = pts;
                }
                let mut mesh = self.delaunay_base.clone().unwrap();
                // 重映射 3D 位置（UV 与拓扑不变，R/r 变化只影响位置）
                for v in &mut mesh.vertices {
                    v.position = torus::torus_position(
                        v.uv.x as f64,
                        v.uv.y as f64,
                        self.ui_state.major_radius,
                        self.ui_state.minor_radius,
                    );
                }
                mesh
            }
            MeshType::ObjFile(_) => {
                // OBJ 导入已改由后台 worker 线程构建（spawn_objfile_build），
                // 同步路径不应再触发；若到达则跳过，避免重复构建/阻塞主线程。
                log::warn!("build_torus_mesh 收到 ObjFile，但 OBJ 已由后台线程构建，跳过");
                return;
            }
        };

        match &self.ui_state.mesh_type {
            MeshType::Quad | MeshType::Delaunay => {
                self.ui_state.uv_range = (
                    0.0,
                    2.0 * std::f64::consts::PI,
                    0.0,
                    2.0 * std::f64::consts::PI,
                );
            }
            MeshType::ObjFile(_) => {
                if !mesh.vertices.is_empty() {
                    let mut min_u = f64::MAX;
                    let mut max_u = f64::MIN;
                    let mut min_v = f64::MAX;
                    let mut max_v = f64::MIN;
                    for v in &mesh.vertices {
                        let u = v.uv.x as f64;
                        let vv = v.uv.y as f64;
                        if u < min_u {
                            min_u = u;
                        }
                        if u > max_u {
                            max_u = u;
                        }
                        if vv < min_v {
                            min_v = vv;
                        }
                        if vv > max_v {
                            max_v = vv;
                        }
                    }
                    if max_u - min_u > 1e-6 && max_v - min_v > 1e-6 {
                        self.ui_state.uv_range = (min_u, max_u, min_v, max_v);
                    }
                }
            }
        }

        let positions_3d: Vec<glam::Vec3> = mesh.vertices.iter().map(|v| v.position).collect();
        let mut unique_positions: Vec<glam::Vec3> = Vec::with_capacity(positions_3d.len());
        let eps_sq = 1e-8f32;
        for &p in &positions_3d {
            if !unique_positions
                .iter()
                .any(|&q| (p - q).length_squared() < eps_sq)
            {
                unique_positions.push(p);
            }
        }
        // 程序生成的网格（Quad/Delaunay）参数已知，直接构造解析模型——
        // 避免拟合的数值路径（特征向量/圆拟合/残差阈值）把确定的环面误判为 Unknown；
        // 拟合仅用于 OBJ 导入的网格（Unknown 是其真实结果，用于跳过 remap）。
        let model = match &self.ui_state.mesh_type {
            MeshType::Quad | MeshType::Delaunay => SurfaceModel::torus_from_params(
                self.ui_state.major_radius,
                self.ui_state.minor_radius,
            ),
            MeshType::ObjFile(_) => SurfaceModel::fit_from_mesh(
                &unique_positions,
                self.ui_state.major_radius,
                self.ui_state.minor_radius,
            ),
        };
        self.ui_state.surface_model = model;

        if let MeshType::ObjFile(_) = &self.ui_state.mesh_type {
            if self.ui_state.surface_model.is_torus() {
                self.remap_uvs_from_analytical_model(&mut mesh);
            }
        }

        let (nu, nv) = self.ui_state.cut_grid_dims();
        self.patch_colors =
            color_scheme::generate_patch_colors(nu, nv, &self.ui_state.color_scheme);
        self.torus_mesh = Some(mesh.clone());
        self.base_mesh = Some(mesh);
        self.cached_stats = self.torus_mesh.as_ref().map(|m| m.compute_stats());
    }

    /// 在后台线程构建 OBJ 导入网格，主线程保持响应。
    fn spawn_objfile_build(&mut self) {
        let paths = match &self.ui_state.mesh_type {
            panel::MeshType::ObjFile(p) => p.clone(),
            _ => return,
        };
        let major = self.ui_state.major_radius;
        let minor = self.ui_state.minor_radius;
        // 使用有界通道（容量 2）：worker 发送进度后若主线程未消费会短暂阻塞，
        // 避免无界队列堆积，确保主线程始终能及时 pump Windows 消息、刷新 UI。
        let (tx, rx) = std::sync::mpsc::sync_channel::<BuildEvent>(2);
        self.build_rx = Some(rx);
        self.is_building = true;
        self.build_status = "Preparing…".to_string();
        self.build_progress = 0.0;
        std::thread::spawn(move || {
            let mut report = |stage: &str, done: usize, total: usize| {
                // try_send：若主线程暂时没消费，丢弃旧进度事件，worker 绝不阻塞。
                let _ = tx.try_send(BuildEvent::Progress {
                    stage: stage.to_string(),
                    done,
                    total,
                });
            };
            match crate::mesh::build::build_objfile_mesh(&paths, major, minor, &mut report) {
                Ok(out) => {
                    let _ = tx.send(BuildEvent::Done(out));
                }
                Err(e) => {
                    let _ = tx.send(BuildEvent::Error(e));
                }
            }
        });
    }

    /// 每帧轮询后台线程（非阻塞），把进度/结果同步到 App 状态。
    fn poll_build_worker(&mut self) {
        let rx = match self.build_rx.take() {
            Some(rx) => rx,
            None => return,
        };
        let mut finished = false;
        // 每帧最多处理 8 个事件，避免 worker 瞬间产生大量进度时单帧卡死。
        for _ in 0..8 {
            match rx.try_recv() {
                Ok(BuildEvent::Progress { stage, done, total }) => {
                    self.build_status = stage;
                    self.build_progress = if total > 0 {
                        done as f32 / total as f32
                    } else {
                        0.0
                    };
                }
                Ok(BuildEvent::Done(out)) => {
                    self.apply_build_output(out);
                    self.is_building = false;
                    self.build_status = "Ready".to_string();
                    self.build_progress = 1.0;
                    finished = true;
                    break;
                }
                Ok(BuildEvent::Error(e)) => {
                    log::error!("OBJ 后台构建失败: {}", e);
                    self.is_building = false;
                    self.build_status = format!("Error: {}", e);
                    self.build_progress = 0.0;
                    finished = true;
                    break;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => break,
            }
        }
        if !finished {
            self.build_rx = Some(rx);
        }
    }

    /// 每帧轮询文件对话框线程（非阻塞）；对话框关闭后触发后台构建。
    fn poll_dialog(&mut self) {
        let rx = match self.dialog_rx.take() {
            Some(rx) => rx,
            None => return,
        };
        match rx.try_recv() {
            Ok(Some(paths)) => {
                if !paths.is_empty() {
                    self.ui_state.mesh_type = panel::MeshType::ObjFile(paths);
                    for l in &mut self.ui_state.cut_loops {
                        l.cut = false;
                    }
                    self.spawn_objfile_build();
                } else {
                    self.is_building = false;
                    self.build_status = "Ready".to_string();
                    self.build_progress = 0.0;
                }
            }
            Ok(None) => {
                // 用户取消选择
                self.is_building = false;
                self.build_status = "Ready".to_string();
                self.build_progress = 0.0;
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                // 对话框仍打开，保留接收端供下一帧继续轮询
                self.dialog_rx = Some(rx);
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.is_building = false;
                self.build_status = "Ready".to_string();
                self.build_progress = 0.0;
            }
        }
    }

    /// 把后台构建产物应用到场景（GPU 相关操作仍在主线程）。
    fn apply_build_output(&mut self, out: crate::mesh::build::BuildOutput) {
        self.ui_state.uv_range = out.uv_range;
        self.ui_state.surface_model = out.surface_model;
        self.torus_mesh = Some(out.mesh.clone());
        self.base_mesh = Some(out.mesh);
        let (nu, nv) = self.ui_state.cut_grid_dims();
        self.patch_colors =
            color_scheme::generate_patch_colors(nu, nv, &self.ui_state.color_scheme);
        self.reapply_cuts();
        self.rebuild_render_state();
    }

    fn reapply_cuts(&mut self) {
        // OBJ 多选导入时，文件数即为补片数（每个文件一个补片）。
        let obj_files = if let MeshType::ObjFile(p) = &self.ui_state.mesh_type {
            p.len()
        } else {
            0
        };
        if let Some(ref base) = self.base_mesh {
            let mut mesh = base.clone();

            // 切割只影响与切割线相交的面：相交的非三角形面在切割处**局部三角化**
            // （cut_face_local），其余面保持原面型——Quad 网格的四边形与
            // Knot 曲线的远处区域都不受影响。
            if self.ui_state.has_cut_loops() {
                match self.ui_state.cut_mode {
                    crate::ui::panel::CutMode::Grid => {
                        let u_vals = self.ui_state.cut_u_values();
                        let v_vals = self.ui_state.cut_v_values();
                        if !u_vals.is_empty() || !v_vals.is_empty() {
                            crate::mesh::cut::cut_mesh_by_grid(
                                &mut mesh,
                                &u_vals,
                                &v_vals,
                                self.ui_state.uv_range,
                                // Delaunay 网格切割后收尾三角化，保持全三角
                                matches!(self.ui_state.mesh_type, MeshType::Delaunay),
                            );
                        }
                        let num_u = u_vals.len().max(1);
                        let num_v = v_vals.len().max(1);
                        self.patch_colors = color_scheme::generate_patch_colors(
                            num_u,
                            num_v,
                            &self.ui_state.color_scheme,
                        );
                    }
                    crate::ui::panel::CutMode::Knot => {
                        let mut k_values = Vec::new();
                        if self.ui_state.knot_cut_1 {
                            k_values.push(self.ui_state.knot_k1);
                        }
                        if self.ui_state.knot_cut_2 {
                            k_values.push(self.ui_state.knot_k2);
                        }
                        if !k_values.is_empty() {
                            crate::mesh::cut::cut_mesh_by_knots(
                                &mut mesh,
                                &k_values,
                                self.ui_state.uv_range,
                            );
                        }
                    }
                }
            } else {
                let np = if obj_files > 0 { obj_files } else { 1 };
                self.patch_colors =
                    color_scheme::generate_patch_colors(np, 1, &self.ui_state.color_scheme);
            }

            let (pu, pv) = self.ui_state.cut_grid_dims();
            let slot = if obj_files > 0 && !self.ui_state.has_cut_loops() {
                obj_files
            } else {
                pu * pv
            };
            self.ui_state.patch_visible = vec![true; slot];

            if self.ui_state.has_cut_loops() {
                match self.ui_state.cut_mode {
                    crate::ui::panel::CutMode::Grid => {
                        let u_vals = self.ui_state.cut_u_values();
                        let v_vals = self.ui_state.cut_v_values();
                        crate::mesh::cut::assign_patch_indices(
                            &mut mesh,
                            &u_vals,
                            &v_vals,
                            self.ui_state.uv_range,
                        );
                    }
                    crate::ui::panel::CutMode::Knot => {
                        let mut k_values = Vec::new();
                        if self.ui_state.knot_cut_1 {
                            k_values.push(self.ui_state.knot_k1);
                        }
                        if self.ui_state.knot_cut_2 {
                            k_values.push(self.ui_state.knot_k2);
                        }
                        if !k_values.is_empty() {
                            crate::mesh::cut::assign_multi_knot_patch_indices(
                                &mut mesh,
                                &k_values,
                                self.ui_state.uv_range,
                            );
                        } else {
                            for f in &mut mesh.faces {
                                if f.valid {
                                    f.patch_index = Some((0, 0));
                                }
                            }
                        }
                    }
                }
            } else if obj_files == 0 {
                for f in &mut mesh.faces {
                    if f.valid {
                        f.patch_index = Some((0, 0));
                    }
                }
            }
            // 注：OBJ 多选导入（无切割线）时 obj_files > 0，不进入上面的
            // else 分支，从而保留导入时写入的逐文件 patch_index。

            if self.ui_state.cut_mode == crate::ui::panel::CutMode::Knot {
                let max_pu = mesh
                    .faces
                    .iter()
                    .filter(|f| f.valid)
                    .filter_map(|f| f.patch_index.map(|p| p.0))
                    .max()
                    .unwrap_or(0);
                self.patch_colors =
                    color_scheme::generate_patch_colors(max_pu + 1, 1, &self.ui_state.color_scheme);
                self.ui_state.patch_visible = vec![true; max_pu + 1];
            }

            mesh = self.to_3d_view_mesh(&mesh);

            // 拓扑已在切割前三角化；此处无需重复（to_3d_view_mesh 只重算顶点位置）
            self.torus_mesh = Some(mesh);
            self.cached_stats = self.torus_mesh.as_ref().map(|m| m.compute_stats());
        }
    }

    /// 将 UV 网格顶点按 surface_model 坐标系重算为 3D 环面位置。
    /// torus_mesh 始终存储 3D 坐标（Unfolded 渲染时由 UV 现算平面坐标）。
    /// UV 退化（OBJ 无 vt 时全为零）时保留原始 3D 位置——
    /// 否则所有顶点会被映射到 torus_position(0,0) 同一点而不可见。
    fn to_3d_view_mesh(&self, mesh: &HalfEdgeMesh) -> HalfEdgeMesh {
        let mut uv_valid = false;
        for v in &mesh.vertices {
            if v.uv.x != 0.0 || v.uv.y != 0.0 {
                uv_valid = true;
                break;
            }
        }
        if !uv_valid {
            return mesh.clone();
        }
        let mut m = mesh.clone();
        let (major_r, minor_r) = self
            .ui_state
            .surface_model
            .radii(self.ui_state.major_radius, self.ui_state.minor_radius);
        let (fc, fa, fu, fv) = self.ui_state.surface_model.frame();
        for v in &mut m.vertices {
            v.position = torus::torus_position_frame(
                v.uv.x as f64,
                v.uv.y as f64,
                major_r,
                minor_r,
                fc,
                fa,
                fu,
                fv,
            );
        }
        m
    }

    /// 将 UV 网格顶点映射为**当前视图坐标系**下的网格：
    /// - 3D 视图：环面位置（to_3d_view_mesh）
    /// - Unfolded 视图：按 unfold_position 映射为 UV 平面坐标
    ///
    /// 拾取与选中高亮必须与渲染路径使用同一坐标系，否则会错位。
    fn to_view_mesh(&self, mesh: &HalfEdgeMesh) -> HalfEdgeMesh {
        if self.ui_state.view_mode == ViewMode::Unfolded {
            let mut uv_valid = false;
            for v in &mesh.vertices {
                if v.uv.x != 0.0 || v.uv.y != 0.0 {
                    uv_valid = true;
                    break;
                }
            }
            if !uv_valid {
                return mesh.clone();
            }
            let mut m = mesh.clone();
            let (major_r, minor_r) = self
                .ui_state
                .surface_model
                .radii(self.ui_state.major_radius, self.ui_state.minor_radius);
            for v in &mut m.vertices {
                v.position = torus::unfold_position(v.uv.x as f64, v.uv.y as f64, major_r, minor_r);
            }
            m
        } else {
            self.to_3d_view_mesh(mesh)
        }
    }

    fn recolor_patches(&mut self) {
        if let Some(ref mut mesh) = self.torus_mesh {
            // OBJ 多选导入（无切割线）：保留导入时写入的逐文件 patch_index，
            // 仅按文件数重新生成配色，避免被 UV 网格重排补片而丢失逐文件分组。
            if let MeshType::ObjFile(paths) = &self.ui_state.mesh_type {
                if !self.ui_state.has_cut_loops() {
                    let n = paths.len().max(1);
                    self.patch_colors =
                        color_scheme::generate_patch_colors(n, 1, &self.ui_state.color_scheme);
                    self.ui_state.patch_visible = vec![true; n];
                    self.rebuild_render_state();
                    return;
                }
            }
            match self.ui_state.cut_mode {
                crate::ui::panel::CutMode::Grid => {
                    let u_vals = self.ui_state.cut_u_values();
                    let v_vals = self.ui_state.cut_v_values();
                    let num_u = u_vals.len().max(1);
                    let num_v = v_vals.len().max(1);
                    self.patch_colors = color_scheme::generate_patch_colors(
                        num_u,
                        num_v,
                        &self.ui_state.color_scheme,
                    );
                    crate::mesh::cut::assign_patch_indices(
                        mesh,
                        &u_vals,
                        &v_vals,
                        self.ui_state.uv_range,
                    );
                }
                crate::ui::panel::CutMode::Knot => {
                    // 与 reapply_cuts 保持一致：只对启用了切割的 knot 重新指派 patch
                    let mut k_values = Vec::new();
                    if self.ui_state.knot_cut_1 {
                        k_values.push(self.ui_state.knot_k1);
                    }
                    if self.ui_state.knot_cut_2 {
                        k_values.push(self.ui_state.knot_k2);
                    }
                    if !k_values.is_empty() {
                        crate::mesh::cut::assign_multi_knot_patch_indices(
                            mesh,
                            &k_values,
                            self.ui_state.uv_range,
                        );
                    }
                    let max_pu = mesh
                        .faces
                        .iter()
                        .filter(|f| f.valid)
                        .filter_map(|f| f.patch_index.map(|p| p.0))
                        .max()
                        .unwrap_or(0);
                    self.patch_colors = color_scheme::generate_patch_colors(
                        max_pu + 1,
                        1,
                        &self.ui_state.color_scheme,
                    );
                    // 同步 patch_visible，避免 rebuild_render_state 按旧长度索引越界
                    self.ui_state.patch_visible = vec![true; max_pu + 1];
                }
            }
            self.rebuild_render_state();
        }
    }

    fn remap_uvs_from_analytical_model(&mut self, mesh: &mut HalfEdgeMesh) {
        let model = &self.ui_state.surface_model;
        for v in &mut mesh.vertices {
            if let Some((u, v_param)) = model.compute_uv(v.position) {
                let mut u = u;
                let mut v_param = v_param;
                while u < 0.0 {
                    u += 2.0 * std::f64::consts::PI;
                }
                while u >= 2.0 * std::f64::consts::PI {
                    u -= 2.0 * std::f64::consts::PI;
                }
                while v_param < 0.0 {
                    v_param += 2.0 * std::f64::consts::PI;
                }
                while v_param >= 2.0 * std::f64::consts::PI {
                    v_param -= 2.0 * std::f64::consts::PI;
                }
                v.uv = glam::Vec2::new(u as f32, v_param as f32);
            }
        }
        self.ui_state.uv_range = (
            0.0,
            2.0 * std::f64::consts::PI,
            0.0,
            2.0 * std::f64::consts::PI,
        );
    }

    fn rebuild_render_state(&mut self) {
        if self.torus_mesh.is_none() {
            // 后台构建进行中（torus_mesh 尚未就绪）时不强制同步构建，避免触发
            // ObjFile 分支或崩溃；待 worker 完成后再由 apply_build_output 调用。
            return;
        }

        let device = self.device.as_ref().unwrap();
        let queue = self.queue.as_ref().unwrap();
        let shader = self.shader.as_ref().unwrap();
        let surface_format = self.render_surface_config.as_ref().unwrap().format;

        let mesh = self.torus_mesh.as_ref().unwrap();
        let patch_colors = &self.patch_colors;
        let is_unfolded = self.ui_state.view_mode == ViewMode::Unfolded;
        let (major_r, minor_r) = self
            .ui_state
            .surface_model
            .radii(self.ui_state.major_radius, self.ui_state.minor_radius);

        let vertex_normals = if is_unfolded {
            None
        } else if self.ui_state.smooth_shading {
            Some(mesh.compute_vertex_normals())
        } else {
            None
        };

        let num_faces = mesh.num_valid_faces();
        let mut gpu_verts: Vec<GpuVertex> = Vec::with_capacity(num_faces * 3);
        let mut gpu_indices: Vec<u32> = Vec::with_capacity(num_faces * 3);
        let default_color = [0.5, 0.5, 0.5, 1.0];

        // patch 索引 (pu, pv) 由 assign_patch_indices 分配：n 条切割线 → n+1 个区域
        // （pv ∈ 0..=n）。行宽必须与 patch_colors 生成时的 nv（= n+1）一致，
        // 否则最后一列颜色错位/越界。
        let cut_nv = self.ui_state.cut_grid_dims().1;

        for (fi, face) in mesh.faces.iter().enumerate() {
            if !face.valid {
                continue;
            }
            if mesh.face_half_edges_iter(FaceId(fi)).next().is_none() {
                continue;
            }

            // 补片显隐：独立于颜色模式，所有模式下隐藏的补片都不渲染
            let visible = match face.patch_index {
                Some((pu, pv)) => {
                    let idx = pu * cut_nv + pv;
                    self.ui_state
                        .patch_visible
                        .get(idx)
                        .copied()
                        .unwrap_or(true)
                }
                None => true,
            };
            if !visible {
                continue;
            }

            let color = match self.ui_state.color_mode {
                ColorMode::Solid => self.ui_state.base_color,
                ColorMode::ByRegion => match face.patch_index {
                    None => self.ui_state.base_color,
                    Some((pu, pv)) => {
                        let idx = pu * cut_nv + pv;
                        patch_colors.get(idx).copied().unwrap_or(default_color)
                    }
                },
            };

            let patch_mat = match face.patch_index {
                Some(patch_idx) => self
                    .ui_state
                    .patch_materials
                    .get(&patch_idx)
                    .copied()
                    .unwrap_or(self.ui_state.default_material)
                    .to_f32(),
                None => self.ui_state.default_material.to_f32(),
            };
            let material = if self.ui_state.use_per_patch_shader {
                let shader_mode = match face.patch_index {
                    Some(patch_idx) => self
                        .ui_state
                        .patch_shaders
                        .get(&patch_idx)
                        .copied()
                        .unwrap_or(self.ui_state.mesh_shader),
                    None => self.ui_state.mesh_shader,
                };
                -1.0 - shader_mode.to_f32()
            } else {
                patch_mat
            };

            let mut face_vert_indices = Vec::new();
            let mut he_iter2 = mesh.face_half_edges_iter(FaceId(fi));
            if let Some(he) = he_iter2.next() {
                face_vert_indices.push(mesh.face_vertex(he));
                for he in he_iter2 {
                    face_vert_indices.push(mesh.face_vertex(he));
                }
            }
            if face_vert_indices.len() < 3 {
                continue;
            }

            let face_positions: Vec<glam::Vec3> = if is_unfolded {
                face_vert_indices
                    .iter()
                    .map(|&vi| {
                        let uv = mesh.vertices[vi].uv;
                        torus::unfold_position(uv.x as f64, uv.y as f64, major_r, minor_r)
                    })
                    .collect()
            } else {
                face_vert_indices
                    .iter()
                    .map(|&vi| mesh.vertices[vi].position)
                    .collect()
            };

            let face_uvs: Vec<glam::Vec2> = face_vert_indices
                .iter()
                .map(|&vi| mesh.vertices[vi].uv)
                .collect();

            for k in 1..face_vert_indices.len() - 1 {
                let i0 = 0;
                let i1 = k;
                let i2 = k + 1;
                let p0 = face_positions[i0];
                let p1 = face_positions[i1];
                let p2 = face_positions[i2];

                let (n0, n1, n2) = if is_unfolded {
                    (glam::Vec3::Z, glam::Vec3::Z, glam::Vec3::Z)
                } else if let Some(ref vnormals) = vertex_normals {
                    (
                        vnormals[face_vert_indices[i0]],
                        vnormals[face_vert_indices[i1]],
                        vnormals[face_vert_indices[i2]],
                    )
                } else {
                    let normal = (p1 - p0).cross(p2 - p0).normalize();
                    let view_dir = p0 - glam::Vec3::ZERO;
                    let normal = if normal.dot(view_dir) < 0.0 {
                        -normal
                    } else {
                        normal
                    };
                    (normal, normal, normal)
                };

                let uv0 = face_uvs[i0];
                let uv1 = face_uvs[i1];
                let uv2 = face_uvs[i2];

                let idx = gpu_verts.len() as u32;
                gpu_verts.push(GpuVertex {
                    position: p0.to_array(),
                    normal: n0.to_array(),
                    color,
                    uv: [uv0.x, uv0.y],
                    material,
                });
                gpu_verts.push(GpuVertex {
                    position: p1.to_array(),
                    normal: n1.to_array(),
                    color,
                    uv: [uv1.x, uv1.y],
                    material,
                });
                gpu_verts.push(GpuVertex {
                    position: p2.to_array(),
                    normal: n2.to_array(),
                    color,
                    uv: [uv2.x, uv2.y],
                    material,
                });
                gpu_indices.push(idx);
                gpu_indices.push(idx + 1);
                gpu_indices.push(idx + 2);
            }
        }

        // 选中高亮必须与渲染使用同一视图坐标系（Unfolded = UV 平面坐标），
        // 因此传给 RenderState 的网格用 to_view_mesh 转换（mesh edges/loop segments
        // 内部已按 view_mode 处理，传入视图网格不影响它们）
        let view_mesh = self.to_view_mesh(mesh);

        let edge_mesh: &HalfEdgeMesh = &view_mesh;
        let (major_r, minor_r) = self
            .ui_state
            .surface_model
            .radii(self.ui_state.major_radius, self.ui_state.minor_radius);
        if let Some(rs) = self.render_state.as_mut() {
            rs.update_mesh_buffers(
                device,
                self.wireframe_shader.as_ref().unwrap(),
                surface_format,
                &gpu_verts,
                &gpu_indices,
                Some(edge_mesh),
                &self.ui_state,
                major_r,
                minor_r,
            );
        } else {
            self.render_state = Some(RenderState::new(
                device,
                queue,
                shader,
                self.wireframe_shader.as_ref().unwrap(),
                surface_format,
                self.render_size.width,
                self.render_size.height,
                &gpu_verts,
                &gpu_indices,
                Some(edge_mesh),
                &self.ui_state,
                major_r,
                minor_r,
            ));
        }
    }

    /// 中央 3D 视口在窗口中的物理像素区域（左上角 + 尺寸）。
    fn scene_viewport(&self) -> (u32, u32, u32, u32) {
        let pp = self.render_window.as_ref().unwrap().scale_factor() as f32;
        let r = self.scene_rect;
        (
            (r.min.x * pp) as u32,
            (r.min.y * pp) as u32,
            ((r.width() * pp).round() as u32).max(1),
            ((r.height() * pp).round() as u32).max(1),
        )
    }

    /// 指针当前是否位于中央 3D 视口内。
    /// 用自维护的 winit 物理坐标（÷scale_factor → egui 逻辑坐标）判定，
    /// 不依赖 egui 内部 pointer 状态（其更新时机不可控）。
    fn pointer_in_scene(&self) -> bool {
        match self.last_pointer {
            Some((x, y)) => {
                let pp = self.render_window.as_ref().unwrap().scale_factor() as f32;
                let p = egui::pos2(x / pp, y / pp);
                self.scene_rect.contains(p)
            }
            None => false,
        }
    }

    /// 窗口坐标 → 场景局部坐标（供相机拖动/拾取使用）。
    fn scene_local(&self, x: f32, y: f32) -> (f32, f32) {
        let pp = self.render_window.as_ref().unwrap().scale_factor() as f32;
        (
            x - self.scene_rect.min.x * pp,
            y - self.scene_rect.min.y * pp,
        )
    }

    /// 场景局部坐标（供投影/拾取使用）。
    fn scene_size_f32(&self) -> (f32, f32) {
        let pp = self.render_window.as_ref().unwrap().scale_factor() as f32;
        (
            (self.scene_rect.width() * pp).max(1.0),
            (self.scene_rect.height() * pp).max(1.0),
        )
    }

    fn build_scene_uniform(&self) -> SceneUniform {
        let (w, h) = self.scene_size_f32();
        let aspect = w / h;
        let view_proj = self.camera.projection_matrix(aspect) * self.camera.view_matrix();

        // Camera position from orbit camera
        let eye = glam::Vec3::new(
            self.camera.radius * self.camera.phi.cos() * self.camera.theta.cos(),
            self.camera.radius * self.camera.phi.sin(),
            self.camera.radius * self.camera.phi.cos() * self.camera.theta.sin(),
        ) + self.camera.target;

        let light0 = &self.ui_state.light0;
        let light1 = &self.ui_state.light1;
        let light2 = &self.ui_state.light2;

        SceneUniform {
            view_proj: view_proj.to_cols_array_2d(),
            camera_position: [
                eye.x,
                eye.y,
                eye.z,
                if self.ui_state.use_per_patch_shader {
                    PER_PATCH_SHADER_FLAG
                } else {
                    self.ui_state.mesh_shader.to_f32()
                },
            ],
            light0_dir: [
                light0.dir_x,
                light0.dir_y,
                light0.dir_z,
                if light0.enabled {
                    light0.intensity
                } else {
                    0.0
                },
            ],
            light0_color: [light0.color[0], light0.color[1], light0.color[2], 0.0],
            light1_dir: [
                light1.dir_x,
                light1.dir_y,
                light1.dir_z,
                if light1.enabled {
                    light1.intensity
                } else {
                    0.0
                },
            ],
            light1_color: [light1.color[0], light1.color[1], light1.color[2], 0.0],
            light2_dir: [
                light2.dir_x,
                light2.dir_y,
                light2.dir_z,
                if light2.enabled {
                    light2.intensity
                } else {
                    0.0
                },
            ],
            light2_color: [light2.color[0], light2.color[1], light2.color[2], 0.0],
            ambient_color: [
                self.ui_state.ambient_color[0],
                self.ui_state.ambient_color[1],
                self.ui_state.ambient_color[2],
                self.ui_state.ambient_intensity,
            ],
            bg_color: [
                self.ui_state.bg_color[0],
                self.ui_state.bg_color[1],
                self.ui_state.bg_color[2],
                self.ui_state.ao_strength,
            ],
            shader_params: [
                self.ui_state.shader_roughness,
                self.ui_state.shader_metallic,
                self.ui_state.shader_specular,
                self.ui_state.shader_shininess,
            ],
        }
    }

    fn draw_edge_pass(
        encoder: &mut wgpu::CommandEncoder,
        label: &str,
        msaa_view: &wgpu::TextureView,
        depth_view: &wgpu::TextureView,
        edge: &ColoredEdgeData,
        scene_viewport: (u32, u32, u32, u32),
    ) {
        let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some(label),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: msaa_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            occlusion_query_set: None,
            timestamp_writes: None,
        });
        let (sx, sy, sw, sh) = scene_viewport;
        rp.set_viewport(sx as f32, sy as f32, sw as f32, sh as f32, 0.0, 1.0);
        rp.set_scissor_rect(sx, sy, sw, sh);
        rp.set_pipeline(&edge.pipeline);
        rp.set_bind_group(0, &edge.uniform_bind_group, &[]);
        rp.set_vertex_buffer(0, edge.vertex_buffer.slice(..));
        rp.draw(0..edge.num_vertices, 0..1);
    }

    fn update(&mut self) {
        let now = std::time::Instant::now();
        let dt = (now - self.last_time).as_secs_f32();
        self.last_time = now;
        self.camera.update_keyboard_motion(dt);
    }

    pub fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        self.update();

        // 镜像后台构建状态到 UI 状态栏，并轮询 worker 线程（不阻塞主线程）
        self.ui_state.is_building = self.is_building;
        self.ui_state.build_status = self.build_status.clone();
        self.ui_state.build_progress = self.build_progress;
        self.poll_build_worker();
        self.poll_dialog();

        // ---- Build egui UI on UI window ----
        let mut needs_rebuild = false;
        let mut needs_reapply_cuts = false;
        let mut needs_recolor = false;
        let mut needs_export_obj = false;
        let mut needs_export_obj_by_patch = false;
        let mut needs_open_obj = false;
        let mut needs_randomize_topology = false;
        let mut needs_refresh_display = false;
        let mut needs_toggle_view = false;
        let mut needs_export_obj_dialog = false;
        let mut needs_export_patch_dialog = false;
        let mut needs_apply_patch_shaders = false;

        let raw_input = self
            .egui_winit_state
            .as_mut()
            .unwrap()
            .take_egui_input(self.render_window.as_ref().unwrap());
        // 网格统计已缓存（仅在网格变化时重算），避免每帧 compute_stats 占用主线程。
        // 后台构建/对话框打开期间不显示统计（网格尚未就绪）。
        let mesh_stats = if self.is_building {
            None
        } else {
            self.cached_stats
        };
        let full_output = self.egui_ctx.run(raw_input, |ctx| {
            let action = panel::render_ui_panel(
                ctx,
                &mut self.ui_state,
                mesh_stats.as_ref(),
                &mut self.scene_rect,
            );
            if action.rebuild {
                needs_rebuild = true;
            }
            if action.reapply_cuts {
                needs_reapply_cuts = true;
            }
            if action.recolor {
                needs_recolor = true;
            }
            if action.export_obj {
                needs_export_obj = true;
            }
            if action.export_obj_by_patch {
                needs_export_obj_by_patch = true;
            }
            if action.open_obj_dialog {
                needs_open_obj = true;
            }
            if action.randomize_topology {
                needs_randomize_topology = true;
            }
            if action.refresh_display {
                needs_refresh_display = true;
            }
            if action.toggle_view {
                needs_toggle_view = true;
            }
            if action.export_obj_dialog {
                needs_export_obj_dialog = true;
            }
            if action.export_patch_dialog {
                needs_export_patch_dialog = true;
            }
            if action.apply_patch_shaders {
                needs_apply_patch_shaders = true;
            }
        });

        // Handle deferred actions
        if needs_open_obj && self.dialog_rx.is_none() {
            // 文件对话框放到后台线程打开：rfd 模态对话框会接管消息循环并阻塞父窗口，
            // 若在主线程同步调用则表现为"未响应"。线程返回结果后由 poll_dialog 触发构建。
            let (tx, rx) = std::sync::mpsc::channel::<Option<Vec<String>>>();
            self.dialog_rx = Some(rx);
            self.is_building = true;
            self.build_status = "Selecting OBJ files…".to_string();
            self.build_progress = 0.0;
            spawn_obj_dialog(tx);
        }
        if needs_rebuild {
            // OBJ 多选导入较重，放到后台 worker 线程，避免主线程卡死（"未响应"）。
            // 其余网格类型（Quad/Delaunay）构建很快，仍同步执行。
            match &self.ui_state.mesh_type {
                panel::MeshType::ObjFile(_) if !self.is_building => self.spawn_objfile_build(),
                _ => {
                    self.build_torus_mesh();
                    self.reapply_cuts();
                    self.rebuild_render_state();
                }
            }
        }
        if needs_reapply_cuts {
            self.reapply_cuts();
            self.rebuild_render_state();
        }
        if needs_recolor {
            self.recolor_patches();
        }
        if needs_randomize_topology {
            if let Some(ref mesh) = self.torus_mesh {
                let randomized_mesh =
                    crate::mesh::delaunay::randomize_mesh_by_edge_flips(mesh, 3, 42);
                self.torus_mesh = Some(randomized_mesh.clone());
                self.base_mesh = Some(randomized_mesh);
                self.reapply_cuts();
                self.rebuild_render_state();
            }
        }
        if needs_refresh_display {
            self.rebuild_render_state();
        }
        if needs_apply_patch_shaders {
            self.rebuild_render_state();
        }
        if needs_toggle_view {
            if self.ui_state.view_mode == ViewMode::Unfolded {
                let (major_r, minor_r) = self
                    .ui_state
                    .surface_model
                    .radii(self.ui_state.major_radius, self.ui_state.minor_radius);
                let center_x = (std::f64::consts::PI * major_r) as f32;
                let center_y = (std::f64::consts::PI * minor_r) as f32;
                self.camera.target = glam::Vec3::new(center_x, center_y, 0.0);
                self.camera.theta = -std::f32::consts::FRAC_PI_4;
                self.camera.phi = std::f32::consts::FRAC_PI_4;
                self.camera.radius = (center_x.max(center_y) * 2.5).max(8.0);
            } else {
                self.camera.target = glam::Vec3::ZERO;
                self.camera.theta = -std::f32::consts::FRAC_PI_4;
                self.camera.phi = std::f32::consts::FRAC_PI_6;
                self.camera.radius = 8.0;
            }
            self.rebuild_render_state();
        }

        if needs_export_obj {
            if let Some(ref mesh) = self.torus_mesh {
                let path = self.ui_state.export_path_obj.clone();
                let fmt = self.ui_state.export_format;
                // 导出与当前视图坐标一致（所见即所得）：UV 平面视图导出展开坐标，
                // 3D 视图导出环面坐标
                let view_mesh = self.to_view_mesh(mesh);
                if let Err(e) = crate::export::export_mesh(&view_mesh, &path, fmt) {
                    log::error!("{} export failed: {}", fmt, e);
                }
            }
        }
        if needs_export_obj_by_patch {
            if let Some(ref mesh) = self.torus_mesh {
                let dir = self.ui_state.export_dir_patches.clone();
                let fmt = self.ui_state.export_format;
                let view_mesh = self.to_view_mesh(mesh);
                if let Err(e) = crate::export::export_by_patch(&view_mesh, &dir, fmt) {
                    log::error!("Patch {} export failed: {}", fmt, e);
                }
            }
        }
        if needs_export_obj_dialog {
            let fmt = self.ui_state.export_format;
            let file = rfd::FileDialog::new()
                .add_filter("Mesh Files", &[fmt.extension()])
                .set_file_name(format!("torus_cut.{}", fmt.extension()))
                .set_title("Save Mesh File")
                .save_file();
            if let Some(path) = file {
                self.ui_state.export_path_obj = path.to_string_lossy().to_string();
            }
        }
        if needs_export_patch_dialog {
            let folder = rfd::FileDialog::new()
                .set_title("Select Export Directory")
                .pick_folder();
            if let Some(path) = folder {
                self.ui_state.export_dir_patches = path.to_string_lossy().to_string();
            }
        }
        let device = self.device.as_ref().unwrap();
        let queue = self.queue.as_ref().unwrap();
        let render_state = self.render_state.as_ref().unwrap();

        // Build scene uniform before borrowing egui_renderer mutably
        let scene_uniform = self.build_scene_uniform();
        let scene_vp = self.scene_viewport();

        let egui_renderer = self.egui_renderer.as_mut().unwrap();

        // Update egui textures
        for (id, image_delta) in full_output.textures_delta.set {
            egui_renderer.update_texture(device, queue, id, &image_delta);
        }
        for id in full_output.textures_delta.free {
            egui_renderer.free_texture(&id);
        }

        // Tessellate egui
        let ui_screen_desc = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [self.render_size.width, self.render_size.height],
            pixels_per_point: self.render_window.as_ref().unwrap().scale_factor() as f32,
        };
        let clipped_primitives = self
            .egui_ctx
            .tessellate(full_output.shapes, full_output.pixels_per_point);

        // Update egui buffers
        let mut buf_encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("egui buffer update"),
        });
        let cmd_buffers = egui_renderer.update_buffers(
            device,
            queue,
            &mut buf_encoder,
            &clipped_primitives,
            &ui_screen_desc,
        );
        queue.submit(std::iter::once(buf_encoder.finish()).chain(cmd_buffers));

        // ---- Update scene uniform ----
        queue.write_buffer(
            &render_state.uniform_buffer,
            0,
            bytemuck::cast_slice(&[scene_uniform]),
        );

        // ==== Render 3D scene to render window ====
        {
            let output = self
                .render_surface
                .as_ref()
                .unwrap()
                .get_current_texture()?;
            let view = output
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default());

            let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            });

            let bg = &self.ui_state.bg_color;
            // Mesh pass（渲染到 4x MSAA 纹理；线框 pass 结束时统一 resolve 到窗口）
            {
                let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Mesh Pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &render_state.msaa_view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color {
                                r: bg[0] as f64,
                                g: bg[1] as f64,
                                b: bg[2] as f64,
                                a: 1.0,
                            }),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &render_state.depth_view,
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Clear(1.0),
                            store: wgpu::StoreOp::Store,
                        }),
                        stencil_ops: None,
                    }),
                    occlusion_query_set: None,
                    timestamp_writes: None,
                });
                // 只渲染到中央 3D 视口区域（左右面板不被场景覆盖）
                let (sx, sy, sw, sh) = scene_vp;
                rp.set_viewport(sx as f32, sy as f32, sw as f32, sh as f32, 0.0, 1.0);
                rp.set_scissor_rect(sx, sy, sw, sh);
                rp.set_pipeline(&render_state.mesh_pipeline);
                rp.set_bind_group(0, &render_state.uniform_bind_group, &[]);
                rp.set_vertex_buffer(0, render_state.vertex_buffer.slice(..));
                rp.set_index_buffer(
                    render_state.index_buffer.slice(..),
                    wgpu::IndexFormat::Uint32,
                );
                rp.draw_indexed(0..render_state.num_indices, 0, 0..1);
            }

            if self.ui_state.show_mesh_edges {
                if let Some(ref me) = render_state.mesh_edges {
                    Self::draw_edge_pass(
                        &mut encoder,
                        "Mesh Edges",
                        &render_state.msaa_view,
                        &render_state.depth_view,
                        me,
                        scene_vp,
                    );
                }
            }
            if self.ui_state.show_patch_edges {
                if let Some(ref pe) = render_state.patch_edges {
                    Self::draw_edge_pass(
                        &mut encoder,
                        "Patch Edges",
                        &render_state.msaa_view,
                        &render_state.depth_view,
                        pe,
                        scene_vp,
                    );
                }
            }

            // 空 pass：把 MSAA 纹理 resolve 到窗口 surface（无论是否有线框 pass）
            {
                encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("MSAA Resolve"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &render_state.msaa_view,
                        resolve_target: Some(&view),
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    occlusion_query_set: None,
                    timestamp_writes: None,
                });
            }

            // egui pass：叠加到同一 surface（不清屏，浮于 3D 场景之上）
            {
                let rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("egui Overlay Pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    occlusion_query_set: None,
                    timestamp_writes: None,
                });
                egui_renderer.render(
                    &mut rp.forget_lifetime(),
                    &clipped_primitives,
                    &ui_screen_desc,
                );
            }

            queue.submit(std::iter::once(encoder.finish()));
            output.present();
        }

        Ok(())
    }

    fn handle_mouse_button(&mut self, button: winit::event::MouseButton, pressed: bool) {
        match button {
            winit::event::MouseButton::Left => {
                // egui 已停靠到渲染窗口：面板区域点击被 egui 消费（上层
                // window_event 已过滤 egui_consumed），此处只处理场景区点击。
                if pressed {
                    self.camera.stop_pan();
                    self.camera
                        .start_drag(self.camera.last_mouse.0, self.camera.last_mouse.1);
                    if let Some(w) = &self.render_window {
                        let _ = w.set_cursor_grab(winit::window::CursorGrabMode::Confined);
                    }
                } else {
                    self.camera.stop_drag();
                    if let Some(w) = &self.render_window {
                        let _ = w.set_cursor_grab(winit::window::CursorGrabMode::None);
                    }
                }
            }
            winit::event::MouseButton::Middle | winit::event::MouseButton::Right => {
                if pressed {
                    self.camera.stop_drag();
                    self.camera
                        .start_pan(self.camera.last_mouse.0, self.camera.last_mouse.1);
                    if let Some(w) = &self.render_window {
                        let _ = w.set_cursor_grab(winit::window::CursorGrabMode::Confined);
                    }
                } else {
                    self.camera.stop_pan();
                    if let Some(w) = &self.render_window {
                        let _ = w.set_cursor_grab(winit::window::CursorGrabMode::None);
                    }
                }
            }
            _ => {}
        }
    }

    fn handle_cursor_move(&mut self, x: f32, y: f32) {
        let (sx, sy) = self.scene_local(x, y);
        self.camera.handle_mouse_drag(sx, sy);
    }

    fn handle_scroll(&mut self, delta: f32) {
        // egui 消费判断已由上层 window_event 按窗口完成（!egui_consumed && is_render_window），
        // 此处不能再查 egui_ctx 指针状态（它反映 UI 窗口，会误吞 render 窗口滚轮）
        self.camera.handle_scroll(delta);
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.render_window.is_none() {
            // Create render window
            let r_attrs = WindowAttributes::default()
                .with_title("3D Renderer — Mesh Viewer")
                .with_inner_size(winit::dpi::LogicalSize::new(1280u32, 720u32))
                .with_visible(false);
            let render_window = Arc::new(event_loop.create_window(r_attrs).unwrap());

            self.render_window_id = Some(render_window.id());

            let (device, queue, r_surface, r_config) =
                pollster::block_on(Self::init_gpu(&render_window));

            let r_size = render_window.inner_size();

            // Create shaders
            let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Mesh Shader"),
                source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/mesh.wgsl").into()),
            });
            let wf_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Wireframe Shader"),
                source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/wireframe.wgsl").into()),
            });

            // Create egui renderer（与 3D 场景共用同一 surface format）
            let egui_renderer = egui_wgpu::Renderer::new(&device, r_config.format, None, 1, false);

            self.device = Some(device);
            self.queue = Some(queue);
            self.render_surface = Some(r_surface);
            self.render_surface_config = Some(r_config);
            self.render_size = r_size;
            self.shader = Some(shader);
            self.wireframe_shader = Some(wf_shader);
            self.egui_renderer = Some(egui_renderer);
            self.render_window = Some(render_window.clone());

            // egui 停靠在渲染窗口内：事件状态绑定渲染窗口
            self.egui_winit_state = Some(egui_winit::State::new(
                self.egui_ctx.clone(),
                self.egui_ctx.viewport_id(),
                render_window.as_ref(),
                None,
                None,
                None,
            ));

            self.build_torus_mesh();

            // 初始 tab 是 Mesh：直接以平面视图初始化（避免先按 3D 重建一次再重建）
            if self.ui_state.active_tab == panel::PanelTab::Mesh {
                self.ui_state.view_mode = ViewMode::Unfolded;
                let (major_r, minor_r) = self
                    .ui_state
                    .surface_model
                    .radii(self.ui_state.major_radius, self.ui_state.minor_radius);
                let center_x = (std::f64::consts::PI * major_r) as f32;
                let center_y = (std::f64::consts::PI * minor_r) as f32;
                self.camera.target = glam::Vec3::new(center_x, center_y, 0.0);
                self.camera.theta = -std::f32::consts::FRAC_PI_4;
                self.camera.phi = std::f32::consts::FRAC_PI_4;
                self.camera.radius = (center_x.max(center_y) * 2.5).max(8.0);
            }
            self.rebuild_render_state();

            render_window.set_visible(true);
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let is_render_window = Some(window_id) == self.render_window_id;

        // egui 停靠在渲染窗口内：所有事件先喂给 egui，消费后不再驱动 3D 相机
        let egui_consumed = if is_render_window {
            self.egui_winit_state
                .as_mut()
                .unwrap()
                .on_window_event(self.render_window.as_ref().unwrap(), &event)
                .consumed
        } else {
            false
        };

        match event {
            WindowEvent::RedrawRequested if is_render_window && !self.rendered_this_frame => {
                self.rendered_this_frame = true;
                let _ = self.render();
            }
            WindowEvent::Resized(size) if is_render_window => {
                self.resize_surface(size);
            }
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::CursorMoved { position, .. } if is_render_window => {
                self.last_pointer = Some((position.x as f32, position.y as f32));
                // 指针在中央 3D 视口内才更新相机坐标（三栏布局下 egui 面板
                // 覆盖全窗口，is_pointer_over_area 恒 true，必须用几何判定）；
                // 拖动/平移进行中例外——拖过面板时继续旋转，避免视角卡顿
                if self.pointer_in_scene() || self.camera.dragging || self.camera.panning {
                    self.handle_cursor_move(position.x as f32, position.y as f32);
                }
            }
            WindowEvent::MouseInput { button, state, .. }
                if is_render_window
                    // 场景区（中央视口内）的按下/松开直接进入相机/拾取流程——
                    // 不再依赖 egui_consumed（egui 对空区域点击的 consumed 语义
                    // 不可靠）；面板控件区域由 pointer_in_scene 几何判定排除。
                    // 松开时若正在拖动则必须放行以复位状态。
                    && (self.pointer_in_scene()
                        || (state == winit::event::ElementState::Released
                            && (self.camera.dragging || self.camera.panning))) =>
            {
                self.handle_mouse_button(button, state.is_pressed());
            }
            WindowEvent::MouseWheel { delta, .. }
                // 与左键同理：不依赖 egui_consumed（egui 对滚轮的 consumed
                // 语义不可靠）；面板内滚动由 pointer_in_scene 排除——
                // 面板 ScrollArea 的滚动由 egui 自行处理，互不干扰。
                if is_render_window && self.pointer_in_scene() =>
            {
                let dy = match delta {
                    winit::event::MouseScrollDelta::LineDelta(_, y) => y,
                    winit::event::MouseScrollDelta::PixelDelta(pos) => pos.y as f32 * 0.1,
                };
                self.handle_scroll(dy);
            }
            WindowEvent::KeyboardInput {
                event:
                    winit::event::KeyEvent {
                        physical_key: PhysicalKey::Code(keycode),
                        state,
                        ..
                    },
                ..
            } if is_render_window && !egui_consumed => {
                // 相机键盘控制只响应 render 窗口且未被 egui 消费
                self.camera.handle_key(keycode, state.is_pressed());
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        self.rendered_this_frame = false;
        if let Some(w) = &self.render_window {
            w.request_redraw();
        }
    }
}
