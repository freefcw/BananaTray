# 目录重组长期规划

> 本文档记录 BananaTray 目录结构的理想目标状态，按优先级分阶段实施。
> 每次迭代保持可编译、可测试，避免一次性大规模重构。

## 已完成

### Phase 1：消除核心命名混淆 ✅ (v0.2)

- [x] `app/` → `ui/`（含 `views/` 子目录归组）
- [x] `provider_error_presenter.rs` → `providers/error_presenter.rs`
- [x] `utils/{http_client,interactive_runner}` → `providers/common/`

### Phase 2 + 3：平台适配层归组 ✅

- [x] `auto_launch.rs` → `platform/auto_launch.rs`
- [x] `notification.rs` → `platform/notification.rs`
- [x] `utils/platform.rs` → `platform/system.rs`
- [x] `infra/assets.rs` → `platform/assets.rs`
- [x] `infra/logging.rs` → `platform/logging.rs`
- [x] `infra/single_instance.rs` → `platform/single_instance.rs`
- [x] `infra/` 目录已删除

### Phase 4：逻辑层重命名（可选）⏳

**动机**：如果 `application/` 的名字仍然造成困惑，可考虑重命名。

**候选名**：`store/`、`logic/`、`core/`

**触发条件**：团队反馈或新成员 onboarding 时评估。

### Phase 5：`app_state.rs` 归入逻辑层（可选）⏳

**动机**：`app_state.rs` 是纯逻辑状态，与 `application/` 的 reducer 紧密关联。

**变更**：
- `app_state.rs` → `application/state.rs`
- `app_state_tests.rs` → `application/state_tests.rs`（测试文件需随迁）

**触发条件**：Phase 4 完成后评估。

## 未纳入规划的模块

| 模块 | 说明 |
|---|---|
| `icons/` | 图标资源模块，当前位于 `src/icons/`，暂无迁移需求 |
| `theme_tests.rs` | 主题测试文件，位于 `src/` 根级，若后续 theme 模块目录化可随迁 |

## 设计原则

1. **每次迭代独立可用** — 不依赖后续 phase
2. **每次迭代后通过完整验证** — `cargo check` + `clippy` + `test --lib` + `fmt`
3. **优先 `git mv`** — 保留文件历史
4. **逻辑变更粒度小于 20 个文件** — `git mv` 产生的纯 rename 不计入
