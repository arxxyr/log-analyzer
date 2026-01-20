# C++ Demo Analyzer - 使用 cxx 封装的 C++ 插件示例

这是一个演示如何使用 `cxx` crate 将 C++ 代码封装为 analyzer 插件的完整示例。

## 特性

- ✅ **类型安全的 FFI**：使用 cxx 自动生成安全的 Rust-C++ 绑定
- ✅ **零成本抽象**：直接调用 C++，无性能损失
- ✅ **内存安全**：自动管理生命周期，防止内存泄漏
- ✅ **自然的 API**：C++ 侧使用 `rust::Str`、`rust::Vec` 等熟悉类型

## 项目结构

```
cpp-demo-analyzer/
├── Cargo.toml          # Rust 包配置
├── build.rs            # cxx 构建脚本
├── src/
│   ├── lib.rs          # Rust 插件实现
│   └── bridge.rs       # cxx 桥接定义（关键！）
└── cpp/
    ├── analyzer.h      # C++ 头文件
    └── analyzer.cpp    # C++ 实现
```

## 核心文件说明

### 1. bridge.rs - cxx 桥接层

```rust
#[cxx::bridge(namespace = "cpp_demo")]
mod ffi {
    // 定义共享结构体
    struct AnalysisResult {
        summary: String,
        output_files: Vec<String>,
        line_count: i32,
        round_count: i32,
    }

    // 声明 C++ 类型和函数
    unsafe extern "C++" {
        include!("cpp/analyzer.h");
        
        type LogAnalyzer;
        
        fn new_log_analyzer() -> UniquePtr<LogAnalyzer>;
        fn analyze(
            self: Pin<&mut LogAnalyzer>,
            input_file: &str,
            output_dir: &str,
        ) -> Result<Box<AnalysisResult>>;
    }
}
```

**关键点**：
- `#[cxx::bridge]` 宏自动生成所有 FFI 绑定代码
- `struct AnalysisResult` 在 Rust 和 C++ 之间共享
- `&str` 自动转换为 `rust::Str`

### 2. analyzer.h - C++ 头文件

```cpp
#pragma once
#include "rust/cxx.h"  // cxx 类型

namespace cpp_demo {

struct AnalysisResult;  // 前向声明

class LogAnalyzer {
public:
    rust::Box<AnalysisResult> analyze(
        rust::Str input_file,
        rust::Str output_dir
    );
};

std::unique_ptr<LogAnalyzer> new_log_analyzer();
}
```

**关键点**：
- 包含 `rust/cxx.h` 获取 cxx 类型
- 使用 `rust::Str` 而不是 `std::string`
- 使用 `rust::Box<T>` 返回到 Rust

### 3. analyzer.cpp - C++ 实现

```cpp
#include "analyzer.h"
#include "cpp-demo-analyzer/src/bridge.rs.h"  // cxx 生成的头文件

rust::Box<AnalysisResult> LogAnalyzer::analyze(
    rust::Str input_file,
    rust::Str output_dir
) {
    // 转换为 std::string
    std::string path(input_file);
    
    // ... C++ 逻辑 ...
    
    // 使用 in_place 创建结果
    return rust::Box<AnalysisResult>::in_place(
        rust::String("分析完成"),
        rust::Vec<rust::String>{},
        line_count,
        round_count
    );
}
```

**关键点**：
- 引入 `bridge.rs.h` 获取完整的结构体定义
- `rust::Str` 可以直接转换为 `std::string`
- 使用 `rust::Box::in_place()` 创建返回值

### 4. build.rs - 构建脚本

```rust
fn main() {
    cxx_build::bridge("src/bridge.rs")
        .file("cpp/analyzer.cpp")
        .include(".")                  // 添加 include 路径
        .std("c++17")
        .compile("cpp-demo-analyzer");

    println!("cargo:rerun-if-changed=src/bridge.rs");
    println!("cargo:rerun-if-changed=cpp/analyzer.h");
    println!("cargo:rerun-if-changed=cpp/analyzer.cpp");
}
```

## 构建和使用

### 构建插件

```bash
cargo build --package cpp-demo-analyzer --release
```

### 复制到插件目录

```bash
cp target/release/libcpp_demo_analyzer.so target/release/plugins/
```

### 运行

```bash
./target/release/analyzer -i logs/your.log -o output --plugin cpp-demo-analyzer
```

## 输出示例

```
正在扫描插件目录: /path/to/plugins
  ✓ 加载插件: cpp-demo-analyzer
成功加载 1 个插件

使用插件: cpp-demo-analyzer v0.1.0
正在分析: logs/master_control_9683_1760086714557.log
输出目录: cpp_demo_output

[Rust] 创建 C++ 分析器实例
[C++] LogAnalyzer 已创建
[Rust] 调用 C++ analyze 方法
[C++] 开始分析: logs/master_control_9683_1760086714557.log
[C++] 解析了 3530 行日志
[C++] 检测到 12 个轮次
[C++] 生成报告: cpp_demo_output/cpp_demo_analysis.csv
[Rust] C++ 分析完成，转换结果
[C++] LogAnalyzer 已销毁
C++ Demo 分析完成 - 12 轮次, 3530 行
  - 行数: 3530
  - 轮次: 12

分析完成！
```

## 优势总结

### vs 传统 FFI (extern "C")

| 特性 | cxx | 传统 FFI |
|------|-----|----------|
| 类型安全 | ✅ 编译期检查 | ❌ 运行时错误 |
| 内存管理 | ✅ 自动 RAII | ❌ 手动管理 |
| std::string 支持 | ✅ 原生支持 | ❌ 需要 C 字符串转换 |
| std::vector 支持 | ✅ 原生支持 | ❌ 需要手动序列化 |
| 生成代码 | ✅ 自动生成 | ❌ 手写包装层 |

### 适用场景

- ✅ 复用现有 C++ 代码库
- ✅ 需要使用 C++ 特定的库（OpenCV、PCL 等）
- ✅ 团队有 C++ 专长，想贡献分析器
- ✅ 追求性能的核心算法（虽然 Rust 也很快）

## 开发提示

1. **结构体定义**：在 `bridge.rs` 中定义，cxx 会自动生成 C++ 对应版本
2. **类型转换**：使用 cxx 提供的类型（`rust::Str`, `rust::String`, `rust::Vec`）
3. **错误处理**：C++ 异常会自动转换为 Rust 的 `Result::Err`
4. **生命周期**：`rust::Box` 和 `UniquePtr` 自动管理内存
5. **调试**：cxx 生成的代码在 `target/cxxbridge/` 目录

## 参考资料

- [cxx 官方文档](https://cxx.rs/)
- [cxx GitHub](https://github.com/dtolnay/cxx)
- [本项目插件开发文档](../../docs/PLUGIN_ARCHITECTURE.md)
