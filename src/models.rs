//! 数据结构定义模块
//!
//! 本模块包含日志分析工具中使用的所有核心数据结构

use clap::Parser;
use serde::Serialize;

/// 命令行参数
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct Args {
    /// 输入日志文件路径
    #[arg(short, long)]
    pub log: String,

    /// 输出目录
    #[arg(short, long, default_value = "output")]
    pub outdir: String,
}

/// 日志行结构，包含时间戳和原始日志内容
#[derive(Debug, Clone)]
pub struct LogLine {
    /// Unix 时间戳（秒）
    pub timestamp: f64,
    /// 日志原始内容
    pub line: String,
}

/// 任务轮次
#[derive(Debug, Clone)]
pub struct Round {
    /// 轮次ID
    pub id: usize,
    /// 循环编号（如循环1、循环2等）
    pub loop_number: Option<u32>,
    /// 开始时间戳
    pub start_ts: f64,
    /// 结束时间戳
    pub end_ts: Option<f64>,
    /// 初始姿态
    pub pose0: Option<String>,
    /// 目标姿态
    pub pose6: Option<String>,
}

/// 动作内部的子步骤
#[derive(Debug, Clone)]
pub struct SubStep {
    /// 子步骤名称
    pub name: String,
    /// 时间戳
    pub timestamp: f64,
}

/// 通用动作操作
#[derive(Debug, Clone)]
pub struct ActionOperation {
    /// 动作类型：arm, navigation, head, waist
    pub action_type: String,
    /// 动作代码
    pub action_code: Option<u32>,
    /// 动作标签
    pub label: String,
    /// 开始时间戳
    pub start_ts: Option<f64>,
    /// 结束时间戳
    pub end_ts: Option<f64>,
    /// 状态
    pub status: String,
    /// 子步骤列表
    pub sub_steps: Vec<SubStep>,
}

/// 导航流程
#[derive(Debug, Clone)]
pub struct NavigationFlow {
    /// 导航开始时间戳
    pub nav_start_ts: Option<f64>,
    /// 导航结束时间戳
    pub nav_end_ts: Option<f64>,
    /// 导航目标位置
    pub nav_target_pos: Option<String>,
    /// 导航目标姿态
    pub nav_target_ori: Option<String>,
    /// 导航状态
    pub nav_status: String,
    /// 导航子步骤
    pub nav_sub_steps: Vec<SubStep>,
    /// 所属轮次ID
    pub round_id: usize,
    /// 关联的其他动作操作
    pub operations: Vec<ActionOperation>,
}

/// 大流程结构，包含从循环1到循环8的完整流程
#[derive(Debug, Clone)]
pub struct MajorFlow {
    /// 大流程ID
    pub id: usize,
    /// 包含的轮次
    pub rounds: Vec<Round>,
    /// 开始时间戳
    pub start_ts: f64,
    /// 结束时间戳
    pub end_ts: f64,
    /// 总持续时间（秒）
    pub duration_s: f64,
    /// 平均每轮持续时间（秒）
    pub average_round_duration_s: f64,
    /// 是否为完整流程（到达循环8）
    pub is_complete: bool,
    /// 失败点（如果不完整）
    pub failure_point: Option<String>,
}

/// CSV记录结构
#[derive(Debug, Serialize)]
pub struct CsvRecord {
    /// 大流程ID
    pub major_flow_id: Option<usize>,
    /// 轮次ID
    pub round_id: usize,
    /// 轮次开始相对时间（秒）
    pub round_start_rel_s: Option<f64>,
    /// 轮次结束相对时间（秒）
    pub round_end_rel_s: Option<f64>,
    /// 轮次持续时间（秒）
    pub round_duration_s: Option<f64>,
    /// 流程ID
    pub flow_id: usize,
    /// 步骤索引
    pub step_idx: usize,
    /// 步骤类型
    pub step_type: String,
    /// 导航目标位置
    pub nav_target_pos: Option<String>,
    /// 导航目标姿态
    pub nav_target_ori: Option<String>,
    /// 动作类型
    pub action_type: Option<String>,
    /// 动作代码
    pub action_code: Option<u32>,
    /// 动作标签
    pub action_label: Option<String>,
    /// 开始相对时间（秒）
    pub start_rel_s: Option<f64>,
    /// 结束相对时间（秒）
    pub end_rel_s: Option<f64>,
    /// 持续时间（秒）
    pub duration_s: Option<f64>,
    /// 状态
    pub status: String,
}
