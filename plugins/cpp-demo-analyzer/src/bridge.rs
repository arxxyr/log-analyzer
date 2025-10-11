//! cxx 桥接层 - 连接 Rust 和 C++

#[cxx::bridge(namespace = "cpp_demo")]
mod ffi {
    // 定义与 C++ 共享的结构体（由 cxx 自动生成 C++ 绑定）
    struct AnalysisResult {
        summary: String,
        output_files: Vec<String>,
        line_count: i32,
        round_count: i32,
    }

    // 声明 C++ 端的类型和函数
    unsafe extern "C++" {
        // 引入 C++ 头文件
        include!("cpp/analyzer.h");

        // C++ 类型声明（只声明类，不重复声明结构体）
        type LogAnalyzer;

        // 工厂函数
        fn new_log_analyzer() -> UniquePtr<LogAnalyzer>;

        // 分析方法（返回我们定义的 AnalysisResult）
        fn analyze(
            self: Pin<&mut LogAnalyzer>,
            input_file: &str,
            output_dir: &str,
        ) -> Result<Box<AnalysisResult>>;  // 使用 Box 而不是 UniquePtr
    }
}

pub use ffi::*;
