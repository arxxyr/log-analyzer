# Analyzer - 通用日志分析器框架 (v0.3.3)

基于插件架构的日志分析工具，使用 Rust + abi_stable 实现 ABI 稳定的插件系统，支持远程日志获取和自动化工作流。

## 特性

- **插件化架构** - 支持动态加载分析器插件，无需重新编译主程序
- **ABI 稳定** - 使用 `abi_stable` 确保跨版本二进制兼容性
- **远程连接** - 内置 SSH 连接和 SCP 文件传输，自动下载远程日志
- **配置驱动** - 基于 YAML 配置的工作流编排
- **自动化工作流** - 一键完成发现、下载、分析全流程
- **智能插件选择** - 根据文件模式自动选择合适的分析器
- **高性能** - Rust 实现，使用 mimalloc 内存分配器
- **可扩展** - 轻松开发新的分析器插件
- **TUI 界面** - 可选的交互式终端界面（使用 `--tui` 启用）

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

#### 2. TUI 交互模式（可选）

```bash
# 启用 TUI 界面
./analyzer --tui
```

**TUI 快捷键：**
- `q` 或 `ESC` - 退出
- `p` 或 `空格` - 暂停/恢复
- `↑/↓` - 滚动日志
- `Tab` - 切换焦点（主区域 ⟷ 插件面板）

#### 3. 列出远程日志

```bash
# 列出所有日志文件
./analyzer list-remote

# 按模式过滤
./analyzer list-remote "master_control_*.log"
```

#### 4. 分析本地文件

```bash
# 自动选择插件
./analyzer analyze -i logs/your.log

# 指定插件
./analyzer analyze -i logs/your.log --plugin master-control-analyzer

# 指定输出目录
./analyzer analyze -i logs/your.log -o ./my_output
```

#### 5. 从远程下载并分析

```bash
./analyzer analyze -i your.log --remote
```

#### 6. 仅下载文件

```bash
./analyzer download your.log
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
│   ├── analyzer-tui/             # TUI 界面模块（可选）
│   ├── analyzer-merger/          # 时间线合并模块
│   ├── analyzer-visualizer/      # 可视化模块
│   └── plugins/                  # 分析器插件
│       └── master-control-analyzer/  # 机器人控制系统日志分析器
├── fonts/
│   └── *.ttf                     # 中文字体（甘特图用）
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

### 插件依赖

- `plotters` - 图表生成
- `csv` - 数据导出
- `regex` - 模式匹配

## 故障排查

### 插件加载失败

1. 检查插件文件扩展名（Linux: `.so`, macOS: `.dylib`, Windows: `.dll`）
2. 确保 `abi_stable` 版本一致
3. 检查插件目录路径

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

### v0.3.3 (当前版本)

- CI/CD 优化：使用 Swatinem/rust-cache 加速构建
- 移除 macOS x64 构建，只保留 ARM64
- 语义化版本：Release 使用 `v0.3.3+commit`，Dev 使用 `v0.3.3+date.commit`

### v0.3.2

- 修复暂停时间未从轮次统计中扣除的问题
- 新增 TaskGraphExecutor 暂停检测模式
- 甘特图中文字符显示修复

详细变更见 [CHANGES.md](CHANGES.md)
