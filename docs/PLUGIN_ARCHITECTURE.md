# 插件架构文档

## 概述

本项目采用基于 `abi_stable` 的插件架构，支持动态加载分析器插件。这种架构具有以下优势：

- ✅ **ABI 稳定性** - 使用 `abi_stable` 确保插件与主程序的二进制兼容性
- ✅ **动态加载** - 无需重新编译主程序即可添加新的分析器
- ✅ **类型安全** - 保留 Rust 的类型安全特性
- ✅ **跨版本兼容** - 不同版本的插件可以共存

## 项目结构

```
analyzer/
├── Cargo.toml                    # Workspace 配置
├── crates/
│   ├── analyzer-core/            # 核心接口库
│   │   ├── Cargo.toml
│   │   └── src/lib.rs            # AnalyzerPlugin trait 定义
│   └── analyzer-cli/             # CLI 主程序
│       ├── Cargo.toml
│       └── src/main.rs           # 插件加载器
└── plugins/
    └── master-control-analyzer/  # 示例插件
        ├── Cargo.toml            # crate-type = ["cdylib", "rlib"]
        └── src/lib.rs            # 插件实现
```

## 核心接口

### AnalyzerPlugin Trait

所有插件必须实现 `AnalyzerPlugin` trait：

```rust
#[sabi_trait]
pub trait AnalyzerPlugin: Clone + Send + Sync {
    /// 获取插件元信息
    fn metadata(&self) -> PluginMetadata;

    /// 执行日志分析
    fn analyze(&self, args: AnalyzeArgs) -> RResult<AnalyzeResult, RBoxError>;
}
```

### 数据结构

#### PluginMetadata
```rust
pub struct PluginMetadata {
    pub name: RString,                          // 插件名称
    pub version: RString,                       // 版本
    pub description: RString,                   // 描述
    pub author: RString,                        // 作者
    pub supported_extensions: RVec<RString>,    // 支持的文件扩展名
}
```

#### AnalyzeArgs
```rust
pub struct AnalyzeArgs {
    pub input_file: RString,              // 输入文件路径
    pub output_dir: RString,              // 输出目录
    pub extra_args: ROption<RString>,     // 额外参数（JSON）
}
```

#### AnalyzeResult
```rust
pub struct AnalyzeResult {
    pub summary: RString,                 // 分析摘要
    pub output_files: RVec<OutputFile>,   // 生成的文件列表
}
```

## 开发新插件

### 1. 创建插件项目

```bash
cd plugins
cargo new --lib my-analyzer
cd my-analyzer
```

### 2. 配置 Cargo.toml

```toml
[package]
name = "my-analyzer"
version = "0.1.0"
edition = "2024"

[lib]
crate-type = ["cdylib", "rlib"]  # 重要：必须生成动态库

[dependencies]
analyzer-core = { path = "../../crates/analyzer-core" }
abi_stable = "0.11"
anyhow = "1.0"
```

### 3. 实现插件

```rust
use abi_stable::{
    export_root_module, prefix_type::PrefixTypeTrait, sabi_extern_fn,
    sabi_trait::prelude::TD_Opaque, std_types::*,
};
use analyzer_core::*;
use anyhow::Result;

// 定义插件结构
#[derive(Clone)]
struct MyAnalyzer;

// 实现 AnalyzerPlugin trait
impl AnalyzerPlugin for MyAnalyzer {
    fn metadata(&self) -> PluginMetadata {
        PluginMetadata {
            name: "my-analyzer".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            description: "我的自定义分析器".into(),
            author: "你的名字".into(),
            supported_extensions: vec![".log".into(), ".txt".into()].into(),
        }
    }

    fn analyze(&self, args: AnalyzeArgs) -> RResult<AnalyzeResult, RBoxError> {
        let input_file = args.input_file.as_str();
        let output_dir = args.output_dir.as_str();

        // 执行分析逻辑
        match run_analysis_internal(input_file, output_dir) {
            Ok(result) => ROk(result),
            Err(e) => {
                #[derive(Debug)]
                struct AnalysisError(String);
                impl std::fmt::Display for AnalysisError {
                    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(f, "{}", self.0)
                    }
                }
                impl std::error::Error for AnalysisError {}

                let err = AnalysisError(format!("{:?}", e));
                RErr(RBoxError::new(err))
            }
        }
    }
}

// 实际的分析函数
fn run_analysis_internal(input_file: &str, output_dir: &str) -> Result<AnalyzeResult> {
    // TODO: 实现你的分析逻辑

    std::fs::create_dir_all(output_dir)?;

    // 构建结果
    Ok(AnalyzeResult {
        summary: format!("分析完成: {}", input_file).into(),
        output_files: vec![
            OutputFile {
                path: format!("{}/result.csv", output_dir).into(),
                file_type: "csv".into(),
                description: "分析结果".into(),
            }
        ].into(),
    })
}

// 导出插件模块
#[export_root_module]
pub fn get_root_module() -> AnalyzerPluginModule_Ref {
    AnalyzerPluginModule {
        create_plugin,
    }
    .leak_into_prefix()
}

// 创建插件实例的工厂函数
#[sabi_extern_fn]
pub fn create_plugin() -> AnalyzerPlugin_TO<'static, RBox<()>> {
    AnalyzerPlugin_TO::from_value(MyAnalyzer, TD_Opaque)
}
```

### 4. 编译插件

```bash
# Debug 模式
cargo build --package my-analyzer

# Release 模式（推荐）
cargo build --package my-analyzer --release
```

### 5. 安装插件

```bash
# 复制动态库到 plugins 目录
cp target/release/libmy_analyzer.so target/release/plugins/
```

## 使用插件

### 列出所有可用插件

```bash
./target/release/analyzer --list
```

### 使用特定插件分析日志

```bash
# 自动选择插件（根据文件扩展名）
./target/release/analyzer -i log.log -o output

# 手动指定插件
./target/release/analyzer -i log.log -o output --plugin my-analyzer

# 指定插件目录
./target/release/analyzer -i log.log -o output --plugin-dir /path/to/plugins
```

## 技术细节

### ABI 稳定性

使用 `abi_stable` crate 确保以下方面的稳定性：

1. **数据布局** - 所有跨 FFI 边界的类型使用 `#[repr(C)]` 和 `StableAbi` derive
2. **函数指针** - 使用 `extern "C"` 调用约定
3. **版本检查** - `RootModule` trait 提供版本信息和兼容性检查

### 根模块系统

每个插件必须导出一个根模块（Root Module）：

```rust
// 在 analyzer-core 中定义
impl abi_stable::library::RootModule for AnalyzerPluginModule_Ref {
    declare_root_module_statics! {AnalyzerPluginModule_Ref}

    const BASE_NAME: &'static str = "analyzer_plugin";
    const NAME: &'static str = "analyzer_plugin";
    const VERSION_STRINGS: abi_stable::sabi_types::VersionStrings =
        package_version_strings!();
}
```

### 错误处理

跨 FFI 边界的错误必须使用 `RBoxError`：

```rust
// 创建符合 ABI 稳定要求的错误类型
#[derive(Debug)]
struct MyError(String);

impl std::fmt::Display for MyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for MyError {}

// 使用
let err = MyError("错误信息".to_string());
RErr(RBoxError::new(err))
```

## 最佳实践

1. **版本管理**
   - 使用语义化版本号
   - 在 `PluginMetadata` 中正确设置版本信息

2. **错误处理**
   - 捕获所有可能的错误，不要让 panic 跨越 FFI 边界
   - 提供清晰的错误消息

3. **性能优化**
   - 使用 Release 模式编译插件
   - 考虑使用 `mimalloc` 等高性能内存分配器

4. **测试**
   - 编写单元测试（使用 `rlib` 模式）
   - 编写集成测试验证插件加载

5. **文档**
   - 为每个插件编写 README
   - 说明支持的日志格式和输出格式

## 故障排查

### 插件加载失败

1. **检查文件扩展名**
   - Linux: `.so`
   - macOS: `.dylib`
   - Windows: `.dll`

2. **检查 ABI 兼容性**
   - 确保 `abi_stable` 版本一致
   - 确保使用相同的 Rust 工具链版本

3. **检查符号导出**
   ```bash
   # Linux
   nm -D libmy_analyzer.so | grep root_module

   # 应该能看到导出的符号
   ```

### 运行时错误

1. **查看详细日志**
   ```bash
   RUST_LOG=debug ./target/release/analyzer -i log.log -o output
   ```

2. **检查内存安全**
   - 使用 `valgrind` 或 `miri` 检测内存问题
   - 确保所有跨 FFI 的指针都是有效的

## 参考资料

- [abi_stable 文档](https://docs.rs/abi_stable/)
- [Rust FFI 指南](https://doc.rust-lang.org/nomicon/ffi.html)
- [本项目 GitHub 仓库](https://github.com/your-repo)
