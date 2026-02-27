# Analyzer - 通用日志分析器框架 (v0.4.0)

基于插件架构的日志分析工具，使用 Rust + abi_stable 实现 ABI 稳定的插件系统，支持远程日志获取和自动化工作流。

## 特性

- **插件化架构** - 支持动态加载分析器插件，无需重新编译主程序
- **ABI 稳定** - 使用 `abi_stable` 确保跨版本二进制兼容性
- **远程连接** - 内置 SSH 连接和 SCP 文件传输，自动下载远程日志
- **配置驱动** - 基于 YAML 配置的工作流编排
- **自动化工作流** - 一键完成发现、下载、分析全流程
- **智能插件选择** - 根据文件模式自动选择合适的分析器
- **国际化 (i18n)** - 支持中文/英文切换（`--lang en`）
- **系统字体检测** - 自动检测系统 CJK 字体，无需捆绑字体文件
- **高性能** - Rust 实现，使用 mimalloc 内存分配器
- **可扩展** - 轻松开发新的分析器插件

## 快速开始

### 安装

```bash
git clone <your-repo>
cd log-analyzer
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
    password: "password"
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

#### 1. 自动模式（推荐）

```bash
# 自动获取最新日志并分析（默认行为）
./analyzer

# 或显式指定
./analyzer auto
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
# 自动选择插件
./analyzer analyze -i logs/your.log

# 指定插件
./analyzer analyze -i logs/your.log --plugin master-control-analyzer

# 指定输出目录
./analyzer analyze -i logs/your.log -o ./my_output
```

#### 4. 从远程下载并分析

```bash
./analyzer analyze -i your.log --remote
```

#### 5. 仅下载文件

```bash
./analyzer download your.log
```

#### 6. 语言切换

```bash
# 使用英文界面
./analyzer --lang en

# 使用中文界面（默认）
./analyzer --lang zh-CN

# 通过环境变量设置
LANG=en_US.UTF-8 ./analyzer
```

#### 7. 其他命令

```bash
# 列出所有可用插件
./analyzer list-plugins

# 验证配置文件
./analyzer check-config

# 多文件分析
./analyzer multi --auto-download

# 查看帮助
./analyzer --help
```

## 项目结构

```
analyzer/
├── Cargo.toml                    # Workspace 配置
├── configs/
│   └── analyzer.yaml             # 主配置文件
├── crates/
│   ├── analyzer-core/            # 核心接口库（定义插件 API）
│   ├── analyzer-cli/             # CLI 主程序（插件加载器）
│   ├── analyzer-remote/          # 远程连接模块（SSH/SCP）
│   ├── analyzer-workflow/        # 工作流编排模块
│   ├── analyzer-merger/          # 时间线合并模块
│   ├── analyzer-visualizer/      # 可视化模块
│   └── plugins/                  # 分析器插件
│       └── master-control-analyzer/  # 机器人控制系统日志分析器
├── scripts/
│   ├── deploy.sh                 # Linux/macOS 部署脚本
│   └── deploy-windows.ps1        # Windows 部署脚本
└── docs/
    ├── PLUGIN_ARCHITECTURE.md    # 插件开发文档
    └── WORKFLOW_ARCHITECTURE.md  # 工作流架构文档
```

## 内置插件

### master-control-analyzer

机器人控制系统日志分析器，支持：

- 轮次检测（基于循环标记）
- 大流程分析（完整/不完整流程识别）
- 导航流程检测
- 机械臂、头部、腰部动作分析
- 暂停时间检测和扣除
- CSV 数据导出
- 甘特图可视化

**输出文件：**
- `output/analysis.csv` - 详细的时序分析数据
- `output/major_flow_stats.csv` - 大流程统计
- `output/cycle_duration_stats.csv` - 轮次时长统计
- `output/round_XX_gantt.png` - 每个轮次的甘特图
- `output/cycle_duration_stats.png` - 轮次时长分布图

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

# 不带 TUI 的精简版本
cargo build --package analyzer-cli --release --no-default-features
```

## 测试

```bash
cargo test --all
```

## 依赖

### 核心依赖

- `abi_stable = "0.11"` - ABI 稳定性
- `mimalloc` - 高性能内存分配器
- `clap = "4.5"` - CLI 参数解析
- `ssh2 = "0.9"` - SSH 连接
- `serde_yaml = "0.9"` - YAML 配置
- `tracing` - 结构化日志
- `rust-i18n = "3"` - 国际化

### 插件依赖

- `plotters` - 图表生成（`ab_glyph` 后端）
- `csv` - 数据导出
- `regex` - 模式匹配

## 故障排查

### 插件加载失败

1. 检查插件文件扩展名（Linux: `.so`, macOS: `.dylib`, Windows: `.dll`）
2. 确保 `abi_stable` 版本一致
3. 检查插件目录路径

### 甘特图中文乱码或无法生成

程序启动时会自动检测系统 CJK 字体。如果缺少字体，CSV 分析结果不受影响，仅甘特图无法生成。

```bash
# 安装 CJK 字体
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
# 检查网络
ping 192.168.4.69

# 测试 SSH
ssh -p 23 firefly@192.168.4.69 "echo OK"

# 验证配置
./analyzer check-config
```

## 许可证

MIT OR Apache-2.0

## 更新日志

### v0.4.0 (当前版本)

- 国际化 (i18n)：支持中文/英文界面切换（`--lang en`）
- 系统字体检测：通过 `fc-match` + `register_font` 自动注册系统字体
- 移除捆绑字体文件，改用系统 CJK 字体
- 部署脚本简化：不再复制字体目录
- 移除 TUI 模块

### v0.3.3

- CI/CD 优化：使用 Swatinem/rust-cache 加速构建
- 移除 macOS x64 构建，只保留 ARM64
- 语义化版本支持

### v0.3.2

- 修复暂停时间未从轮次统计中扣除的问题
- 新增 TaskGraphExecutor 暂停检测模式
- 甘特图中文字符显示修复

详细变更见 [CHANGES.md](CHANGES.md)
