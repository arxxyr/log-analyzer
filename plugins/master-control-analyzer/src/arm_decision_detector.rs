//! arm_decision 日志解析模块
//!
//! 解析 arm_decision 日志，提取任务执行的时序信息，用于合并到主甘特图

use anyhow::Result;
use regex::Regex;

use crate::models::{ArmDecisionModule, ArmDecisionTask, LogLine};

/// 从 arm_decision 日志文件加载并解析日志行
pub fn load_arm_decision_log_lines(path: &str) -> Result<Vec<LogLine>> {
    let content = std::fs::read(path)?;
    let text = String::from_utf8_lossy(&content);

    // 时间戳正则：[INFO] [1766304091.784694087] [rclcpp]:
    let ts_regex = Regex::new(r"\[INFO\]\s*\[(\d+\.\d+)\]")?;

    let mut lines = Vec::new();
    for line in text.lines() {
        if let Some(caps) = ts_regex.captures(line) {
            if let Ok(ts) = caps[1].parse::<f64>() {
                lines.push(LogLine {
                    timestamp: ts,
                    line: line.to_string(),
                });
            }
        }
    }

    Ok(lines)
}

/// 检测 arm_decision 日志中的任务
///
/// 每个任务从 "Received goal." 开始，到 "result->message:" 结束
pub fn detect_arm_decision_tasks(lines: &[LogLine]) -> Result<Vec<ArmDecisionTask>> {
    // 任务边界正则
    let received_goal_regex = Regex::new(r"\[rclcpp\]:\s*Received goal\.")?;
    let body_task_start_regex = Regex::new(r"\[rclcpp\]:\s*=+BodyTask start=+")?;
    let body_task_end_regex = Regex::new(r"\[rclcpp\]:\s*=+BodyTask end=+")?;
    let result_status_regex = Regex::new(r"\[rclcpp\]:\s*result->status:\s*(-?\d+)")?;
    let result_message_regex = Regex::new(r"\[rclcpp\]:\s*result->message:\s*(.+)")?;

    // 模块检测正则
    let module_separator_regex = Regex::new(r"\[rclcpp\]:\s*-{10,}")?;
    let module_tick_regex = Regex::new(r"\[rclcpp\]:\s*(\w+)\s+tick!")?;
    let cmd_code_regex = Regex::new(r"\[rclcpp\]:\s*cmd_code:\s*(\d+)")?;
    let task_type_regex = Regex::new(r"\[rclcpp\]:\s*task_type:\s*(\d+)")?;
    let cost_regex = Regex::new(r"\[rclcpp\]:\s*cost\(s\):\s*([\d.]+)")?;
    let module_ok_regex = Regex::new(r"\[rclcpp\]:\s*(\w+)\s+ok\.")?;

    let mut tasks = Vec::new();
    let mut current_task: Option<ArmDecisionTask> = None;
    let mut current_module: Option<ArmDecisionModule> = None;
    let mut in_body_task = false;

    for line in lines {
        // 检测任务开始
        if received_goal_regex.is_match(&line.line) {
            // 保存之前的任务（如果存在）
            if let Some(mut task) = current_task.take() {
                // 完成最后一个模块
                if let Some(mut module) = current_module.take() {
                    if module.end_ts.is_none() {
                        module.end_ts = Some(line.timestamp);
                        module.status = "incomplete".to_string();
                    }
                    task.modules.push(module);
                }
                tasks.push(task);
            }

            current_task = Some(ArmDecisionTask {
                start_ts: line.timestamp,
                end_ts: None,
                body_task_start_ts: None,
                body_task_end_ts: None,
                task_type: None,
                result_status: None,
                result_message: None,
                modules: Vec::new(),
            });
            in_body_task = false;
            continue;
        }

        // 必须在一个任务中
        let Some(ref mut task) = current_task else {
            continue;
        };

        // 检测 BodyTask 开始
        if body_task_start_regex.is_match(&line.line) {
            task.body_task_start_ts = Some(line.timestamp);
            in_body_task = true;
            continue;
        }

        // 检测 BodyTask 结束
        if body_task_end_regex.is_match(&line.line) {
            task.body_task_end_ts = Some(line.timestamp);
            // 完成最后一个模块
            if let Some(mut module) = current_module.take() {
                if module.end_ts.is_none() {
                    module.end_ts = Some(line.timestamp);
                }
                task.modules.push(module);
            }
            in_body_task = false;
            continue;
        }

        // 检测结果状态
        if let Some(caps) = result_status_regex.captures(&line.line) {
            task.result_status = caps[1].parse().ok();
            continue;
        }

        // 检测结果消息（任务结束）
        if let Some(caps) = result_message_regex.captures(&line.line) {
            task.result_message = Some(caps[1].trim().to_string());
            task.end_ts = Some(line.timestamp);
            continue;
        }

        // 以下只在 BodyTask 内处理
        if !in_body_task {
            continue;
        }

        // 检测模块分隔符 --------------------
        if module_separator_regex.is_match(&line.line) {
            // 完成前一个模块
            if let Some(mut module) = current_module.take() {
                if module.end_ts.is_none() {
                    module.end_ts = Some(line.timestamp);
                }
                task.modules.push(module);
            }
            // 准备新模块（名称在下一行 tick! 中获取）
            current_module = Some(ArmDecisionModule {
                name: String::new(),
                cmd_code: None,
                start_ts: line.timestamp,
                end_ts: None,
                cost_s: None,
                status: "pending".to_string(),
            });
            continue;
        }

        // 检测模块名称（XXXAction tick!）
        if let Some(caps) = module_tick_regex.captures(&line.line) {
            if let Some(ref mut module) = current_module {
                module.name = caps[1].to_string();
            }
            continue;
        }

        // 检测 cmd_code
        if let Some(caps) = cmd_code_regex.captures(&line.line) {
            if let Some(ref mut module) = current_module {
                module.cmd_code = caps[1].parse().ok();
            }
            continue;
        }

        // 检测 task_type（通常在 GetTaskTypeAction 中）
        if let Some(caps) = task_type_regex.captures(&line.line) {
            task.task_type = caps[1].parse().ok();
            continue;
        }

        // 检测 cost(s)
        if let Some(caps) = cost_regex.captures(&line.line) {
            if let Some(ref mut module) = current_module {
                module.cost_s = caps[1].parse().ok();
            }
            continue;
        }

        // 检测模块完成（XXXAction ok.）
        if module_ok_regex.is_match(&line.line) {
            if let Some(ref mut module) = current_module {
                module.end_ts = Some(line.timestamp);
                module.status = "ok".to_string();
            }
            continue;
        }
    }

    // 保存最后一个任务
    if let Some(mut task) = current_task.take() {
        if let Some(mut module) = current_module.take() {
            if module.end_ts.is_none() {
                module.status = "incomplete".to_string();
            }
            task.modules.push(module);
        }
        tasks.push(task);
    }

    Ok(tasks)
}
