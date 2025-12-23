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
pub mod arm_decision_detector;
pub mod csv_exporter;
pub mod flow_detector;
mod font_loader;
pub mod gantt;
pub mod models;
pub mod parser;
pub mod round_detector;
pub mod utils;

// 重新导出常用类型
pub use models::{
    ActionOperation, ArmDecisionModule, ArmDecisionTask, CsvRecord, LogLine, MajorFlow,
    NavigationFlow, Round, SubStep,
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

/// 查找同目录下的 arm_decision 日志文件
fn find_arm_decision_logs(input_file: &str) -> Vec<String> {
    use std::path::Path;

    let input_path = Path::new(input_file);
    let parent_dir = match input_path.parent() {
        Some(dir) => dir,
        None => return Vec::new(),
    };

    let mut arm_decision_files = Vec::new();

    if let Ok(entries) = std::fs::read_dir(parent_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
                // 匹配 arm_decision*.log 文件
                if filename.starts_with("arm_decision") && filename.ends_with(".log") {
                    if let Some(path_str) = path.to_str() {
                        arm_decision_files.push(path_str.to_string());
                    }
                }
            }
        }
    }

    arm_decision_files
}

/// 内部分析函数（使用原生 Rust 类型）
///
/// 封装了完整的分析流程，从日志解析到结果输出。
fn run_analysis_internal(input_file: &str, output_dir: &str) -> Result<AnalyzeResult> {
    use arm_decision_detector::{detect_arm_decision_tasks, load_arm_decision_log_lines};
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

    // 4.5. 自动查找并加载 arm_decision 日志
    let arm_decision_logs = find_arm_decision_logs(input_file);
    let mut all_arm_decision_tasks = Vec::new();
    for arm_log in &arm_decision_logs {
        if let Ok(arm_lines) = load_arm_decision_log_lines(arm_log) {
            if let Ok(tasks) = detect_arm_decision_tasks(&arm_lines) {
                all_arm_decision_tasks.extend(tasks);
            }
        }
    }
    let arm_decision_task_count = all_arm_decision_tasks.len();

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
    export_major_flow_stats(&major_flows, output_dir, t0, t_last)?;
    output_files.push(OutputFile {
        path: format!("{}/major_flow_stats.csv", output_dir).into(),
        file_type: "csv".into(),
        description: "大流程统计（完整/不完整流程汇总）".into(),
    });

    // 生成甘特图（包含 arm_decision 数据）
    generate_gantt_charts(&flows, &rounds, &all_arm_decision_tasks, output_dir, t0)?;
    for round in &rounds {
        let width = if rounds.len() >= 100 {
            3
        } else if rounds.len() >= 10 {
            2
        } else {
            1
        };
        output_files.push(OutputFile {
            path: format!(
                "{}/round_{:0width$}_gantt.png",
                output_dir,
                round.id,
                width = width
            )
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

    // 8. 构建标准化时间线数据
    let timeline = build_timeline(input_file, &rounds, &flows, t0, t_last)?;

    // 9. 构建分析摘要
    let arm_decision_info = if arm_decision_task_count > 0 {
        format!(
            "\n         - 检测到 {} 个 arm_decision 任务（来自 {} 个日志文件）",
            arm_decision_task_count,
            arm_decision_logs.len()
        )
    } else {
        String::new()
    };

    let summary = format!(
        "分析完成！\n\
         - 检测到 {} 个轮次\n\
         - 检测到 {} 个大流程\n\
         - 检测到 {} 个导航流程\n\
         - 动作统计: {} 导航, {} 手臂, {} 头部, {} 腰部{}\n\
         - 生成 {} 条 CSV 记录\n\
         - 输出目录: {}",
        round_count,
        major_flow_count,
        flow_count,
        nav_count,
        arm_count,
        head_count,
        waist_count,
        arm_decision_info,
        record_count,
        output_dir
    );

    Ok(AnalyzeResult {
        summary: summary.into(),
        output_files: output_files.into(),
        timeline,
    })
}

/// 构建标准化时间线数据
///
/// 将 rounds、flows 和 operations 转换为标准的 Timeline 结构
fn build_timeline(
    source_file: &str,
    rounds: &[Round],
    flows: &[NavigationFlow],
    log_start_time: f64,
    log_end_time: f64,
) -> Result<timeline::Timeline> {
    use analyzer_core::timeline::*;

    let mut events = Vec::new();

    // 1. 添加轮次标记事件
    for (idx, round) in rounds.iter().enumerate() {
        let event = TimelineEvent {
            id: format!("round_{}", round.id).into(),
            track: Track::RoundMarker,
            name: format!("轮次 {}", round.id).into(),
            start_time: round.start_ts,
            end_time: round.end_ts.map(|t| t.into()).into(),
            status: if round.end_ts.is_some() {
                EventStatus::Success
            } else {
                EventStatus::InProgress
            },
            source: "master_control".into(),
            parent_id: RNone,
            metadata: {
                let mut meta = serde_json::json!({
                    "loop_number": round.loop_number,
                    "round_index": idx,
                });
                if let Some(ref pose0) = round.pose0 {
                    meta["pose0"] = serde_json::json!(pose0);
                }
                if let Some(ref pose6) = round.pose6 {
                    meta["pose6"] = serde_json::json!(pose6);
                }
                RSome(meta.to_string().into())
            },
            color_hint: RSome("#E8F4F8".into()), // 浅蓝色
        };
        events.push(event);
    }

    // 2. 添加导航流程事件
    for (flow_idx, flow) in flows.iter().enumerate() {
        let round_id = format!("round_{}", flow.round_id);

        // 导航主事件
        if let (Some(nav_start), Some(nav_end)) = (flow.nav_start_ts, flow.nav_end_ts) {
            let nav_event = TimelineEvent {
                id: format!("nav_{}", flow_idx).into(),
                track: Track::Navigation,
                name: "导航".into(),
                start_time: nav_start,
                end_time: RSome(nav_end),
                status: match flow.nav_status.as_str() {
                    "成功" => EventStatus::Success,
                    "失败" => EventStatus::Failed,
                    "进行中" => EventStatus::InProgress,
                    _ => EventStatus::Success,
                },
                source: "master_control".into(),
                parent_id: RSome(round_id.clone().into()),
                metadata: {
                    let meta = serde_json::json!({
                        "target_pos": flow.nav_target_pos,
                        "target_ori": flow.nav_target_ori,
                        "sub_steps": flow.nav_sub_steps.iter().map(|s| {
                            serde_json::json!({
                                "name": s.name,
                                "timestamp": s.timestamp,
                            })
                        }).collect::<Vec<_>>(),
                    });
                    RSome(meta.to_string().into())
                },
                color_hint: RSome("#ADD8E6".into()), // 浅蓝色
            };
            events.push(nav_event);
        }

        // 3. 添加机械臂、头部、腰部动作事件
        for (op_idx, op) in flow.operations.iter().enumerate() {
            if let (Some(start), Some(end)) = (op.start_ts, op.end_ts) {
                let (track, color) = match op.action_type.as_str() {
                    "arm" => (Track::Arm, "#90EE90"),     // 浅绿色
                    "head" => (Track::Head, "#FFB366"),   // 浅橙色
                    "waist" => (Track::Waist, "#DDA0DD"), // 浅紫色
                    _ => continue,                        // 跳过未知类型
                };

                let op_event = TimelineEvent {
                    id: format!("{}_{}_op_{}", op.action_type, flow_idx, op_idx).into(),
                    track,
                    name: op.label.clone().into(),
                    start_time: start,
                    end_time: RSome(end),
                    status: match op.status.as_str() {
                        "成功" | "completed" => EventStatus::Success,
                        "失败" | "failed" => EventStatus::Failed,
                        "进行中" | "in_progress" => EventStatus::InProgress,
                        "取消" | "cancelled" => EventStatus::Cancelled,
                        _ => EventStatus::Success,
                    },
                    source: "master_control".into(),
                    parent_id: RSome(format!("nav_{}", flow_idx).into()),
                    metadata: {
                        let meta = serde_json::json!({
                            "action_code": op.action_code,
                            "action_type": op.action_type,
                            "sub_steps": op.sub_steps.iter().map(|s| {
                                serde_json::json!({
                                    "name": s.name,
                                    "timestamp": s.timestamp,
                                })
                            }).collect::<Vec<_>>(),
                        });
                        RSome(meta.to_string().into())
                    },
                    color_hint: RSome(color.into()),
                };
                events.push(op_event);
            }
        }
    }

    // 构建最终的 Timeline
    let timeline = Timeline {
        name: "master_control".into(),
        source_file: source_file.into(),
        log_start_time,
        log_end_time,
        events: events.into(),
        is_primary: true, // master_control 是主时间轴
        metadata: {
            let meta = serde_json::json!({
                "round_count": rounds.len(),
                "flow_count": flows.len(),
                "analyzer_version": env!("CARGO_PKG_VERSION"),
            });
            RSome(meta.to_string().into())
        },
    };

    Ok(timeline)
}

// ============================================================================
// 插件导出
// ============================================================================

// 在插件中导出根模块，使用 analyzer-core 中定义的类型
#[export_root_module]
pub fn get_root_module() -> AnalyzerPluginModule_Ref {
    AnalyzerPluginModule { create_plugin }.leak_into_prefix()
}

/// 创建插件实例的工厂函数
#[sabi_extern_fn]
pub fn create_plugin() -> AnalyzerPlugin_TO<'static, RBox<()>> {
    // 初始化字体（提取嵌入的字体）
    use crate::font_loader::FontLoader;
    match FontLoader::new() {
        Ok(loader) => {
            if let Some(path) = loader.font_path() {
                eprintln!("[master-control-analyzer] 字体已准备: {:?}", path);
            }
        }
        Err(e) => {
            eprintln!("[master-control-analyzer] 字体初始化失败: {}", e);
        }
    }

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
        assert!(
            meta.supported_extensions
                .iter()
                .any(|ext| ext.as_str() == ".log")
        );
    }
}
