//! 数据结构定义模块
//!
//! 定义 arm_decision 日志分析中使用的核心数据结构

use serde::Serialize;

/// 日志行结构，包含时间戳和原始日志内容
#[derive(Debug, Clone)]
pub struct LogLine {
    /// Unix 时间戳（秒）
    pub timestamp: f64,
    /// 日志原始内容
    pub line: String,
}

/// arm_decision 任务中的子模块
#[derive(Debug, Clone, Serialize)]
pub struct ArmDecisionModule {
    /// 模块名称（如 GetTaskTypeAction, ModifyArmObstacleAction 等）
    pub name: String,
    /// cmd_code
    pub cmd_code: Option<u32>,
    /// 开始时间戳
    pub start_ts: f64,
    /// 结束时间戳
    pub end_ts: Option<f64>,
    /// 耗时（秒）- 从日志中的 cost(s) 提取
    pub cost_s: Option<f64>,
    /// 状态（ok/pending）
    pub status: String,
}

/// arm_decision 完整任务（从 Received goal 到 result->message）
#[derive(Debug, Clone, Serialize)]
pub struct ArmDecisionTask {
    /// 任务开始时间戳（Received goal）
    pub start_ts: f64,
    /// 任务结束时间戳（result->message）
    pub end_ts: Option<f64>,
    /// BodyTask 开始时间戳
    pub body_task_start_ts: Option<f64>,
    /// BodyTask 结束时间戳
    pub body_task_end_ts: Option<f64>,
    /// task_type（如 2000, 2015 等）
    pub task_type: Option<u32>,
    /// 结果状态码
    pub result_status: Option<i32>,
    /// 结果消息
    pub result_message: Option<String>,
    /// 子模块列表
    pub modules: Vec<ArmDecisionModule>,
}

/// CSV 导出记录
#[derive(Debug, Serialize)]
pub struct CsvRecord {
    /// 任务索引
    pub task_index: usize,
    /// 任务开始时间（相对秒）
    pub task_start_rel_s: f64,
    /// 任务结束时间（相对秒）
    pub task_end_rel_s: Option<f64>,
    /// 任务耗时（秒）
    pub task_duration_s: Option<f64>,
    /// task_type
    pub task_type: Option<u32>,
    /// 结果状态
    pub result_status: Option<i32>,
    /// 模块索引
    pub module_index: usize,
    /// 模块名称
    pub module_name: String,
    /// cmd_code
    pub cmd_code: Option<u32>,
    /// 模块开始时间（相对秒）
    pub module_start_rel_s: f64,
    /// 模块结束时间（相对秒）
    pub module_end_rel_s: Option<f64>,
    /// 模块耗时（秒）
    pub module_duration_s: Option<f64>,
    /// 模块状态
    pub module_status: String,
}
