# Torus Grid Cutter

[![CI](https://github.com/nil-is-lin/torus-grid-cutter/actions/workflows/ci.yml/badge.svg)](https://github.com/nil-is-lin/torus-grid-cutter/actions/workflows/ci.yml)

基于 **wgpu + egui** 的环面网格切割与 UV 展开工具：参数化生成环面网格（四边形 / 周期 Delaunay / OBJ），沿 U/V 网格线或 Torus Knot 曲线切割成补片（patch），支持 42 种着色器渲染、UV 平面展开视图与按补片导出 OBJ。

## 功能特性

- **网格生成**：四边形网格（可调 U/V 分辨率）、周期 Delaunay 三角网格（泊松盘采样 + 边翻转）、加载 OBJ 模型
- **切割**：
  - U/V Grid 切割：Quad 模式下切割线对齐网格顶点线，**保持四边形面型**（不三角化）
  - Torus Knot 曲线切割（`v = k·u + 2πn` 全部分支），仅影响与曲线相交的面
- **补片管理**：按补片显隐、赋予材质、逐补片着色器覆盖（42 种模式：PBR / Glass / X-Ray / Toon / Holographic…）
- **双视图**：3D 环面视图（轨道相机、光照预设、平滑/平直着色）与 UV 平面展开视图（`(u·R, v·r)` 映射），右侧停靠面板随时切换
- **区域渲染**：按补片配色（Rainbow / Checkerboard / Heatmap / Grayscale）
- **网格统计**：闭合性 / 边界环数 / 欧拉示性数 / 表面积 / 体积 / 平均面度数
- **导出**：OBJ / STL / PLY（ASCII），整体与按补片批量

## 演示视频

> 演示视频随仓库分发：mp4 放在 `docs/demo.mp4`，通过 **GitHub Pages** 托管，
> 浏览器点击即可在线播放（不会强制下载）。需要在仓库
> **Settings → Pages** 中将 Source 设为 `main` 分支的 `/docs` 目录。

[▶ 观看演示视频（15 MB）](https://nil-is-lin.github.io/torus-grid-cutter/demo.mp4)

录制建议：窗口 1280×720，OBS / Windows Game Bar 均可；建议时长 1–3 分钟。
上传步骤：将 `demo.mp4` 放入本仓库 `docs/` 目录并提交推送即可。

## 构建


```bash
# 调试
cargo run

# 发布
cargo build --release
```

要求 Rust 1.85+（edition 2021）。依赖 wgpu 24 / egui 0.31 / winit 0.30。

## 使用流程

UI 按算法操作流程组织为 4 个步骤页：

1. **Mesh** — 选择网格类型（Quad / Delaunay / OBJ）并设置分辨率与环面参数
2. **Cut** — 选择切割模式（Grid / Knot），配置切割线并执行
3. **Shader** — 补片级外观：显隐、材质、逐补片着色器覆盖
4. **Export** — 导出整体或按补片（OBJ/STL/PLY）

右侧面板（常驻）：视图模式、背景、光照、边线、全局着色器与参数、面颜色、平滑/平直着色、网格统计。

## 架构

```
src/
├── main.rs / lib.rs     入口
├── app.rs               应用状态机：网格构建 / 切割 / 渲染状态重建 / 输入处理
├── mesh/                核心算法（纯数据，无渲染依赖）
│   ├── half_edge.rs     半边网格结构（顶点 / 半边 / 面 / 边翻转 / 三角化）
│   ├── torus.rs         环面参数化与展开（unfold_position）
│   ├── cut.rs           U/V/Knot 切割、补片索引分配
│   ├── delaunay.rs      周期 Delaunay 生成
│   ├── surface.rs       环面曲面拟合
│   └── obj_loader.rs    OBJ 解析
├── render/              wgpu 渲染：管线 / 顶点缓冲 / 线框（网格边、切割线、Knot 曲线）
├── ui/panel.rs          egui 面板（4 个流程页）
├── color_scheme.rs      补片配色方案
├── camera.rs            轨道相机
└── export/              OBJ / STL / PLY 导出（obj.rs / stl.rs / ply.rs）
```

设计要点：

- **切割算法与 UI 独立**：`mesh/` 模块为纯数据操作，可单独测试（110+ 单元测试 + 导出集成测试）
- **渲染与切割位置单一来源**：`UiState::loop_u_position` 同时驱动切割线与渲染线，保证显示一致
- **停靠 UI**：egui 面板停靠在渲染窗口右侧（可拖宽），全局显示设置常驻，流程操作按步骤切换
- **UV 为主域、3D 为映射**：网格生成、切割求交、区域划分全部在 UV 平面完成；3D 视图只是 `torus_position` 的映射，两个视图显示同一 UV 几何
- **切割交点的面内定位**：插入交点时在**面边界内**查找目标半边（而非绕顶点遍历）——展开网格的接缝副本顶点处存在 `twin = MAX` 的边界出边，绕顶点遍历会提前退出导致漏切；面内查找必然命中，接缝/边界处也可靠
- **U/V 切割交替迭代**：一条切割线切出的新面可能跨越另一条切割线（如 V 线切分产生的四边形仍跨 U 线）——`cut_mesh_by_grid` 反复执行全部切割线直到面数不再变化（通常 2-3 轮收敛），保证**无任何面跨越切割线**
- **三角网格切割后保持全三角**：切割线穿过三角形必然产生四边形+三角形，切分后统一收尾三角化（fan 三角化对新产生的面递归处理）；Quad 网格的网格线切割不穿过面，面型不受影响——Delaunay 网格任意切割后全部为三角形，Quad 网格保持四边形
- **Delaunay 拓扑缓存**：泊松采样+三角化只依赖点数（与 R/r 无关），拖动 R/r 时仅重映射顶点 3D 位置（`torus_position`），跳过重复采样与三角化——2000 点下重建从约 4.2ms 降至 0.38ms
- **解析模型直接构造**：Quad/Delaunay 网格由参数方程生成，其 `SurfaceModel` 直接由 (R, r) 构造，不做数值拟合（拟合仅用于 OBJ 导入）——避免拟合的数值路径把确定的环面误判为 Unknown
- **OBJ 导入/导出 UV 完整链路**：导出写 `vt`（`f v/vt//vn`），导入时若 UV 退化（无 vt 的旧文件）保留原始 3D 位置、不做 UV→3D 重映射——避免顶点坍缩到同一点不可见
- **GPU 缓冲上限保护**：网格边线/切割线缓冲超过 `device.limits().max_buffer_size`（通常 256MB）时降级跳过边线渲染并告警——极端参数（细网格 + 大量切割线产生 8 万+ 面）下程序不崩溃，面渲染照常
- **切割收敛保护**：U/V 线交叉处（线与 Delaunay 顶点重合）存在反复切分导致面数指数增长的可能——迭代期间面数超 15 万即停止并告警

## 测试

```bash
cargo test        # 110+ 单元测试 + 导出集成测试（含漏切检测、质心-区域一致性、全三角保持断言）
cargo clippy      # 静态检查（CI 强制 -D warnings）
```

算法细节见 [`doc/`](doc/)（LaTeX 文档：torus.tex / knot_algorithm.tex / algorithm.tex / flowchart.tex）。

## 许可证

[MIT](LICENSE)
