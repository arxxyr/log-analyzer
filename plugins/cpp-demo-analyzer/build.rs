//! 构建脚本 - 编译 C++ 代码并设置 cxx 桥接

fn main() {
    // 使用 cxx-build 编译 C++ 代码
    let mut build = cxx_build::bridge("src/bridge.rs");

    build
        .file("cpp/analyzer.cpp") // C++ 实现文件
        .include("."); // 添加当前目录到 include 路径（用于 cpp/analyzer.h）

    let target_env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    if target_env == "msvc" {
        // MSVC 特定标志
        build
            .flag_if_supported("/std:c++20")
            .flag_if_supported("/utf-8")     // 强制源码为 UTF-8 编码
            .flag_if_supported("/EHsc")      // 启用标准 C++ 异常处理
            .flag("/W3");                    // 警告级别 3
    } else {
        // GCC/Clang 标志
        build
            .flag_if_supported("-std=c++20")
            .flag_if_supported("-Wall");
    }

    // 尝试编译，如果失败，输出详细信息
    if let Err(e) = build.try_compile("cpp-demo-analyzer") {
        eprintln!("=== C++ 编译失败 ===");
        eprintln!("错误: {}", e);
        eprintln!("===");
        panic!("C++ 编译失败");
    }

    // 告诉 Cargo 监听文件变化
    println!("cargo:rerun-if-changed=src/bridge.rs");
    println!("cargo:rerun-if-changed=cpp/analyzer.h");
    println!("cargo:rerun-if-changed=cpp/analyzer.cpp");
}
