//! 轮次检测模块
//!
//! 本模块负责从日志中检测任务轮次和大流程

use anyhow::Result;
use regex::Regex;

use crate::models::{LogLine, MajorFlow, Round};

/// 检测日志中的任务轮次
///
/// 基于循环标记（"循环N: 开始循环N"）自动检测任务轮次
///
/// # 参数
/// * `lines` - 日志行切片
/// * `t_last` - 最后一行日志的时间戳
///
/// # 返回
/// 包含所有检测到的轮次的向量
pub fn detect_rounds(lines: &[LogLine], t_last: f64) -> Result<Vec<Round>> {
    // 循环标记格式：循环N: 开始循环N
    let loop_regex = Regex::new(r"\[发布日志节点\]:\s*\[INFO\]\s*循环(\d+):\s*开始循环\d+")?;
    let pose_regex = Regex::new(r"\[master_control\]:\s*姿态字符串:\s*(\{.*\})")?;

    let mut rounds = Vec::new();
    let mut current: Option<Round> = None;

    for line in lines {
        // 检测循环开始
        if let Some(caps) = loop_regex.captures(&line.line) {
            let loop_number = caps[1].parse::<u32>().ok();

            // 如果当前有进行中的轮次，先结束它
            if let Some(mut round) = current {
                if round.end_ts.is_none() {
                    // 使用当前行时间戳作为上一轮的结束时间
                    round.end_ts = Some(line.timestamp);
                }
                rounds.push(round);
            }

            // 开始新的轮次（每个循环标记都创建一个新轮次）
            let id = rounds.len() + 1;
            current = Some(Round {
                id,
                loop_number,
                start_ts: line.timestamp,
                end_ts: None,
                pose0: None,
                pose6: None,
            });
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

/// 检测大流程（从循环1到循环8为一个完整流程）
///
/// # 参数
/// * `rounds` - 轮次切片
///
/// # 返回
/// 包含所有检测到的大流程的向量
pub fn detect_major_flows(rounds: &[Round]) -> Vec<MajorFlow> {
    let mut major_flows = Vec::new();
    let mut current_flow_rounds: Vec<Round> = Vec::new();

    for round in rounds {
        // 如果有循环编号信息
        if let Some(loop_num) = round.loop_number {
            // 跳过循环5（通常是空闲或等待轮次）
            if loop_num == 5 {
                // 如果当前有流程在进行，先保存它
                if !current_flow_rounds.is_empty() {
                    let start_ts = current_flow_rounds.first().unwrap().start_ts;
                    let end_ts = current_flow_rounds
                        .last()
                        .unwrap()
                        .end_ts
                        .unwrap_or(current_flow_rounds.last().unwrap().start_ts);
                    let duration_s = end_ts - start_ts;
                    let num_rounds = current_flow_rounds.len() as f64;
                    let average_round_duration_s = duration_s / num_rounds;
                    let last_loop = current_flow_rounds
                        .last()
                        .and_then(|r| r.loop_number)
                        .map(|n| format!("循环{}", n))
                        .unwrap_or_else(|| "未知".to_string());

                    major_flows.push(MajorFlow {
                        id: major_flows.len() + 1,
                        rounds: current_flow_rounds.clone(),
                        start_ts,
                        end_ts,
                        duration_s,
                        average_round_duration_s,
                        is_complete: false, // 遇到循环5，流程中断
                        failure_point: Some(format!("中断于{}", last_loop)),
                    });

                    // 清空，准备新流程
                    current_flow_rounds.clear();
                }
                // 跳过循环5，不加入任何流程
                continue;
            }

            // 先检测是否为新流程开始（循环1）
            if loop_num == 1 && !current_flow_rounds.is_empty() {
                // 遇到新的循环1，且之前已有轮次（但未达到循环8），说明上一个流程不完整
                // 保存不完整的流程（包括单轮次流程）
                if current_flow_rounds.len() >= 1 {
                    let start_ts = current_flow_rounds.first().unwrap().start_ts;
                    let end_ts = current_flow_rounds
                        .last()
                        .unwrap()
                        .end_ts
                        .unwrap_or(current_flow_rounds.last().unwrap().start_ts);
                    let duration_s = end_ts - start_ts;
                    let num_rounds = current_flow_rounds.len() as f64;
                    let average_round_duration_s = duration_s / num_rounds;
                    let last_loop = current_flow_rounds
                        .last()
                        .and_then(|r| r.loop_number)
                        .map(|n| format!("循环{}", n))
                        .unwrap_or_else(|| "未知".to_string());

                    major_flows.push(MajorFlow {
                        id: major_flows.len() + 1,
                        rounds: current_flow_rounds.clone(),
                        start_ts,
                        end_ts,
                        duration_s,
                        average_round_duration_s,
                        is_complete: false, // 未到达循环8，不完整流程
                        failure_point: Some(format!("中断于{}", last_loop)),
                    });
                }

                // 清空，准备新流程
                current_flow_rounds.clear();
            }

            // 将当前轮次加入流程
            current_flow_rounds.push(round.clone());

            // 检测是否达到循环8（完整流程结束）
            if loop_num == 8 {
                // 创建完整大流程
                if !current_flow_rounds.is_empty() {
                    let start_ts = current_flow_rounds.first().unwrap().start_ts;
                    let end_ts = current_flow_rounds
                        .last()
                        .unwrap()
                        .end_ts
                        .unwrap_or(current_flow_rounds.last().unwrap().start_ts);
                    let duration_s = end_ts - start_ts;

                    // 对于完整流程（包含循环8），平均时间为总时间除以8
                    let average_round_duration_s = duration_s / 8.0;

                    major_flows.push(MajorFlow {
                        id: major_flows.len() + 1,
                        rounds: current_flow_rounds.clone(),
                        start_ts,
                        end_ts,
                        duration_s,
                        average_round_duration_s,
                        is_complete: true, // 到达循环8，是完整流程
                        failure_point: None,
                    });

                    // 清空当前流程的轮次，准备下一个流程
                    current_flow_rounds.clear();
                }
            }
        }
    }

    // 处理最后的未完成流程（包括单轮次流程）
    if !current_flow_rounds.is_empty() {
        let start_ts = current_flow_rounds.first().unwrap().start_ts;
        let end_ts = current_flow_rounds
            .last()
            .unwrap()
            .end_ts
            .unwrap_or(current_flow_rounds.last().unwrap().start_ts);
        let duration_s = end_ts - start_ts;
        let num_rounds = current_flow_rounds.len() as f64;
        let average_round_duration_s = duration_s / num_rounds;

        let last_loop = current_flow_rounds
            .last()
            .and_then(|r| r.loop_number)
            .map(|n| format!("循环{}", n))
            .unwrap_or_else(|| "未知".to_string());
        let is_complete = current_flow_rounds.last().and_then(|r| r.loop_number) == Some(8);

        major_flows.push(MajorFlow {
            id: major_flows.len() + 1,
            rounds: current_flow_rounds,
            start_ts,
            end_ts,
            duration_s,
            average_round_duration_s,
            is_complete,
            failure_point: if is_complete {
                None
            } else {
                Some(format!("中断于{}", last_loop))
            },
        });
    }

    major_flows
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
    for round in rounds {
        let end_ts = round.end_ts.unwrap_or(f64::INFINITY);
        if ts >= round.start_ts && ts < end_ts {
            return round.id;
        }
    }
    if let Some(last_round) = rounds.last()
        && ts >= last_round.end_ts.unwrap_or(0.0)
    {
        return last_round.id;
    }
    0
}
