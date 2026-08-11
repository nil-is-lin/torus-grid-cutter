# Changelog

本项目遵循 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/) 格式，
版本号遵循 [Semantic Versioning](https://semver.org/lang/zh-CN/)。

## [Unreleased]

### 新增
- 导出格式扩展：STL（ASCII）与 PLY（ASCII），整体与按补片导出均支持（Export 页 Format 选择）
- 网格统计：闭合性（watertight）、边界环数、欧拉示性数 χ、表面积、体积（散度定理）、平均面度数（View 页 Mesh info）
- 集成测试 `tests/export_formats.rs`：完整 pipeline（生成 → 切割 → 三种格式导出）验证

### 新增
- CI 增加 Windows 构建检查（test + clippy + fmt）
- 相机轨道数学与环面参数方程单元测试（视图矩阵/拖动/周期性/几何性质）

### 性能
- Delaunay 网格拓扑缓存：拖动 R/r 仅重映射顶点 3D 位置，跳过重复采样+三角化（2000 点下 4.2ms → 0.38ms）

### 重构
- 删除无 UI 入口的拾取功能整链：`pick.rs` 模块、`SelectionMode`/`selected_*`/`ctrl_pressed`/`mouse_press_pos` 状态、选中高亮渲染（marker）——约 610 行死代码
- `show_patch_edges` 补全渲染逻辑（此前 UI 开关无效）：关闭时跳过切割线渲染

### 修复
- 闪退修复（wgpu 缓冲超限）：网格边线/切割线缓冲超过 GPU 上限（256MB）时降级跳过渲染并告警——大量切割线导致 8 万+ 面时不再崩溃
- 切割收敛保护：迭代期间面数超过 15 万停止（防 U/V 线交叉处反复切分导致指数增长）
- Delaunay 切割后全三角收尾：`cut_mesh_by_grid` 新增 `finalize_triangles`，覆盖被相邻面 split_edge 增边而未切分面的残留多边形（此前 3000 点+多线切割残留 1.6 万个非三角面）
- 区域渲染颜色错位：patch 索引行宽与 `assign_patch_indices` 的周期分段语义（n 条切割线 → n 段）统一，By Region 下每补片颜色正确对应
- Per-patch shader override 无切割时无提示：补片数 ≤ 1 时禁用并显示警告
- Knot 切割多分支漏切：`v = k·u + 2πn` 的**所有**分支逐一求交切割（粗网格 + 大 k 场景）
- 切割线位置：Quad 模式沿网格顶点线均匀等分（渲染与切割共用 `loop_u_position`/`loop_v_position`）
- UV/3D 视图的切割线渲染与实际切割位置不一致

### 变更
- **单窗口停靠布局**：egui 面板从独立浮动窗口改为停靠在渲染窗口右侧（可拖宽）；显示设置常驻右侧、流程操作左侧步骤切换
- UI 重构为流程页（Mesh / Cut / Shader / Export）：
  - Shader 页：全局着色器、参数、面颜色模式（Solid/ByRegion）、Smooth/Flat 与 AO、逐补片覆盖
  - View 页：视图模式、背景、光照、边线显示、网格统计
  - 移除假功能（Texture mapping 未接入 GPU）、重复入口（菜单栏导出/视图切换、Quick presets、Recolor 按钮）
- 网格统计模块 `mesh::stats`、导出格式抽象 `export::{ExportFormat, export_mesh, export_by_patch}` 独立成公开 API

## [0.1.0] - 2026-08

首个可运行版本：环面网格生成（Quad / 周期 Delaunay / OBJ）、U/V Grid 与 Torus Knot 切割、
补片管理（显隐 / 材质 / 逐补片着色器）、3D 与 UV 展开双视图、OBJ 导出、42 种着色器。
