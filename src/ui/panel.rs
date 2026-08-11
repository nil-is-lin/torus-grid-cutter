use crate::color_scheme::{self, ColorScheme};
use std::collections::HashMap;

// ============================================================
// Shader Mode — 42 professional rendering modes
// ============================================================
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(clippy::upper_case_acronyms)] // PBR/SSS 等为公认缩写
pub enum ShaderMode {
    PBR,
    BlinnPhong,
    Clay,
    Matcap,
    NormalVis,
    Toon,
    RimLight,
    WireframeSolid,
    XRay,
    FresnelGlow,
    Iridescence,
    Hatching,
    Stippling,
    CheckerUV,
    CurvatureHeat,
    AO,
    SSS,
    ClearCoat,
    Anisotropic,
    CelShading,
    Gooch,
    HemiLight,
    Contour,
    FlatShading,
    Glass,
    Chrome,
    Plastic,
    Metallic,
    Velvet,
    Holographic,
    Ghost,
    NeonGlow,
    Crystal,
    Liquid,
    Fire,
    ForceField,
    Blueprint,
    DepthVis,
    Rainbow,
    Frost,
    Plasma,
    Sketch,
}

impl ShaderMode {
    pub const ALL: &'static [ShaderMode] = &[
        ShaderMode::PBR,
        ShaderMode::BlinnPhong,
        ShaderMode::Clay,
        ShaderMode::Matcap,
        ShaderMode::NormalVis,
        ShaderMode::Toon,
        ShaderMode::RimLight,
        ShaderMode::WireframeSolid,
        ShaderMode::XRay,
        ShaderMode::FresnelGlow,
        ShaderMode::Iridescence,
        ShaderMode::Hatching,
        ShaderMode::Stippling,
        ShaderMode::CheckerUV,
        ShaderMode::CurvatureHeat,
        ShaderMode::AO,
        ShaderMode::SSS,
        ShaderMode::ClearCoat,
        ShaderMode::Anisotropic,
        ShaderMode::CelShading,
        ShaderMode::Gooch,
        ShaderMode::HemiLight,
        ShaderMode::Contour,
        ShaderMode::FlatShading,
        ShaderMode::Glass,
        ShaderMode::Chrome,
        ShaderMode::Plastic,
        ShaderMode::Metallic,
        ShaderMode::Velvet,
        ShaderMode::Holographic,
        ShaderMode::Ghost,
        ShaderMode::NeonGlow,
        ShaderMode::Crystal,
        ShaderMode::Liquid,
        ShaderMode::Fire,
        ShaderMode::ForceField,
        ShaderMode::Blueprint,
        ShaderMode::DepthVis,
        ShaderMode::Rainbow,
        ShaderMode::Frost,
        ShaderMode::Plasma,
        ShaderMode::Sketch,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            ShaderMode::PBR => "PBR Metallic-Roughness",
            ShaderMode::BlinnPhong => "Enhanced Blinn-Phong",
            ShaderMode::Clay => "Clay / Studio",
            ShaderMode::Matcap => "Matcap Style",
            ShaderMode::NormalVis => "Normal Visualization",
            ShaderMode::Toon => "Toon Shading",
            ShaderMode::RimLight => "Rim Lighting",
            ShaderMode::WireframeSolid => "Wireframe Overlay",
            ShaderMode::XRay => "X-Ray (Enhanced)",
            ShaderMode::FresnelGlow => "Fresnel Glow",
            ShaderMode::Iridescence => "Iridescence",
            ShaderMode::Hatching => "Hatching",
            ShaderMode::Stippling => "Stippling",
            ShaderMode::CheckerUV => "Checkerboard UV",
            ShaderMode::CurvatureHeat => "Curvature Heatmap",
            ShaderMode::AO => "AO Approximation",
            ShaderMode::SSS => "Subsurface Scattering",
            ShaderMode::ClearCoat => "Clear Coat",
            ShaderMode::Anisotropic => "Anisotropic",
            ShaderMode::CelShading => "Cel-Shading (4 Tones)",
            ShaderMode::Gooch => "Gooch (Warm-Cool)",
            ShaderMode::HemiLight => "Hemispheric Lighting",
            ShaderMode::Contour => "Contour / Silhouette",
            ShaderMode::FlatShading => "Flat Shading",
            ShaderMode::Glass => "Glass (Enhanced)",
            ShaderMode::Chrome => "Chrome / Mirror",
            ShaderMode::Plastic => "Plastic",
            ShaderMode::Metallic => "Metallic",
            ShaderMode::Velvet => "Velvet",
            ShaderMode::Holographic => "Holographic",
            ShaderMode::Ghost => "Ghost Transparency",
            ShaderMode::NeonGlow => "Neon Glow",
            ShaderMode::Crystal => "Crystal / Diamond",
            ShaderMode::Liquid => "Liquid / Water",
            ShaderMode::Fire => "Fire / Lava",
            ShaderMode::ForceField => "Force Field",
            ShaderMode::Blueprint => "Blueprint",
            ShaderMode::DepthVis => "Depth Visualization",
            ShaderMode::Rainbow => "Rainbow Spectrum",
            ShaderMode::Frost => "Frost / Ice",
            ShaderMode::Plasma => "Plasma / Energy",
            ShaderMode::Sketch => "Sketch / Pencil",
        }
    }

    pub fn to_f32(self) -> f32 {
        Self::ALL.iter().position(|s| *s == self).unwrap_or(0) as f32
    }

    /// Category label for grouped display
    pub fn category(&self) -> &'static str {
        match self {
            ShaderMode::PBR
            | ShaderMode::BlinnPhong
            | ShaderMode::Clay
            | ShaderMode::Plastic
            | ShaderMode::Metallic
            | ShaderMode::ClearCoat
            | ShaderMode::Anisotropic
            | ShaderMode::Velvet => "Realistic",
            ShaderMode::Matcap
            | ShaderMode::NormalVis
            | ShaderMode::CheckerUV
            | ShaderMode::CurvatureHeat
            | ShaderMode::AO
            | ShaderMode::DepthVis
            | ShaderMode::FlatShading => "Analysis",
            ShaderMode::Toon
            | ShaderMode::RimLight
            | ShaderMode::CelShading
            | ShaderMode::Gooch
            | ShaderMode::HemiLight
            | ShaderMode::Contour
            | ShaderMode::Sketch => "Stylized",
            ShaderMode::XRay
            | ShaderMode::Glass
            | ShaderMode::Ghost
            | ShaderMode::Frost
            | ShaderMode::Liquid => "Transparent",
            ShaderMode::FresnelGlow
            | ShaderMode::Iridescence
            | ShaderMode::Holographic
            | ShaderMode::NeonGlow
            | ShaderMode::ForceField
            | ShaderMode::Plasma
            | ShaderMode::Rainbow => "Effects",
            ShaderMode::WireframeSolid
            | ShaderMode::Hatching
            | ShaderMode::Stippling
            | ShaderMode::Blueprint => "Technical",
            ShaderMode::SSS | ShaderMode::Chrome | ShaderMode::Crystal | ShaderMode::Fire => {
                "Special"
            }
        }
    }
}

// ============================================================
// Material Type
// ============================================================
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaterialType {
    Default,
    Plastic,
    Glass,
    Transparent,
    Steel,
}

impl MaterialType {
    pub fn all() -> &'static [MaterialType] {
        &[
            MaterialType::Default,
            MaterialType::Plastic,
            MaterialType::Glass,
            MaterialType::Transparent,
            MaterialType::Steel,
        ]
    }
    pub fn to_f32(self) -> f32 {
        match self {
            MaterialType::Default => 0.0,
            MaterialType::Plastic => 1.0,
            MaterialType::Glass => 2.0,
            MaterialType::Transparent => 3.0,
            MaterialType::Steel => 4.0,
        }
    }
}
impl std::fmt::Display for MaterialType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MaterialType::Default => write!(f, "Default"),
            MaterialType::Plastic => write!(f, "Plastic"),
            MaterialType::Glass => write!(f, "Glass"),
            MaterialType::Transparent => write!(f, "Transparent"),
            MaterialType::Steel => write!(f, "Steel"),
        }
    }
}

// ============================================================
// Light Settings
// ============================================================
#[derive(Debug, Clone, Copy)]
pub struct LightSettings {
    pub enabled: bool,
    pub dir_x: f32,
    pub dir_y: f32,
    pub dir_z: f32,
    pub color: [f32; 3],
    pub intensity: f32,
}

impl Default for LightSettings {
    fn default() -> Self {
        LightSettings {
            enabled: true,
            dir_x: -0.577,
            dir_y: -0.577,
            dir_z: -0.577,
            color: [1.0, 0.98, 0.95],
            intensity: 1.0,
        }
    }
}

// ============================================================
// Other Enums
// ============================================================

#[derive(Debug, Clone, PartialEq)]
pub enum MeshType {
    Quad,
    Delaunay,
    ObjFile(Vec<String>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorMode {
    Solid,
    ByRegion,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CutDirection {
    U,
    V,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CutMode {
    Grid,
    Knot,
}
impl std::fmt::Display for CutMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CutMode::Grid => write!(f, "Grid (U/V)"),
            CutMode::Knot => write!(f, "Torus Knot"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewMode {
    Torus3D,
    Unfolded,
}

#[derive(Debug, Clone)]
pub struct CutLoop {
    pub direction: CutDirection,
    pub index: usize,
    pub active: bool,
    pub cut: bool,
    pub color: [f32; 4],
}

impl CutLoop {
    pub fn new(direction: CutDirection, index: usize) -> Self {
        let hue = match direction {
            CutDirection::U => index as f32 * 0.618_034,
            CutDirection::V => index as f32 * 0.381_966_02 + 0.3,
        };
        CutLoop {
            direction,
            index,
            active: false,
            cut: false,
            color: color_scheme::hsl_to_rgb(hue % 1.0, 0.9, 0.55),
        }
    }
    pub fn label(&self) -> String {
        match self.direction {
            CutDirection::U => format!("U-Loop {}", self.index + 1),
            CutDirection::V => format!("V-Loop {}", self.index + 1),
        }
    }
}

// ============================================================
// Tabs — workflow-oriented steps
// ============================================================
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelTab {
    Mesh,
    Cut,
    Shader,
    Export,
}

impl PanelTab {
    pub fn all() -> &'static [PanelTab] {
        &[
            PanelTab::Mesh,
            PanelTab::Cut,
            PanelTab::Shader,
            PanelTab::Export,
        ]
    }
    pub fn icon_label(&self) -> &'static str {
        match self {
            PanelTab::Mesh => "1.Mesh",
            PanelTab::Cut => "2.Cut",
            PanelTab::Shader => "3.Shader",
            PanelTab::Export => "4.Export",
        }
    }
}

// ============================================================
// UI State
// ============================================================
pub struct UiState {
    // Mesh params
    pub major_radius: f64,
    pub minor_radius: f64,
    pub resolution_u: usize,
    pub resolution_v: usize,
    pub num_u_loops: usize,
    pub num_v_loops: usize,
    pub cut_mode: CutMode,
    pub knot_k1: usize,
    pub knot_k2: usize,
    pub knot_show_1: bool,
    pub knot_show_2: bool,
    pub knot_cut_1: bool,
    pub knot_cut_2: bool,
    pub cut_loops: Vec<CutLoop>,
    pub patch_visible: Vec<bool>,
    pub color_scheme: ColorScheme,
    pub mesh_type: MeshType,
    pub delaunay_points: usize,
    pub color_mode: ColorMode,
    pub base_color: [f32; 4],
    pub smooth_shading: bool,
    pub view_mode: ViewMode,

    // Display
    pub show_mesh_edges: bool,
    pub show_patch_edges: bool,
    pub mesh_edge_width: f32,
    pub loop_line_width: f32,
    pub bg_color: [f32; 3],

    // Shader
    pub mesh_shader: ShaderMode,
    pub shader_roughness: f32,
    pub shader_metallic: f32,
    pub shader_specular: f32,
    pub shader_shininess: f32,

    // Lighting
    pub ambient_intensity: f32,
    pub ambient_color: [f32; 3],
    pub light0: LightSettings,
    pub light1: LightSettings,
    pub light2: LightSettings,
    pub ao_strength: f32,

    // Selection

    // Materials
    pub patch_materials: HashMap<(usize, usize), MaterialType>,
    pub default_material: MaterialType,

    // Per-patch shader
    pub patch_shaders: HashMap<(usize, usize), ShaderMode>,
    pub use_per_patch_shader: bool,

    // Export
    pub export_path_obj: String,
    pub export_dir_patches: String,
    pub export_format: crate::export::ExportFormat,

    // UI state
    pub show_properties: bool,
    pub active_tab: PanelTab,
    pub uv_range: (f64, f64, f64, f64),
    pub surface_model: crate::mesh::surface::SurfaceModel,

    // 后台构建（OBJ 导入）状态——由 App 每帧镜像，状态栏常驻显示
    pub is_building: bool,
    pub build_status: String,
    pub build_progress: f32,
}

impl Default for UiState {
    fn default() -> Self {
        let num_u_loops = 4;
        let num_v_loops = 6;
        UiState {
            major_radius: 2.0,
            minor_radius: 0.6,
            resolution_u: 40,
            resolution_v: 24,
            num_u_loops,
            num_v_loops,
            cut_mode: CutMode::Grid,
            knot_k1: 2,
            knot_k2: 3,
            knot_show_1: true,
            knot_show_2: true,
            knot_cut_1: false,
            knot_cut_2: false,
            cut_loops: Self::generate_loops(num_u_loops, num_v_loops),
            patch_visible: Vec::new(),
            color_scheme: ColorScheme::Rainbow,
            mesh_type: MeshType::Quad,
            delaunay_points: 500,
            color_mode: ColorMode::Solid,
            base_color: [0.2706, 0.3529, 1.0, 1.0],
            smooth_shading: false,
            view_mode: ViewMode::Torus3D,
            show_mesh_edges: true,
            show_patch_edges: true,
            mesh_edge_width: 1.0,
            loop_line_width: 3.0,
            bg_color: [1.0, 1.0, 1.0],
            mesh_shader: ShaderMode::PBR,
            shader_roughness: 0.5,
            shader_metallic: 0.0,
            shader_specular: 0.5,
            shader_shininess: 32.0,
            ambient_intensity: 0.25,
            ambient_color: [0.95, 0.95, 1.0],
            light0: LightSettings {
                enabled: true,
                dir_x: -0.577,
                dir_y: -0.577,
                dir_z: -0.577,
                color: [1.0, 0.98, 0.95],
                intensity: 1.0,
            },
            light1: LightSettings {
                enabled: true,
                dir_x: 0.577,
                dir_y: -0.289,
                dir_z: 0.577,
                color: [0.85, 0.9, 1.0],
                intensity: 0.6,
            },
            light2: LightSettings {
                enabled: true,
                dir_x: 0.0,
                dir_y: 0.577,
                dir_z: -0.577,
                color: [1.0, 0.95, 0.9],
                intensity: 0.4,
            },
            ao_strength: 0.5,
            patch_materials: HashMap::new(),
            default_material: MaterialType::Default,
            patch_shaders: HashMap::new(),
            use_per_patch_shader: false,
            export_path_obj: "models/torus_cut.obj".to_string(),
            export_format: crate::export::ExportFormat::Obj,
            export_dir_patches: "models/patches".to_string(),
            show_properties: true,
            active_tab: PanelTab::Mesh,
            uv_range: (
                0.0,
                2.0 * std::f64::consts::PI,
                0.0,
                2.0 * std::f64::consts::PI,
            ),
            surface_model: crate::mesh::surface::SurfaceModel::torus_from_params(2.0, 0.6),
            is_building: false,
            build_status: "Ready".to_string(),
            build_progress: 0.0,
        }
    }
}

impl UiState {
    pub fn generate_loops(num_u: usize, num_v: usize) -> Vec<CutLoop> {
        let mut loops = Vec::new();
        for i in 0..num_u {
            loops.push(CutLoop::new(CutDirection::U, i));
        }
        for i in 0..num_v {
            loops.push(CutLoop::new(CutDirection::V, i));
        }
        loops
    }
    /// U-loop 的 UV 位置——**渲染与切割共用**（保证显示线与实际切割边一致）。
    /// Quad 网格：沿网格顶点线均匀分布（n 条线把 u 域等分成 n+1 段，
    /// 吸附到最近的网格线 → 沿面边界走、不穿过面内部 → 保持四边形面型）；
    /// Delaunay/OBJ：取单元中心等分（穿过面内部，切割本身三角化）。
    pub fn loop_u_position(&self, index: usize) -> f64 {
        let (min_u, max_u, _, _) = self.uv_range;
        let range = max_u - min_u;
        if matches!(self.mesh_type, MeshType::Quad) {
            let n = self.num_u_loops.max(1) as f64;
            let res = self.resolution_u.max(2) as f64;
            let ideal = min_u + range / (n + 1.0) * (index as f64 + 1.0);
            let k = ((ideal - min_u) / range * res)
                .round()
                .clamp(1.0, res - 1.0) as usize;
            min_u + range / res * k as f64
        } else {
            min_u + range / self.num_u_loops.max(1) as f64 * (index as f64 + 0.5)
        }
    }

    /// V-loop 的 UV 位置（与 loop_u_position 同理）。
    pub fn loop_v_position(&self, index: usize) -> f64 {
        let (_, _, min_v, max_v) = self.uv_range;
        let range = max_v - min_v;
        if matches!(self.mesh_type, MeshType::Quad) {
            let n = self.num_v_loops.max(1) as f64;
            let res = self.resolution_v.max(2) as f64;
            let ideal = min_v + range / (n + 1.0) * (index as f64 + 1.0);
            let k = ((ideal - min_v) / range * res)
                .round()
                .clamp(1.0, res - 1.0) as usize;
            min_v + range / res * k as f64
        } else {
            min_v + range / self.num_v_loops.max(1) as f64 * (index as f64 + 0.5)
        }
    }

    pub fn cut_u_values(&self) -> Vec<f64> {
        let (min_u, max_u, _, _) = self.uv_range;
        if max_u - min_u < 1e-6 {
            return Vec::new();
        }
        self.cut_loops
            .iter()
            .filter(|l| l.direction == CutDirection::U && l.cut)
            .map(|l| self.loop_u_position(l.index))
            .collect()
    }

    pub fn cut_v_values(&self) -> Vec<f64> {
        let (_, _, min_v, max_v) = self.uv_range;
        if max_v - min_v < 1e-6 {
            return Vec::new();
        }
        self.cut_loops
            .iter()
            .filter(|l| l.direction == CutDirection::V && l.cut)
            .map(|l| self.loop_v_position(l.index))
            .collect()
    }
    pub fn has_cut_loops(&self) -> bool {
        match self.cut_mode {
            CutMode::Grid => self.cut_loops.iter().any(|l| l.cut),
            CutMode::Knot => self.knot_cut_1 || self.knot_cut_2,
        }
    }
    pub fn cut_grid_dims(&self) -> (usize, usize) {
        match self.cut_mode {
            CutMode::Grid => {
                // 周期语义：n 条切割线把环切成 n 段
                // （find_region_index：值 < 第一条线归入最后一段）
                let nu = self
                    .cut_loops
                    .iter()
                    .filter(|l| l.direction == CutDirection::U && l.cut)
                    .count()
                    .max(1);
                let nv = self
                    .cut_loops
                    .iter()
                    .filter(|l| l.direction == CutDirection::V && l.cut)
                    .count()
                    .max(1);
                (nu, nv)
            }
            CutMode::Knot => (4, 1),
        }
    }

    /// 实际补片网格尺寸。Knot 模式的补片数由切割结果决定（max_pu+1，
    /// 以 patch_visible 长度为准），不能使用 cut_grid_dims 的硬编码 (4,1)。
    pub fn actual_patch_dims(&self) -> (usize, usize) {
        // OBJ 多选导入：补片数 = 文件数（每个文件一个补片），不能使用
        // cut_grid_dims 的硬编码 (4,1)，否则补片面板条目数与实际不符。
        if let MeshType::ObjFile(paths) = &self.mesh_type {
            return (paths.len().max(1), 1);
        }
        let (mut nu, mut nv) = self.cut_grid_dims();
        if self.cut_mode == CutMode::Knot && self.patch_visible.len() > 1 {
            nu = self.patch_visible.len();
            nv = 1;
        }
        (nu, nv)
    }
    pub fn ensure_patch_visible(&mut self, total: usize) {
        if self.patch_visible.len() != total {
            self.patch_visible = vec![true; total];
        }
    }
}

// ============================================================
// UI Action
// ============================================================
pub struct UiAction {
    pub rebuild: bool,
    pub reapply_cuts: bool,
    pub recolor: bool,
    pub export_obj: bool,
    pub export_obj_by_patch: bool,
    pub open_obj_dialog: bool,
    pub randomize_topology: bool,
    pub refresh_display: bool,
    pub toggle_view: bool,
    pub export_obj_dialog: bool,
    pub export_patch_dialog: bool,
    pub apply_patch_shaders: bool,
}

impl UiAction {
    pub fn new() -> Self {
        UiAction {
            rebuild: false,
            reapply_cuts: false,
            recolor: false,
            export_obj: false,
            export_obj_by_patch: false,
            open_obj_dialog: false,
            randomize_topology: false,
            refresh_display: false,
            toggle_view: false,
            export_obj_dialog: false,
            export_patch_dialog: false,
            apply_patch_shaders: false,
        }
    }
}

// ============================================================
// Numbered step helper
// ============================================================
fn step_label(step: usize, text: &str) -> String {
    format!("{:>2}. {}", step, text)
}

fn shader_combo(ui: &mut egui::Ui, id: &str, current: &mut ShaderMode) -> bool {
    let prev = *current;
    egui::ComboBox::from_id_salt(id)
        .selected_text(current.label())
        .width(220.0)
        .show_ui(ui, |ui| {
            let mut last_cat = "";
            for (i, mode) in ShaderMode::ALL.iter().enumerate() {
                let cat = mode.category();
                if cat != last_cat {
                    ui.separator();
                    ui.label(egui::RichText::new(format!("[{}]", cat)).small().strong());
                    last_cat = cat;
                }
                ui.selectable_value(current, *mode, format!("{:>2}. {}", i + 1, mode.label()));
            }
        });
    *current != prev
}

// ============================================================
// Main UI render function
// ============================================================
pub fn render_ui_panel(
    ctx: &egui::Context,
    ui_state: &mut UiState,
    stats: Option<&crate::mesh::stats::MeshStats>,
    scene_rect: &mut egui::Rect,
) -> UiAction {
    let mut action = UiAction::new();

    // Top menu bar — 仅保留面板外必需入口（流程内操作统一在各 tab）
    egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
        egui::menu::bar(ui, |ui| {
            ui.menu_button("File", |ui| {
                if ui.button("Quit").clicked() {
                    std::process::exit(0);
                }
            });
            ui.menu_button("View", |ui| {
                if ui.button("Toggle Panel").clicked() {
                    ui_state.show_properties = !ui_state.show_properties;
                    ui.close_menu();
                }
            });
        });
    });

    // 底部状态栏：后台构建进度常驻显示（即使面板隐藏也可见）
    egui::TopBottomPanel::bottom("status_bar")
        .resizable(false)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui_state.is_building {
                    ui.spinner();
                    ui.label(ui_state.build_status.as_str());
                    ui.add(
                        egui::ProgressBar::new(ui_state.build_progress.clamp(0.0, 1.0))
                            .show_percentage(),
                    );
                } else {
                    ui.colored_label(egui::Color32::from_rgb(80, 170, 90), "●");
                    ui.label(ui_state.build_status.as_str());
                }
            });
        });

    if !ui_state.show_properties {
        return action;
    }

    // 左侧栏：按算法步骤切换（Mesh → Cut → Shader → Export）
    egui::SidePanel::left("steps_panel")
        .resizable(true)
        .default_width(280.0)
        .show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                for tab in PanelTab::all() {
                    let is_active = ui_state.active_tab == *tab;
                    if ui.selectable_label(is_active, tab.icon_label()).clicked() {
                        ui_state.active_tab = *tab;
                    }
                }
            });
            ui.separator();

            egui::ScrollArea::vertical().show(ui, |ui| match ui_state.active_tab {
                PanelTab::Mesh => render_mesh_tab(ui, ui_state, &mut action),
                PanelTab::Cut => render_cut_tab(ui, ui_state, &mut action),
                PanelTab::Shader => render_shader_tab(ui, ui_state, &mut action),
                PanelTab::Export => render_export_tab(ui, ui_state, &mut action),
            });
        });

    // 右侧栏：全局显示设置（视图/光照/边线/着色），随时可调
    egui::SidePanel::right("display_panel")
        .resizable(true)
        .default_width(300.0)
        .show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                render_display_panel(ui, ui_state, &mut action, stats);
            });
        });

    // 中央：3D 主显示区——透明背景，3D 场景只渲染到此区域（不被面板遮挡）
    egui::CentralPanel::default()
        .frame(egui::Frame::NONE)
        .show(ctx, |ui| {
            *scene_rect = ui.max_rect();
        });

    action
}

// ============================================================
// 右侧常驻显示面板 — 全局显示设置（视图/背景/光照/边线/着色）
// ============================================================
fn render_display_panel(
    ui: &mut egui::Ui,
    ui_state: &mut UiState,
    action: &mut UiAction,
    stats: Option<&crate::mesh::stats::MeshStats>,
) {
    ui.heading("Display Settings");
    ui.label("全局显示调节：视图、光照、边线与着色。");
    ui.add_space(4.0);

    // 1. View mode
    ui.label(egui::RichText::new(step_label(1, "View mode")).strong());
    ui.indent("view_mode", |ui| {
        let prev = ui_state.view_mode;
        ui.horizontal(|ui| {
            ui.selectable_value(&mut ui_state.view_mode, ViewMode::Torus3D, " 3D Torus");
            ui.selectable_value(&mut ui_state.view_mode, ViewMode::Unfolded, " Planar UV");
        });
        if prev != ui_state.view_mode {
            action.toggle_view = true;
        }
    });
    ui.label("  Controls: LMB=Orbit, RMB/MMB=Pan, Scroll=Zoom, WASD=Move");
    ui.add_space(4.0);

    // 2. Background
    ui.label(egui::RichText::new(step_label(2, "Background")).strong());
    ui.indent("bg", |ui| {
        if ui
            .horizontal(|ui| {
                ui.label("  Color:");
                ui.color_edit_button_rgb(&mut ui_state.bg_color)
            })
            .inner
            .changed()
        {
            action.refresh_display = true;
        }
    });
    ui.add_space(4.0);

    // 3. Lighting
    ui.label(egui::RichText::new(step_label(3, "Lighting")).strong());
    egui::CollapsingHeader::new("Lighting Settings")
        .default_open(false)
        .show(ui, |ui| {
            if ui
                .add(
                    egui::Slider::new(&mut ui_state.ambient_intensity, 0.0..=1.0)
                        .text("  Ambient")
                        .step_by(0.01),
                )
                .changed()
            {
                action.refresh_display = true;
            }
            if ui
                .horizontal(|ui| {
                    ui.label("  Ambient color:");
                    ui.color_edit_button_rgb(&mut ui_state.ambient_color)
                })
                .inner
                .changed()
            {
                action.refresh_display = true;
            }
            ui.separator();
            for (li, light_label) in [(0, "Key Light"), (1, "Fill Light"), (2, "Rim Light")] {
                let light = match li {
                    0 => &mut ui_state.light0,
                    1 => &mut ui_state.light1,
                    _ => &mut ui_state.light2,
                };
                egui::CollapsingHeader::new(light_label)
                    .default_open(li == 0)
                    .show(ui, |ui| {
                        if ui.checkbox(&mut light.enabled, "Enable").changed() {
                            action.refresh_display = true;
                        }
                        if light.enabled {
                            if ui
                                .add(
                                    egui::Slider::new(&mut light.dir_x, -1.0..=1.0)
                                        .text("  Dir X")
                                        .step_by(0.01),
                                )
                                .changed()
                            {
                                action.refresh_display = true;
                            }
                            if ui
                                .add(
                                    egui::Slider::new(&mut light.dir_y, -1.0..=1.0)
                                        .text("  Dir Y")
                                        .step_by(0.01),
                                )
                                .changed()
                            {
                                action.refresh_display = true;
                            }
                            if ui
                                .add(
                                    egui::Slider::new(&mut light.dir_z, -1.0..=1.0)
                                        .text("  Dir Z")
                                        .step_by(0.01),
                                )
                                .changed()
                            {
                                action.refresh_display = true;
                            }
                            if ui
                                .horizontal(|ui| {
                                    ui.label("  Color:");
                                    ui.color_edit_button_rgb(&mut light.color)
                                })
                                .inner
                                .changed()
                            {
                                action.refresh_display = true;
                            }
                            if ui
                                .add(
                                    egui::Slider::new(&mut light.intensity, 0.0..=3.0)
                                        .text("  Intensity")
                                        .step_by(0.01),
                                )
                                .changed()
                            {
                                action.refresh_display = true;
                            }
                        }
                    });
            }
            ui.separator();
            ui.horizontal_wrapped(|ui| {
                if ui.small_button(" Studio ").clicked() {
                    ui_state.light0 = LightSettings {
                        enabled: true,
                        dir_x: -0.5,
                        dir_y: -0.8,
                        dir_z: -0.3,
                        color: [1.0, 0.98, 0.95],
                        intensity: 1.2,
                    };
                    ui_state.light1 = LightSettings {
                        enabled: true,
                        dir_x: 0.6,
                        dir_y: -0.2,
                        dir_z: 0.5,
                        color: [0.9, 0.92, 1.0],
                        intensity: 0.5,
                    };
                    ui_state.light2 = LightSettings {
                        enabled: true,
                        dir_x: 0.0,
                        dir_y: 0.5,
                        dir_z: -0.5,
                        color: [1.0, 0.95, 0.9],
                        intensity: 0.3,
                    };
                    ui_state.ambient_intensity = 0.2;
                    action.refresh_display = true;
                }
                if ui.small_button(" Outdoor ").clicked() {
                    ui_state.light0 = LightSettings {
                        enabled: true,
                        dir_x: -0.3,
                        dir_y: -1.0,
                        dir_z: -0.2,
                        color: [1.0, 0.95, 0.85],
                        intensity: 1.5,
                    };
                    ui_state.light1 = LightSettings {
                        enabled: true,
                        dir_x: 0.0,
                        dir_y: 1.0,
                        dir_z: 0.0,
                        color: [0.6, 0.7, 0.9],
                        intensity: 0.4,
                    };
                    ui_state.light2 = LightSettings {
                        enabled: false,
                        ..Default::default()
                    };
                    ui_state.ambient_intensity = 0.35;
                    action.refresh_display = true;
                }
                if ui.small_button(" Dramatic ").clicked() {
                    ui_state.light0 = LightSettings {
                        enabled: true,
                        dir_x: -1.0,
                        dir_y: -0.3,
                        dir_z: 0.0,
                        color: [1.0, 0.85, 0.7],
                        intensity: 2.0,
                    };
                    ui_state.light1 = LightSettings {
                        enabled: false,
                        ..Default::default()
                    };
                    ui_state.light2 = LightSettings {
                        enabled: true,
                        dir_x: 0.5,
                        dir_y: 0.0,
                        dir_z: -0.8,
                        color: [0.3, 0.4, 0.8],
                        intensity: 0.8,
                    };
                    ui_state.ambient_intensity = 0.08;
                    action.refresh_display = true;
                }
            });
        });
    ui.add_space(4.0);

    // 4. Display options（边线）
    ui.label(egui::RichText::new(step_label(4, "Display options")).strong());
    ui.indent("display_options", |ui| {
        if ui
            .checkbox(&mut ui_state.show_mesh_edges, "Show mesh edges")
            .changed()
        {
            action.refresh_display = true;
        }
        if ui_state.show_mesh_edges
            && ui
                .add(
                    egui::Slider::new(&mut ui_state.mesh_edge_width, 0.5..=5.0)
                        .text("    Width")
                        .step_by(0.1),
                )
                .changed()
        {
            action.refresh_display = true;
        }
        if ui
            .checkbox(&mut ui_state.show_patch_edges, "Show cut lines")
            .changed()
        {
            action.refresh_display = true;
        }
        if ui
            .add(
                egui::Slider::new(&mut ui_state.loop_line_width, 0.5..=8.0)
                    .text("Cut line width")
                    .step_by(0.1),
            )
            .changed()
        {
            action.refresh_display = true;
        }
    });
    ui.add_space(4.0);

    // 5. Global shader
    ui.label(egui::RichText::new(step_label(5, "Global mesh shader")).strong());
    ui.indent("global_shader", |ui| {
        if shader_combo(ui, "mesh_body_shader", &mut ui_state.mesh_shader) {
            action.refresh_display = true;
        }
    });
    ui.add_space(2.0);

    // 6. Shader parameters
    ui.label(egui::RichText::new(step_label(6, "Shader parameters")).strong());
    ui.indent("shader_params", |ui| {
        if ui
            .add(
                egui::Slider::new(&mut ui_state.shader_roughness, 0.0..=1.0)
                    .text("  a. Roughness")
                    .step_by(0.01),
            )
            .changed()
        {
            action.refresh_display = true;
        }
        if ui
            .add(
                egui::Slider::new(&mut ui_state.shader_metallic, 0.0..=1.0)
                    .text("  b. Metallic")
                    .step_by(0.01),
            )
            .changed()
        {
            action.refresh_display = true;
        }
        if ui
            .add(
                egui::Slider::new(&mut ui_state.shader_specular, 0.0..=1.0)
                    .text("  c. Specular")
                    .step_by(0.01),
            )
            .changed()
        {
            action.refresh_display = true;
        }
        if ui
            .add(
                egui::Slider::new(&mut ui_state.shader_shininess, 1.0..=256.0)
                    .text("  d. Shininess")
                    .step_by(1.0),
            )
            .changed()
        {
            action.refresh_display = true;
        }
    });
    ui.add_space(4.0);

    // 7. Face colors
    ui.label(egui::RichText::new(step_label(7, "Face colors")).strong());
    ui.indent("face_colors", |ui| {
        let prev_mode = ui_state.color_mode;
        ui.horizontal(|ui| {
            ui.selectable_value(&mut ui_state.color_mode, ColorMode::Solid, "Solid");
            ui.selectable_value(&mut ui_state.color_mode, ColorMode::ByRegion, "By Region");
        });
        if prev_mode != ui_state.color_mode {
            action.refresh_display = true;
        }
        if ui_state.color_mode == ColorMode::Solid {
            if ui
                .horizontal(|ui| {
                    ui.label("  Base color:");
                    ui.color_edit_button_rgba_premultiplied(&mut ui_state.base_color)
                })
                .inner
                .changed()
            {
                action.refresh_display = true;
            }
        } else {
            let prev = ui_state.color_scheme;
            egui::ComboBox::from_id_salt("color_scheme_combo")
                .selected_text(ui_state.color_scheme.to_string())
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut ui_state.color_scheme,
                        ColorScheme::Rainbow,
                        "Rainbow",
                    );
                    ui.selectable_value(
                        &mut ui_state.color_scheme,
                        ColorScheme::Checkerboard,
                        "Checkerboard",
                    );
                    ui.selectable_value(
                        &mut ui_state.color_scheme,
                        ColorScheme::Heatmap,
                        "Heatmap",
                    );
                    ui.selectable_value(
                        &mut ui_state.color_scheme,
                        ColorScheme::Grayscale,
                        "Grayscale",
                    );
                });
            if prev != ui_state.color_scheme {
                action.recolor = true;
            }
        }
    });
    ui.add_space(4.0);

    // 8. Shading
    ui.label(egui::RichText::new(step_label(8, "Shading")).strong());
    ui.indent("shading", |ui| {
        let prev_smooth = ui_state.smooth_shading;
        egui::ComboBox::from_id_salt("shading_mode")
            .selected_text(if ui_state.smooth_shading {
                "Smooth"
            } else {
                "Flat"
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut ui_state.smooth_shading, false, "Flat Shading");
                ui.selectable_value(&mut ui_state.smooth_shading, true, "Smooth Shading");
            });
        if prev_smooth != ui_state.smooth_shading {
            action.refresh_display = true;
        }
        if ui
            .add(
                egui::Slider::new(&mut ui_state.ao_strength, 0.0..=2.0)
                    .text("AO strength")
                    .step_by(0.01),
            )
            .changed()
        {
            action.refresh_display = true;
        }
    });
    ui.add_space(4.0);

    // 9. Mesh info
    ui.label(egui::RichText::new(step_label(9, "Mesh info")).strong());
    ui.indent("mesh_info", |ui| match stats {
        Some(s) => {
            ui.label(format!("  Vertices:  {}", s.vertices));
            ui.label(format!("  Faces:     {}", s.faces));
            ui.label(format!("  Edges:     {}", s.edges));
            ui.label(format!(
                "  Closed:    {} ({} boundary half-edges, {} loops)",
                if s.is_closed { "yes" } else { "no" },
                s.boundary_half_edges,
                s.boundary_loops
            ));
            ui.label(format!("  Euler χ:   {}", s.euler_characteristic));
            ui.label(format!("  Area:      {:.4}", s.surface_area));
            ui.label(format!("  Volume:    {:.4}", s.volume));
            ui.label(format!("  Avg face:  {:.1}-gon", s.avg_face_degree));
        }
        None => {
            ui.label("  (no mesh)");
        }
    });
}

// ============================================================
// TAB 1: Mesh — 生成/加载网格并定义曲面参数
// ============================================================
fn render_mesh_tab(ui: &mut egui::Ui, ui_state: &mut UiState, action: &mut UiAction) {
    ui.heading("Step 1: Generate Mesh");
    ui.label("选择网格来源，定义平面 UV 参数化与环面几何参数。");
    ui.add_space(4.0);

    // 1.1 Mesh source
    ui.label(egui::RichText::new(step_label(1, "Select mesh source")).strong());
    ui.indent("mesh_type", |ui| {
        let prev_type = ui_state.mesh_type.clone();
        ui.horizontal(|ui| {
            if ui
                .radio(matches!(ui_state.mesh_type, MeshType::Quad), " Quad Grid")
                .clicked()
            {
                ui_state.mesh_type = MeshType::Quad;
            }
            if ui
                .radio(
                    matches!(ui_state.mesh_type, MeshType::Delaunay),
                    " Delaunay",
                )
                .clicked()
            {
                ui_state.mesh_type = MeshType::Delaunay;
            }
        });
        if ui
            .radio(
                matches!(ui_state.mesh_type, MeshType::ObjFile(_)),
                " OBJ File",
            )
            .clicked()
        {
            action.open_obj_dialog = true;
        }
        if let MeshType::ObjFile(ref paths) = ui_state.mesh_type {
            ui.label(format!("     Loaded {} OBJ file(s):", paths.len()));
            for p in paths {
                ui.label(format!("       • {}", p));
            }
        }
        if ui_state.mesh_type != prev_type {
            action.rebuild = true;
        }
    });
    ui.add_space(4.0);

    // 1.2 Grid resolution
    ui.label(egui::RichText::new(step_label(2, "Set grid resolution")).strong());
    ui.indent("mesh_params", |ui| match &mut ui_state.mesh_type {
        MeshType::Quad => {
            if ui
                .add(
                    egui::Slider::new(&mut ui_state.resolution_u, 4..=120).text("  a. U divisions"),
                )
                .changed()
            {
                action.rebuild = true;
            }
            if ui
                .add(
                    egui::Slider::new(&mut ui_state.resolution_v, 4..=120).text("  b. V divisions"),
                )
                .changed()
            {
                action.rebuild = true;
            }
            ui.label(format!(
                "     Total quads: {} x {} = {}",
                ui_state.resolution_u,
                ui_state.resolution_v,
                ui_state.resolution_u * ui_state.resolution_v
            ));
        }
        MeshType::Delaunay => {
            let prev_pts = ui_state.delaunay_points;
            ui.add(
                egui::Slider::new(&mut ui_state.delaunay_points, 500..=5000)
                    .text("  a. Point count"),
            );
            if prev_pts != ui_state.delaunay_points {
                action.rebuild = true;
            }
            if ui.button("  b. Randomize topology (edge flips)").clicked() {
                action.randomize_topology = true;
            }
        }
        MeshType::ObjFile(_) => {
            ui.label("    UV from OBJ file");
        }
    });
    ui.add_space(4.0);

    // 1.3 Torus parameters
    ui.label(egui::RichText::new(step_label(3, "Torus parameters")).strong());
    ui.indent("surface_params", |ui| {
        if ui
            .add(
                egui::Slider::new(&mut ui_state.major_radius, 0.5..=5.0)
                    .text("  a. Major radius R")
                    .step_by(0.01),
            )
            .changed()
        {
            action.rebuild = true;
        }
        if ui
            .add(
                egui::Slider::new(&mut ui_state.minor_radius, 0.1..=3.0)
                    .text("  b. Minor radius r")
                    .step_by(0.01),
            )
            .changed()
        {
            action.rebuild = true;
        }
        let ratio = ui_state.major_radius / ui_state.minor_radius;
        ui.label(format!("     R/r ratio: {:.2}", ratio));
    });
    ui.add_space(4.0);

    // 1.4 UV domain
    ui.label(egui::RichText::new(step_label(4, "UV domain (auto from mesh)")).strong());
    ui.indent("uv_domain", |ui| {
        let (min_u, max_u, min_v, max_v) = ui_state.uv_range;
        ui.label(format!(
            "  U: [{:.4}, {:.4}]  span={:.4}",
            min_u,
            max_u,
            max_u - min_u
        ));
        ui.label(format!(
            "  V: [{:.4}, {:.4}]  span={:.4}",
            min_v,
            max_v,
            max_v - min_v
        ));
    });
    ui.add_space(4.0);

    // 1.5 Surface model info
    ui.label(egui::RichText::new(step_label(5, "Surface model info")).strong());
    ui.indent("surface_info", |ui| {
        let is_torus = ui_state.surface_model.is_torus();
        ui.label(format!(
            "  Type: {}",
            if is_torus { "Torus" } else { "Unknown" }
        ));
        if is_torus {
            let (r, r2) = ui_state
                .surface_model
                .radii(ui_state.major_radius, ui_state.minor_radius);
            ui.label(format!("  Fitted R: {:.4}, r: {:.4}", r, r2));
        }
    });
}

// ============================================================
// ============================================================
// TAB 2: Cut — 定义并执行切割，管理补片
// ============================================================
fn render_cut_tab(ui: &mut egui::Ui, ui_state: &mut UiState, action: &mut UiAction) {
    ui.heading("Step 2: Cut Mesh");
    ui.label("定义切割曲线（U/V Grid 或 Torus Knot）并执行切割，管理补片。");
    ui.add_space(4.0);

    // 2.1 Cut mode
    ui.label(egui::RichText::new(step_label(1, "Select cut mode")).strong());
    ui.indent("cut_mode", |ui| {
        let prev = ui_state.cut_mode;
        ui.horizontal(|ui| {
            ui.selectable_value(&mut ui_state.cut_mode, CutMode::Grid, " U/V Grid Loops");
            ui.selectable_value(&mut ui_state.cut_mode, CutMode::Knot, " Torus Knot Curves");
        });
        if ui_state.cut_mode != prev {
            match ui_state.cut_mode {
                CutMode::Grid => {
                    for l in &mut ui_state.cut_loops {
                        l.cut = false;
                    }
                }
                CutMode::Knot => {
                    ui_state.knot_cut_1 = false;
                    ui_state.knot_cut_2 = false;
                }
            }
            action.reapply_cuts = true;
        }
    });
    ui.add_space(4.0);

    // 2.2 Define cuts
    match ui_state.cut_mode {
        CutMode::Grid => {
            ui.label(egui::RichText::new(step_label(2, "Set number of loops")).strong());
            ui.indent("loop_count", |ui| {
                let all_cut_before =
                    !ui_state.cut_loops.is_empty() && ui_state.cut_loops.iter().all(|l| l.cut);
                // Quad 网格的切割线对齐网格顶点线，loop 数量上限 = 内部网格线数（res−1）
                let max_u_loops = if matches!(ui_state.mesh_type, MeshType::Quad) {
                    ui_state.resolution_u.saturating_sub(1).max(1)
                } else {
                    20
                };
                let max_v_loops = if matches!(ui_state.mesh_type, MeshType::Quad) {
                    ui_state.resolution_v.saturating_sub(1).max(1)
                } else {
                    20
                };
                if ui
                    .add(
                        egui::Slider::new(&mut ui_state.num_u_loops, 1..=max_u_loops)
                            .text("  a. U-Loops"),
                    )
                    .changed()
                {
                    ui_state.cut_loops =
                        UiState::generate_loops(ui_state.num_u_loops, ui_state.num_v_loops);
                    if all_cut_before {
                        for l in &mut ui_state.cut_loops {
                            l.cut = true;
                        }
                    }
                    action.reapply_cuts = true;
                }
                if ui
                    .add(
                        egui::Slider::new(&mut ui_state.num_v_loops, 1..=max_v_loops)
                            .text("  b. V-Loops"),
                    )
                    .changed()
                {
                    ui_state.cut_loops =
                        UiState::generate_loops(ui_state.num_u_loops, ui_state.num_v_loops);
                    if all_cut_before {
                        for l in &mut ui_state.cut_loops {
                            l.cut = true;
                        }
                    }
                    action.reapply_cuts = true;
                }
                ui.label(format!(
                    "     Total loops: {} U + {} V = {}",
                    ui_state.num_u_loops,
                    ui_state.num_v_loops,
                    ui_state.num_u_loops + ui_state.num_v_loops
                ));
            });
            ui.add_space(4.0);

            ui.label(egui::RichText::new(step_label(3, "Configure individual loops")).strong());
            ui.indent("loop_config", |ui| {
                ui.horizontal(|ui| {
                    if ui.small_button(" Show All ").clicked() {
                        for l in &mut ui_state.cut_loops {
                            l.active = true;
                        }
                        action.refresh_display = true;
                    }
                    if ui.small_button(" Hide All ").clicked() {
                        for l in &mut ui_state.cut_loops {
                            l.active = false;
                        }
                        action.refresh_display = true;
                    }
                    if ui.small_button(" Cut All U/V Loops ").clicked() {
                        for l in &mut ui_state.cut_loops {
                            l.cut = true;
                        }
                        action.reapply_cuts = true;
                    }
                    if ui.small_button(" UnCut All ").clicked() {
                        for l in &mut ui_state.cut_loops {
                            l.cut = false;
                        }
                        action.reapply_cuts = true;
                    }
                });
                let mut any_changed = false;
                let mut any_cut_changed = false;
                for l in &mut ui_state.cut_loops {
                    ui.horizontal(|ui| {
                        any_changed |= ui.checkbox(&mut l.active, "Show").changed();
                        any_cut_changed |= ui.checkbox(&mut l.cut, "Cut").changed();
                        ui.label(l.label());
                        if ui
                            .color_edit_button_rgba_premultiplied(&mut l.color)
                            .changed()
                        {
                            any_changed = true;
                        }
                    });
                }
                if any_cut_changed {
                    action.reapply_cuts = true;
                }
                if any_changed {
                    action.refresh_display = true;
                }
            });
        }
        CutMode::Knot => {
            ui.label(egui::RichText::new(step_label(2, "Configure knot K1")).strong());
            let k1_color = egui::Color32::from_rgb(40, 90, 245);
            ui.indent("knot_k1", |ui| {
                ui.horizontal(|ui| {
                    if ui.checkbox(&mut ui_state.knot_show_1, "Show").changed() {
                        action.refresh_display = true;
                    }
                    if ui.checkbox(&mut ui_state.knot_cut_1, "Cut").changed() {
                        action.reapply_cuts = true;
                    }
                    ui.label(egui::RichText::new("■").color(k1_color));
                    ui.label("k1:");
                    if ui
                        .add(
                            egui::DragValue::new(&mut ui_state.knot_k1)
                                .speed(0.1)
                                .range(1..=20),
                        )
                        .changed()
                    {
                        if ui_state.knot_cut_1 {
                            action.reapply_cuts = true;
                        } else {
                            action.refresh_display = true;
                        }
                    }
                });
            });
            ui.add_space(2.0);
            ui.label(egui::RichText::new(step_label(3, "Configure knot K2")).strong());
            let k2_color = egui::Color32::from_rgb(226, 40, 217);
            ui.indent("knot_k2", |ui| {
                ui.horizontal(|ui| {
                    if ui.checkbox(&mut ui_state.knot_show_2, "Show").changed() {
                        action.refresh_display = true;
                    }
                    if ui.checkbox(&mut ui_state.knot_cut_2, "Cut").changed() {
                        action.reapply_cuts = true;
                    }
                    ui.label(egui::RichText::new("■").color(k2_color));
                    ui.label("k2:");
                    if ui
                        .add(
                            egui::DragValue::new(&mut ui_state.knot_k2)
                                .speed(0.1)
                                .range(1..=20),
                        )
                        .changed()
                    {
                        if ui_state.knot_cut_2 {
                            action.reapply_cuts = true;
                        } else {
                            action.refresh_display = true;
                        }
                    }
                });
            });
        }
    }
    ui.add_space(4.0);

    // 2.3 Cut result
    let (nu, nv) = ui_state.actual_patch_dims();
    let cut_count = match ui_state.cut_mode {
        CutMode::Grid => ui_state.cut_loops.iter().filter(|l| l.cut).count(),
        CutMode::Knot => (ui_state.knot_cut_1 as usize) + (ui_state.knot_cut_2 as usize),
    };
    ui.label(egui::RichText::new(step_label(4, "Cut result")).strong());
    ui.indent("cut_result", |ui| {
        ui.label(format!("  Cut mode:  {}", ui_state.cut_mode));
        ui.label(format!("  Active cuts: {}", cut_count));
        ui.label(format!("  Patches:  {} x {} = {} total", nu, nv, nu * nv));
        if cut_count == 0 {
            ui.label(egui::RichText::new("  (no cuts — single patch)").italics());
        }
    });
    ui.add_space(4.0);
}

// ============================================================
// TAB 3: Shader — 全局/逐补片着色器 + 纹理
// ============================================================
fn render_shader_tab(ui: &mut egui::Ui, ui_state: &mut UiState, action: &mut UiAction) {
    ui.heading("Step 3: Patch Appearance");
    ui.label("补片级外观：显隐、材质与逐补片着色器（全局着色在右侧面板）。");
    ui.add_space(4.0);

    // 3.1 Patch visibility & materials
    let (nu, nv) = ui_state.actual_patch_dims();
    if nu > 1 || nv > 1 {
        ui.label(egui::RichText::new(step_label(1, "Patch visibility & materials")).strong());
        egui::CollapsingHeader::new("Show/Hide Patches")
            .default_open(true)
            .show(ui, |ui| {
                let total = nu * nv;
                ui_state.ensure_patch_visible(total);
                ui.horizontal(|ui| {
                    if ui.small_button(" Show All ").clicked() {
                        ui_state.patch_visible = vec![true; total];
                        action.refresh_display = true;
                    }
                    if ui.small_button(" Hide All ").clicked() {
                        ui_state.patch_visible = vec![false; total];
                        action.refresh_display = true;
                    }
                });
                for i in 0..nu {
                    for j in 0..nv {
                        let idx = i * nv + j;
                        let mat = ui_state
                            .patch_materials
                            .get(&(i, j))
                            .copied()
                            .unwrap_or(MaterialType::Default);
                        let mut new_mat = mat;
                        ui.horizontal(|ui| {
                            let label = format!("({},{})", i, j);
                            if idx < ui_state.patch_visible.len()
                                && ui
                                    .checkbox(&mut ui_state.patch_visible[idx], &label)
                                    .changed()
                            {
                                action.refresh_display = true;
                            }
                            egui::ComboBox::from_id_salt(format!("patch_mat_{}_{}", i, j))
                                .selected_text(new_mat.to_string())
                                .show_ui(ui, |ui| {
                                    for m in MaterialType::all() {
                                        ui.selectable_value(&mut new_mat, *m, m.to_string());
                                    }
                                });
                        });
                        if new_mat != mat {
                            if new_mat == MaterialType::Default {
                                ui_state.patch_materials.remove(&(i, j));
                            } else {
                                ui_state.patch_materials.insert((i, j), new_mat);
                            }
                            action.refresh_display = true;
                        }
                    }
                }
            });
    }

    // 3.2 Per-patch shader
    let (nu, nv) = ui_state.actual_patch_dims();
    ui.label(egui::RichText::new(step_label(2, "Per-patch shader override")).strong());
    ui.indent("per_patch_shader", |ui| {
        let has_patches = nu > 1 || nv > 1;
        if !has_patches {
            ui.label(
                egui::RichText::new("⚠ 需要先执行切割（至少 2 个补片）才能使用 per-patch shader")
                    .color(egui::Color32::from_rgb(230, 170, 60))
                    .small(),
            );
        }
        if ui
            .add_enabled(
                has_patches,
                egui::Checkbox::new(
                    &mut ui_state.use_per_patch_shader,
                    "  Enable per-patch shaders",
                ),
            )
            .changed()
        {
            if ui_state.use_per_patch_shader {
                action.apply_patch_shaders = true;
            } else {
                action.refresh_display = true;
            }
        }
        if ui_state.use_per_patch_shader && (nu > 1 || nv > 1) {
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                if ui.small_button(" Apply All ").clicked() {
                    action.apply_patch_shaders = true;
                }
                if ui.small_button(" Reset All ").clicked() {
                    ui_state.patch_shaders.clear();
                    action.apply_patch_shaders = true;
                }
            });
            ui.horizontal(|ui| {
                if ui.small_button(" All Glass ").clicked() {
                    for i in 0..nu {
                        for j in 0..nv {
                            ui_state.patch_shaders.insert((i, j), ShaderMode::Glass);
                        }
                    }
                    action.apply_patch_shaders = true;
                }
                if ui.small_button(" All X-Ray ").clicked() {
                    for i in 0..nu {
                        for j in 0..nv {
                            ui_state.patch_shaders.insert((i, j), ShaderMode::XRay);
                        }
                    }
                    action.apply_patch_shaders = true;
                }
                if ui.small_button(" All Crystal ").clicked() {
                    for i in 0..nu {
                        for j in 0..nv {
                            ui_state.patch_shaders.insert((i, j), ShaderMode::Crystal);
                        }
                    }
                    action.apply_patch_shaders = true;
                }
                if ui.small_button(" Random ").clicked() {
                    use std::collections::hash_map::DefaultHasher;
                    use std::hash::{Hash, Hasher};
                    let all = ShaderMode::ALL;
                    for i in 0..nu {
                        for j in 0..nv {
                            let mut h = DefaultHasher::new();
                            (i * 31
                                + j * 17
                                + std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .unwrap_or_default()
                                    .subsec_nanos() as usize)
                                .hash(&mut h);
                            let idx = (h.finish() as usize) % all.len();
                            ui_state.patch_shaders.insert((i, j), all[idx]);
                        }
                    }
                    action.apply_patch_shaders = true;
                }
            });
            ui.add_space(2.0);
            for i in 0..nu {
                for j in 0..nv {
                    let patch = (i, j);
                    let mut current = ui_state
                        .patch_shaders
                        .get(&patch)
                        .copied()
                        .unwrap_or(ui_state.mesh_shader);
                    ui.horizontal(|ui| {
                        ui.label(format!("  ({},{}):", i, j));
                        let id = format!("patch_shader_{}_{}", i, j);
                        let prev = current;
                        egui::ComboBox::from_id_salt(&id)
                            .selected_text(current.label())
                            .width(180.0)
                            .show_ui(ui, |ui| {
                                let mut last_cat = "";
                                for mode in ShaderMode::ALL.iter() {
                                    let cat = mode.category();
                                    if cat != last_cat {
                                        ui.separator();
                                        ui.label(
                                            egui::RichText::new(format!("[{}]", cat))
                                                .small()
                                                .strong(),
                                        );
                                        last_cat = cat;
                                    }
                                    ui.selectable_value(&mut current, *mode, mode.label());
                                }
                            });
                        if current != prev {
                            ui_state.patch_shaders.insert(patch, current);
                            action.apply_patch_shaders = true;
                        }
                    });
                }
            }
        } else if nu <= 1 && nv <= 1 {
            ui.label(egui::RichText::new("  (cut the mesh first to create patches)").italics());
        }
    });
    ui.add_space(4.0);
}

// ============================================================
// ============================================================
// TAB 5: Export — 导出 OBJ
// ============================================================
fn render_export_tab(ui: &mut egui::Ui, ui_state: &mut UiState, action: &mut UiAction) {
    ui.heading("Step 4: Export");
    ui.add_space(4.0);

    // 5.0 Format
    ui.label(egui::RichText::new(step_label(0, "Format")).strong());
    ui.indent("export_format", |ui| {
        egui::ComboBox::from_id_salt("export_format_combo")
            .selected_text(ui_state.export_format.label())
            .show_ui(ui, |ui| {
                for fmt in crate::export::ExportFormat::all() {
                    ui.selectable_value(&mut ui_state.export_format, *fmt, fmt.label());
                }
            });
    });
    ui.add_space(4.0);

    // 5.1 Export Full Mesh
    ui.label(egui::RichText::new(step_label(1, "Export Full Mesh")).strong());
    ui.indent("export_obj", |ui| {
        ui.label("  1. Set output path:");
        ui.horizontal(|ui| {
            ui.label("     Path:");
            ui.text_edit_singleline(&mut ui_state.export_path_obj);
        });
        ui.horizontal(|ui| {
            if ui.small_button("  2. Browse...  ").clicked() {
                action.export_obj_dialog = true;
            }
            if ui.small_button("  3. Export OBJ  ").clicked() {
                action.export_obj = true;
            }
        });
    });
    ui.add_space(8.0);

    ui.separator();
    ui.add_space(4.0);

    // 5.2 Export by Patch
    ui.label(egui::RichText::new(step_label(2, "Export OBJ by Patch")).strong());
    ui.indent("export_patch", |ui| {
        ui.label("  1. Set output directory:");
        ui.horizontal(|ui| {
            ui.label("     Dir:");
            ui.text_edit_singleline(&mut ui_state.export_dir_patches);
        });
        ui.horizontal(|ui| {
            if ui.small_button("  2. Browse...  ").clicked() {
                action.export_patch_dialog = true;
            }
            if ui.small_button("  3. Export Patches  ").clicked() {
                action.export_obj_by_patch = true;
            }
        });
    });
    ui.add_space(8.0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[allow(clippy::field_reassign_with_default)] // 测试辅助：逐字段覆盖默认值
    fn quad_state() -> UiState {
        let mut s = UiState::default();
        s.mesh_type = MeshType::Quad;
        s.resolution_u = 40;
        s.resolution_v = 24;
        s.num_u_loops = 4;
        s.num_v_loops = 6;
        s.cut_loops = UiState::generate_loops(4, 6);
        s.uv_range = (
            0.0,
            2.0 * std::f64::consts::PI,
            0.0,
            2.0 * std::f64::consts::PI,
        );
        s
    }

    /// Quad 模式：切割线必须均匀分布（等分 n+1 段）且落在网格顶点线上。
    #[test]
    fn test_quad_loop_positions_uniform_on_grid_lines() {
        let s = quad_state();
        let res = 40.0;
        let u_vals: Vec<f64> = (0..4).map(|i| s.loop_u_position(i)).collect();
        // 4 条线把 [0,2π] 等分成 5 段：理想位置 1/5..4/5，吸附到网格线 = 8,16,24,32 格
        let expected: Vec<f64> = [8.0, 16.0, 24.0, 32.0]
            .iter()
            .map(|k| 2.0 * std::f64::consts::PI / res * k)
            .collect();
        for (got, exp) in u_vals.iter().zip(expected.iter()) {
            assert!(
                (got - exp).abs() < 1e-9,
                "U-loop 位置 {got:.6} 应等于网格线 {exp:.6}"
            );
        }
        // 严格递增且等分（相邻间隔相同）
        let d = u_vals[1] - u_vals[0];
        for i in 1..u_vals.len() {
            assert!((u_vals[i] - u_vals[i - 1] - d).abs() < 1e-9);
        }
        // 不在边界（不等于 0 或 2π）
        assert!(u_vals[0] > 1e-6 && u_vals[3] < 2.0 * std::f64::consts::PI - 1e-6);

        // V 同理：6 条线 → 7 段 → 网格线 3,7,10,14,17,21
        let v_vals: Vec<f64> = (0..6).map(|i| s.loop_v_position(i)).collect();
        let res_v = 24.0;
        let expected_v: Vec<f64> = [3.0, 7.0, 10.0, 14.0, 17.0, 21.0]
            .iter()
            .map(|k| 2.0 * std::f64::consts::PI / res_v * k)
            .collect();
        for (got, exp) in v_vals.iter().zip(expected_v.iter()) {
            assert!(
                (got - exp).abs() < 1e-9,
                "V-loop 位置 {got:.6} 应等于网格线 {exp:.6}"
            );
        }
    }

    /// 切割线数量与 patch 段数一致（周期语义）：n 条线 → n 段。
    #[test]
    fn test_cut_grid_dims_matches_loop_count() {
        let mut s = quad_state();
        // 默认 4 U + 6 V 全 cut
        for l in &mut s.cut_loops {
            l.cut = true;
        }
        assert_eq!(s.cut_grid_dims(), (4, 6));

        // 只启用部分
        for l in &mut s.cut_loops {
            l.cut = false;
        }
        s.cut_loops[0].cut = true;
        s.cut_loops[4].cut = true; // 第一条 V
        assert_eq!(s.cut_grid_dims(), (1, 1));

        // 无切割 → 1 段
        for l in &mut s.cut_loops {
            l.cut = false;
        }
        assert_eq!(s.cut_grid_dims(), (1, 1));
    }
}
