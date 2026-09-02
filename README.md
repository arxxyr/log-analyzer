# Analyzer - 通用日志分析器框架 (v0.5.8)

基于插件架构的日志分析工具，使用 Rust + `abi_stable` 实现 ABI 稳定的插件系统，支持远程日志获取、多日志源时间线合并和自动化工作流。

## 特性

- **插件化架构** - 支持动态加载分析器插件，无需重新编译主程序
- **ABI 稳定** - 使用 `abi_stable` 确保跨版本二进制兼容性
- **远程连接** - 内置 SSH 连接和 SCP 文件传输，自动下载远程日志
- **配置驱动** - 基于 YAML 配置的工作流编排
- **自动化工作流** - 一键完成发现、下载、分析全流程
- **智能插件选择** - 根据文件模式自动选择合适的分析器
- **多源时间线合并** - 多个日志源对齐到统一时间轴，生成合并甘特图
- **国际化 (i18n)** - 支持中文/英文切换（`--lang en`）
- **系统字体检测** - Linux / macOS 自动查找并注册系统 CJK 字体，无需捆绑字体文件
- **高性能** - Rust 实现，release 开启 fat LTO，使用 mimalloc 内存分配器
- **可扩展** - 轻松开发新的分析器插件

## 快速开始

### 环境要求

- Rust 1.85+（workspace 使用 edition 2024）
- Linux 甘特图渲染需系统安装 CJK 字体（见[故障排查](#甘特图中文乱码或无法生成)）；macOS / Windows 使用系统自带字体

### 安装

```bash
git clone <your-repo>
cd log-analyzer
cargo build --release

# 收集可执行文件、插件和配置到 bin/ 目录
./scripts/deploy.sh              # Linux / macOS
./scripts/deploy-windows.ps1     # Windows
```

`deploy.sh` 会生成如下结构，并打包成 `bin/analyzer-v<版本>.zip` 便于分发：

```
bin/
├── analyzer                     # 可执行文件
├── analyzer-v0.5.8.zip          # 分发压缩包
├── plugins/                     # 插件动态库（.so / .dylib / .dll）
└── configs/                     # 配置文件
```

### 配置

配置文件默认读取 `configs/analyzer.yaml`（可用 `-c` 覆盖）：

```yaml
remote:
  enabled: true
  host: "192.168.5.101"
  port: 22
  user: "linux"
  auth:
    # 优先级：key_file > use_agent > password
    # key_file: "~/.ssh/id_rsa"
    # use_agent: true
    password: "linux"
  log_dir: "/home/linux/.ros/log"
  timeouts:
    connect: 30
    transfer: 300

local:
  log_dir: "./logs"
  output_dir: "./output"
  plugin_dir: "./plugins"
  cleanup_old_output: true       # 分析前清理旧输出

file_discovery:
  sort_by: "mtime"               # mtime | name | size
  sort_order: "desc"             # desc | asc
  auto_select: "latest"          # latest | oldest | none

analyzers:
  - name: "master-control"
    pattern: "master_control_*.log"
    plugin: "master-control-analyzer"
    enabled: true
    required: true               # 缺失则报错
    is_primary: true             # 作为合并时的主时间轴
    use_latest_date_dir: true    # 日志在日期子目录中（如 log/20251226/）
    max_files: 5                 # 最多分析的文件数，CLI -n 可覆盖
```

完整配置项（`multi_file` 多源对齐、`workflow` 重试与进度、`logging`、`advanced`）见 `configs/analyzer.yaml` 内的注释。

### 基本用法

#### 1. 自动模式（推荐）

```bash
# 自动获取最新日志并分析（默认行为）
./analyzer

# 或显式指定
./analyzer auto

# 覆盖文件模式和输出目录
./analyzer auto --pattern "master_control_*.log" --output ./my_output
```

#### 2. 列出远程日志

```bash
# 列出所有日志文件
./analyzer list-remote

# 按模式过滤
./analyzer list-remote "master_control_*.log"
```

#### 3. 分析本地文件

```bash
# 输入文件是位置参数，自动选择插件
./analyzer analyze logs/your.log

# 指定插件
./analyzer analyze logs/your.log --plugin master-control-analyzer

# 指定输出目录
./analyzer analyze logs/your.log -o ./my_output
```

#### 4. 从远程下载并分析

```bash
./analyzer analyze your.log --remote
```

#### 5. 仅下载文件

```bash
./analyzer download your.log
./analyzer download your.log -o ./logs/local_name.log
```

#### 6. 多文件分析

```bash
# 合并多个日志源，生成统一甘特图 <prefix>_merged_gantt.png
./analyzer multi --auto-download
./analyzer multi --prefix merged -o ./output
```

#### 7. 语言切换

```bash
# 使用英文界面
./analyzer --lang en

# 使用中文界面（默认）
./analyzer --lang zh-CN

# 通过环境变量设置
LANG=en_US.UTF-8 ./analyzer
```

#### 8. 其他命令与全局参数

```bash
# 列出所有可用插件
./analyzer list-plugins

# 验证配置文件
./analyzer check-config

# 查看帮助 / 版本
./analyzer --help
./analyzer --version
```

全局参数（所有子命令通用）：

| 参数 | 说明 | 默认值 |
|------|------|--------|
| `-c, --config <PATH>` | 配置文件路径 | `configs/analyzer.yaml` |
| `--log-level <LEVEL>` | 日志级别（trace/debug/info/warn/error） | `info` |
| `--plugin-dir <PATH>` | 插件目录（覆盖配置文件） | 配置文件 |
| `-n, --max-files <N>` | 每个模式最多分析的文件数（覆盖 `max_files`） | 配置文件 |
| `-v, --verbose` | 详细输出 | 关闭 |
| `--lang <LANG>` | 界面语言（`zh-CN` / `en`） | `zh-CN` |

> 软件自身日志时间戳固定为 UTC+8，与日志内容中的北京时间对齐，不受运行机器时区影响。

## 项目结构

```
log-analyzer/
├── Cargo.toml                    # Workspace 配置（版本号唯一权威来源）
├── configs/
│   └── analyzer.yaml             # 主配置文件
├── crates/
│   ├── analyzer-core/            # 核心接口库（插件 API、统一时间线类型）
│   ├── analyzer-cli/             # CLI 主程序（插件加载器 + 工作流调度）
│   ├── analyzer-remote/          # 远程连接模块（SSH/SCP）
│   ├── analyzer-workflow/        # 工作流编排模块（配置、文件发现、插件选择）
│   ├── analyzer-merger/          # 时间线合并模块（多源对齐）
│   ├── analyzer-visualizer/      # 可视化模块（多泳道甘特图）
│   └── plugins/                  # 分析器插件
│       ├── master-control-analyzer/  # 机器人控制系统日志分析器
│       └── cpp-demo-analyzer/        # C++ 互操作示例插件（cxx）
├── scripts/
│   ├── deploy.sh                 # Linux/macOS 部署脚本
│   └── deploy-windows.ps1        # Windows 部署脚本
└── docs/
    ├── PLUGIN_ARCHITECTURE.md    # 插件开发文档
    ├── WORKFLOW_ARCHITECTURE.md  # 工作流架构文档
    ├── FONT_EMBEDDING.md         # （已过时）v0.4.0 前的字体嵌入方案
    └── TUI_GUIDE.md              # （已过时）TUI 模块已于 v0.4.0 移除
```

## 内置插件

### master-control-analyzer

机器人控制系统日志分析器，支持：

- 轮次检测（识别多代循环标记格式，自动跳过 LogNode 模板注册行）
- 大流程分析（完整/不完整流程识别）
- 导航流程与预打舵检测
- 机械臂（规划/执行）、头部、腰部、夹爪动作分析
- 视觉目标检测、障碍物、过渡点等阶段识别
- 暂停时间检测与扣除（重叠暂停区间先合并再累加，避免重复扣除）
- CSV 数据导出
- 甘特图可视化（横轴为扣除暂停后的有效时长）

**输出文件：**

| 文件 | 说明 |
|------|------|
| `output/analysis.csv` | 详细的时序分析数据 |
| `output/action_timeline.csv` | 动作时间轴汇总表 |
| `output/cycle_duration_stats.csv` | 轮次时长统计 |
| `output/cycle_duration_stats.png` | 轮次时长分布图 |
| `output/round_XX_gantt.png` | 每个轮次的甘特图 |
| `output/auto_merged_gantt.png` | 多源合并甘特图（`auto` 模式） |
| `output/<prefix>_merged_gantt.png` | 多源合并甘特图（`multi` 模式，`--prefix` 默认 `merged`） |

**甘特图色块含义：**

| 颜色 | 动作 | 颜色 | 动作 |
|------|------|------|------|
| 浅蓝 | 导航 | 浅黄橙 | 目标检测 |
| 浅黄 | 预打舵 | 浅粉红 | 障碍物 |
| 浅绿 | 手臂 | 淡紫 | 过渡点 |
| 浅橙 | 头部 | 卡其 | 夹爪 |
| 浅紫 | 腰部 | 粉蓝 | 准备阶段 |

### cpp-demo-analyzer

C++ 互操作示例插件，演示如何通过 `cxx` 在插件中调用现有 C++ 分析代码。

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
        // 插件是 cdylib，静态状态与主程序隔离，必须在入口重新设置语言
        rust_i18n::set_locale(args.locale.as_str());
        // 实现分析逻辑
    }
}

#[export_root_module]
pub fn get_root_module() -> AnalyzerPluginModule_Ref {
    AnalyzerPluginModule { create_plugin }.leak_into_prefix()
}

#[sabi_extern_fn]
pub fn create_plugin() -> AnalyzerPlugin_TO<'static, RBox<()>> {
    AnalyzerPlugin_TO::from_value(MyAnalyzer, TD_Opaque)
}
```

## 构建

```bash
# 构建所有组件
cargo build --release

# 单独构建插件
cargo build --package master-control-analyzer --release

# 提交前检查（零警告要求）
cargo fmt --all
cargo clippy --all --all-targets -- -D warnings
```

## 测试

```bash
cargo test --workspace
```

## 依赖

### 核心依赖

- `abi_stable = "0.11"` - ABI 稳定性
- `libloading = "0.9"` - 插件动态加载
- `clap = "4.5"` - CLI 参数解析
- `ssh2 = "0.9"` - SSH / SCP 连接
- `serde_yaml_ng = "0.10"` - YAML 配置
- `chrono` / `chrono-tz` - 时间处理（日志时间戳固定 UTC+8）
- `indicatif` - 传输进度显示
- `tracing` - 结构化日志
- `rust-i18n = "3"` - 国际化
- `mimalloc` - 高性能内存分配器

### 插件依赖

- `plotters` - 图表生成（`ab_glyph` 后端）
- `csv` - 数据导出
- `regex` - 模式匹配
- `cxx` - C++ 互操作（`cpp-demo-analyzer`）

## 故障排查

### 插件加载失败

1. 检查插件文件扩展名（Linux: `.so`, macOS: `.dylib`, Windows: `.dll`）
2. 确保插件与主程序用同一版本的 `abi_stable` 和同一 Rust 工具链编译
3. 检查插件目录路径（配置文件 `local.plugin_dir` 或 `--plugin-dir`）

### 甘特图中文乱码或无法生成

plotters 的 `ab_glyph` 后端不查 fontconfig / CoreText，程序会在启动时主动查找系统 CJK 字体文件并注册。字体缺失时 CSV 分析结果不受影响，仅甘特图跳过。

- **Linux**：通过 `fc-match` 定位字体文件，缺失时按发行版安装
- **macOS**：按候选路径查找系统字体（PingFang 在 macOS 15+ 位于带哈希的 `AssetsV2` 目录，会动态查找；备选 Hiragino Sans GB / STHeiti / Songti / Arial Unicode），通常无需额外安装

```bash
# 安装 CJK 字体（仅 Linux 需要）
# Ubuntu/Debian
sudo apt install fonts-noto-cjk

# Fedora/RHEL
sudo dnf install google-noto-sans-cjk-fonts

# Arch Linux
sudo pacman -S noto-fonts-cjk

# Alpine
apk add font-noto-cjk
```

### SSH 连接失败

```bash
# 检查网络（替换为配置文件中的实际地址）
ping 192.168.5.101

# 测试 SSH
ssh -p 22 linux@192.168.5.101 "echo OK"

# 验证配置
./analyzer check-config
```

## 许可证

Apache-2.0

## 更新日志

### v0.5.8（当前版本）

- 适配新版 master_control 循环标记格式，跳过 LogNode 模板注册行造成的误判
- 修复暂停时间重复扣除导致轮次有效时长归零
- 修复甘特图跨暂停动作条冲出时间轴
- 修复 macOS 甘特图字体不可用（`FontUnavailable`）
- 软件自身日志时间戳固定 UTC+8

### v0.5.7

- 适配新版 master_control 日志格式（视觉、手臂、BT 节点失败/跳过）
- 夹爪相关配置与日志解析适配，新增夹爪统计
- 修复甘特图标题中换行符渲染为方框

### v0.4.0

- 国际化 (i18n)：支持中文/英文界面切换（`--lang en`）
- 系统字体检测：`fc-match` + `register_font` 自动注册系统字体
- 移除捆绑字体文件与 TUI 模块

详细变更见 [CHANGES.md](CHANGES.md)
