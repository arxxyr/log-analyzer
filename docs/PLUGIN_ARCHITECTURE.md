# 插件开发指南

## 核心接口

```rust
#[sabi_trait]
pub trait AnalyzerPlugin: Clone + Send + Sync {
    fn metadata(&self) -> PluginMetadata;
    fn analyze(&self, args: AnalyzeArgs) -> RResult<AnalyzeResult, RBoxError>;
}
```

## 快速开始

### 1. 创建项目

```bash
cd plugins
cargo new --lib my-analyzer
```

### 2. 配置 Cargo.toml

```toml
[lib]
crate-type = ["cdylib", "rlib"]  # 必须生成动态库

[dependencies]
analyzer-core = { path = "../../crates/analyzer-core" }
abi_stable = "0.11"
```

### 3. 实现插件

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
        let result = AnalyzeResult {
            summary: "分析完成".into(),
            output_files: vec![].into(),
            timeline: None.into(),
        };
        ROk(result)
    }
}

// 导出插件模块
#[export_root_module]
pub fn get_root_module() -> AnalyzerPluginModule_Ref {
    AnalyzerPluginModule { create_plugin }.leak_into_prefix()
}

#[sabi_extern_fn]
pub fn create_plugin() -> AnalyzerPlugin_TO<'static, RBox<()>> {
    AnalyzerPlugin_TO::from_value(MyAnalyzer, TD_Opaque)
}
```

### 4. 编译安装

```bash
# 编译
cargo build --release --package my-analyzer

# 安装（Linux）
cp target/release/libmy_analyzer.so target/release/plugins/

# 使用
./analyzer analyze -i test.log --plugin my-analyzer
```

## 关键点

### ABI 稳定性

- 使用 `RString`, `RVec`, `ROption` 等类型
- 所有跨 FFI 的数据结构使用 `#[repr(C)]`
- `abi_stable` 版本必须一致

### 错误处理

```rust
// 不要跨 FFI panic，使用 RResult
match do_work() {
    Ok(result) => ROk(result),
    Err(e) => RErr(RBoxError::new(e)),
}
```

### 数据结构

**AnalyzeArgs：**

- `input_file: RString` - 输入文件路径
- `output_dir: RString` - 输出目录
- `extra_args: ROption<RString>` - 额外参数（JSON）

**AnalyzeResult：**

- `summary: RString` - 分析摘要
- `output_files: RVec<OutputFile>` - 生成的文件列表
- `timeline: ROption<Timeline>` - 时间线数据（可选）

## 故障排查

**插件加载失败：**

```bash
# 检查符号导出
nm -D libmy_analyzer.so | grep root_module

# 详细日志
RUST_LOG=debug ./analyzer analyze -i test.log
```

**ABI 不兼容：**

- 确保 `abi_stable` 版本一致
- 使用相同的 Rust 工具链

## 参考

- [abi_stable 文档](https://docs.rs/abi_stable/)
- [master-control-analyzer](../../plugins/master-control-analyzer) - 完整示例
