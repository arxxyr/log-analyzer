//! Master Control Analyzer Plugin
//!
//! 用于分析机器人控制系统（master_control）日志的插件。
//!
//! # 功能
//!
//! - 解析日志文件并提取时间戳信息
//! - 检测任务轮次（基于循环标记）
//! - 检测大流程（完整/不完整）
//! - 分析导航流程和机械臂操作
//! - 生成 CSV 报告和甘特图

use abi_stable::{
    export_root_module, prefix_type::PrefixTypeTrait, sabi_extern_fn,
    sabi_trait::prelude::TD_Opaque, std_types::*,
};
use analyzer_core::*;
use anyhow::Result;

// 模块声明
pub mod csv_exporter;
pub mod flow_detector;
pub mod gantt;
pub mod models;
pub mod parser;
pub mod round_detector;
pub mod utils;

// 重新导出常用类型
pub use models::{
    ActionOperation, CsvRecord, LogLine, MajorFlow, NavigationFlow, Round, SubStep,
};

// ============================================================================
// 插件实现
// ============================================================================

/// Master Control Analyzer 插件
#[derive(Clone)]
struct MasterControlAnalyzer;

impl AnalyzerPlugin for MasterControlAnalyzer {
    fn metadata(&self) -> PluginMetadata {
        PluginMetadata {
            name: "master-control-analyzer".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            description: "机器人控制系统日志分析器，支持轮次检测、流程分析和甘特图生成".into(),
            author: "loosqk".into(),
            supported_extensions: vec![".log".into(), ".txt".into()].into(),
        }
    }

    fn analyze(&self, args: AnalyzeArgs) -> RResult<AnalyzeResult, RBoxError> {
        // 将 ABI 稳定类型转换为 Rust 原生类型
        let input_file = args.input_file.as_str();
        let output_dir = args.output_dir.as_str();

        // 执行分析
        match run_analysis_internal(input_file, output_dir) {
            Ok(result) => ROk(result),
            Err(e) => {
                // 创建一个简单的错误类型
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

/// 内部分析函数（使用原生 Rust 类型）
///
/// 封装了完整的分析流程，从日志解析到结果输出。
fn run_analysis_internal(input_file: &str, output_dir: &str) -> Result<AnalyzeResult> {
    use csv_exporter::{
        build_csv_records, export_csv, export_major_flow_stats, generate_action_timeline_csv,
    };
    use flow_detector::detect_flows;
    use gantt::generate_gantt_charts;
    use parser::load_log_lines;
    use round_detector::{detect_major_flows, detect_rounds};

    // 1. 加载日志行
    let lines = load_log_lines(input_file)?;
    if lines.is_empty() {
        anyhow::bail!("日志文件中未找到有效的时间戳行");
    }

    let t0 = lines[0].timestamp;
    let t_last = lines.last().unwrap().timestamp;

    // 2. 检测轮次
    let rounds = detect_rounds(&lines, t_last)?;
    let round_count = rounds.len();

    // 3. 检测大流程
    let major_flows = detect_major_flows(&rounds);
    let major_flow_count = major_flows.len();

    // 4. 检测导航流程
    let flows = detect_flows(&lines, &rounds)?;
    let flow_count = flows.len();

    // 统计各类动作
    let mut nav_count = 0;
    let mut arm_count = 0;
    let mut head_count = 0;
    let mut waist_count = 0;
    for flow in &flows {
        nav_count += 1;
        for op in &flow.operations {
            match op.action_type.as_str() {
                "arm" => arm_count += 1,
                "head" => head_count += 1,
                "waist" => waist_count += 1,
                _ => {}
            }
        }
    }

    // 5. 构建CSV记录
    let records = build_csv_records(&flows, &rounds, &major_flows, t0);
    let record_count = records.len();

    // 6. 创建输出目录
    std::fs::create_dir_all(output_dir)?;

    // 7. 导出所有结果
    let mut output_files = Vec::new();

    // 导出主分析CSV
    export_csv(&records, output_dir)?;
    output_files.push(OutputFile {
        path: format!("{}/analysis.csv", output_dir).into(),
        file_type: "csv".into(),
        description: "主分析数据（所有操作的时序记录）".into(),
    });

    // 导出大流程统计
    export_major_flow_stats(&major_flows, output_dir, t0)?;
    output_files.push(OutputFile {
        path: format!("{}/major_flow_stats.csv", output_dir).into(),
        file_type: "csv".into(),
        description: "大流程统计（完整/不完整流程汇总）".into(),
    });

    // 生成甘特图
    generate_gantt_charts(&flows, &rounds, output_dir, t0)?;
    for round in &rounds {
        let width = if rounds.len() >= 100 {
            3
        } else if rounds.len() >= 10 {
            2
        } else {
            1
        };
        output_files.push(OutputFile {
            path: format!("{}/round_{:0width$}_gantt.png", output_dir, round.id, width = width)
                .into(),
            file_type: "png".into(),
            description: format!("轮次 {} 甘特图", round.id).into(),
        });
    }

    // 生成动作时间轴
    generate_action_timeline_csv(&flows, &rounds, output_dir, t0)?;
    output_files.push(OutputFile {
        path: format!("{}/action_timeline.csv", output_dir).into(),
        file_type: "csv".into(),
        description: "动作时间轴汇总表".into(),
    });

    // 8. 构建分析摘要
    let summary = format!(
        "分析完成！\n\
         - 检测到 {} 个轮次\n\
         - 检测到 {} 个大流程\n\
         - 检测到 {} 个导航流程\n\
         - 动作统计: {} 导航, {} 机械臂, {} 头部, {} 腰部\n\
         - 生成 {} 条 CSV 记录\n\
         - 输出目录: {}",
        round_count,
        major_flow_count,
        flow_count,
        nav_count,
        arm_count,
        head_count,
        waist_count,
        record_count,
        output_dir
    );

    Ok(AnalyzeResult {
        summary: summary.into(),
        output_files: output_files.into(),
    })
}

// ============================================================================
// 插件导出
// ============================================================================

// 在插件中导出根模块，使用 analyzer-core 中定义的类型
#[export_root_module]
pub fn get_root_module() -> AnalyzerPluginModule_Ref {
    AnalyzerPluginModule {
        create_plugin,
    }
    .leak_into_prefix()
}

/// 创建插件实例的工厂函数
#[sabi_extern_fn]
pub fn create_plugin() -> AnalyzerPlugin_TO<'static, RBox<()>> {
    AnalyzerPlugin_TO::from_value(MasterControlAnalyzer, TD_Opaque)
}

// ============================================================================
// 公共 API（供直接调用使用，非插件模式）
// ============================================================================

/// 执行日志分析（公共 API）
///
/// 这个函数可以在非插件模式下直接调用，方便测试和独立使用。
///
/// # 参数
///
/// - `input_file`: 日志文件路径
/// - `output_dir`: 输出目录路径
///
/// # 返回
///
/// - `Ok(String)`: 分析成功，返回摘要信息
/// - `Err(anyhow::Error)`: 分析失败
pub fn run_analysis(input_file: &str, output_dir: &str) -> Result<String> {
    let result = run_analysis_internal(input_file, output_dir)?;
    Ok(result.summary.to_string())
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_metadata() {
        let plugin = MasterControlAnalyzer;
        let meta = plugin.metadata();
        assert_eq!(meta.name.as_str(), "master-control-analyzer");
        assert!(meta
            .supported_extensions
            .iter()
            .any(|ext| ext.as_str() == ".log"));
    }
}
