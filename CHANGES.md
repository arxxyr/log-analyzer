# TUI 默认启用 - 变更总结

## 📅 更新时间

2025-10-13 (v0.3.0-beta.5)

## 🎯 主要变更

### 运行时行为

**之前 (v0.3.0-alpha):**

```bash
./analyzer --tui  # 需要显式指定参数
./analyzer        # 默认执行 auto 模式
```

**现在 (v0.3.0-beta.5):**

```bash
./analyzer              # 默认启动 TUI
./analyzer auto         # CLI 模式
./analyzer --no-tui auto  # 强制禁用 TUI
```

### 命令行参数

| 参数 | 之前 | 现在 |
|------|------|------|
| 启动 TUI | `--tui` | *默认行为* |
| 禁用 TUI | *不可用* | `--no-tui` |

## 🔧 技术修复

**Tokio 运行时错误：**

将 `EventHandler` 创建延迟到 async 上下文，修改 `App.events` 为 `Option<EventHandler>`。

## 🚀 使用示例

```bash
# 交互式使用（推荐）
./analyzer

# CLI 模式
./analyzer --no-tui auto
./analyzer analyze -i logs/test.log -o output

# 精简部署
cargo build --release --no-default-features
```

## 🔧 TUI 优化历史

### v0.3.0-beta.5 - 进度显示

**问题：** beta.4 禁用了进度条但无替代方案

**解决：**

- 实现 `download_with_callback()` 自定义进度回调
- 创建 `TuiProgressCallback` 集成到 AppState
- 手动控制下载流程，完整显示进度（0%-100%）

### v0.3.0-beta.4 - 布局修复

**问题：** `indicatif::ProgressBar` 直接输出到 stdout 破坏 TUI 布局

**解决：**

- TUI 模式下禁用 indicatif 进度条
- 静默插件输出（注释 100+ println）
- 简化摘要显示（≤40 字符/行，前 5 个完整 + 前 3 个不完整）

### v0.3.0-beta.3 - 日志优化

- 减少进度更新频率（25% -> 1 次）
- 汇总显示甘特图/CSV 数量
- 优化摘要格式适配窗口

---

## 📦 Release Notes (v0.3.0-beta.5)

### 核心功能

- ✅ TUI 默认启用，`./analyzer` 直接启动
- ✅ 下载进度显示（0%-100%）+ 实时进度条
- ✅ 插件输出静默（100+ println 已注释）
- ✅ 智能摘要显示（紧凑格式，前 5 完整 + 前 3 不完整）

### 技术改进

- 自定义进度回调机制（`download_with_callback()`）
- TUI 进度回调集成到 AppState
- 每 25% 更新一次日志，避免刷屏
- 编译完全无警告

### 兼容性

- ✅ 所有 CLI 子命令保持不变
- ✅ `--no-tui` 强制使用 CLI 模式
- ✅ 输出文件格式向后兼容

---

**版本：** v0.3.0-beta.5
**状态：** Ready for testing
**版本规范：** [语义化版本 2.0.0](https://semver.org/lang/zh-CN/)
