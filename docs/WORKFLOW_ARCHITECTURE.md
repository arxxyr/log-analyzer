# 工作流架构设计

## 目标

实现 SSH 远程日志获取、自动插件选择和分析的完整工作流。

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
│  - 插件选择               │    └──────────────────────────┘
│  - 流程编排               │
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

## 核心模块

### 1. analyzer-remote - 远程连接

- SSH 连接管理 (`ssh.rs`)
- SCP 文件传输 (`transfer.rs`)
- 支持密钥、Agent、密码认证

### 2. analyzer-workflow - 工作流编排

- 配置管理 (`config.rs`) - YAML 配置解析和验证
- 文件发现 (`discoverer.rs`) - 本地/远程文件查找和排序
- 插件选择 (`selector.rs`) - 基于模式自动选择插件
- 流程编排 (`orchestrator.rs`) - 完整工作流调度

### 3. analyzer-core - 插件接口

定义 `AnalyzerPlugin` trait 和 ABI 稳定类型。

### 4. analyzer-cli - CLI 入口

命令行接口和工作流调度。

## CLI 命令

```bash
# 自动模式（获取最新日志并分析）
analyzer auto

# 分析本地文件
analyzer analyze -i logs/file.log

# 从远程获取并分析
analyzer analyze -i file.log --remote

# 列出远程文件
analyzer list-remote [pattern]

# 仅下载
analyzer download file.log

# 列出插件
analyzer list-plugins

# 验证配置
analyzer check-config

# 使用自定义配置
analyzer --config my_config.yaml auto
```

## 配置文件

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

workflow:
  auto_download: true
  auto_analyze: true
```

## 技术栈

### 核心依赖

- `ssh2` - SSH 连接和 SCP 传输
- `serde_yaml` - YAML 配置解析
- `glob` - 文件模式匹配
- `abi_stable` - ABI 稳定性
- `clap` - CLI 参数解析
- `tracing` - 结构化日志

## 工作流阶段

1. **连接远程** - SSH 连接到目标主机
2. **发现文件** - 列出并排序日志文件
3. **下载文件** - SCP 传输到本地
4. **选择插件** - 根据文件模式匹配插件
5. **运行分析** - 调用插件分析日志
6. **输出结果** - 生成 CSV、甘特图等

## 优势

- ✅ 统一工具链 - 单一二进制
- ✅ 真正跨平台 - Windows/Linux/macOS
- ✅ 配置驱动 - 灵活可扩展
- ✅ 类型安全 - Rust 编译时检查
- ✅ 进度显示 - 友好用户体验
- ✅ 易于测试 - 模块化设计
