# 工作流架构设计

## 目标

将 `analyze.sh` 的功能移植到 Rust 程序中，实现：
- 🔌 **模块化设计** - 各功能独立可测试
- 📝 **配置驱动** - 通过 YAML 配置文件管理
- 🧩 **插件集成** - 自动根据文件模式选择插件
- 🌍 **跨平台支持** - Linux 和 Windows 统一使用

---

## 架构概览

```
┌─────────────────────────────────────────────────┐
│          analyzer-cli (CLI 入口)                │
│  - 命令行参数解析                                │
│  - 配置文件加载                                  │
│  - 工作流调度                                    │
└────────────┬────────────────────────────────────┘
             │
             ├──────────────────────────────────┐
             │                                  │
┌────────────▼────────────┐    ┌───────────────▼──────────┐
│  analyzer-workflow       │    │  analyzer-remote         │
│  (工作流编排)             │◄───┤  (远程连接)              │
│  - 配置管理               │    │  - SSH 连接管理          │
│  - 文件发现               │    │  - SCP/SFTP 传输         │
│  - 插件选择               │    │  - 远程命令执行          │
│  - 流程编排               │    └──────────────────────────┘
└────────────┬────────────┘
             │
             │ 加载插件
             │
┌────────────▼────────────┐
│  analyzer-core           │
│  (插件接口)               │
│  - AnalyzerPlugin trait  │
│  - ABI 稳定类型          │
└────────────┬────────────┘
             │
             │ 实现
             │
┌────────────▼────────────┐
│  各类分析器插件           │
│  - master-control       │
│  - navigation           │
│  - vision               │
│  - ...                  │
└─────────────────────────┘
```

---

## 核心模块设计

### 1. analyzer-remote (远程连接模块)

**职责：** SSH 连接管理和文件传输

#### 1.1 SSH 连接管理 (`ssh.rs`)

```rust
/// SSH 连接配置
pub struct SshConfig {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub auth: AuthMethod,
    pub timeouts: Timeouts,
}

/// 认证方式
pub enum AuthMethod {
    /// SSH 密钥文件
    KeyFile(PathBuf),
    /// SSH Agent
    Agent,
    /// 密码（不推荐）
    Password(String),
    /// 交互式（运行时输入）
    Interactive,
}

/// 超时配置
pub struct Timeouts {
    pub connect: Duration,
    pub transfer: Duration,
}

/// SSH 连接管理器
pub struct SshConnection {
    session: Session,
    config: SshConfig,
}

impl SshConnection {
    /// 建立连接
    pub fn connect(config: SshConfig) -> Result<Self>;

    /// 执行远程命令
    pub fn execute(&mut self, command: &str) -> Result<String>;

    /// 列出远程目录文件
    pub fn list_files(&mut self, path: &str, pattern: &str) -> Result<Vec<RemoteFile>>;

    /// 关闭连接
    pub fn disconnect(self) -> Result<()>;
}
```

#### 1.2 文件传输 (`transfer.rs`)

```rust
/// 远程文件信息
pub struct RemoteFile {
    pub path: PathBuf,
    pub name: String,
    pub size: u64,
    pub mtime: SystemTime,
}

/// 传输进度回调
pub trait TransferProgress {
    fn on_progress(&mut self, bytes: u64, total: u64);
    fn on_complete(&mut self);
}

/// 文件传输器
pub struct FileTransfer {
    connection: SshConnection,
}

impl FileTransfer {
    /// 下载单个文件
    pub fn download(
        &mut self,
        remote_path: &Path,
        local_path: &Path,
        progress: Option<&mut dyn TransferProgress>,
    ) -> Result<()>;

    /// 批量下载
    pub fn download_batch(
        &mut self,
        files: Vec<(PathBuf, PathBuf)>,
        progress: Option<&mut dyn TransferProgress>,
    ) -> Result<Vec<Result<PathBuf>>>;

    /// 上传文件（用于回传结果，可选功能）
    pub fn upload(
        &mut self,
        local_path: &Path,
        remote_path: &Path,
    ) -> Result<()>;
}
```

---

### 2. analyzer-workflow (工作流编排模块)

**职责：** 配置管理、文件发现、插件选择、流程编排

#### 2.1 配置管理 (`config.rs`)

```rust
use serde::{Deserialize, Serialize};

/// 主配置
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AnalyzerConfig {
    pub remote: RemoteConfig,
    pub local: LocalConfig,
    pub file_discovery: FileDiscoveryConfig,
    pub analyzers: Vec<AnalyzerMapping>,
    pub workflow: WorkflowConfig,
    pub logging: LoggingConfig,
    pub advanced: AdvancedConfig,
}

/// 远程配置
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RemoteConfig {
    pub enabled: bool,
    pub host: String,
    pub port: u16,
    pub user: String,
    pub auth: AuthConfig,
    pub log_dir: PathBuf,
    pub timeouts: TimeoutsConfig,
}

/// 分析器映射规则
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AnalyzerMapping {
    pub name: String,
    pub pattern: String,            // Glob 模式
    pub plugin: String,
    pub description: String,
    pub enabled: bool,
    #[serde(default)]
    pub priority: i32,              // 优先级（数字越大越优先）
    pub config: Option<serde_json::Value>,  // 插件特定配置
}

impl AnalyzerConfig {
    /// 从文件加载配置
    pub fn load_from_file(path: &Path) -> Result<Self>;

    /// 合并多个配置（支持配置继承）
    pub fn merge(base: Self, override_cfg: Self) -> Self;

    /// 验证配置有效性
    pub fn validate(&self) -> Result<()>;
}
```

#### 2.2 文件发现 (`discoverer.rs`)

```rust
use glob::Pattern;

/// 文件信息（统一本地和远程）
#[derive(Debug, Clone)]
pub struct FileInfo {
    pub path: PathBuf,
    pub name: String,
    pub size: u64,
    pub mtime: SystemTime,
    pub is_remote: bool,
}

/// 文件发现器
pub struct FileDiscoverer {
    config: FileDiscoveryConfig,
    remote_connection: Option<SshConnection>,
}

impl FileDiscoverer {
    /// 创建发现器
    pub fn new(config: FileDiscoveryConfig) -> Self;

    /// 设置远程连接（可选）
    pub fn with_remote(mut self, conn: SshConnection) -> Self;

    /// 发现本地文件
    pub fn discover_local(&self, pattern: &str) -> Result<Vec<FileInfo>>;

    /// 发现远程文件
    pub fn discover_remote(&mut self, pattern: &str) -> Result<Vec<FileInfo>>;

    /// 统一发现（自动判断本地/远程）
    pub fn discover(&mut self, pattern: &str) -> Result<Vec<FileInfo>>;

    /// 根据配置排序文件
    pub fn sort_files(&self, files: &mut [FileInfo]);

    /// 自动选择文件（latest/oldest）
    pub fn auto_select(&self, files: Vec<FileInfo>) -> Option<FileInfo>;
}
```

#### 2.3 插件选择器 (`selector.rs`)

```rust
use glob::Pattern;

/// 插件选择器
pub struct PluginSelector {
    mappings: Vec<AnalyzerMapping>,
}

impl PluginSelector {
    /// 创建选择器
    pub fn new(mappings: Vec<AnalyzerMapping>) -> Self;

    /// 根据文件名选择插件
    pub fn select_plugin(&self, file_name: &str) -> Option<&AnalyzerMapping>;

    /// 列出所有启用的插件
    pub fn list_enabled(&self) -> Vec<&AnalyzerMapping>;

    /// 按优先级排序
    fn sort_by_priority(&mut self);
}

impl PluginSelector {
    fn matches_pattern(pattern: &str, file_name: &str) -> bool {
        Pattern::new(pattern)
            .map(|p| p.matches(file_name))
            .unwrap_or(false)
    }
}
```

#### 2.4 工作流编排器 (`orchestrator.rs`)

```rust
/// 工作流步骤
#[derive(Debug)]
pub enum WorkflowStep {
    /// 连接远程
    ConnectRemote,
    /// 发现文件
    DiscoverFiles { pattern: String },
    /// 下载文件
    DownloadFile { remote: PathBuf, local: PathBuf },
    /// 选择插件
    SelectPlugin { file_name: String },
    /// 加载插件
    LoadPlugin { plugin_name: String },
    /// 运行分析
    RunAnalysis { input: PathBuf, output: PathBuf },
    /// 清理
    Cleanup,
}

/// 工作流结果
#[derive(Debug)]
pub struct WorkflowResult {
    pub success: bool,
    pub steps_completed: Vec<WorkflowStep>,
    pub output_files: Vec<PathBuf>,
    pub errors: Vec<String>,
}

/// 工作流编排器
pub struct WorkflowOrchestrator {
    config: AnalyzerConfig,
    remote_connection: Option<SshConnection>,
    discoverer: FileDiscoverer,
    selector: PluginSelector,
    plugin_loader: PluginLoader,  // 来自 analyzer-cli
}

impl WorkflowOrchestrator {
    /// 创建编排器
    pub fn new(config: AnalyzerConfig) -> Result<Self>;

    /// 自动模式：获取最新日志并分析
    pub fn run_auto(&mut self) -> Result<WorkflowResult>;

    /// 指定文件模式
    pub fn run_with_file(&mut self, file: &str) -> Result<WorkflowResult>;

    /// 列出远程可用日志
    pub fn list_remote_logs(&mut self, pattern: &str) -> Result<Vec<FileInfo>>;

    /// 仅下载模式
    pub fn download_only(&mut self, file: &str) -> Result<PathBuf>;

    // 内部实现
    fn connect_remote(&mut self) -> Result<()>;
    fn discover_and_select(&mut self, pattern: &str) -> Result<FileInfo>;
    fn download_file(&mut self, file: &FileInfo) -> Result<PathBuf>;
    fn analyze_file(&mut self, file_path: &Path) -> Result<Vec<PathBuf>>;
}
```

---

## 命令行接口设计

### CLI 参数

```bash
# 自动模式（获取最新日志并分析）
analyzer --auto

# 指定本地文件
analyzer -i logs/master_control_123.log -o output/

# 从远程获取并分析特定文件
analyzer --remote master_control_123.log

# 列出远程可用的日志
analyzer --list-remote [pattern]

# 使用自定义配置
analyzer --auto --config my_config.yaml

# 只下载不分析
analyzer --download-only master_control_123.log

# 手动指定插件（覆盖自动选择）
analyzer -i file.log --plugin master-control-analyzer

# 列出所有可用插件
analyzer --list-plugins

# 验证配置文件
analyzer --check-config

# 调试模式
analyzer --auto --verbose
```

### 更新后的 CLI 结构

```rust
// crates/analyzer-cli/src/main.rs

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "analyzer")]
#[command(about = "日志分析工具（支持远程获取和插件化分析）", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// 配置文件路径
    #[arg(short, long, default_value = "configs/analyzer.yaml")]
    config: PathBuf,

    /// 日志级别
    #[arg(short, long, default_value = "info")]
    log_level: String,

    /// 详细输出
    #[arg(short, long)]
    verbose: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// 自动模式：获取最新日志并分析
    Auto {
        /// 指定文件模式（覆盖配置文件）
        #[arg(short, long)]
        pattern: Option<String>,
    },

    /// 分析本地或远程文件
    Analyze {
        /// 输入文件（本地路径或远程文件名）
        #[arg(short, long)]
        input: String,

        /// 输出目录
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// 手动指定插件
        #[arg(long)]
        plugin: Option<String>,

        /// 从远程获取
        #[arg(long)]
        remote: bool,
    },

    /// 列出远程可用的日志文件
    ListRemote {
        /// 文件模式
        pattern: Option<String>,
    },

    /// 列出所有可用的分析器插件
    ListPlugins,

    /// 下载文件（不分析）
    Download {
        /// 远程文件名
        file: String,

        /// 本地保存路径
        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    /// 验证配置文件
    CheckConfig,
}
```

---

## 技术栈

### 核心依赖

```toml
# analyzer-remote/Cargo.toml
[dependencies]
ssh2 = "0.9"           # SSH 连接和 SCP 传输
thiserror = "1.0"      # 错误定义
tracing = "0.1"        # 日志

# analyzer-workflow/Cargo.toml
[dependencies]
serde = { version = "1.0", features = ["derive"] }
serde_yaml = "0.9"     # YAML 配置解析
serde_json = "1.0"     # JSON 配置支持
glob = "0.3"           # 文件模式匹配
thiserror = "1.0"
tracing = "0.1"
indicatif = "0.17"     # 进度条显示

analyzer-core = { path = "../analyzer-core" }
analyzer-remote = { path = "../analyzer-remote" }

# analyzer-cli/Cargo.toml
[dependencies]
clap = { version = "4.5", features = ["derive"] }
tracing-subscriber = "0.3"  # 日志订阅器
abi_stable = "0.11"

analyzer-core = { path = "../analyzer-core" }
analyzer-workflow = { path = "../analyzer-workflow" }
analyzer-remote = { path = "../analyzer-remote" }
```

---

## 实现路线图

### 阶段 1：基础设施（1-2 天）

- [ ] 创建 `analyzer-remote` crate
  - [ ] SSH 连接管理
  - [ ] SCP 文件传输
  - [ ] 单元测试
- [ ] 创建 `analyzer-workflow` crate
  - [ ] 配置文件解析和验证
  - [ ] 单元测试

### 阶段 2：文件发现（1 天）

- [ ] 实现 `FileDiscoverer`
  - [ ] 本地文件查找
  - [ ] 远程文件列表
  - [ ] 文件排序和过滤
- [ ] 编写集成测试

### 阶段 3：插件选择（0.5 天）

- [ ] 实现 `PluginSelector`
- [ ] 编写规则匹配测试

### 阶段 4：工作流编排（1-2 天）

- [ ] 实现 `WorkflowOrchestrator`
- [ ] 集成所有模块
- [ ] 错误处理和重试逻辑
- [ ] 进度显示

### 阶段 5：CLI 集成（1 天）

- [ ] 更新 `analyzer-cli`
- [ ] 添加新的子命令
- [ ] 命令行参数处理
- [ ] 帮助文档

### 阶段 6：测试和文档（1 天）

- [ ] 端到端测试
- [ ] 用户文档
- [ ] 示例配置文件
- [ ] 迁移指南（从 shell 脚本）

### 阶段 7：优化和发布（可选）

- [ ] 性能优化（并发下载）
- [ ] Windows 兼容性测试
- [ ] 缓存优化
- [ ] 发布 v0.3.0

**总计：5-8 天工作量**

---

## 向后兼容性

### 保留 Shell 脚本

初期可以保留 `analyze.sh`，让用户逐步迁移：

```bash
# analyze.sh (简化版)
#!/bin/bash
# 注意：此脚本已过时，请使用 `analyzer --auto` 替代

echo "警告：analyze.sh 将在未来版本中移除"
echo "请使用新的命令：analyzer --auto"
echo ""
echo "继续执行（3秒后）..."
sleep 3

# 转发到新的 analyzer
./analyzer --auto "$@"
```

### 配置迁移

提供配置生成工具：

```bash
# 从旧的环境变量生成新配置
analyzer --generate-config --from-env > configs/analyzer.yaml
```

---

## 优势总结

1. ✅ **统一工具链** - 单一二进制，不依赖 Shell
2. ✅ **真正跨平台** - Windows/Linux/macOS 统一
3. ✅ **配置驱动** - 灵活的 YAML 配置，易于扩展
4. ✅ **类型安全** - Rust 编译时检查，减少运行时错误
5. ✅ **与插件系统集成** - 自动选择合适的分析器
6. ✅ **更好的错误处理** - 详细的错误信息和重试机制
7. ✅ **进度显示** - 友好的用户体验
8. ✅ **易于测试** - 模块化设计，单元测试全覆盖

---

## 风险和挑战

1. **SSH 依赖复杂度**
   - 需要处理多种认证方式
   - Windows 下 SSH 配置可能不同
   - **缓解：** 提供详细的配置文档和错误提示

2. **配置文件复杂度**
   - 过多配置项可能让用户困惑
   - **缓解：** 提供合理的默认值和配置向导

3. **性能问题**
   - 大文件传输可能较慢
   - **缓解：** 实现增量传输、断点续传（可选）

4. **向后兼容**
   - 用户习惯旧的 Shell 脚本
   - **缓解：** 提供迁移指南和兼容层

---

## 下一步

建议按以下顺序推进：

1. **原型验证** - 先实现最小可用版本（MVP）
   - 远程连接 + 文件下载
   - 配置文件解析
   - 基本工作流

2. **逐步迭代** - 完善功能
   - 插件选择
   - 进度显示
   - 错误处理

3. **生产就绪** - 优化和测试
   - 性能优化
   - 跨平台测试
   - 文档完善

是否需要我开始实现某个模块的原型代码？
