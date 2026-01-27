# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## 项目概述

这是一个基于**插件架构**的 **Rust 日志分析框架**。

### v0.3.3 架构（当前版本）

项目已演进为完整的工作流分析平台：

- **插件系统** - 使用 `abi_stable` 实现 ABI 稳定的插件架构
- **Workspace 结构** - 分为核心库、CLI、工作流编排、远程连接、TUI、合并器、可视化器和插件
- **动态加载** - 支持运行时加载分析器插件
- **类型安全** - 保留 Rust 的类型安全和性能优势
- **远程连接** - 内置 SSH 连接和 SCP 文件传输
- **配置驱动** - 基于 YAML 配置的工作流编排
- **自动化工作流** - 自动发现、下载、选择插件和分析
- **TUI 界面** - 可选的交互式终端界面（使用 `--tui` 启用）
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
│   ├── analyzer-remote/          # 远程连接模块（SSH/SCP）
│   ├── analyzer-workflow/        # 工作流编排模块
│   ├── analyzer-tui/             # TUI 界面模块（可选，--tui 启用）
│   ├── analyzer-merger/          # 时间线合并模块
│   ├── analyzer-visualizer/      # 可视化模块
│   └── plugins/                  # 分析器插件目录
│       └── master-control-analyzer/  # 机器人控制系统日志分析器
├── fonts/
│   └── *.ttf                     # 中文字体（甘特图标签用）
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

# 不带 TUI 的精简版本
cargo build --package analyzer-cli --release --no-default-features
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

# TUI 交互式界面（可选）
./analyzer --tui

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
cargo fmt
cargo clippy -- -W clippy::all
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
- 统一的时间线数据结构（`Timeline`, `TimelineEvent`, `Track`）

#### 2. analyzer-cli - 主程序
- 命令行参数解析（使用 `clap`）
- 插件动态加载和管理
- 工作流编排调度

#### 3. analyzer-remote - 远程连接模块
- SSH 连接管理
- SCP 文件传输
- 进度回调支持

#### 4. analyzer-workflow - 工作流编排模块
- 配置管理（YAML）
- 文件发现（本地/远程）
- 插件选择（基于文件模式）
- 流程编排

#### 5. analyzer-tui - TUI 界面模块（可选）
- 交互式终端界面
- 实时日志显示
- 插件面板
- 使用 `--tui` 参数启用

#### 6. analyzer-merger - 时间线合并模块
- 合并多个日志源的时间线
- 时间对齐和容差处理

#### 7. analyzer-visualizer - 可视化模块
- 多泳道甘特图生成
- 自定义颜色和布局

### 插件模块

#### master-control-analyzer - 机器人控制系统日志分析器

主要模块：

1. **parser.rs** - 日志解析
2. **round_detector.rs** - 轮次与大流程检测、暂停时间检测
3. **flow_detector.rs** - 导航流程与动作检测
4. **csv_exporter.rs** - CSV 数据导出
5. **gantt.rs** - 甘特图可视化

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
- `output/major_flow_stats.csv` - 大流程统计
- `output/cycle_duration_stats.csv` - 轮次时长统计
- `output/cycle_duration_stats.png` - 轮次时长分布图
- `output/round_N_gantt.png` - 每个轮次的甘特图

甘特图颜色说明：
- 浅蓝色: 导航动作
- 浅黄色: 预打舵
- 浅绿色: 机械臂动作
- 浅橙色: 头部控制
- 浅紫色: 腰部控制

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
5. 编译并复制到 plugins 目录

### 修改现有插件

#### 添加新的动作类型
1. 在 `flow_detector.rs` 中添加正则表达式模式
2. 在 `detect_flows()` 中添加匹配逻辑
3. 在 `gantt.rs` 的颜色映射中添加新颜色

#### 添加新的暂停检测模式
1. 在 `round_detector.rs` 中添加正则表达式
2. 在 `detect_rounds()` 或 `detect_pause_events()` 中添加匹配逻辑

### 代码风格

遵循全局 CLAUDE.md 中的 Rust 编码规范：
- 使用 `snake_case` 命名函数和变量
- 使用 `UpperCamelCase` 命名类型
- 运行 `cargo fmt` 和 `cargo clippy` 保持代码质量

## 注意事项

1. **TUI 界面**：使用 `--tui` 参数启用，默认为 CLI 模式
2. **配置文件**：位于 `configs/analyzer.yaml`
3. **日志编码**：自动处理 UTF-8 编码问题
4. **时间处理**：内部使用 Unix 时间戳，甘特图显示北京时间
5. **暂停时间**：自动检测并从轮次统计中扣除
6. **依赖版本**：Rust edition 2024，`abi_stable = "0.11"`
7. **内存分配**：使用 mimalloc 提升性能

## 版本变更

### v0.3.3（当前版本）

- CI/CD 优化：使用 Swatinem/rust-cache 加速构建
- 移除 macOS x64 构建，只保留 ARM64
- 语义化版本支持

### v0.3.2

- 修复暂停时间未从轮次统计中扣除的问题
- 新增 TaskGraphExecutor 暂停检测模式
- 甘特图中文字符显示修复
- 使用 mimalloc 内存分配器

详细变更见 [CHANGES.md](CHANGES.md)
