//! 轮次检测模块
//!
//! 本模块负责从日志中检测任务轮次
//!
//! 日志格式（同时兼容新旧两种）：
//! - 初始循环开始:
//!   - 新: `[初始循环] ===== 初始填充开始，已上件工位数 N =====`
//!   - 旧: `[初始循环] ===== 初始循环开始（气密设备为空）=====`
//! - 初始循环完成:
//!   - 新: `[初始循环] 初始填充完成，已上件工位数 N`
//!   - 旧: `[初始循环] 初始循环完成，气密设备中有工件`
//! - 常规循环开始:
//!   - 新: `[常规循环] 常规循环 N，目标工位 M`
//!   - 旧: `[常规循环] 常规循环 N`
//! - 常规循环完成: `[常规循环] 常规循环 N 放置完成`（未变）
//! - 收尾/最终循环开始:
//!   - 新: `[收尾前校正] ...`
//!   - 旧: `[最终循环] 最终循环：取出气密工件并放置`
//! - 收尾/最终循环完成:
//!   - 新: `[收尾循环] 双工位收尾完成`
//!   - 旧: `[最终循环] 最终循环完成`
//!
//! 注意：LogNode 节点注册时会在 log_node.cpp:39 输出一行包含模板字符串（含
//! `{variable}` 占位符）的日志，正则通过强制要求 `\d+` 等具体值来避免误匹配。

use std::sync::LazyLock;

use anyhow::Result;
use regex::Regex;

use crate::models::{CycleType, LogLine, PauseEvent, Round};

// ===== 循环检测正则 =====
//
// 所有正则都必须用 `\d+` 等具体值形式，避免匹配 LogNode 注册行中的
// `{variable}` 占位符（那一行与真实事件长得像，但 `log_node.cpp:39`，
// 真实事件在 `log_node.cpp:62`）。

/// 初始循环开始（新旧格式并存）
static INIT_START_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"\[初始循环\]\s*=+\s*(?:初始填充开始，已上件工位数\s+\d+|初始循环开始（气密设备为空）)\s*=+",
    )
    .expect("invalid regex: INIT_START_REGEX")
});

/// 初始循环完成（新旧格式并存）
static INIT_END_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"\[初始循环\]\s*(?:初始填充完成，已上件工位数\s+\d+|初始循环完成，气密设备中有工件)",
    )
    .expect("invalid regex: INIT_END_REGEX")
});

/// 常规循环开始:
///   - 新: `[常规循环] 常规循环 N，目标工位 M`
///   - 旧: `[常规循环] 常规循环 N`（行尾无后缀）
static NORMAL_START_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\[常规循环\]\s*常规循环\s*(\d+)(?:，目标工位\s+\d+|\s*$)")
        .expect("invalid regex: NORMAL_START_REGEX")
});

/// 常规循环完成: [常规循环] 常规循环 N 放置完成
static NORMAL_END_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\[常规循环\]\s*常规循环\s*(\d+)\s*放置完成")
        .expect("invalid regex: NORMAL_END_REGEX")
});

/// 收尾/最终循环开始:
///   - 新: `[收尾前校正] loaded_leak_station_count=N, ...`（收尾阶段唯一入口）
///   - 旧: `[最终循环] 最终循环：取出气密工件并放置`
///
/// 必须要求 `loaded_leak_station_count=\d+`，才能避开：
///   - `log_node.cpp:39` 打印的 `message='[收尾前校正] loaded_leak_station_count={...}'`
///     模板注册行（占位符 `{...}` 非 `\d+`）
///   - `[收尾前校正] loaded_leak_station_count 与 put_poses 剩余数量不一致`
///     警告行（`count` 与 `与` 之间无 `=`）
static FINAL_START_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"\[收尾前校正\]\s+loaded_leak_station_count=\d+|\[最终循环\]\s*最终循环：取出气密工件并放置",
    )
    .expect("invalid regex: FINAL_START_REGEX")
});

/// 收尾/最终循环完成:
///   - 新: `[收尾循环] 双工位收尾完成`
///   - 旧: `[最终循环] 最终循环完成`
static FINAL_END_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\[收尾循环\]\s*双工位收尾完成|\[最终循环\]\s*最终循环完成")
        .expect("invalid regex: FINAL_END_REGEX")
});

// ===== 姿态信息正则 =====

/// 姿态字符串: [master_control]: 姿态字符串: {...}
static POSE_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\[master_control\]:\s*姿态字符串:\s*(\{.*\})").expect("invalid regex: POSE_REGEX")
});

// ===== 暂停/恢复检测正则 =====

/// 暂停检测模式 1: PauseTaskNode[...]: 请求暂停任务，等待操作员 RESUME
static PAUSE_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"PauseTaskNode\[.*?\]:\s*请求暂停任务，等待操作员 RESUME")
        .expect("invalid regex: PAUSE_REGEX")
});

/// 恢复检测模式 1: PauseTaskNode[...]: 任务已恢复，继续执行
static RESUME_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"PauseTaskNode\[.*?\]:\s*任务已恢复，继续执行")
        .expect("invalid regex: RESUME_REGEX")
});

/// 暂停检测模式 2: TaskGraphExecutor: 节点 ... 失败，进入失败暂停状态
static FAIL_PAUSE_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"TaskGraphExecutor:\s*节点\s+\S+\s+失败，进入失败暂停状态")
        .expect("invalid regex: FAIL_PAUSE_REGEX")
});

/// 恢复检测模式 2: TaskGraphExecutor: 重试节点 ...
static RETRY_RESUME_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"TaskGraphExecutor:\s*重试节点\s+\S+").expect("invalid regex: RETRY_RESUME_REGEX")
});

/// 暂停检测模式 3: ROS2ActionAdapter[xxx] - 暂停（动作被暂停）
static ADAPTER_PAUSE_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"ROS2ActionAdapter\[\w+\]\s*-\s*暂停").expect("invalid regex: ADAPTER_PAUSE_REGEX")
});

/// 恢复检测模式 3: ROS2ActionAdapter[xxx] - 恢复（动作恢复执行）
static ADAPTER_RESUME_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"ROS2ActionAdapter\[\w+\]\s*-\s*恢复").expect("invalid regex: ADAPTER_RESUME_REGEX")
});

/// 暂停检测模式 4: TaskGraphExecutor: 用户请求暂停任务 xxx
static USER_PAUSE_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"TaskGraphExecutor:\s*用户请求暂停任务\s+\S+")
        .expect("invalid regex: USER_PAUSE_REGEX")
});

/// 恢复检测模式 4: TaskGraphExecutor: 恢复任务 xxx
static USER_RESUME_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"TaskGraphExecutor:\s*恢复任务\s+\S+").expect("invalid regex: USER_RESUME_REGEX")
});

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
    // 引用静态预编译正则
    let init_start_regex = &*INIT_START_REGEX;
    let init_end_regex = &*INIT_END_REGEX;
    let normal_start_regex = &*NORMAL_START_REGEX;
    let normal_end_regex = &*NORMAL_END_REGEX;
    let final_start_regex = &*FINAL_START_REGEX;
    let final_end_regex = &*FINAL_END_REGEX;
    let pose_regex = &*POSE_REGEX;
    let pause_regex = &*PAUSE_REGEX;
    let resume_regex = &*RESUME_REGEX;
    let fail_pause_regex = &*FAIL_PAUSE_REGEX;
    let retry_resume_regex = &*RETRY_RESUME_REGEX;
    let adapter_pause_regex = &*ADAPTER_PAUSE_REGEX;
    let adapter_resume_regex = &*ADAPTER_RESUME_REGEX;
    let user_pause_regex = &*USER_PAUSE_REGEX;
    let user_resume_regex = &*USER_RESUME_REGEX;

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

        // 检测最终/收尾循环开始
        // 幂等：如果已在 Final 轮次中，忽略后续的重复起始标记（新日志中
        // `[收尾前校正]` 可能伴随多条警告/模板行，全部归入同一个收尾轮次）
        if final_start_regex.is_match(&line.line) {
            if !matches!(
                current.as_ref().map(|r| &r.cycle_type),
                Some(CycleType::Final)
            ) {
                finalize_current_round(&mut current, &mut rounds, line.timestamp);
                current = Some(create_round(&rounds, CycleType::Final, 999, line.timestamp));
            }
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
