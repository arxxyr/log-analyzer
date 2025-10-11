# Analyzer - 通用日志分析器框架

基于插件架构的日志分析工具，使用 Rust + abi_stable 实现 ABI 稳定的插件系统。

## 特性

- 🔌 **插件化架构** - 支持动态加载分析器插件，无需重新编译主程序
- 🛡️ **ABI 稳定** - 使用 `abi_stable` 确保跨版本二进制兼容性
- ⚡ **高性能** - Rust 实现，零开销抽象
- 🎯 **类型安全** - 保留 Rust 的类型安全特性
- 📊 **可扩展** - 轻松开发新的分析器插件

## 快速开始

### 安装

```bash
git clone <your-repo>
cd master_control_analyzer
cargo build --release
```

### 基本用法

```bash
# 列出所有可用插件
./target/release/analyzer --list

# 分析日志文件（自动选择插件）
./target/release/analyzer -i logs/your.log -o output

# 指定插件
./target/release/analyzer -i logs/your.log -o output --plugin master-control-analyzer
```

## 项目结构

```
analyzer/
├── crates/
│   ├── analyzer-core/         # 核心接口库（定义插件 API）
│   └── analyzer-cli/          # CLI 主程序（插件加载器）
├── plugins/
│   └── master-control-analyzer/  # 机器人控制系统日志分析器
├── docs/
│   └── PLUGIN_ARCHITECTURE.md    # 插件开发文档
└── README.md
```

## 内置插件

### master-control-analyzer

机器人控制系统日志分析器，支持：

- ✅ 轮次检测（基于循环标记）
- ✅ 大流程分析（完整/不完整流程识别）
- ✅ 导航流程检测
- ✅ 机械臂、头部、腰部动作分析
- ✅ CSV 数据导出
- ✅ 甘特图可视化

**使用示例：**

```bash
cargo run --release -- \
  --log logs/master_control_xxx.log \
  --outdir output
```

**输出文件：**
- `analysis.csv` - 详细的时序分析数据
- `major_flow_stats.csv` - 大流程统计
- `round_XX_gantt.png` - 每个轮次的甘特图
- `action_timeline.csv` - 动作时间轴汇总

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

## 架构

### 核心组件

1. **analyzer-core** - 定义 `AnalyzerPlugin` trait 和 ABI 稳定的数据结构
2. **analyzer-cli** - 主程序，负责：
   - 扫描并加载插件目录
   - 根据文件扩展名选择合适的插件
   - 调用插件执行分析
3. **插件** - 实现 `AnalyzerPlugin` trait 的动态库

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

## 依赖

- `abi_stable = "0.11"` - ABI 稳定性
- `anyhow = "1.0"` - 错误处理
- `clap = "4.5"` - CLI 参数解析
- 插件特定依赖（见各插件的 Cargo.toml）

## 开发指南

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

## 更新日志

### v0.2.0 (2025-10-11)

- 🎉 实现基于 `abi_stable` 的插件架构
- ✨ 添加 CLI 插件加载器
- 📝 完善插件开发文档
- ✅ 完整测试通过

### v0.1.0

- 初始版本：master-control-analyzer 单体应用
