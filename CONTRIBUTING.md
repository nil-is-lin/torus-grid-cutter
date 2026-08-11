# 贡献指南

感谢你愿意为 Torus Grid Cutter 贡献代码！请遵循以下约定，让协作更顺畅。

## 开发环境

- Rust stable（edition 2021），建议通过 rustup 安装
- 运行/测试前无需额外系统依赖（wgpu/winit 自动拉取）

## 常用命令

```bash
cargo run                 # 运行应用
cargo test                # 全部测试（单元 + 集成）
cargo test --lib stats    # 运行指定模块测试
cargo clippy --all-targets -- -D warnings   # 静态检查（CI 强制）
cargo fmt --all --check   # 格式检查（CI 强制）
```

## 代码约定

1. **分层**：`mesh/` 是纯算法（无渲染/UI 依赖），新增算法放这里并配单元测试；
   渲染与 UI 分别位于 `render/` 与 `ui/`。
2. **单一来源**：切割线位置等"渲染与切割共用"的数值必须由同一函数提供
   （如 `UiState::loop_u_position`），避免两处公式漂移。
3. **注释语言**：代码注释使用中文；标识符、提交信息保持英文或中文均可。
4. **测试**：新功能必须带测试。算法逻辑 → `mesh/*` 内 `#[cfg(test)]`；
   跨模块流程 → `tests/` 集成测试。

## 提交与 PR

1. 从 `main` 切分支：`git checkout -b feat/xxx`
2. 提交信息格式：`type: 摘要`，type 取 `feat` / `fix` / `refactor` / `docs` / `test` / `chore`
3. 推送前确保：`cargo fmt --all --check`、`cargo clippy -- -D warnings`、`cargo test` 全部通过
4. 开 PR 时在描述中说明改动动机与验证方式

## 版本记录

用户可见的变更请同步更新 [CHANGELOG.md](CHANGELOG.md)。
