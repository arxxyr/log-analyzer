# Analyzer - 通用日志分析器框架 (v0.3.0-beta.5)

基于插件架构的日志分析工具，使用 Rust + abi_stable 实现 ABI 稳定的插件系统，支持远程日志获取和自动化工作流。

## ✨ 特性

- 🎨 **TUI 交互界面** - 默认启动 TUI，实时显示下载进度和分析结果
- 🔌 **插件化架构** - 支持动态加载分析器插件，无需重新编译主程序
- 🛡️ **ABI 稳定** - 使用 `abi_stable` 确保跨版本二进制兼容性
- 🌐 **远程连接** - 内置 SSH 连接和 SCP 文件传输，自动下载远程日志
- 📝 **配置驱动** - 基于 YAML 配置的工作流编排
- 🚀 **自动化工作流** - 一键完成发现、下载、分析全流程
- 🎯 **智能插件选择** - 根据文件模式自动选择合适的分析器
- ⚡ **高性能** - Rust 实现，零开销抽象
- 📊 **可扩展** - 轻松开发新的分析器插件

## 🚀 快速开始

### 安装

```bash
git clone <your-repo>
cd master_control_analyzer
cargo build --release
```

### 配置

创建配置文件 `configs/analyzer.yaml`（或使用默认配置）：

```yaml
remote:
  enabled: true
  host: "192.168.4.69"
  port: 23
  user: "firefly"
  auth:
    use_agent: true
  log_dir: "/home/firefly/.ros/log"

local:
  log_dir: "./logs"
  output_dir: "./output"
  plugin_dir: "./plugins"

analyzers:
  - name: "master-control"
    pattern: "master_control_*.log"
    plugin: "master-control-analyzer"
    enabled: true
```

### 基本用法

#### 1. TUI 交互模式（推荐，v0.3.0-beta.5 新增）

直接启动 TUI，实时查看进度：

```bash
# 默认启动 TUI（无需参数）
./target/release/analyzer

# 使用自定义配置
./target/release/analyzer --config my_config.yaml
```

**TUI 快捷键：**

- `q` 或 `ESC` - 退出
- `p` 或 `空格` - 暂停/恢复
- `↑/↓` - 滚动日志
- `Home/End` - 跳转到日志首尾

#### 2. CLI 自动模式

```bash
# 使用 CLI 模式（不启动 TUI）
./target/release/analyzer --no-tui auto

# 或直接使用子命令
./target/release/analyzer auto
```

#### 3. 列出远程日志

```bash
# 列出所有日志文件
./target/release/analyzer list-remote

# 按模式过滤
./target/release/analyzer list-remote "master_control_*.log"
```

#### 3. 分析本地文件

```bash
# 自动选择插件
./target/release/analyzer analyze -i logs/your.log

# 指定插件
./target/release/analyzer analyze -i logs/your.log --plugin master-control-analyzer
```

#### 4. 从远程下载并分析

```bash
./target/release/analyzer analyze -i your.log --remote
```

#### 5. 仅下载文件

```bash
./target/release/analyzer download your.log
```

#### 6. 其他命令

```bash
# 列出所有可用插件
./target/release/analyzer list-plugins

# 验证配置文件
./target/release/analyzer check-config

# 查看帮助
./target/release/analyzer --help
```

## 📦 项目结构

```
analyzer/
├── Cargo.toml                    # Workspace 配置（v0.3.0）
├── configs/
│   └── analyzer.yaml             # 主配置文件
├── crates/
│   ├── analyzer-core/            # 核心接口库（定义插件 API）
│   ├── analyzer-cli/             # CLI 主程序（插件加载器）
│   ├── analyzer-remote/          # 远程连接模块（SSH/SCP）
│   └── analyzer-workflow/        # 工作流编排模块
├── plugins/
│   ├── master-control-analyzer/  # 机器人控制系统日志分析器
│   └── cpp-demo-analyzer/        # C++ demo 插件示例
└── docs/
    ├── PLUGIN_ARCHITECTURE.md    # 插件开发文档
    ├── WORKFLOW_ARCHITECTURE.md  # 工作流架构文档
    └── MIGRATION_GUIDE.md        # 迁移指南
```

## 🔌 内置插件

### master-control-analyzer

机器人控制系统日志分析器，支持：

- ✅ 轮次检测（基于循环标记）
- ✅ 大流程分析（完整/不完整流程识别）
- ✅ 导航流程检测
- ✅ 机械臂、头部、腰部动作分析
- ✅ CSV 数据导出
- ✅ 甘特图可视化

**使用方式：**

```bash
# v0.3.0 推荐方式 - 自动模式
./target/release/analyzer auto

# 或指定文件
./target/release/analyzer analyze -i logs/master_control_xxx.log
```

**输出文件：**
- `output/analysis.csv` - 详细的时序分析数据
- `output/major_flow_stats.csv` - 大流程统计
- `output/round_XX_gantt.png` - 每个轮次的甘特图
- `output/action_timeline.csv` - 动作时间轴汇总

## 开发新插件

查看详细的[插件开发文档](docs/PLUGIN_ARCHITECTURE.md)。

### 快速示例

```rust
use analyzer_core::*;
use abi_stable::{export_root_module, sabi_extern_fn, std_types::*};

#[derive(Clone)]
struct MyAnalyzer;

impl AnalyzerPlugin for MyAnalyzer {
    fn metadata(&self) -> PluginMetadata {
        PluginMetadata {
            name: "my-analyzer".into(),
            version: "0.1.0".into(),
            description: "我的分析器".into(),
            author: "你的名字".into(),
            supported_extensions: vec![".log".into()].into(),
        }
    }

    fn analyze(&self, args: AnalyzeArgs) -> RResult<AnalyzeResult, RBoxError> {
        // 实现分析逻辑
        // ...
    }
}

// 导出插件
#[export_root_module]
pub fn get_root_module() -> AnalyzerPluginModule_Ref {
    AnalyzerPluginModule { create_plugin }.leak_into_prefix()
}

#[sabi_extern_fn]
pub fn create_plugin() -> AnalyzerPlugin_TO<'static, RBox<()>> {
    AnalyzerPlugin_TO::from_value(MyAnalyzer, TD_Opaque)
}
```

编译：
```bash
cargo build --release --package my-analyzer
cp target/release/libmy_analyzer.so target/release/plugins/
```

## 🏗️ 架构

### 核心组件（v0.3.0）

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
│  - cpp-demo             │
│  - (可扩展...)          │
└─────────────────────────┘
```

1. **analyzer-core** - 定义 `AnalyzerPlugin` trait 和 ABI 稳定的数据结构
2. **analyzer-cli** - 主程序，负责：
   - 扫描并加载插件目录
   - 工作流编排调度
   - 配置文件管理
3. **analyzer-remote** - 远程连接模块：
   - SSH 连接管理
   - SCP 文件传输
   - 进度显示
4. **analyzer-workflow** - 工作流编排模块：
   - 配置管理
   - 文件发现（本地/远程）
   - 插件智能选择
   - 流程编排
5. **插件** - 实现 `AnalyzerPlugin` trait 的动态库

### 插件接口

```rust
#[sabi_trait]
pub trait AnalyzerPlugin: Clone + Send + Sync {
    fn metadata(&self) -> PluginMetadata;
    fn analyze(&self, args: AnalyzeArgs) -> RResult<AnalyzeResult, RBoxError>;
}
```

### ABI 稳定性

使用 `abi_stable` crate 确保：
- 数据结构布局稳定（`#[repr(C)]` + `StableAbi`）
- 函数调用约定稳定（`extern "C"`）
- 版本兼容性检查（`RootModule` trait）

## 构建

```bash
# 构建所有组件
cargo build --release

# 单独构建核心库
cargo build --package analyzer-core --release

# 单独构建 CLI
cargo build --package analyzer-cli --release

# 单独构建插件
cargo build --package master-control-analyzer --release
```

## 测试

```bash
# 运行所有测试
cargo test --all

# 测试特定包
cargo test --package analyzer-core
cargo test --package master-control-analyzer
```

## 性能

使用 Release 模式编译后，性能特征：

- **加载速度** - 插件加载时间 < 10ms
- **内存占用** - 核心库 < 1MB，每个插件 3-5MB
- **分析速度** - 取决于具体插件实现

## 📚 依赖

### 核心依赖

- `abi_stable = "0.11"` - ABI 稳定性
- `anyhow = "1.0"` - 错误处理
- `clap = "4.5"` - CLI 参数解析

### v0.3.0 新增依赖

- `ssh2 = "0.9"` - SSH 连接和文件传输
- `serde_yaml = "0.9"` - YAML 配置文件解析
- `tracing = "0.1"` - 结构化日志
- `indicatif = "0.17"` - 进度条显示

### 插件特定依赖

见各插件的 Cargo.toml（如 `plotters`、`csv`、`regex` 等）

## 🛠️ 开发指南

遵循全局 CLAUDE.md 中的 Rust 编码规范：

- 使用 `snake_case` 命名函数和变量
- 使用 `UpperCamelCase` 命名类型
- 优先使用 `&str`/`&[T]` 而非 `String`/`Vec<T>` 作为参数
- 运行 `cargo fmt` 和 `cargo clippy` 保持代码质量
- 添加中文文档注释说明模块和函数用途

## 故障排查

### 插件加载失败

1. 检查插件文件扩展名（Linux: `.so`, macOS: `.dylib`, Windows: `.dll`）
2. 确保 `abi_stable` 版本一致
3. 检查插件目录路径

### 运行时错误

```bash
# 启用详细日志
RUST_LOG=debug ./target/release/analyzer -i log.log -o output

# 检查插件符号
nm -D libmy_analyzer.so | grep root_module
```

## 贡献

欢迎提交 PR 和 Issue！

## 许可证

MIT OR Apache-2.0

## 作者

loosqk

## 📝 更新日志

### v0.3.0-beta.5 (2025-10-13) - 当前版本

- 🎨 **TUI 交互界面** - 默认启动 TUI，实时显示进度和结果
- 📊 **下载进度显示** - TUI 中完整显示下载进度（0%-100%）
- 🔇 **插件输出静默** - 注释 100+ println，保持界面整洁
- 📈 **智能摘要** - 自动显示大流程统计（紧凑格式）

### v0.3.0-beta.4

- 🔧 **修复 TUI 布局** - 禁用 indicatif 进度条，避免破坏布局

### v0.3.0-beta.3

- 📝 **日志优化** - 减少更新频率，简化输出

### v0.3.0-alpha

- 🌐 **远程连接** - 新增 SSH/SCP 支持
- 🚀 **工作流编排** - 配置驱动的自动化流程
- 🎯 **智能插件选择** - 基于文件模式自动选择
- 🔧 **CLI 子命令** - 重构命令架构

**详细变更:** 详见 [CHANGES.md](CHANGES.md)

