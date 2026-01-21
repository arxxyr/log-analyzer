//! C++ Demo Analyzer Plugin
//!
//! 展示如何使用 cxx 封装 C++ 代码作为分析器插件

use abi_stable::{
    export_root_module, prefix_type::PrefixTypeTrait, sabi_extern_fn,
    sabi_trait::prelude::TD_Opaque, std_types::*,
};
use analyzer_core::*;

mod bridge;
use bridge::*;

/// C++ Demo 分析器插件
#[derive(Clone)]
struct CppDemoAnalyzer;

impl AnalyzerPlugin for CppDemoAnalyzer {
    fn metadata(&self) -> PluginMetadata {
        PluginMetadata {
            name: "cpp-demo-analyzer".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            description: "C++ 实现的演示分析器（使用 cxx 封装）".into(),
            author: "loosqk".into(),
            supported_extensions: vec![".log".into(), ".txt".into()].into(),
        }
    }

    fn analyze(&self, args: AnalyzeArgs) -> RResult<AnalyzeResult, RBoxError> {
        let input_file = args.input_file.as_str();
        let output_dir = args.output_dir.as_str();

        // 调用 C++ 分析逻辑
        match run_cpp_analysis(input_file, output_dir) {
            Ok(result) => ROk(result),
            Err(e) => {
                #[derive(Debug)]
                struct CppAnalysisError(String);

                impl std::fmt::Display for CppAnalysisError {
                    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(f, "C++ 分析失败: {}", self.0)
                    }
                }

                impl std::error::Error for CppAnalysisError {}

                let err = CppAnalysisError(format!("{:?}", e));
                RErr(RBoxError::new(err))
            }
        }
    }
}

/// 调用 C++ 分析逻辑
fn run_cpp_analysis(input_file: &str, output_dir: &str) -> anyhow::Result<AnalyzeResult> {
    println!("[Rust] 创建 C++ 分析器实例");

    // 创建 C++ 分析器（通过 cxx 桥接）
    let mut analyzer = new_log_analyzer();

    // 创建输出目录
    std::fs::create_dir_all(output_dir)?;

    println!("[Rust] 调用 C++ analyze 方法");

    // 调用 C++ 的 analyze 方法
    let cpp_result = analyzer.pin_mut().analyze(input_file, output_dir)?;

    println!("[Rust] C++ 分析完成，转换结果");

    // 转换 C++ 结果为 Rust 插件结果
    let output_files: Vec<OutputFile> = cpp_result
        .output_files
        .iter()
        .map(|path| OutputFile {
            path: path.clone().into(),
            file_type: "csv".into(),
            description: "C++ 生成的分析报告".into(),
        })
        .collect();

    // 创建一个空的时间线（demo 插件不生成真实事件）
    let timeline = timeline::Timeline {
        name: "cpp_demo".into(),
        source_file: input_file.into(),
        log_start_time: 0.0,
        log_end_time: 0.0,
        events: RVec::new(),
        is_primary: false,
        metadata: RNone,
    };

    Ok(AnalyzeResult {
        summary: format!(
            "{}\n  - 行数: {}\n  - 轮次: {}",
            cpp_result.summary, cpp_result.line_count, cpp_result.round_count
        )
        .into(),
        output_files: output_files.into(),
        timeline,
    })
}

// ============================================================================
// 插件导出
// ============================================================================

#[export_root_module]
pub fn get_root_module() -> AnalyzerPluginModule_Ref {
    AnalyzerPluginModule { create_plugin }.leak_into_prefix()
}

#[sabi_extern_fn]
pub fn create_plugin() -> AnalyzerPlugin_TO<'static, RBox<()>> {
    AnalyzerPlugin_TO::from_value(CppDemoAnalyzer, TD_Opaque)
}
