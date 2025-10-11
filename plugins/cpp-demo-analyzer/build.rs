//! 构建脚本 - 编译 C++ 代码并设置 cxx 桥接

fn main() {
    // 使用 cxx-build 编译 C++ 代码
    cxx_build::bridge("src/bridge.rs")
        .file("cpp/analyzer.cpp")     // C++ 实现文件
        .include(".")                  // 添加当前目录到 include 路径（用于 cpp/analyzer.h）
        .std("c++17")                  // 使用 C++17 标准
        .flag_if_supported("-Wall")    // 开启警告
        .compile("cpp-demo-analyzer");

    // 告诉 Cargo 监听文件变化
    println!("cargo:rerun-if-changed=src/bridge.rs");
    println!("cargo:rerun-if-changed=cpp/analyzer.h");
    println!("cargo:rerun-if-changed=cpp/analyzer.cpp");
}
