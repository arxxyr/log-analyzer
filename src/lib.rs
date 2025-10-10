//! Master Control Analyzer Library
//!
//! 这是一个用于分析机器人控制系统日志的库，提供了日志解析、
//! 轮次检测、流程分析、数据导出和可视化等功能。
//!
//! # 模块
//!
//! - [`models`] - 核心数据结构定义
//! - [`utils`] - 工具函数（时间转换等）
//! - [`parser`] - 日志解析功能
//! - [`round_detector`] - 轮次和大流程检测
//! - [`flow_detector`] - 导航流程和动作检测
//! - [`csv_exporter`] - CSV数据导出
//! - [`gantt`] - 甘特图可视化生成
//!
//! # 示例
//!
//! ```no_run
//! use master_control_analyzer::*;
//!
//! # fn main() -> anyhow::Result<()> {
//! // 解析日志
//! let lines = parser::load_log_lines("path/to/log.log")?;
//!
//! // 检测轮次
//! let t_last = lines.last().unwrap().timestamp;
//! let rounds = round_detector::detect_rounds(&lines, t_last)?;
//!
//! // 检测大流程
//! let major_flows = round_detector::detect_major_flows(&rounds);
//!
//! // 检测流程
//! let flows = flow_detector::detect_flows(&lines, &rounds)?;
//!
//! // 构建CSV记录
//! let t0 = lines[0].timestamp;
//! let records = csv_exporter::build_csv_records(&flows, &rounds, &major_flows, t0);
//!
//! // 导出CSV
//! csv_exporter::export_csv(&records, "output")?;
//!
//! // 生成甘特图
//! gantt::generate_gantt_charts(&flows, &rounds, "output", t0)?;
//! # Ok(())
//! # }
//! ```

pub mod csv_exporter;
pub mod flow_detector;
pub mod gantt;
pub mod models;
pub mod parser;
pub mod round_detector;
pub mod utils;

// 重新导出常用类型
pub use models::{
    ActionOperation, Args, CsvRecord, LogLine, MajorFlow, NavigationFlow, Round, SubStep,
};
