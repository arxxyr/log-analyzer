//! 日志解析模块
//!
//! 本模块负责从日志文件中提取带时间戳的日志行

use std::fs;

use anyhow::Result;
use regex::Regex;

use crate::models::LogLine;

/// 加载并解析日志文件
///
/// 从日志文件中提取所有带有时间戳的日志行，并按时间戳排序
///
/// # 参数
/// * `log_path` - 日志文件路径
///
/// # 返回
/// 包含所有带时间戳的日志行的向量，按时间戳升序排列
///
/// # 日志格式
/// 支持的时间戳格式：`[INFO/WARN/ERROR/DEBUG] [timestamp]`
/// 例如：`[INFO] [1756803704.695] [master_control]: 日志内容`
///
/// # 错误处理
/// - 如果文件不存在，返回错误
/// - 如果文件不是有效的UTF-8编码，使用lossy转换处理无效字符
pub fn load_log_lines(log_path: &str) -> Result<Vec<LogLine>> {
    // 尝试以UTF-8格式读取文件，如果失败则使用lossy转换
    let content = match fs::read_to_string(log_path) {
        Ok(content) => content,
        Err(_) => {
            // 如果UTF-8读取失败，尝试读取字节并转换
            let bytes = fs::read(log_path)?;
            String::from_utf8_lossy(&bytes).into_owned()
        }
    };

    // 时间戳格式：[INFO/WARN/ERROR/DEBUG] [timestamp]
    let ts_regex = Regex::new(r"\[(?:INFO|WARN|ERROR|DEBUG)\]\s*\[(\d{9,}\.\d+)\]")?;
    let mut lines = Vec::new();

    for line in content.lines() {
        if let Some(caps) = ts_regex.captures(line)
            && let Ok(timestamp) = caps[1].parse::<f64>()
        {
            lines.push(LogLine {
                timestamp,
                line: line.to_string(),
            });
        }
    }

    // 按时间戳排序
    lines.sort_by(|a, b| a.timestamp.partial_cmp(&b.timestamp).unwrap());
    Ok(lines)
}
