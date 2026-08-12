# Changelog

本项目遵循 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/) 格式，
版本号遵循 [Semantic Versioning](https://semver.org/lang/zh-CN/)。

## [Unreleased]

### 新增
- 导出格式扩展：STL（ASCII）与 PLY（ASCII），整体与按补片导出均支持（Export 页 Format 选择）
- 网格统计：闭合性（watertight）、边界环数、欧拉示性数 χ、表面积、体积（散度定理）、平均面度数（View 页 Mesh info）
- 集成测试 `tests/export_formats.rs`：完整 pipeline（生成 → 切割 → 三种格式导出）验证
- CI 增加 Windows 构建检查（test + clippy + fmt）
- 相机轨道数学与环面参数方程单元测试（视图矩阵/拖动/周期性/几何性质）
- Knot 区域指派改用解析拓扑不变量 `assign_connected_components_knot(mesh, k1, k2, uv_range)`：
  逐面按 `L = (⌊(v−k₁u)/2π⌋ − ⌊(v−k₂u)/2π⌋) mod |k₂−k₁|` 指派区域，同时写入
  `component_id` 与 `patch_index`。该量在 `u→u+2π`、`v→v+2π` 下均不变，是合法环面拓扑不变量，
  对任意 k 与任意网格分辨率恒给出 `|k₂−k₁|` 个区域（单条 `(1,k)` 曲线为非分离曲线 → 1 区域）
- `HalfEdge::cut` 屏障标记与 `Face::component_id` 拓扑连通块字段
- `HalfEdgeMesh::insert_interior_vertex_fan`：在面内插入顶点并扇三角化，使两条切割曲线在面内成为
  真正的**横截交叉**（四面对应），而非在共享顶点处串接成单条环（串接不会分开环面）
- Knot 回归测试：`test_knot_seam_welding_three_regions`（48 组分辨率×seed，k=[3,6] 恰 3 区域）、
  `test_knot_single_curve_one_region`（单条曲线恰 1 区域）

### 性能
- Delaunay 网格拓扑缓存：拖动 R/r 仅重映射顶点 3D 位置，跳过重复采样+三角化（2000 点下 4.2ms → 0.38ms）

### 重构
- 删除无 UI 入口的拾取功能整链：`pick.rs` 模块、`SelectionMode`/`selected_*`/`ctrl_pressed`/`mouse_press_pos` 状态、选中高亮渲染（marker）——约 610 行死代码
- `show_patch_edges` 补全渲染逻辑（此前 UI 开关无效）：关闭时跳过切割线渲染
- 删除 `assign_multi_knot_patch_indices`：其“每-k `div_euclid` 条带向量 + `BTreeSet` 去重”方案给出的是
  **UV 平面条带**（k+1 条）而非环面拓扑区域，语义本身就是错的，由解析不变量函数取代
- 删除死代码 `sync_seam_cut_flags`（接缝 cut 同步的失败尝试）
- Knot 切割入口收敛为 `cut_face_knots`：一个面在一次调用内处理**所有曲线的所有分支**，
  取代逐曲线串行切割 + 最多 5 轮“修复遍”的旧结构
- `assign_connected_components`（flood-fill）职责收窄为仅服务导入的 OBJ 网格（无环面接缝语义可利用）

### 修复
- 闪退修复（wgpu 缓冲超限）：网格边线/切割线缓冲超过 GPU 上限（256MB）时降级跳过渲染并告警——大量切割线导致 8 万+ 面时不再崩溃
- 切割收敛保护：迭代期间面数超过 15 万停止（防 U/V 线交叉处反复切分导致指数增长）
- Delaunay 切割后全三角收尾：`cut_mesh_by_grid` 新增 `finalize_triangles`，覆盖被相邻面 split_edge 增边而未切分面的残留多边形（此前 3000 点+多线切割残留 1.6 万个非三角面）
- 区域渲染颜色错位：patch 索引行宽与 `assign_patch_indices` 的周期分段语义（n 条切割线 → n 段）统一，By Region 下每补片颜色正确对应
- Per-patch shader override 无切割时无提示：补片数 ≤ 1 时禁用并显示警告
- Knot 切割多分支漏切：`v = k·u + 2πn` 的**所有**分支逐一求交切割（粗网格 + 大 k 场景）
- **Knot 分支范围 ceil bug（整面漏切）**：`knot_branch_range` 旧用 `n_lo = ⌈(φ_min−snap)/2π⌉`，
  φ 为负时算大一格（φ_min=−12.49 ⇒ ⌈−1.988⌉=−1，正确应为 ⌊−1.988⌋=−2），导致
  `n_lo > n_hi`、`for branch in n_lo..=n_hi` 静默空循环，该面整条曲线**完全没被切**。
  两端统一改为 `⌊·⌋` 并加 swap 防御
- **Knot 大 k 值区域错误合并**：`k=[3,10]` 应 7 区域实得 1 区域（整片一色）。根因是
  flood-fill + 接缝缝合本质脆弱——曲线在接缝只切出端点、接缝边自身 `cut=false`，缝合逻辑跨缝
  连通把区域合并回一块（`k=[3,6]` 曾正确只是切点位置的数学巧合）。改用解析不变量彻底绕开
- Knot 曲线顶点吸附（sliver 消除）：新增与面无关的容差 `KNOT_SNAP_UV = 1e-3`（φ 空间乘 `√(1+k²)`），
  曲线“几乎穿过”既有顶点时吸附为 `Side::OnLine`，消除近零长边与 sliver 面导致的切割链断裂
- Knot 面内交点排序：改为沿分支方向 `(1,k)` 投影 `u + k·v` 排序连成**开口折线**；旧实现用绕面边界
  排序并首尾闭合，2 点时重复连同一条弦、≥3 点时连成闭合三角形，凭空多出屏障边使区域被切碎
- Knot 切割弦不再静默丢弃：依次尝试本面已有边（`mark_edge_cut_between`）→ 顶点扇形内已有边
  （`mark_edge_cut_anywhere`）→ 真正的对角线弦（`add_chord`），任一段丢弃都会让切割链断成
  degree-1 端点
- 顶点扇形遍历边界安全：`faces_around_vertex` 改双向遍历。单向轨道 `he → next(twin(he))` 遇
  `twin = MAX`（接缝/域边界）即断、只覆盖半个扇形，导致落在接缝点与四个角点上的弦找不到共面被丢弃
- `face_half_edges` 死循环保护：畸形半边环不再无限循环（上界取半边总数）
- Delaunay 接缝顶点退化：边界点循环改为 `1..boundary_n−1`（跳过与角点重合的参数位置，消除相距
  ~1e-4 的重复顶点与退化 sliver），且对边共用同一微扰量（u=0/u=2π 用同一 `ju`，v=0/v=2π 用同一 `jv`）
  使对边顶点 3D 位置精确重合、`build_seam_pairs` 能一一配对
- **ByRegion 一维调色板色相退化**：旧 2D 公式 `(j+4i)%8` 在 `nv==1` 时退化为 `hue∈{0,4,0}`
  （两红一青），改为沿色相环均匀铺开 `hue = i/n`
- **ByRegion 区域被 PBR 洗白**：饱和色 albedo 经 PBR 高光 + ACES 色调映射后各通道均趋近 1.0，
  各区域塌成同一片奶白。一维调色板 `lightness` 上限定为 0.40（加亮后最大通道约 0.48，仍在色相可辨区），
  饱和度提到 0.85 补偿亮度损失
- 切割线位置：Quad 模式沿网格顶点线均匀等分（渲染与切割共用 `loop_u_position`/`loop_v_position`）
- UV/3D 视图的切割线渲染与实际切割位置不一致

### 变更
- **单窗口停靠布局**：egui 面板从独立浮动窗口改为停靠在渲染窗口右侧（可拖宽）；显示设置常驻右侧、流程操作左侧步骤切换
- UI 重构为流程页（Mesh / Cut / Shader / Export）：
  - Shader 页：全局着色器、参数、面颜色模式（Solid/ByRegion）、Smooth/Flat 与 AO、逐补片覆盖
  - View 页：视图模式、背景、光照、边线显示、网格统计
  - 移除假功能（Texture mapping 未接入 GPU）、重复入口（菜单栏导出/视图切换、Quick presets、Recolor 按钮）
- 网格统计模块 `mesh::stats`、导出格式抽象 `export::{ExportFormat, export_mesh, export_by_patch}` 独立成公开 API
- Knot 默认参数从 `k=[2,3]` 改为 `k=[3,6]`（3 个区域，直观展示拓扑区域语义）
- Knot 与 Grid 两种模式的着色链路按 `cut_mode` 分派：Knot 走 `generate_patch_colors(n, 1, …)` 一维分支；
  Grid 按网格单元 `component_id = pu·num_v + pv` 走二维 64 色分支（环面内部 U/V 切割线是非分离曲线，
  整网格切完仍只有 1 个拓扑连通块，故 Grid 不能按连通块着色）
- 文档同步现行实现：重写 `doc/knot_algorithm.tex`（分支范围 `⌊·⌋`、`cut_face_knots` 一次性多曲线多分支切割、
  `KNOT_SNAP_UV` 吸附、解析不变量与区域数定理 `|pq′−p′q|`、为何取代 flood-fill）；
  修正 `doc/algorithm.tex`、`doc/flowchart.tex` 中的过期函数名

## [0.1.0] - 2026-08

首个可运行版本：环面网格生成（Quad / 周期 Delaunay / OBJ）、U/V Grid 与 Torus Knot 切割、
补片管理（显隐 / 材质 / 逐补片着色器）、3D 与 UV 展开双视图、OBJ 导出、42 种着色器。
