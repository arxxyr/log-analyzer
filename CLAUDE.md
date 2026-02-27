# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## 项目概述

这是一个基于**插件架构**的 **Rust 日志分析框架**。

### v0.4.0 架构（当前版本）

项目已演进为完整的工作流分析平台：

- **插件系统** - 使用 `abi_stable` 实现 ABI 稳定的插件架构
- **Workspace 结构** - 分为核心库、CLI、工作流编排、远程连接、合并器、可视化器和插件
- **动态加载** - 支持运行时加载分析器插件
- **类型安全** - 保留 Rust 的类型安全和性能优势
- **远程连接** - 内置 SSH 连接和 SCP 文件传输
- **配置驱动** - 基于 YAML 配置的工作流编排
- **自动化工作流** - 自动发现、下载、选择插件和分析
- **国际化 (i18n)** - 使用 `rust-i18n` v3，支持中文/英文切换
- **系统字体检测** - 通过 `fc-match` + `register_font` 自动注册系统字体
- **时间线合并** - 多日志源时间轴合并和对齐
- **多泳道可视化** - 统一甘特图生成
- **高性能** - 使用 mimalloc 内存分配器

### 核心功能

- 从日志中提取导航和机械臂操作的时序信息
- 检测并分析多轮任务执行情况
- 暂停时间检测和扣除
- 生成 CSV 报告和甘特图可视化
- **远程日志获取** - 通过 SSH 自动下载远程日志文件
- **智能插件选择** - 根据文件模式自动选择合适的分析器
- **工作流编排** - 配置驱动的端到端分析流程
- **支持扩展** - 可轻松添加新的日志分析器

## 项目结构（Workspace）

```
analyzer/
├── Cargo.toml                    # Workspace 配置
├── configs/
│   └── analyzer.yaml             # 主配置文件（远程连接、插件映射）
├── crates/
│   ├── analyzer-core/            # 核心接口库（定义插件 API）
│   ├── analyzer-cli/             # CLI 主程序（插件加载器）
│   │   └── locales/app.yml       # CLI 翻译文件
│   ├── analyzer-remote/          # 远程连接模块（SSH/SCP）
│   │   └── locales/app.yml       # 远程模块翻译文件
│   ├── analyzer-workflow/        # 工作流编排模块
│   ├── analyzer-merger/          # 时间线合并模块
│   │   └── locales/app.yml       # 合并模块翻译文件
│   ├── analyzer-visualizer/      # 可视化模块
│   │   └── locales/app.yml       # 可视化模块翻译文件
│   └── plugins/                  # 分析器插件目录
│       └── master-control-analyzer/  # 机器人控制系统日志分析器
│           └── locales/app.yml   # 插件翻译文件
├── scripts/
│   ├── deploy.sh                 # Linux/macOS 部署脚本
│   └── deploy-windows.ps1        # Windows 部署脚本
└── 其他文件
    ├── CHANGES.md                # 变更历史
    ├── README.md                 # 用户文档
    └── SOP.md                    # 标准操作流程
```

## 常用命令

### 构建

```bash
# 构建所有组件（workspace）
cargo build --release

# 单独构建插件
cargo build --package master-control-analyzer --release
```

### 运行

```bash
# 默认行为 - 自动模式（获取最新日志并分析）
./analyzer

# 显式自动模式
./analyzer auto

# 使用自定义配置文件
./analyzer --config my_config.yaml auto

# 指定输出目录
./analyzer auto --output ./my_output

# 分析本地文件
./analyzer analyze -i logs/your.log

# 手动指定插件
./analyzer analyze -i logs/your.log --plugin master-control-analyzer

# 从远程下载并分析
./analyzer analyze -i your.log --remote

# 列出远程可用日志
./analyzer list-remote
./analyzer list-remote "master_control_*.log"

# 仅下载文件（不分析）
./analyzer download your.log

# 列出所有可用插件
./analyzer list-plugins

# 验证配置文件
./analyzer check-config

# 多文件分析
./analyzer multi --auto-download

# 英文界面
./analyzer --lang en

# 详细日志输出
./analyzer --verbose auto
```

### 测试

```bash
cargo test
cargo test -- --nocapture
```

### 代码检查

```bash
cargo fmt --all
cargo clippy --all -- -W clippy::all
```

## 架构说明

### 整体架构

```
┌──────────────────────────────────────┐
│      analyzer-cli (CLI 入口)         │
│  - 命令行参数解析                      │
│  - 配置文件加载                        │
│  - 插件管理和加载                      │
│  - 工作流调度                          │
│  - i18n 语言设置                      │
└────────────┬─────────────────────────┘
             │
             ├──────────────────────────────┐
             │                              │
┌────────────▼────────────┐  ┌─────────────▼──────────┐
│  analyzer-workflow       │  │  analyzer-remote       │
│  (工作流编排)             │◄─┤  (远程连接)            │
│  - 配置管理               │  │  - SSH 连接管理        │
│  - 文件发现               │  │  - SCP/SFTP 传输       │
│  - 插件选择               │  │  - 进度显示            │
│  - 流程编排               │  └────────────────────────┘
└────────────┬────────────┘
             │ 加载插件
             │
┌────────────▼────────────┐
│  analyzer-core           │
│  (插件接口)               │
│  - AnalyzerPlugin trait  │
│  - ABI 稳定类型          │
│  - AnalyzeArgs.locale    │
└────────────┬────────────┘
             │ 实现
             │
┌────────────▼────────────┐
│  各类分析器插件           │
│  - master-control       │
│  - (可扩展...)          │
└─────────────────────────┘
```

### 核心模块（Crates）

#### 1. analyzer-core - 插件接口定义
- 定义 `AnalyzerPlugin` trait
- ABI 稳定的数据结构（`PluginMetadata`, `AnalyzeArgs`, `AnalyzeResult`）
- `AnalyzeArgs` 包含 `locale: RString` 字段，用于传递语言设置给插件
- 统一的时间线数据结构（`Timeline`, `TimelineEvent`, `Track`）

#### 2. analyzer-cli - 主程序
- 命令行参数解析（使用 `clap`）
- 插件动态加载和管理
- 工作流编排调度
- i18n 初始化和语言设置

#### 3. analyzer-remote - 远程连接模块
- SSH 连接管理
- SCP 文件传输
- 进度回调支持

#### 4. analyzer-workflow - 工作流编排模块
- 配置管理（YAML）
- 文件发现（本地/远程）
- 插件选择（基于文件模式）
- 流程编排

#### 5. analyzer-merger - 时间线合并模块
- 合并多个日志源的时间线
- 时间对齐和容差处理

#### 6. analyzer-visualizer - 可视化模块
- 多泳道甘特图生成
- 自定义颜色和布局
- 字体预检（`check_font_availability()`）

### 插件模块

#### master-control-analyzer - 机器人控制系统日志分析器

主要模块：

1. **parser.rs** - 日志解析
2. **round_detector.rs** - 轮次与大流程检测、暂停时间检测
3. **flow_detector.rs** - 导航流程与动作检测
4. **csv_exporter.rs** - CSV 数据导出
5. **gantt.rs** - 甘特图可视化
6. **font_loader.rs** - 字体检测与注册

### 支持的动作类型

1. **导航 (Navigation)** - `[导航]: NavAction2`
2. **机械臂 (DoubleArmAction)** - `[机械臂]: DoubleArmAction`
3. **头部控制 (Head Control)** - `[头部控制]: HeadControlAction2`
4. **腰部控制 (Waist Control)** - `[腰部]: WaistAction2`
5. **预打舵 (PrePlanNavigation)** - `[预打舵]: PrePlanNavigation`

### 暂停检测模式

支持四种暂停检测模式：
1. `master_control: 任务暂停`
2. `暂停: 暂停中...`
3. `[暂停]`
4. `TaskGraphExecutor: 用户请求暂停任务`

### 输出文件

- `output/analysis.csv` - 详细时序数据
- `output/action_timeline.csv` - 动作时间轴汇总表
- `output/cycle_duration_stats.png` - 轮次时长分布图
- `output/round_N_gantt.png` - 每个轮次的甘特图

甘特图颜色说明：
- 浅蓝色: 导航动作
- 浅黄色: 预打舵
- 浅绿色: 机械臂动作
- 浅橙色: 头部控制
- 浅紫色: 腰部控制

## i18n 国际化

### 技术实现

- **库**: `rust-i18n` v3，YAML 翻译文件，编译时 key 检查
- **默认语言**: zh-CN
- **支持语言**: zh-CN, en
- **切换方式**: `--lang` CLI 参数 > `LANG` 环境变量 > 默认 zh-CN

### 翻译文件格式

```yaml
_version: 2

key.name:
  zh-CN: "中文文本"
  en: "English text"
```

### 翻译文件位置

每个 crate 有独立的 `locales/app.yml`：

```
crates/
├── analyzer-cli/locales/app.yml
├── analyzer-remote/locales/app.yml
├── analyzer-merger/locales/app.yml
├── analyzer-visualizer/locales/app.yml
└── plugins/master-control-analyzer/locales/app.yml
```

### 翻译范围

- **翻译**: `println!` 消息、`anyhow::bail!/context`、CSV 表头、甘特图标签
- **不翻译**: `tracing` 日志、`thiserror` 定义、正则模式、`eprintln!` 调试信息

### 插件 i18n 注意事项

- cdylib 插件有独立的静态状态，不共享主程序的 locale 设置
- 必须在 `analyze()` 方法开头调用 `rust_i18n::set_locale(args.locale.as_str())`
- `AnalyzeArgs.locale` 字段由 CLI 传入

## 字体策略

### Linux 字体检测流程

plotters 的 `ab_glyph` 后端**无法通过字体名称解析字体文件**（不使用 fontconfig）。
因此采用以下策略：

1. `fc-list :lang=zh` 检测系统是否安装 CJK 字体
2. `fc-match "FontName" --format=%{file}` 获取字体文件实际路径
3. `std::fs::read()` 读取字体文件数据
4. `plotters::style::register_font()` 将字体数据直接注册到 plotters
5. 注册成功后，plotters 可通过字体名称正常渲染

### 字体回退链

CJK 字体（按优先级）→ 英文字体（DejaVu Sans 等）→ sans-serif 兜底

### 甘特图生成失败不影响分析

字体不可用时，CSV 分析结果照常生成，仅甘特图跳过（非致命错误）。

### font_loader.rs 双份文件

`analyzer-visualizer` 和 `master-control-analyzer` 各有一份相同的 `font_loader.rs`。
原因：cdylib 插件有独立的静态状态，无法共享主程序的 `OnceLock` 缓存。

## 配置文件

### 主配置文件：`configs/analyzer.yaml`

```yaml
remote:
  enabled: true
  host: "192.168.4.69"
  port: 23
  user: "firefly"
  auth:
    password: "password"
  log_dir: "/home/firefly/.ros/log"

local:
  log_dir: "./logs"
  output_dir: "./output"
  plugin_dir: "./plugins"
  cleanup_old_output: true

file_discovery:
  sort_by: "mtime"
  sort_order: "desc"
  auto_select: "latest"

analyzers:
  - name: "master-control"
    pattern: "master_control_*.log"
    plugin: "master-control-analyzer"
    enabled: true
    priority: 0
```

## 开发指南

### 开发新插件

1. 创建插件项目：`cd crates/plugins && cargo new --lib my-analyzer`
2. 配置 Cargo.toml：`crate-type = ["cdylib", "rlib"]`
3. 实现 `AnalyzerPlugin` trait
4. 导出插件模块（`#[export_root_module]`）
5. 添加 `locales/app.yml` 翻译文件
6. 在 `analyze()` 开头设置 locale：`rust_i18n::set_locale(args.locale.as_str())`
7. 编译并复制到 plugins 目录

### 修改现有插件

#### 添加新的动作类型
1. 在 `flow_detector.rs` 中添加正则表达式模式
2. 在 `detect_flows()` 中添加匹配逻辑
3. 在 `gantt.rs` 的颜色映射中添加新颜色
4. 在 `locales/app.yml` 中添加新动作的翻译 key

#### 添加新的暂停检测模式
1. 在 `round_detector.rs` 中添加正则表达式
2. 在 `detect_rounds()` 或 `detect_pause_events()` 中添加匹配逻辑

### 代码风格

遵循全局 CLAUDE.md 中的 Rust 编码规范：
- 使用 `snake_case` 命名函数和变量
- 使用 `UpperCamelCase` 命名类型
- 运行 `cargo fmt --all` 和 `cargo clippy --all -- -W clippy::all` 保持代码质量

## 注意事项

1. **配置文件**：位于 `configs/analyzer.yaml`
2. **日志编码**：自动处理 UTF-8 编码问题
3. **时间处理**：内部使用 Unix 时间戳，甘特图显示北京时间
4. **暂停时间**：自动检测并从轮次统计中扣除
5. **依赖版本**：Rust edition 2024，`abi_stable = "0.11"`
6. **内存分配**：使用 mimalloc 提升性能
7. **字体依赖**：Linux 需要系统安装 CJK 字体（`fonts-noto-cjk`），不再捆绑字体文件
8. **插件 i18n**：插件是 cdylib，需在 `analyze()` 开头手动设置 locale

## 版本变更

### v0.4.0（当前版本）

- 国际化 (i18n)：`rust-i18n` v3，支持中文/英文切换
- 系统字体检测：`fc-match` + `register_font` 自动注册
- 移除捆绑字体，改用系统 CJK 字体
- 移除 TUI 模块
- 部署脚本简化

### v0.3.3

- CI/CD 优化：使用 Swatinem/rust-cache 加速构建
- 移除 macOS x64 构建，只保留 ARM64
- 语义化版本支持

### v0.3.2

- 修复暂停时间未从轮次统计中扣除的问题
- 新增 TaskGraphExecutor 暂停检测模式
- 甘特图中文字符显示修复
- 使用 mimalloc 内存分配器

详细变更见 [CHANGES.md](CHANGES.md)
