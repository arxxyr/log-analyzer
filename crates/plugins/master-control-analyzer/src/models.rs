//! 数据结构定义模块
//!
//! 本模块包含日志分析工具中使用的所有核心数据结构

use serde::Serialize;

/// 日志行结构，包含时间戳和原始日志内容
#[derive(Debug, Clone)]
pub struct LogLine {
    /// Unix 时间戳（秒）
    pub timestamp: f64,
    /// 日志原始内容
    pub line: String,
}

/// 循环类型枚举
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CycleType {
    /// 初始循环（气密设备为空时执行）
    Initial,
    /// 常规循环（迭代执行的主循环，编号从1开始）
    Normal(u32),
    /// 最终循环（取出气密工件并放置）
    Final,
}

impl std::fmt::Display for CycleType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CycleType::Initial => write!(f, "初始循环"),
            CycleType::Normal(n) => write!(f, "常规循环 {}", n),
            CycleType::Final => write!(f, "最终循环"),
        }
    }
}

/// 暂停事件
#[derive(Debug, Clone)]
pub struct PauseEvent {
    /// 暂停开始时间戳
    pub pause_ts: f64,
    /// 恢复时间戳
    pub resume_ts: Option<f64>,
}

impl PauseEvent {
    /// 计算暂停持续时间
    pub fn duration(&self) -> f64 {
        self.resume_ts
            .map(|resume| resume - self.pause_ts)
            .unwrap_or(0.0)
    }
}

/// 任务轮次
#[derive(Debug, Clone)]
pub struct Round {
    /// 轮次ID
    pub id: usize,
    /// 循环编号（如循环1、循环2等）- 保留用于向后兼容
    pub loop_number: Option<u32>,
    /// 循环类型（初始循环/常规循环/最终循环）
    pub cycle_type: CycleType,
    /// 层级索引（用于多层级场景）
    pub layer_index: u32,
    /// 开始时间戳
    pub start_ts: f64,
    /// 结束时间戳
    pub end_ts: Option<f64>,
    /// 初始姿态
    pub pose0: Option<String>,
    /// 目标姿态
    pub pose6: Option<String>,
    /// 暂停事件列表
    pub pause_events: Vec<PauseEvent>,
}

impl Round {
    /// 计算总暂停时间
    pub fn total_pause_duration(&self) -> f64 {
        self.pause_events.iter().map(|e| e.duration()).sum()
    }

    /// 计算有效持续时间（总时长减去暂停时间）
    pub fn effective_duration(&self) -> f64 {
        let total = self.end_ts.map(|end| end - self.start_ts).unwrap_or(0.0);
        (total - self.total_pause_duration()).max(0.0)
    }
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
    /// 暂停事件列表
    pub pause_events: Vec<PauseEvent>,
}

impl ActionOperation {
    /// 计算总暂停时间
    pub fn total_pause_duration(&self) -> f64 {
        self.pause_events.iter().map(|e| e.duration()).sum()
    }

    /// 计算有效持续时间（总时长减去暂停时间）
    pub fn effective_duration(&self) -> f64 {
        match (self.start_ts, self.end_ts) {
            (Some(start), Some(end)) => (end - start - self.total_pause_duration()).max(0.0),
            _ => 0.0,
        }
    }
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

// ============================================================================
// CSV 导出结构
// ============================================================================

/// CSV记录结构
#[derive(Debug, Serialize)]
pub struct CsvRecord {
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
