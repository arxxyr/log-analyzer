//! 工具函数模块
//!
//! 本模块提供通用的工具函数，如时间转换等

use chrono::{DateTime, FixedOffset};

/// 将Unix时间戳转换为北京时间字符串
///
/// # 参数
/// * `timestamp` - Unix时间戳（秒级精度）
///
/// # 返回
/// 北京时间字符串，格式为 "YYYY-MM-DD HH:MM:SS"
///
/// # 示例
/// ```
/// use master_control_analyzer::utils::timestamp_to_beijing_time;
///
/// let beijing_time = timestamp_to_beijing_time(1756803704.695);
/// // 返回类似 "2025-09-03 14:35:04" 的时间字符串
/// ```
pub fn timestamp_to_beijing_time(timestamp: f64) -> String {
    // 创建北京时区 (UTC+8)
    let beijing_tz = FixedOffset::east_opt(8 * 3600).unwrap();

    // 转换为秒和纳秒
    let secs = timestamp as i64;
    let nanos = ((timestamp - secs as f64) * 1_000_000_000.0) as u32;

    // 创建DateTime
    if let Some(dt) = DateTime::from_timestamp(secs, nanos) {
        let beijing_time = dt.with_timezone(&beijing_tz);
        beijing_time.format("%Y-%m-%d %H:%M:%S").to_string()
    } else {
        "0000-00-00 00:00:00".to_string()
    }
}
