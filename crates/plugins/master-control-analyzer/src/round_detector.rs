//! 轮次检测模块
//!
//! 本模块负责从日志中检测任务轮次
//!
//! 日志格式：
//! - 初始循环开始: `[初始循环] ===== 初始循环开始（气密设备为空）=====`
//! - 初始循环完成: `[初始循环] 初始循环完成，气密设备中有工件`
//! - 常规循环开始: `[常规循环] 常规循环 N`
//! - 常规循环完成: `[常规循环] 常规循环 N 放置完成`
//! - 最终循环开始: `[最终循环] 最终循环：取出气密工件并放置`
//! - 最终循环完成: `[最终循环] 最终循环完成`

use anyhow::Result;
use regex::Regex;

use crate::models::{CycleType, LogLine, PauseEvent, Round};

/// 结束当前轮次并推入列表
fn finalize_current_round(current: &mut Option<Round>, rounds: &mut Vec<Round>, end_ts: f64) {
    if let Some(mut round) = current.take() {
        if round.end_ts.is_none() {
            round.end_ts = Some(end_ts);
        }
        rounds.push(round);
    }
}

/// 创建新轮次
fn create_round(rounds: &[Round], cycle_type: CycleType, loop_number: u32, start_ts: f64) -> Round {
    Round {
        id: rounds.len() + 1,
        loop_number: Some(loop_number),
        cycle_type,
        layer_index: 0,
        start_ts,
        end_ts: None,
        pose0: None,
        pose6: None,
        pause_events: Vec::new(),
    }
}

/// 检测日志中的任务轮次
///
/// 基于循环标记自动检测任务轮次，支持三种循环类型：
/// - 初始循环：气密设备为空时执行
/// - 常规循环：迭代执行的主循环（可执行N次）
/// - 最终循环：取出气密工件并放置
///
/// # 参数
/// * `lines` - 日志行切片
/// * `t_last` - 最后一行日志的时间戳
///
/// # 返回
/// 包含所有检测到的轮次的向量
pub fn detect_rounds(lines: &[LogLine], t_last: f64) -> Result<Vec<Round>> {
    // 初始循环开始: [初始循环] ===== 初始循环开始（气密设备为空）=====
    let init_start_regex = Regex::new(r"\[初始循环\]\s*=+\s*初始循环开始（气密设备为空）\s*=+")?;
    // 初始循环完成: [初始循环] 初始循环完成，气密设备中有工件
    let init_end_regex = Regex::new(r"\[初始循环\]\s*初始循环完成，气密设备中有工件")?;
    // 常规循环开始: [常规循环] 常规循环 N
    let normal_start_regex = Regex::new(r"\[常规循环\]\s*常规循环\s*(\d+)\s*$")?;
    // 常规循环完成: [常规循环] 常规循环 N 放置完成
    let normal_end_regex = Regex::new(r"\[常规循环\]\s*常规循环\s*(\d+)\s*放置完成")?;
    // 最终循环开始: [最终循环] 最终循环：取出气密工件并放置
    let final_start_regex = Regex::new(r"\[最终循环\]\s*最终循环：取出气密工件并放置")?;
    // 最终循环完成: [最终循环] 最终循环完成
    let final_end_regex = Regex::new(r"\[最终循环\]\s*最终循环完成")?;

    // 姿态信息
    let pose_regex = Regex::new(r"\[master_control\]:\s*姿态字符串:\s*(\{.*\})")?;

    // 暂停检测模式 1: PauseTaskNode[...]: 请求暂停任务，等待操作员 RESUME
    let pause_regex = Regex::new(r"PauseTaskNode\[.*?\]:\s*请求暂停任务，等待操作员 RESUME")?;
    // 恢复检测模式 1: PauseTaskNode[...]: 任务已恢复，继续执行
    let resume_regex = Regex::new(r"PauseTaskNode\[.*?\]:\s*任务已恢复，继续执行")?;

    // 暂停检测模式 2: TaskGraphExecutor: 节点 ... 失败，进入失败暂停状态
    let fail_pause_regex = Regex::new(r"TaskGraphExecutor:\s*节点\s+\S+\s+失败，进入失败暂停状态")?;
    // 恢复检测模式 2: TaskGraphExecutor: 重试节点 ...
    let retry_resume_regex = Regex::new(r"TaskGraphExecutor:\s*重试节点\s+\S+")?;

    // 暂停检测模式 3: ROS2ActionAdapter[xxx] - 暂停（动作被暂停）
    let adapter_pause_regex = Regex::new(r"ROS2ActionAdapter\[\w+\]\s*-\s*暂停")?;
    // 恢复检测模式 3: ROS2ActionAdapter[xxx] - 恢复（动作恢复执行）
    let adapter_resume_regex = Regex::new(r"ROS2ActionAdapter\[\w+\]\s*-\s*恢复")?;

    // 暂停检测模式 4: TaskGraphExecutor: 用户请求暂停任务 xxx
    let user_pause_regex = Regex::new(r"TaskGraphExecutor:\s*用户请求暂停任务\s+\S+")?;
    // 恢复检测模式 4: TaskGraphExecutor: 恢复任务 xxx
    let user_resume_regex = Regex::new(r"TaskGraphExecutor:\s*恢复任务\s+\S+")?;

    let mut rounds = Vec::new();
    let mut current: Option<Round> = None;
    let mut pending_pause_ts: Option<f64> = None; // 待匹配恢复的暂停时间戳（模式1）
    let mut pending_fail_pause_ts: Option<f64> = None; // 待匹配恢复的失败暂停时间戳（模式2）
    let mut pending_adapter_pause_ts: Option<f64> = None; // 待匹配恢复的动作暂停时间戳（模式3）
    let mut pending_user_pause_ts: Option<f64> = None; // 待匹配恢复的用户暂停时间戳（模式4）

    for line in lines {
        // 检测初始循环开始
        if init_start_regex.is_match(&line.line) {
            finalize_current_round(&mut current, &mut rounds, line.timestamp);
            current = Some(create_round(&rounds, CycleType::Initial, 0, line.timestamp));
            continue;
        }

        // 检测初始循环完成
        if init_end_regex.is_match(&line.line) {
            if let Some(ref mut round) = current
                && matches!(round.cycle_type, CycleType::Initial)
            {
                round.end_ts = Some(line.timestamp);
                rounds.push(current.take().unwrap());
            }
            continue;
        }

        // 检测常规循环开始
        if let Some(caps) = normal_start_regex.captures(&line.line) {
            let cycle_number = caps[1].parse::<u32>().unwrap_or(1);
            finalize_current_round(&mut current, &mut rounds, line.timestamp);
            current = Some(create_round(
                &rounds,
                CycleType::Normal(cycle_number),
                cycle_number,
                line.timestamp,
            ));
            continue;
        }

        // 检测常规循环完成
        if let Some(caps) = normal_end_regex.captures(&line.line) {
            let cycle_number = caps[1].parse::<u32>().unwrap_or(1);
            if let Some(ref mut round) = current
                && matches!(round.cycle_type, CycleType::Normal(n) if n == cycle_number)
            {
                round.end_ts = Some(line.timestamp);
                rounds.push(current.take().unwrap());
            }
            continue;
        }

        // 检测最终循环开始
        if final_start_regex.is_match(&line.line) {
            finalize_current_round(&mut current, &mut rounds, line.timestamp);
            current = Some(create_round(&rounds, CycleType::Final, 999, line.timestamp));
            continue;
        }

        // 检测最终循环完成
        if final_end_regex.is_match(&line.line) {
            if let Some(ref mut round) = current
                && matches!(round.cycle_type, CycleType::Final)
            {
                round.end_ts = Some(line.timestamp);
                rounds.push(current.take().unwrap());
            }
            continue;
        }

        // 检测暂停事件（模式1: PauseTaskNode）
        if pause_regex.is_match(&line.line) {
            pending_pause_ts = Some(line.timestamp);
            continue;
        }

        // 检测恢复事件（模式1: PauseTaskNode）
        if resume_regex.is_match(&line.line) {
            if let Some(pause_ts) = pending_pause_ts.take() {
                // 将暂停事件添加到当前轮次
                if let Some(ref mut round) = current {
                    round.pause_events.push(PauseEvent {
                        pause_ts,
                        resume_ts: Some(line.timestamp),
                    });
                }
            }
            continue;
        }

        // 检测失败暂停事件（模式2: 失败暂停状态）
        if fail_pause_regex.is_match(&line.line) {
            pending_fail_pause_ts = Some(line.timestamp);
            continue;
        }

        // 检测重试恢复事件（模式2: 重试节点）
        if retry_resume_regex.is_match(&line.line) {
            if let Some(pause_ts) = pending_fail_pause_ts.take() {
                // 将暂停事件添加到当前轮次
                if let Some(ref mut round) = current {
                    round.pause_events.push(PauseEvent {
                        pause_ts,
                        resume_ts: Some(line.timestamp),
                    });
                }
            }
            continue;
        }

        // 检测动作暂停事件（模式3: ROS2ActionAdapter 暂停）
        if adapter_pause_regex.is_match(&line.line) {
            pending_adapter_pause_ts = Some(line.timestamp);
            continue;
        }

        // 检测动作恢复事件（模式3: ROS2ActionAdapter 恢复）
        if adapter_resume_regex.is_match(&line.line) {
            if let Some(pause_ts) = pending_adapter_pause_ts.take() {
                // 将暂停事件添加到当前轮次
                if let Some(ref mut round) = current {
                    round.pause_events.push(PauseEvent {
                        pause_ts,
                        resume_ts: Some(line.timestamp),
                    });
                }
            }
            continue;
        }

        // 检测用户暂停事件（模式4: 用户请求暂停任务）
        if user_pause_regex.is_match(&line.line) {
            pending_user_pause_ts = Some(line.timestamp);
            continue;
        }

        // 检测用户恢复事件（模式4: 恢复任务）
        if user_resume_regex.is_match(&line.line) {
            if let Some(pause_ts) = pending_user_pause_ts.take() {
                // 将暂停事件添加到当前轮次
                if let Some(ref mut round) = current {
                    round.pause_events.push(PauseEvent {
                        pause_ts,
                        resume_ts: Some(line.timestamp),
                    });
                }
            }
            continue;
        }

        // 收集姿态信息
        if let Some(ref mut round) = current
            && let Some(caps) = pose_regex.captures(&line.line)
        {
            if round.pose0.is_none() {
                round.pose0 = Some(caps[1].to_string());
            } else if round.pose6.is_none() {
                round.pose6 = Some(caps[1].to_string());
            }
        }
    }

    // 最后一轮处理
    if let Some(mut round) = current {
        if round.end_ts.is_none() {
            round.end_ts = Some(t_last);
        }
        rounds.push(round);
    }

    Ok(rounds)
}

/// 将时间戳转换为对应的轮次ID
///
/// # 参数
/// * `ts` - 时间戳
/// * `rounds` - 轮次切片
///
/// # 返回
/// 对应的轮次ID，如果没有找到则返回0或最后一个轮次的ID
pub fn ts_to_round_id(ts: f64, rounds: &[Round]) -> usize {
    // 查找时间戳落在哪个轮次内
    rounds
        .iter()
        .find(|r| ts >= r.start_ts && ts < r.end_ts.unwrap_or(f64::INFINITY))
        .map(|r| r.id)
        .or_else(|| {
            // 如果在最后一个轮次之后，返回最后一个轮次的 ID
            rounds
                .last()
                .filter(|r| ts >= r.end_ts.unwrap_or(0.0))
                .map(|r| r.id)
        })
        .unwrap_or(0)
}
