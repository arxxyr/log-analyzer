//! arm_decision Analyzer Plugin
//!
//! 用于分析机械臂决策系统（arm_decision）日志的插件。
//!
//! # 功能
//!
//! - 解析 arm_decision 日志文件
//! - 检测任务边界（Received goal 到 result->message）
//! - 检测 BodyTask 内的各个模块执行情况
//! - 生成 CSV 报告和时间线数据

use abi_stable::{
    export_root_module, prefix_type::PrefixTypeTrait, sabi_extern_fn,
    sabi_trait::prelude::TD_Opaque, std_types::*,
};
use analyzer_core::*;
use anyhow::Result;
use serde::{Deserialize, Serialize};

// ============================================================================
// 责任链上下文（从主分析器传递）
// ============================================================================

/// 轮次时间范围
#[derive(Debug, Clone, Serialize, Deserialize)]
struct RoundTimeRange {
    round_id: usize,
    start: f64,
    end: f64,
}

/// 从主分析器传递的上下文
#[derive(Debug, Clone, Serialize, Deserialize)]
struct AnalyzerContext {
    round_time_ranges: Vec<RoundTimeRange>,
}

impl AnalyzerContext {
    /// 查找任务属于哪个轮次（如果在轮次时间范围内）
    fn find_round_for_task(&self, task_start: f64, task_end: Option<f64>) -> Option<usize> {
        let task_end = task_end.unwrap_or(task_start);

        for range in &self.round_time_ranges {
            // 任务与轮次有时间重叠则认为属于该轮次
            if task_start <= range.end && task_end >= range.start {
                return Some(range.round_id);
            }
        }
        None
    }
}

// 模块声明
pub mod csv_exporter;
pub mod detector;
pub mod models;

// 重新导出常用类型
pub use models::{ArmDecisionModule, ArmDecisionTask, CsvRecord, LogLine};

// ============================================================================
// 插件实现
// ============================================================================

/// arm_decision Analyzer 插件
#[derive(Clone)]
struct ArmDecisionAnalyzer;

impl AnalyzerPlugin for ArmDecisionAnalyzer {
    fn metadata(&self) -> PluginMetadata {
        PluginMetadata {
            name: "arm-decision-analyzer".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            description: "机械臂决策系统日志分析器，支持任务检测和模块耗时分析".into(),
            author: "loosqk".into(),
            supported_extensions: vec![".log".into(), ".txt".into()].into(),
        }
    }

    fn analyze(&self, args: AnalyzeArgs) -> RResult<AnalyzeResult, RBoxError> {
        let input_file = args.input_file.as_str();
        let output_dir = args.output_dir.as_str();

        // 解析上下文（如果有）
        let context: Option<AnalyzerContext> = args
            .extra_args
            .as_ref()
            .map(|s| s.as_str())
            .into_option()
            .and_then(|s| serde_json::from_str(s).ok());

        match run_analysis_internal(input_file, output_dir, context.as_ref()) {
            ROk(result) => ROk(result),
            RErr(e) => {
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

/// 内部分析函数
fn run_analysis_internal(
    input_file: &str,
    output_dir: &str,
    context: Option<&AnalyzerContext>,
) -> RResult<AnalyzeResult, anyhow::Error> {
    use csv_exporter::{build_csv_records, export_csv, export_task_summary};
    use detector::{detect_tasks, load_log_lines};

    // 1. 加载日志行
    let lines = match load_log_lines(input_file) {
        Ok(l) => l,
        Err(e) => return RErr(e),
    };

    if lines.is_empty() {
        return RErr(anyhow::anyhow!("日志文件中未找到有效的时间戳行"));
    }

    let t0 = lines[0].timestamp;
    let t_last = lines.last().unwrap().timestamp;

    // 2. 检测任务
    let all_tasks = match detect_tasks(&lines) {
        Ok(t) => t,
        Err(e) => return RErr(e),
    };

    // 3. 如果有上下文，过滤只保留与轮次时间重叠的任务
    let (tasks, filtered_count) = if let Some(ctx) = context {
        let filtered: Vec<_> = all_tasks
            .into_iter()
            .filter(|task| {
                let task_end = task.body_task_end_ts.or(task.end_ts);
                ctx.find_round_for_task(task.start_ts, task_end).is_some()
            })
            .collect();
        let filtered_count = filtered.len();
        (filtered, Some(filtered_count))
    } else {
        (all_tasks, None)
    };

    let task_count = tasks.len();
    let module_count: usize = tasks.iter().map(|t| t.modules.len()).sum();

    // 4. 构建 CSV 记录
    let records = build_csv_records(&tasks, t0);
    let record_count = records.len();

    // 5. 创建输出目录
    if let Err(e) = std::fs::create_dir_all(output_dir) {
        return RErr(e.into());
    }

    // 6. 导出结果
    let mut output_files = Vec::new();

    // 导出详细分析 CSV
    if let Err(e) = export_csv(&records, output_dir) {
        return RErr(e);
    }
    output_files.push(OutputFile {
        path: format!("{}/arm_decision_analysis.csv", output_dir).into(),
        file_type: "csv".into(),
        description: "arm_decision 详细分析（任务和模块时序）".into(),
    });

    // 导出任务汇总
    if let Err(e) = export_task_summary(&tasks, output_dir, t0) {
        return RErr(e);
    }
    output_files.push(OutputFile {
        path: format!("{}/arm_decision_summary.csv", output_dir).into(),
        file_type: "csv".into(),
        description: "arm_decision 任务汇总".into(),
    });

    // 7. 构建标准化时间线数据（带轮次关联）
    let timeline = match build_timeline(input_file, &tasks, t0, t_last, context) {
        Ok(t) => t,
        Err(e) => return RErr(e),
    };

    // 8. 构建分析摘要
    let filter_note = if let Some(count) = filtered_count {
        format!("（与轮次重叠: {} 个）", count)
    } else {
        String::new()
    };

    let summary = format!(
        "arm_decision 分析完成！\n\
         - 检测到 {} 个任务{}\n\
         - 共 {} 个模块执行记录\n\
         - 生成 {} 条 CSV 记录\n\
         - 输出目录: {}",
        task_count, filter_note, module_count, record_count, output_dir
    );

    ROk(AnalyzeResult {
        summary: summary.into(),
        output_files: output_files.into(),
        timeline,
    })
}

/// 构建标准化时间线数据
fn build_timeline(
    source_file: &str,
    tasks: &[ArmDecisionTask],
    log_start_time: f64,
    log_end_time: f64,
    context: Option<&AnalyzerContext>,
) -> Result<timeline::Timeline> {
    use analyzer_core::timeline::*;

    let mut events = Vec::new();

    for (task_idx, task) in tasks.iter().enumerate() {
        // 使用 BodyTask 的时间范围（如果有）
        let (start_ts, end_ts) = if let (Some(body_start), Some(body_end)) =
            (task.body_task_start_ts, task.body_task_end_ts)
        {
            (body_start, Some(body_end))
        } else {
            (task.start_ts, task.end_ts)
        };

        // 查找关联的轮次
        let round_id = context.and_then(|ctx| ctx.find_round_for_task(start_ts, end_ts));

        // 主任务事件
        let task_event = TimelineEvent {
            id: format!("arm_decision_task_{}", task_idx).into(),
            track: Track::Custom("ArmDecision".into()), // 与配置中的 track_priority 匹配
            name: format!(
                "手臂决策(type={})",
                task.task_type.map(|t| t.to_string()).unwrap_or_default()
            )
            .into(),
            start_time: start_ts,
            end_time: end_ts.map(|t| t.into()).into(),
            status: match task.result_status {
                Some(0) => EventStatus::Success,
                Some(_) => EventStatus::Failed,
                None => EventStatus::InProgress,
            },
            source: "arm_decision".into(),
            parent_id: RNone,
            metadata: {
                let meta = serde_json::json!({
                    "task_type": task.task_type,
                    "result_status": task.result_status,
                    "result_message": task.result_message,
                    "module_count": task.modules.len(),
                    "round_id": round_id,
                });
                RSome(meta.to_string().into())
            },
            color_hint: RSome("#98FB98".into()), // 淡绿色
        };
        events.push(task_event);

        // 模块事件（不添加到主事件列表，避免甘特图过于密集）
        // 如果需要详细模块信息，可以取消注释下面的代码
        /*
        for (mod_idx, module) in task.modules.iter().enumerate() {
            if module.name.is_empty() {
                continue;
            }

            let mod_event = TimelineEvent {
                id: format!("arm_decision_task_{}_mod_{}", task_idx, mod_idx).into(),
                track: Track::Custom("ArmDecisionModule".into()),
                name: module.name.clone().into(),
                start_time: module.start_ts,
                end_time: module.end_ts.map(|t| t.into()).into(),
                status: match module.status.as_str() {
                    "ok" => EventStatus::Success,
                    "incomplete" => EventStatus::Failed,
                    _ => EventStatus::InProgress,
                },
                source: "arm_decision".into(),
                parent_id: RSome(format!("arm_decision_task_{}", task_idx).into()),
                metadata: {
                    let meta = serde_json::json!({
                        "cmd_code": module.cmd_code,
                        "cost_s": module.cost_s,
                        "round_id": round_id,
                    });
                    RSome(meta.to_string().into())
                },
                color_hint: RSome("#90EE90".into()), // 浅绿色
            };
            events.push(mod_event);
        }
        */
    }

    let timeline = Timeline {
        name: "arm_decision".into(),
        source_file: source_file.into(),
        log_start_time,
        log_end_time,
        events: events.into(),
        is_primary: false, // arm_decision 不是主时间轴
        metadata: {
            let meta = serde_json::json!({
                "task_count": tasks.len(),
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

#[export_root_module]
pub fn get_root_module() -> AnalyzerPluginModule_Ref {
    AnalyzerPluginModule { create_plugin }.leak_into_prefix()
}

/// 创建插件实例的工厂函数
#[sabi_extern_fn]
pub fn create_plugin() -> AnalyzerPlugin_TO<'static, RBox<()>> {
    AnalyzerPlugin_TO::from_value(ArmDecisionAnalyzer, TD_Opaque)
}

// ============================================================================
// 公共 API
// ============================================================================

/// 执行日志分析（公共 API）
pub fn run_analysis(input_file: &str, output_dir: &str) -> Result<String> {
    match run_analysis_internal(input_file, output_dir, None) {
        ROk(result) => Ok(result.summary.to_string()),
        RErr(e) => Err(e),
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_metadata() {
        let plugin = ArmDecisionAnalyzer;
        let meta = plugin.metadata();
        assert_eq!(meta.name.as_str(), "arm-decision-analyzer");
        assert!(
            meta.supported_extensions
                .iter()
                .any(|ext| ext.as_str() == ".log")
        );
    }
}
