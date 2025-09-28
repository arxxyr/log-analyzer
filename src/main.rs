use std::collections::HashMap;
use std::fs;

use anyhow::Result;
use chrono::{DateTime, FixedOffset};
use clap::Parser;
use plotters::prelude::*;
use plotters::style::text_anchor::{HPos, Pos, VPos};
use regex::Regex;
use serde::Serialize;

// 添加时间转换函数
fn timestamp_to_beijing_time(timestamp: f64) -> String {
    // 创建北京时区 (UTC+8)
    let beijing_tz = FixedOffset::east_opt(8 * 3600).unwrap();

    // 转换为秒和纳秒
    let secs = timestamp as i64;
    let nanos = ((timestamp - secs as f64) * 1_000_000_000.0) as u32;

    // 创建DateTime
    if let Some(dt) = DateTime::from_timestamp(secs, nanos) {
        let beijing_time = dt.with_timezone(&beijing_tz);
        beijing_time.format("%H:%M:%S").to_string()
    } else {
        "00:00:00".to_string()
    }
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Input log file path
    #[arg(short, long)]
    log: String,

    /// Output directory
    #[arg(short, long, default_value = "output")]
    outdir: String,
}

#[derive(Debug, Clone)]
struct LogLine {
    timestamp: f64,
    line: String,
}

#[derive(Debug, Clone)]
struct Round {
    id: usize,
    loop_number: Option<u32>, // 循环编号（如循环1、循环2等）
    start_ts: f64,
    end_ts: Option<f64>,
    pose0: Option<String>,
    pose6: Option<String>,
}

#[derive(Debug, Clone)]
struct SubStep {
    name: String,
    timestamp: f64,
}

#[derive(Debug, Clone)]
struct ActionOperation {
    action_type: String, // "arm", "navigation", "head", "waist"
    action_code: Option<u32>,
    label: String,
    start_ts: Option<f64>,
    end_ts: Option<f64>,
    status: String,
    sub_steps: Vec<SubStep>, // 记录动作内部的子步骤
}

#[derive(Debug, Clone)]
struct NavigationFlow {
    nav_start_ts: Option<f64>,
    nav_end_ts: Option<f64>,
    nav_target_pos: Option<String>,
    nav_target_ori: Option<String>,
    nav_status: String,
    nav_sub_steps: Vec<SubStep>, // 导航的子步骤
    round_id: usize,
    operations: Vec<ActionOperation>, // 支持所有类型的动作
}

// 大流程结构，包含从循环1到循环8的完整流程
#[derive(Debug, Clone)]
struct MajorFlow {
    id: usize,
    rounds: Vec<Round>,        // 包含的轮次
    start_ts: f64,
    end_ts: f64,
    duration_s: f64,
    average_round_duration_s: f64,  // 完整流程：总时间/8；不完整流程：总时间/实际轮次数
    is_complete: bool,              // 是否为完整流程（到达循环8）
    failure_point: Option<String>,  // 失败点（如果不完整）
}

#[derive(Debug, Serialize)]
struct CsvRecord {
    major_flow_id: Option<usize>,    // 大流程ID
    round_id: usize,
    round_start_rel_s: Option<f64>,
    round_end_rel_s: Option<f64>,
    round_duration_s: Option<f64>,
    flow_id: usize,
    step_idx: usize,
    step_type: String,
    nav_target_pos: Option<String>,
    nav_target_ori: Option<String>,
    action_type: Option<String>, // 动作类型
    action_code: Option<u32>,
    action_label: Option<String>,
    start_rel_s: Option<f64>,
    end_rel_s: Option<f64>,
    duration_s: Option<f64>,
    status: String,
}

fn main() -> Result<()> {
    let args = Args::parse();

    // Load log lines
    let lines = load_log_lines(&args.log)?;
    if lines.is_empty() {
        anyhow::bail!("No timestamped lines found in log file");
    }

    let t0 = lines[0].timestamp;
    let t_last = lines.last().unwrap().timestamp;

    // Detect rounds
    let rounds = detect_rounds(&lines, t_last)?;
    println!("Detected {} rounds", rounds.len());

    // 调试输出：显示每个轮次的循环编号
    for round in &rounds {
        let loop_info = if let Some(loop_num) = round.loop_number {
            format!("循环{}", loop_num)
        } else {
            "无循环编号".to_string()
        };
        println!("Round {}: {}", round.id, loop_info);
    }

    // Detect major flows（大流程）
    let major_flows = detect_major_flows(&rounds);
    println!("Detected {} major flows", major_flows.len());
    for major_flow in &major_flows {
        let status = if major_flow.is_complete {
            "完整流程"
        } else {
            &format!("不完整流程 ({})", major_flow.failure_point.as_ref().unwrap_or(&"未知".to_string()))
        };
        println!(
            "Major Flow {}: {} - 包含{}个轮次, 总时长{:.2}秒, 平均每轮{:.2}秒",
            major_flow.id,
            status,
            major_flow.rounds.len(),
            major_flow.duration_s,
            major_flow.average_round_duration_s
        );
    }

    // Detect navigation flows
    let flows = detect_flows(&lines, &rounds)?;
    println!("Detected {} navigation flows", flows.len());

    // 调试：统计各类动作
    let mut nav_count = 0;
    let mut arm_count = 0;
    let mut head_count = 0;
    let mut waist_count = 0;
    for flow in &flows {
        nav_count += 1;
        for op in &flow.operations {
            match op.action_type.as_str() {
                "arm" => arm_count += 1,
                "head" => head_count += 1,
                "waist" => waist_count += 1,
                _ => {}
            }
        }
    }
    println!(
        "Actions: {} nav, {} arm, {} head, {} waist",
        nav_count, arm_count, head_count, waist_count
    );

    // Build CSV records
    let records = build_csv_records(&flows, &rounds, &major_flows, t0);
    println!("Generated {} CSV records", records.len());

    // Create output directory
    std::fs::create_dir_all(&args.outdir)?;

    // Export CSV
    export_csv(&records, &args.outdir)?;

    // Export major flow statistics
    export_major_flow_stats(&major_flows, &args.outdir, t0)?;

    // Generate Gantt charts with flows for sub-steps
    generate_gantt_charts(&flows, &rounds, &args.outdir, t0)?;

    // Generate action timeline summary CSV
    generate_action_timeline_csv(&flows, &rounds, &args.outdir, t0)?;

    println!("Analysis complete! Output in: {}", args.outdir);

    Ok(())
}

fn load_log_lines(log_path: &str) -> Result<Vec<LogLine>> {
    // Try to read the file as UTF-8, with lossy conversion for invalid chars
    let content = match fs::read_to_string(log_path) {
        Ok(content) => content,
        Err(_) => {
            // If UTF-8 reading fails, try reading as bytes and convert
            let bytes = fs::read(log_path)?;
            String::from_utf8_lossy(&bytes).into_owned()
        }
    };

    // 时间戳格式：[INFO/WARN/ERROR/DEBUG] [timestamp]
    let ts_regex = Regex::new(r"\[(?:INFO|WARN|ERROR|DEBUG)\]\s*\[(\d{9,}\.\d+)\]")?;
    let mut lines = Vec::new();

    for line in content.lines() {
        if let Some(caps) = ts_regex.captures(line) {
            if let Ok(timestamp) = caps[1].parse::<f64>() {
                lines.push(LogLine {
                    timestamp,
                    line: line.to_string(),
                });
            }
        }
    }

    lines.sort_by(|a, b| a.timestamp.partial_cmp(&b.timestamp).unwrap());
    Ok(lines)
}

fn detect_rounds(lines: &[LogLine], t_last: f64) -> Result<Vec<Round>> {
    // 新的循环标记格式：循环N: 开始循环N
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
        if let Some(ref mut round) = current {
            if let Some(caps) = pose_regex.captures(&line.line) {
                if round.pose0.is_none() {
                    round.pose0 = Some(caps[1].to_string());
                } else if round.pose6.is_none() {
                    round.pose6 = Some(caps[1].to_string());
                }
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

// 检测大流程（从循环1到循环8为一个完整流程）
fn detect_major_flows(rounds: &[Round]) -> Vec<MajorFlow> {
    let mut major_flows = Vec::new();
    let mut current_flow_rounds = Vec::new();

    for round in rounds {
        // 如果有循环编号信息
        if let Some(loop_num) = round.loop_number {
            current_flow_rounds.push(round.clone());

            // 检测是否达到循环8（完整流程结束）
            if loop_num == 8 {
                // 创建大流程
                if !current_flow_rounds.is_empty() {
                    let start_ts = current_flow_rounds.first().unwrap().start_ts;
                    let end_ts = current_flow_rounds.last().unwrap()
                        .end_ts.unwrap_or(current_flow_rounds.last().unwrap().start_ts);
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
                        is_complete: true,  // 到达循环8，是完整流程
                        failure_point: None,
                    });

                    // 清空当前流程的轮次，准备下一个流程
                    current_flow_rounds.clear();
                }
            } else if loop_num == 1 && !current_flow_rounds.is_empty() && current_flow_rounds.len() > 1 {
                // 如果遇到新的循环1，且之前已有轮次（但未达到循环8），说明上一个流程不完整
                // 保存不完整的流程（如果需要）
                if current_flow_rounds.len() >= 2 {  // 至少有2个轮次才创建流程
                    let start_ts = current_flow_rounds.first().unwrap().start_ts;
                    let end_ts = current_flow_rounds.last().unwrap()
                        .end_ts.unwrap_or(current_flow_rounds.last().unwrap().start_ts);
                    let duration_s = end_ts - start_ts;
                    let num_rounds = current_flow_rounds.len() as f64;
                    let average_round_duration_s = duration_s / num_rounds;
                    let last_loop = current_flow_rounds.last()
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
                        is_complete: false,  // 未到达循环8，不完整流程
                        failure_point: Some(format!("中断于{}", last_loop)),
                    });
                }

                // 开始新的流程
                current_flow_rounds.clear();
                current_flow_rounds.push(round.clone());
            }
        }
    }

    // 处理最后的未完成流程
    if !current_flow_rounds.is_empty() && current_flow_rounds.len() >= 2 {
        let start_ts = current_flow_rounds.first().unwrap().start_ts;
        let end_ts = current_flow_rounds.last().unwrap()
            .end_ts.unwrap_or(current_flow_rounds.last().unwrap().start_ts);
        let duration_s = end_ts - start_ts;
        let num_rounds = current_flow_rounds.len() as f64;
        let average_round_duration_s = duration_s / num_rounds;

        let last_loop = current_flow_rounds.last()
            .and_then(|r| r.loop_number)
            .map(|n| format!("循环{}", n))
            .unwrap_or_else(|| "未知".to_string());
        let is_complete = current_flow_rounds.last()
            .and_then(|r| r.loop_number)
            .map_or(false, |n| n == 8);

        major_flows.push(MajorFlow {
            id: major_flows.len() + 1,
            rounds: current_flow_rounds,
            start_ts,
            end_ts,
            duration_s,
            average_round_duration_s,
            is_complete,
            failure_point: if is_complete { None } else { Some(format!("中断于{}", last_loop)) },
        });
    }

    major_flows
}

fn ts_to_round_id(ts: f64, rounds: &[Round]) -> usize {
    for round in rounds {
        let end_ts = round.end_ts.unwrap_or(f64::INFINITY);
        if ts >= round.start_ts && ts < end_ts {
            return round.id;
        }
    }
    if let Some(last_round) = rounds.last() {
        if ts >= last_round.end_ts.unwrap_or(0.0) {
            return last_round.id;
        }
    }
    0
}

fn detect_flows(lines: &[LogLine], rounds: &[Round]) -> Result<Vec<NavigationFlow>> {
    // 导航相关正则 (支持 NavAction 和 NavAction2)
    let nav_start_regex = Regex::new(r"\[导航\]:\s*NavAction2?\[NavAction2?\]\s*-\s*开始执行")?;
    let nav_target_regex = Regex::new(r"设置导航目标:\s*pos\(([^)]+)\),\s*ori\(([^)]+)\)")?;
    let nav_send_regex = Regex::new(r"\[导航\]:\s*发送导航目标")?;
    let nav_response_regex = Regex::new(r"\[导航\]:\s*\[RESPONSE CALLBACK\]\s*-\s*目标已被服务端接受")?;
    let nav_result_regex = Regex::new(r"\[导航\]:\s*\[RESULT CALLBACK\]")?;
    let nav_end_regex = Regex::new(r"\[导航\]:\s*NavAction2?\[NavAction2?\]\s*-\s*执行完成，结果:")?;

    // 导航完成的正则（匹配日志中的实际格式）
    let nav_complete_regex = Regex::new(r"\[导航\]:\s*\[RESULT CALLBACK\]\s*-\s*导航完成，结果代码:")?;

    // 额外添加对 NavAction[NavAction] 格式的支持
    let nav_start_alt_regex = Regex::new(r"\[导航\]:\s*NavAction\[NavAction\]\s*-\s*开始执行")?;

    // 机械臂相关正则
    // DoubleArmAction[动作名称] - 开始执行
    let arm_start_regex = Regex::new(r"\[机械臂\]:\s*DoubleArmAction\[([^\]]+)\]\s*-\s*开始执行")?;
    // setGoal action_type_code
    let arm_setgoal_regex =
        Regex::new(r"\[机械臂\]:\s*DoubleArmAction\s+setGoal\s+action_type_code:\s*(\d+)")?;
    // 发送机械臂目标
    let arm_send_regex = Regex::new(r"\[机械臂\]:\s*发送机械臂控制目标")?;
    // Response callback
    let arm_response_regex = Regex::new(r"\[机械臂\]:\s*\[RESPONSE CALLBACK\]\s*-\s*目标已被服务端接受")?;
    // [RESULT CALLBACK] - 机械臂动作完成
    let arm_result_regex =
        Regex::new(r"\[机械臂\]:\s*\[RESULT CALLBACK\]\s*-\s*机械臂动作完成，状态:\s*(\d+)")?;
    // 执行完成
    let arm_complete_regex =
        Regex::new(r"\[机械臂\]:\s*DoubleArmAction\[([^\]]+)\]\s*-\s*执行完成，结果:")?;

    // 头部控制相关正则 (支持 HeadControlAction 和 HeadControlAction2)
    let head_start_regex =
        Regex::new(r"\[头部控制\]:\s*HeadControlAction2?\[head_control\]\s*-\s*开始执行")?;
    let head_send_regex = Regex::new(r"\[头部控制\]:\s*发送头部控制目标")?;
    let head_response_regex = Regex::new(r"\[头部控制\]:\s*\[RESPONSE CALLBACK\]\s*-\s*目标已被服务端接受")?;
    let head_result_regex = Regex::new(r"\[头部控制\]:\s*\[RESULT CALLBACK\]\s*-\s*头部动作完成")?;
    let head_end_regex =
        Regex::new(r"\[头部控制\]:\s*HeadControlAction2?\[head_control\]\s*-\s*执行完成")?;

    // 腰部控制相关正则 (支持 WaistAction 和 WaistAction2)
    let waist_start_regex = Regex::new(r"\[腰部\]:\s*WaistAction2?\[WaistAction2?\]\s*-\s*开始执行")?;
    let waist_send_regex = Regex::new(r"\[腰部\]:\s*发送腰部控制目标")?;
    let waist_response_regex = Regex::new(r"\[腰部\]:\s*\[RESPONSE CALLBACK\]\s*-\s*目标已被服务端接受")?;
    let waist_result_regex = Regex::new(r"\[腰部\]:\s*\[RESULT CALLBACK\]\s*-\s*腰部动作完成")?;
    let waist_end_regex =
        Regex::new(r"\[腰部\]:\s*WaistAction2?\[WaistAction2?\]\s*-\s*执行完成，结果:")?;

    let mut flows = Vec::new();
    let mut current_flow: Option<NavigationFlow> = None;

    for line in lines {
        // 导航开始 - NavAction2[NavAction2] 或 NavAction[NavAction] - 开始执行
        if nav_start_regex.is_match(&line.line) || nav_start_alt_regex.is_match(&line.line) {
            // 结束当前流程如果存在且未完成
            if let Some(mut flow) = current_flow {
                if flow.nav_end_ts.is_none() {
                    flow.nav_end_ts = Some(line.timestamp);
                    flow.nav_status = "incomplete".to_string();
                }
                flows.push(flow);
            }

            // 创建新的导航流程
            current_flow = Some(NavigationFlow {
                nav_start_ts: Some(line.timestamp),
                nav_end_ts: None,
                nav_target_pos: None,
                nav_target_ori: None,
                nav_status: "ok".to_string(),
                nav_sub_steps: vec![SubStep {
                    name: "开始执行".to_string(),
                    timestamp: line.timestamp,
                }],
                round_id: ts_to_round_id(line.timestamp, rounds),
                operations: Vec::new(),
            });
            continue;
        }

        // 导航目标设置 - 补充目标信息
        if let Some(caps) = nav_target_regex.captures(&line.line) {
            if let Some(ref mut flow) = current_flow {
                if flow.nav_target_pos.is_none() {
                    flow.nav_target_pos = Some(caps[1].replace(' ', ""));
                    flow.nav_target_ori = Some(caps[2].replace(' ', ""));
                    flow.nav_sub_steps.push(SubStep {
                        name: "设置导航目标".to_string(),
                        timestamp: line.timestamp,
                    });
                }
            }
            continue;
        }

        // 导航 - 发送导航目标
        if nav_send_regex.is_match(&line.line) {
            if let Some(ref mut flow) = current_flow {
                flow.nav_sub_steps.push(SubStep {
                    name: "发送导航目标".to_string(),
                    timestamp: line.timestamp,
                });
            }
            continue;
        }

        // 导航 - Response callback
        if nav_response_regex.is_match(&line.line) {
            if let Some(ref mut flow) = current_flow {
                flow.nav_sub_steps.push(SubStep {
                    name: "服务端接受".to_string(),
                    timestamp: line.timestamp,
                });
            }
            continue;
        }

        // 导航 - Result callback（不是完成的回调）
        if nav_result_regex.is_match(&line.line) && !nav_complete_regex.is_match(&line.line) {
            if let Some(ref mut flow) = current_flow {
                flow.nav_sub_steps.push(SubStep {
                    name: "结果回调".to_string(),
                    timestamp: line.timestamp,
                });
            }
            continue;
        }

        // 导航完成 - [RESULT CALLBACK] - 导航完成，结果代码:
        if nav_complete_regex.is_match(&line.line) {
            if let Some(mut flow) = current_flow.take() {
                flow.nav_end_ts = Some(line.timestamp);
                flow.nav_sub_steps.push(SubStep {
                    name: "导航完成".to_string(),
                    timestamp: line.timestamp,
                });
                flows.push(flow);
            }
            continue;
        }

        // 导航结束 - NavAction[NavAction] - 执行完成（旧格式兼容）
        if nav_end_regex.is_match(&line.line) {
            if let Some(mut flow) = current_flow.take() {
                flow.nav_end_ts = Some(line.timestamp);
                flow.nav_sub_steps.push(SubStep {
                    name: "执行完成".to_string(),
                    timestamp: line.timestamp,
                });
                flows.push(flow);
            }
            continue;
        }

        // 机械臂动作开始
        if let Some(caps) = arm_start_regex.captures(&line.line) {
            let action_label = caps[1].to_string();

            // 查找下一行是否有action_code
            let mut action_code = None;
            for (idx, l) in lines.iter().enumerate() {
                if std::ptr::eq(l, line) && idx + 1 < lines.len() {
                    if let Some(code_caps) = arm_setgoal_regex.captures(&lines[idx + 1].line) {
                        action_code = code_caps[1].parse().ok();
                    }
                    break;
                }
            }

            // 创建新的机械臂动作
            let arm_action = ActionOperation {
                action_type: "arm".to_string(),
                action_code,
                label: action_label.clone(),
                start_ts: Some(line.timestamp),
                end_ts: None,
                status: "pending".to_string(),
                sub_steps: vec![SubStep {
                    name: format!("开始执行[{}]", action_label),
                    timestamp: line.timestamp,
                }],
            };

            // 添加到当前流程
            if let Some(ref mut flow) = current_flow {
                flow.operations.push(arm_action);
            } else if let Some(flow) = flows.last_mut() {
                flow.operations.push(arm_action);
            }
            continue;
        }

        // 机械臂 - 发送目标
        if arm_send_regex.is_match(&line.line) {
            // 找到最近的未完成的机械臂动作
            if let Some(ref mut flow) = current_flow {
                for op in flow.operations.iter_mut().rev() {
                    if op.action_type == "arm" && op.end_ts.is_none() {
                        op.sub_steps.push(SubStep {
                            name: "发送目标".to_string(),
                            timestamp: line.timestamp,
                        });
                        break;
                    }
                }
            } else if let Some(flow) = flows.last_mut() {
                for op in flow.operations.iter_mut().rev() {
                    if op.action_type == "arm" && op.end_ts.is_none() {
                        op.sub_steps.push(SubStep {
                            name: "发送目标".to_string(),
                            timestamp: line.timestamp,
                        });
                        break;
                    }
                }
            }
            continue;
        }

        // 机械臂 - Response callback
        if arm_response_regex.is_match(&line.line) {
            // 找到最近的未完成的机械臂动作
            if let Some(ref mut flow) = current_flow {
                for op in flow.operations.iter_mut().rev() {
                    if op.action_type == "arm" && op.end_ts.is_none() {
                        op.sub_steps.push(SubStep {
                            name: "服务端接受".to_string(),
                            timestamp: line.timestamp,
                        });
                        break;
                    }
                }
            } else if let Some(flow) = flows.last_mut() {
                for op in flow.operations.iter_mut().rev() {
                    if op.action_type == "arm" && op.end_ts.is_none() {
                        op.sub_steps.push(SubStep {
                            name: "服务端接受".to_string(),
                            timestamp: line.timestamp,
                        });
                        break;
                    }
                }
            }
            continue;
        }

        // 机械臂动作完成 - [RESULT CALLBACK]
        if let Some(caps) = arm_result_regex.captures(&line.line) {
            let status = caps[1].trim();

            // 找到最近的未完成的机械臂动作
            // 先在当前flow中查找
            let mut found = false;
            if let Some(ref mut flow) = current_flow {
                for op in flow.operations.iter_mut().rev() {
                    if op.action_type == "arm" && op.end_ts.is_none() {
                        op.sub_steps.push(SubStep {
                            name: format!("动作完成(状态:{})", status),
                            timestamp: line.timestamp,
                        });
                        op.end_ts = Some(line.timestamp);
                        op.status = if status == "0" {
                            "ok".to_string()
                        } else {
                            format!("status_{}", status)
                        };
                        found = true;
                        break;
                    }
                }
            }

            // 如果当前flow中没找到，在所有已完成的flows中反向查找
            if !found {
                for flow in flows.iter_mut().rev() {
                    for op in flow.operations.iter_mut().rev() {
                        if op.action_type == "arm" && op.end_ts.is_none() {
                            op.sub_steps.push(SubStep {
                                name: format!("动作完成(状态:{})", status),
                                timestamp: line.timestamp,
                            });
                            op.end_ts = Some(line.timestamp);
                            op.status = if status == "0" {
                                "ok".to_string()
                            } else {
                                format!("status_{}", status)
                            };
                            found = true;
                            break;
                        }
                    }
                    if found {
                        break;
                    }
                }
            }
            continue;
        }

        // 机械臂动作完成 - 执行完成
        if arm_complete_regex.is_match(&line.line) {
            // 找到最近的机械臂动作添加执行完成子步骤
            let mut found = false;
            if let Some(ref mut flow) = current_flow {
                for op in flow.operations.iter_mut().rev() {
                    if op.action_type == "arm" {
                        op.sub_steps.push(SubStep {
                            name: "执行完成".to_string(),
                            timestamp: line.timestamp,
                        });
                        found = true;
                        break;
                    }
                }
            }

            if !found {
                for flow in flows.iter_mut().rev() {
                    for op in flow.operations.iter_mut().rev() {
                        if op.action_type == "arm" {
                            op.sub_steps.push(SubStep {
                                name: "执行完成".to_string(),
                                timestamp: line.timestamp,
                            });
                            found = true;
                            break;
                        }
                    }
                    if found {
                        break;
                    }
                }
            }
            continue;
        }

        // 头部控制开始 - 直接添加到flow
        if head_start_regex.is_match(&line.line) {
            let head_action = ActionOperation {
                action_type: "head".to_string(),
                action_code: None,
                label: "头部控制".to_string(),
                start_ts: Some(line.timestamp),
                end_ts: None,
                status: "pending".to_string(),
                sub_steps: vec![SubStep {
                    name: "开始执行".to_string(),
                    timestamp: line.timestamp,
                }],
            };

            if let Some(ref mut flow) = current_flow {
                flow.operations.push(head_action);
            } else if let Some(flow) = flows.last_mut() {
                flow.operations.push(head_action);
            }
            continue;
        }

        // 头部控制 - 发送目标
        if head_send_regex.is_match(&line.line) {
            if let Some(ref mut flow) = current_flow {
                for op in flow.operations.iter_mut().rev() {
                    if op.action_type == "head" && op.end_ts.is_none() {
                        op.sub_steps.push(SubStep {
                            name: "发送目标".to_string(),
                            timestamp: line.timestamp,
                        });
                        break;
                    }
                }
            } else if let Some(flow) = flows.last_mut() {
                for op in flow.operations.iter_mut().rev() {
                    if op.action_type == "head" && op.end_ts.is_none() {
                        op.sub_steps.push(SubStep {
                            name: "发送目标".to_string(),
                            timestamp: line.timestamp,
                        });
                        break;
                    }
                }
            }
            continue;
        }

        // 头部控制 - Response callback
        if head_response_regex.is_match(&line.line) {
            if let Some(ref mut flow) = current_flow {
                for op in flow.operations.iter_mut().rev() {
                    if op.action_type == "head" && op.end_ts.is_none() {
                        op.sub_steps.push(SubStep {
                            name: "服务端接受".to_string(),
                            timestamp: line.timestamp,
                        });
                        break;
                    }
                }
            } else if let Some(flow) = flows.last_mut() {
                for op in flow.operations.iter_mut().rev() {
                    if op.action_type == "head" && op.end_ts.is_none() {
                        op.sub_steps.push(SubStep {
                            name: "服务端接受".to_string(),
                            timestamp: line.timestamp,
                        });
                        break;
                    }
                }
            }
            continue;
        }

        // 头部控制 - Result callback
        if head_result_regex.is_match(&line.line) {
            if let Some(ref mut flow) = current_flow {
                for op in flow.operations.iter_mut().rev() {
                    if op.action_type == "head" && op.end_ts.is_none() {
                        op.sub_steps.push(SubStep {
                            name: "动作完成".to_string(),
                            timestamp: line.timestamp,
                        });
                        break;
                    }
                }
            } else if let Some(flow) = flows.last_mut() {
                for op in flow.operations.iter_mut().rev() {
                    if op.action_type == "head" && op.end_ts.is_none() {
                        op.sub_steps.push(SubStep {
                            name: "动作完成".to_string(),
                            timestamp: line.timestamp,
                        });
                        break;
                    }
                }
            }
            continue;
        }

        // 腰部控制开始 - 直接添加到flow
        if waist_start_regex.is_match(&line.line) {
            let waist_action = ActionOperation {
                action_type: "waist".to_string(),
                action_code: None,
                label: "腰部控制".to_string(),
                start_ts: Some(line.timestamp),
                end_ts: None,
                status: "pending".to_string(),
                sub_steps: vec![SubStep {
                    name: "开始执行".to_string(),
                    timestamp: line.timestamp,
                }],
            };

            if let Some(ref mut flow) = current_flow {
                flow.operations.push(waist_action);
            } else if let Some(flow) = flows.last_mut() {
                flow.operations.push(waist_action);
            }
            continue;
        }

        // 腰部控制 - 发送目标
        if waist_send_regex.is_match(&line.line) {
            if let Some(ref mut flow) = current_flow {
                for op in flow.operations.iter_mut().rev() {
                    if op.action_type == "waist" && op.end_ts.is_none() {
                        op.sub_steps.push(SubStep {
                            name: "发送目标".to_string(),
                            timestamp: line.timestamp,
                        });
                        break;
                    }
                }
            } else if let Some(flow) = flows.last_mut() {
                for op in flow.operations.iter_mut().rev() {
                    if op.action_type == "waist" && op.end_ts.is_none() {
                        op.sub_steps.push(SubStep {
                            name: "发送目标".to_string(),
                            timestamp: line.timestamp,
                        });
                        break;
                    }
                }
            }
            continue;
        }

        // 腰部控制 - Response callback
        if waist_response_regex.is_match(&line.line) {
            if let Some(ref mut flow) = current_flow {
                for op in flow.operations.iter_mut().rev() {
                    if op.action_type == "waist" && op.end_ts.is_none() {
                        op.sub_steps.push(SubStep {
                            name: "服务端接受".to_string(),
                            timestamp: line.timestamp,
                        });
                        break;
                    }
                }
            } else if let Some(flow) = flows.last_mut() {
                for op in flow.operations.iter_mut().rev() {
                    if op.action_type == "waist" && op.end_ts.is_none() {
                        op.sub_steps.push(SubStep {
                            name: "服务端接受".to_string(),
                            timestamp: line.timestamp,
                        });
                        break;
                    }
                }
            }
            continue;
        }

        // 腰部控制 - Result callback
        if waist_result_regex.is_match(&line.line) {
            if let Some(ref mut flow) = current_flow {
                for op in flow.operations.iter_mut().rev() {
                    if op.action_type == "waist" && op.end_ts.is_none() {
                        op.sub_steps.push(SubStep {
                            name: "动作完成".to_string(),
                            timestamp: line.timestamp,
                        });
                        break;
                    }
                }
            } else if let Some(flow) = flows.last_mut() {
                for op in flow.operations.iter_mut().rev() {
                    if op.action_type == "waist" && op.end_ts.is_none() {
                        op.sub_steps.push(SubStep {
                            name: "动作完成".to_string(),
                            timestamp: line.timestamp,
                        });
                        break;
                    }
                }
            }
            continue;
        }

        // 头部控制完成 - 找到对应的pending头部动作并更新
        if head_end_regex.is_match(&line.line) {
            // 在当前流程或最后一个流程中找到未完成的头部动作
            if let Some(ref mut flow) = current_flow {
                for op in flow.operations.iter_mut().rev() {
                    if op.action_type == "head" && op.end_ts.is_none() {
                        op.sub_steps.push(SubStep {
                            name: "执行完成".to_string(),
                            timestamp: line.timestamp,
                        });
                        op.end_ts = Some(line.timestamp);
                        op.status = "ok".to_string();
                        break;
                    }
                }
            } else if let Some(flow) = flows.last_mut() {
                for op in flow.operations.iter_mut().rev() {
                    if op.action_type == "head" && op.end_ts.is_none() {
                        op.sub_steps.push(SubStep {
                            name: "执行完成".to_string(),
                            timestamp: line.timestamp,
                        });
                        op.end_ts = Some(line.timestamp);
                        op.status = "ok".to_string();
                        break;
                    }
                }
            }
            continue;
        }

        // 腰部控制完成 - 找到对应的pending腰部动作并更新
        if waist_end_regex.is_match(&line.line) {
            // 在当前流程或最后一个流程中找到未完成的腰部动作
            if let Some(ref mut flow) = current_flow {
                for op in flow.operations.iter_mut().rev() {
                    if op.action_type == "waist" && op.end_ts.is_none() {
                        op.sub_steps.push(SubStep {
                            name: "执行完成".to_string(),
                            timestamp: line.timestamp,
                        });
                        op.end_ts = Some(line.timestamp);
                        op.status = "ok".to_string();
                        break;
                    }
                }
            } else if let Some(flow) = flows.last_mut() {
                for op in flow.operations.iter_mut().rev() {
                    if op.action_type == "waist" && op.end_ts.is_none() {
                        op.sub_steps.push(SubStep {
                            name: "执行完成".to_string(),
                            timestamp: line.timestamp,
                        });
                        op.end_ts = Some(line.timestamp);
                        op.status = "ok".to_string();
                        break;
                    }
                }
            }
            continue;
        }
    }

    // 完成剩余的流程
    if let Some(mut flow) = current_flow {
        flow.nav_status = "incomplete".to_string();
        flows.push(flow);
    }

    Ok(flows)
}

fn build_csv_records(
    flows: &[NavigationFlow],
    rounds: &[Round],
    major_flows: &[MajorFlow],
    t0: f64,
) -> Vec<CsvRecord> {
    let mut records = Vec::new();

    // 创建轮次ID到大流程ID的映射
    let mut round_to_major_flow: HashMap<usize, usize> = HashMap::new();
    for major_flow in major_flows {
        for round in &major_flow.rounds {
            round_to_major_flow.insert(round.id, major_flow.id);
        }
    }

    for (flow_id, flow) in flows.iter().enumerate() {
        let flow_id = flow_id + 1;
        let round_info = rounds.iter().find(|r| r.id == flow.round_id);

        let nav_start_rel = flow.nav_start_ts.map(|ts| ts - t0);
        let nav_end_rel = flow.nav_end_ts.map(|ts| ts - t0);
        let nav_duration = match (nav_start_rel, nav_end_rel) {
            (Some(start), Some(end)) => Some(end - start),
            _ => None,
        };

        // Navigation record
        let major_flow_id = round_to_major_flow.get(&flow.round_id).copied();
        records.push(CsvRecord {
            major_flow_id,
            round_id: flow.round_id,
            round_start_rel_s: round_info
                .map(|r| (r.start_ts - t0) * 1000.0)
                .map(|x| (x as u64) as f64 / 1000.0),
            round_end_rel_s: round_info
                .and_then(|r| r.end_ts)
                .map(|ts| (ts - t0) * 1000.0)
                .map(|x| (x as u64) as f64 / 1000.0),
            round_duration_s: round_info
                .and_then(|r| r.end_ts)
                .map(|end_ts| (end_ts - round_info.unwrap().start_ts) * 1000.0)
                .map(|x| (x as u64) as f64 / 1000.0),
            flow_id,
            step_idx: 1,
            step_type: "nav".to_string(),
            nav_target_pos: flow.nav_target_pos.clone(),
            nav_target_ori: flow.nav_target_ori.clone(),
            action_type: Some("navigation".to_string()),
            action_code: None,
            action_label: None,
            start_rel_s: nav_start_rel.map(|x| (x * 1000.0) as u64 as f64 / 1000.0),
            end_rel_s: nav_end_rel.map(|x| (x * 1000.0) as u64 as f64 / 1000.0),
            duration_s: nav_duration.map(|x| (x * 1000.0) as u64 as f64 / 1000.0),
            status: flow.nav_status.clone(),
        });

        // 其他动作记录
        for (op_idx, operation) in flow.operations.iter().enumerate() {
            let start_rel = operation.start_ts.map(|ts| ts - t0);
            let end_rel = operation.end_ts.map(|ts| ts - t0);
            let duration = match (start_rel, end_rel) {
                (Some(start), Some(end)) => Some(end - start),
                _ => None,
            };

            let major_flow_id = round_to_major_flow.get(&flow.round_id).copied();
            records.push(CsvRecord {
                major_flow_id,
                round_id: flow.round_id,
                round_start_rel_s: round_info
                    .map(|r| (r.start_ts - t0) * 1000.0)
                    .map(|x| (x as u64) as f64 / 1000.0),
                round_end_rel_s: round_info
                    .and_then(|r| r.end_ts)
                    .map(|ts| (ts - t0) * 1000.0)
                    .map(|x| (x as u64) as f64 / 1000.0),
                round_duration_s: round_info
                    .and_then(|r| r.end_ts)
                    .map(|end_ts| (end_ts - round_info.unwrap().start_ts) * 1000.0)
                    .map(|x| (x as u64) as f64 / 1000.0),
                flow_id,
                step_idx: op_idx + 2, // Start from 2 since nav is 1
                step_type: operation.action_type.clone(),
                nav_target_pos: flow.nav_target_pos.clone(),
                nav_target_ori: flow.nav_target_ori.clone(),
                action_type: Some(operation.action_type.clone()),
                action_code: operation.action_code,
                action_label: Some(operation.label.clone()),
                start_rel_s: start_rel.map(|x| (x * 1000.0) as u64 as f64 / 1000.0),
                end_rel_s: end_rel.map(|x| (x * 1000.0) as u64 as f64 / 1000.0),
                duration_s: duration.map(|x| (x * 1000.0) as u64 as f64 / 1000.0),
                status: operation.status.clone(),
            });
        }
    }

    records
}

fn export_csv(records: &[CsvRecord], outdir: &str) -> Result<()> {
    let mut wtr = csv::Writer::from_path(format!("{}/analysis.csv", outdir))?;

    for record in records {
        wtr.serialize(record)?;
    }

    wtr.flush()?;
    println!("CSV exported to {}/analysis.csv", outdir);
    Ok(())
}

// 导出大流程统计信息到CSV
fn export_major_flow_stats(major_flows: &[MajorFlow], outdir: &str, t0: f64) -> Result<()> {
    let file_path = format!("{}/major_flow_stats.csv", outdir);
    let mut wtr = csv::Writer::from_path(&file_path)?;

    // 写入标题行
    wtr.write_record(&[
        "major_flow_id",
        "status",
        "is_complete",
        "num_rounds",
        "start_time",
        "end_time",
        "start_rel_s",
        "end_rel_s",
        "total_duration_s",
        "average_round_duration_s",
        "failure_point",
        "round_ids",
        "round_loop_numbers",
    ])?;

    // 写入每个大流程的统计信息
    for major_flow in major_flows {
        let round_ids: Vec<String> = major_flow.rounds.iter()
            .map(|r| r.id.to_string())
            .collect();
        let round_loops: Vec<String> = major_flow.rounds.iter()
            .map(|r| r.loop_number.map_or("N/A".to_string(), |n| format!("循环{}", n)))
            .collect();

        let status = if major_flow.is_complete {
            "完整流程(循环1->8)".to_string()
        } else {
            format!("不完整流程")
        };

        wtr.write_record(&[
            major_flow.id.to_string(),
            status,
            major_flow.is_complete.to_string(),
            major_flow.rounds.len().to_string(),
            timestamp_to_beijing_time(major_flow.start_ts),
            timestamp_to_beijing_time(major_flow.end_ts),
            format!("{:.3}", major_flow.start_ts - t0),
            format!("{:.3}", major_flow.end_ts - t0),
            format!("{:.3}", major_flow.duration_s),
            format!("{:.3}", major_flow.average_round_duration_s),
            major_flow.failure_point.clone().unwrap_or_else(|| "N/A".to_string()),
            round_ids.join(","),
            round_loops.join(","),
        ])?;
    }

    wtr.flush()?;
    println!("Major flow statistics exported to {}", file_path);

    // 导出简单的大流程时间汇总文件（每行一个大流程时间）
    let summary_path = format!("{}/major_flow_time_summary.txt", outdir);
    let mut summary = Vec::new();

    for major_flow in major_flows {
        let status = if major_flow.is_complete {
            "完整"
        } else {
            "不完整"
        };

        // 格式：大流程ID | 状态 | 总时长(秒) | 平均每轮时长(秒) | 包含轮次数 | 开始时间 | 结束时间
        let line = format!(
            "大流程{} | {} | {:.2}秒 | 平均{:.2}秒/轮 | {}个轮次 | {} -> {}",
            major_flow.id,
            status,
            major_flow.duration_s,
            major_flow.average_round_duration_s,
            major_flow.rounds.len(),
            timestamp_to_beijing_time(major_flow.start_ts),
            timestamp_to_beijing_time(major_flow.end_ts)
        );
        summary.push(line);
    }

    std::fs::write(&summary_path, summary.join("\n"))?;
    println!("Major flow time summary exported to {}", summary_path);

    Ok(())
}

fn generate_gantt_charts(
    flows: &[NavigationFlow],
    rounds: &[Round],
    outdir: &str,
    t0: f64,
) -> Result<()> {
    // Group flows by round
    let mut round_flows: HashMap<usize, Vec<&NavigationFlow>> = HashMap::new();
    for flow in flows {
        round_flows
            .entry(flow.round_id)
            .or_default()
            .push(flow);
    }

    for round in rounds {
        if let Some(round_flows_data) = round_flows.get(&round.id) {
            generate_round_gantt(round, round_flows_data, outdir, t0)?;
        }
    }

    Ok(())
}

fn generate_round_gantt(
    round: &Round,
    flows: &[&NavigationFlow],
    outdir: &str,
    t0: f64,
) -> Result<()> {
    let _round_start = round.start_ts - t0;
    let round_duration = round.end_ts.map(|end| end - round.start_ts).unwrap_or(0.0);

    // Calculate Beijing time for round start and end
    let round_start_beijing = timestamp_to_beijing_time(round.start_ts);
    let round_end_beijing = round
        .end_ts
        .map(|end| timestamp_to_beijing_time(end))
        .unwrap_or_else(|| "未结束".to_string());

    // Prepare chart data: (label, detail_info, start, duration, type, sub_steps)
    let mut chart_data = Vec::new();

    for (flow_idx, flow) in flows.iter().enumerate() {
        let flow_id = flow_idx + 1;

        // 添加导航动作
        if let Some(nav_start_ts) = flow.nav_start_ts {
            let nav_start = nav_start_ts - round.start_ts;
            let nav_duration = flow.nav_end_ts
                .map(|end| end - nav_start_ts)
                .unwrap_or(1.0); // 如果导航未完成，也给1秒最小duration

            let target_pos = flow.nav_target_pos.as_deref().unwrap_or("unknown");
            let label = format!("F{}-nav", flow_id);
            let detail_info = format!("导航→{}", target_pos);

            chart_data.push((
                label,
                detail_info,
                nav_start.max(0.0),
                nav_duration.max(0.0),
                "navigation".to_string(),
                flow.nav_sub_steps.clone(),
            ));
        }

        // 添加其他动作
        for operation in &flow.operations {
            if let Some(op_start_ts) = operation.start_ts {
                let op_start = op_start_ts - round.start_ts;
                // 如果动作未完成（pending），给它一个最小可见的duration (比如1秒)
                let op_duration = operation.end_ts
                    .map(|end| end - op_start_ts)
                    .unwrap_or(1.0); // 改为1秒，确保pending动作也能在图上显示

                let (label, detail_info) = match operation.action_type.as_str() {
                    "arm" => {
                        let action_code = operation.action_code.unwrap_or(0);
                        let status_suffix = if operation.end_ts.is_none() { "[未完成]" } else { "" };
                        (
                            format!("F{}-arm-{}", flow_id, action_code),
                            format!("机械臂:{}({}){}", operation.label, action_code, status_suffix),
                        )
                    }
                    "head" => (format!("F{}-head", flow_id), "头部控制".to_string()),
                    "waist" => (format!("F{}-waist", flow_id), "腰部控制".to_string()),
                    _ => (
                        format!("F{}-{}", flow_id, operation.action_type),
                        operation.label.clone(),
                    ),
                };

                chart_data.push((
                    label,
                    detail_info,
                    op_start.max(0.0),
                    op_duration.max(0.0),
                    operation.action_type.clone(),
                    operation.sub_steps.clone(),
                ));
            }
        }
    }

    if chart_data.is_empty() {
        return Ok(());
    }

    // 按动作类型分组并计算每种类型的总耗时
    let mut action_types: Vec<String> = Vec::new();
    let mut action_type_map: HashMap<String, usize> = HashMap::new();
    let mut action_type_durations: HashMap<String, f64> = HashMap::new();

    // 先计算每种动作类型的总耗时
    for (_, _, _, duration, step_type, _) in &chart_data {
        *action_type_durations.entry(step_type.clone()).or_insert(0.0) += duration;
    }

    // 定义动作类型的顺序和显示名称
    let type_order = vec![
        ("navigation", "导航"),
        ("arm", "机械臂"),
        ("head", "头部控制"),
        ("waist", "腰部控制"),
    ];

    // 收集所有出现的动作类型并分配Y轴位置
    for (type_key, type_name) in &type_order {
        if chart_data.iter().any(|(_, _, _, _, t, _)| t == type_key) {
            action_type_map.insert(type_key.to_string(), action_types.len());
            let total_duration = action_type_durations.get(*type_key).unwrap_or(&0.0);
            action_types.push(format!("{} (总计: {:.1}s)", type_name, total_duration));
        }
    }

    // 处理其他未定义的动作类型
    for (_, _, _, _, action_type, _) in &chart_data {
        if !action_type_map.contains_key(action_type) {
            action_type_map.insert(action_type.clone(), action_types.len());
            let total_duration = action_type_durations.get(action_type).unwrap_or(&0.0);
            action_types.push(format!("其他({}) (总计: {:.1}s)", action_type, total_duration));
        }
    }

    let filename = format!("{}/round_{}_gantt.png", outdir, round.id);
    // 增加分辨率，使用更大的画布和更清晰的渲染
    let canvas_width = 3600; // 更高分辨率
    let bar_height = 180; // 增加条形高度，为每种类型留出更多空间
    let canvas_height = (action_types.len() * bar_height + 400) as u32; // 基于动作类型数量计算高度

    let root = BitMapBackend::new(&filename, (canvas_width, canvas_height)).into_drawing_area();
    root.fill(&WHITE)?;

    let max_time = round_duration.max(
        chart_data
            .iter()
            .map(|(_, _, start, dur, _, _)| start + dur)
            .fold(0.0, f64::max),
    );

    // 构建标题，包含循环编号
    let title = if let Some(loop_num) = round.loop_number {
        format!(
            "循环{} (Round {}) Timeline (Total: {:.3}s)\n北京时间: {} - {}",
            loop_num, round.id, round_duration, round_start_beijing, round_end_beijing
        )
    } else {
        format!(
            "Round {} Timeline (Total: {:.3}s)\n北京时间: {} - {}",
            round.id, round_duration, round_start_beijing, round_end_beijing
        )
    };

    let mut chart = ChartBuilder::on(&root)
        .caption(&title, ("sans-serif", 48)) // 增大标题字体
        .margin(50) // 增加边距
        .x_label_area_size(100) // 增加标签区域
        .y_label_area_size(200)
        .build_cartesian_2d(0.0..max_time * 1.1, 0.0..(action_types.len() as f64))?;

    // 配置网格和标签
    let mut mesh = chart.configure_mesh();
    mesh.y_desc("动作类型")
        .x_desc("时间 (相对于轮次开始的秒数)")
        .axis_desc_style(("sans-serif", 24)) // 增大轴标签字体
        .label_style(("sans-serif", 18)) // 增大刻度标签字体
        .y_label_formatter(&|y| {
            let idx = *y as usize;
            if idx < action_types.len() {
                action_types[idx].clone()
            } else {
                String::new()
            }
        })
        .draw()?;

    // 为同一类型的重叠动作计算垂直偏移
    let mut type_action_layers: HashMap<String, Vec<(f64, f64)>> = HashMap::new();

    for (_label, detail_info, start, duration, step_type, sub_steps) in &chart_data {
        let base_color = match step_type.as_str() {
            "nav" | "navigation" => RGBColor(173, 216, 230), // 浅蓝色 - 导航
            "arm" => RGBColor(144, 238, 144),                // 浅绿色 - 机械臂
            "head" => RGBColor(255, 218, 185),               // 浅橙色 - 头部控制
            "waist" => RGBColor(221, 160, 221),              // 浅紫色 - 腰部控制
            _ => RGBColor(192, 192, 192),                    // 灰色 - 其他
        };

        // 获取该动作类型的Y轴位置
        let y_base = *action_type_map.get(step_type).unwrap() as f64;

        // 检查是否有重叠的动作，如果有则稍微偏移
        let layers = type_action_layers.entry(step_type.clone()).or_insert_with(Vec::new);
        let mut layer_idx = 0;
        for layer in layers.iter() {
            let (layer_start, layer_end) = layer;
            if !(*start >= *layer_end || *start + *duration <= *layer_start) {
                layer_idx += 1;
            }
        }

        // 如果这是新的层，添加到层列表中
        if layer_idx >= layers.len() {
            layers.push((*start, *start + *duration));
        } else {
            layers[layer_idx] = (*start, *start + *duration);
        }

        let y_offset = layer_idx as f64 * 0.25; // 每层偏移0.25
        let y_pos = y_base + y_offset;
        let y_height = 0.4; // 条形高度

        // 绘制主方块 - 半透明填充
        chart.draw_series(std::iter::once(Rectangle::new(
            [
                (*start, y_pos + 0.15),
                (*start + *duration, y_pos + y_height + 0.15),
            ],
            base_color.mix(0.3).filled(), // 浅色填充
        )))?;

        // 绘制边框
        chart.draw_series(std::iter::once(Rectangle::new(
            [
                (*start, y_pos + 0.15),
                (*start + *duration, y_pos + y_height + 0.15),
            ],
            base_color,
        )))?;

        // 绘制子步骤
        if !sub_steps.is_empty() && *duration > 0.0 {
            let sub_y_start = y_pos + 0.15;  // 子步骤从主方块顶部开始
            let sub_y_height = y_height; // 子步骤填满整个主方块高度

            // 为不同类型的子步骤定义颜色
            let get_sub_step_color = |name: &str| -> RGBColor {
                if name == "开始执行" || name.starts_with("开始执行") {
                    RGBColor(100, 149, 237) // 矢车菊蓝
                } else if name == "设置导航目标" {
                    RGBColor(70, 130, 180) // 钢蓝
                } else if name == "发送目标" || name == "发送导航目标" || name == "发送头部控制目标" || name == "发送腰部控制目标" {
                    RGBColor(255, 165, 0) // 橙色
                } else if name == "服务端接受" {
                    RGBColor(60, 179, 113) // 中海绿
                } else if name == "结果回调" {
                    RGBColor(147, 112, 219) // 中紫色
                } else if name == "动作完成" || name.starts_with("动作完成") {
                    RGBColor(255, 69, 0) // 红橙色
                } else if name == "执行完成" {
                    RGBColor(34, 139, 34) // 森林绿
                } else {
                    RGBColor(128, 128, 128) // 灰色
                }
            };

            // 绘制每个子步骤之间的时间段
            for i in 0..sub_steps.len() {
                let sub_start_ts = sub_steps[i].timestamp;
                let sub_start = (sub_start_ts - round.start_ts).max(0.0);

                // 确定子步骤的结束时间
                let sub_end = if i + 1 < sub_steps.len() {
                    (sub_steps[i + 1].timestamp - round.start_ts).max(0.0)
                } else {
                    *start + *duration // 最后一个子步骤延伸到主动作结束
                };

                let sub_duration = sub_end - sub_start;

                if sub_duration > 0.0 {
                    // 根据子步骤名称获取颜色
                    let sub_color = get_sub_step_color(&sub_steps[i].name);

                    // 绘制子步骤方块 - 使用不同颜色
                    chart.draw_series(std::iter::once(Rectangle::new(
                        [
                            (sub_start, sub_y_start),
                            (sub_end, sub_y_start + sub_y_height),
                        ],
                        sub_color.filled(),
                    )))?;
                }
            }
        }

        // 在动作起点添加时间标注（小字体，位于方块下方）
        let start_time_text = format!("{:.1}s", *start);
        chart.draw_series(std::iter::once(Text::new(
            start_time_text,
            (*start, y_pos + y_height + 0.25), // 放在方块下方
            ("sans-serif", 14)
                .into_font()
                .color(&BLACK)
                .pos(Pos::new(HPos::Left, VPos::Top))
                .transform(FontTransform::None),
        )))?;

        // 在主方块顶部添加主标签
        let text_x = *start + *duration / 2.0;
        let text_y = y_pos + 0.08; // 放在方块顶部

        // 创建主标签文本
        let label_text = if *duration > 20.0 {
            format!("{}\n总计:{:.1}s", detail_info, duration)
        } else if *duration > 10.0 {
            format!("{} ({:.1}s)", detail_info, duration)
        } else if *duration > 5.0 {
            let short_detail = if detail_info.chars().count() > 10 {
                let truncated: String = detail_info.chars().take(7).collect();
                format!("{}...", truncated)
            } else {
                detail_info.clone()
            };
            format!("{} ({:.1}s)", short_detail, duration)
        } else if *duration > 2.0 {
            format!(
                "{} ({:.1}s)",
                match step_type.as_str() {
                    "nav" | "navigation" => "导航",
                    "arm" => "机械臂",
                    "head" => "头部",
                    "waist" => "腰部",
                    _ => "动作",
                },
                duration
            )
        } else {
            format!("{:.1}s", duration)
        };

        // 选择字体大小（适应更高分辨率）
        let font_size = if *duration > 15.0 {
            22
        } else if *duration > 8.0 {
            20
        } else if *duration > 2.0 {
            18
        } else {
            16
        };

        // 绘制主标签
        if *duration > 0.5 {
            chart.draw_series(std::iter::once(Text::new(
                label_text,
                (text_x, text_y),
                ("sans-serif", font_size)
                    .into_font()
                    .color(&BLACK)
                    .pos(Pos::new(HPos::Center, VPos::Top))
                    .transform(FontTransform::None),
            )))?;
        }
    }

    chart
        .configure_series_labels()
        .background_style(&WHITE.mix(0.8))
        .border_style(&BLACK)
        .draw()?;
    root.present()?;

    println!("Gantt chart saved: {}", filename);
    Ok(())
}

// 生成动作时间汇总表CSV
fn generate_action_timeline_csv(
    flows: &[NavigationFlow],
    _rounds: &[Round],
    outdir: &str,
    t0: f64,
) -> Result<()> {
    #[derive(Debug)]
    struct ActionRecord {
        round_id: usize,
        flow_id: usize,
        action_type: String,
        action_label: String,
        start_time_abs: f64,   // 绝对时间戳
        start_time_rel: f64,   // 相对时间（秒）
        end_time_abs: Option<f64>,
        end_time_rel: Option<f64>,
        duration: Option<f64>,
        status: String,
    }

    let mut all_actions = Vec::new();

    // 收集所有动作
    for (flow_idx, flow) in flows.iter().enumerate() {
        let flow_id = flow_idx + 1;
        let round_id = flow.round_id;

        // 添加导航动作
        if let Some(nav_start) = flow.nav_start_ts {
            let nav_end = flow.nav_end_ts;
            let duration = nav_end.map(|end| end - nav_start);

            all_actions.push(ActionRecord {
                round_id,
                flow_id,
                action_type: "导航".to_string(),
                action_label: format!("导航→{}", flow.nav_target_pos.as_deref().unwrap_or("未知")),
                start_time_abs: nav_start,
                start_time_rel: nav_start - t0,
                end_time_abs: nav_end,
                end_time_rel: nav_end.map(|e| e - t0),
                duration,
                status: flow.nav_status.clone(),
            });
        }

        // 添加其他动作
        for operation in &flow.operations {
            if let Some(start) = operation.start_ts {
                let end = operation.end_ts;
                let duration = end.map(|e| e - start);

                let action_label = match operation.action_type.as_str() {
                    "arm" => format!("机械臂:{}(code:{})", operation.label, operation.action_code.unwrap_or(0)),
                    "head" => "头部控制".to_string(),
                    "waist" => "腰部控制".to_string(),
                    _ => operation.label.clone(),
                };

                let action_type_display = match operation.action_type.as_str() {
                    "arm" => "机械臂".to_string(),
                    "head" => "头部".to_string(),
                    "waist" => "腰部".to_string(),
                    _ => operation.action_type.clone(),
                };

                all_actions.push(ActionRecord {
                    round_id,
                    flow_id,
                    action_type: action_type_display,
                    action_label,
                    start_time_abs: start,
                    start_time_rel: start - t0,
                    end_time_abs: end,
                    end_time_rel: end.map(|e| e - t0),
                    duration,
                    status: operation.status.clone(),
                });
            }
        }
    }

    // 按开始时间排序
    all_actions.sort_by(|a, b| a.start_time_rel.partial_cmp(&b.start_time_rel).unwrap());

    // 写入CSV文件
    let file_path = format!("{}/action_timeline.csv", outdir);
    let mut wtr = csv::Writer::from_path(&file_path)?;

    // 写入标题行
    wtr.write_record(&[
        "序号",
        "轮次",
        "流程",
        "动作类型",
        "动作详情",
        "开始时间(秒)",
        "结束时间(秒)",
        "持续时间(秒)",
        "开始时间(北京)",
        "结束时间(北京)",
        "状态",
    ])?;

    // 写入数据
    for (idx, action) in all_actions.iter().enumerate() {
        let seq_num = (idx + 1).to_string();
        let round_str = format!("Round {}", action.round_id);
        let flow_str = format!("Flow {}", action.flow_id);

        let start_rel_str = format!("{:.3}", action.start_time_rel);
        let end_rel_str = action.end_time_rel
            .map(|e| format!("{:.3}", e))
            .unwrap_or_else(|| "N/A".to_string());
        let duration_str = action.duration
            .map(|d| format!("{:.3}", d))
            .unwrap_or_else(|| "N/A".to_string());

        let start_beijing = timestamp_to_beijing_time(action.start_time_abs);
        let end_beijing = action.end_time_abs
            .map(|e| timestamp_to_beijing_time(e))
            .unwrap_or_else(|| "未完成".to_string());

        wtr.write_record(&[
            seq_num,
            round_str,
            flow_str,
            action.action_type.clone(),
            action.action_label.clone(),
            start_rel_str,
            end_rel_str,
            duration_str,
            start_beijing,
            end_beijing,
            action.status.clone(),
        ])?;
    }

    wtr.flush()?;
    println!("Action timeline exported to {}", file_path);

    // 生成简要统计
    let total_actions = all_actions.len();
    let completed_actions = all_actions.iter().filter(|a| a.end_time_abs.is_some()).count();
    let pending_actions = total_actions - completed_actions;

    let stats_path = format!("{}/action_timeline_stats.txt", outdir);
    let stats_content = format!(
        "动作时间轴统计\n\
        ==============\n\
        总动作数: {}\n\
        已完成: {} ({:.1}%)\n\
        未完成: {} ({:.1}%)\n\n\
        各类型动作统计:\n\
        导航: {} 个\n\
        机械臂: {} 个\n\
        头部: {} 个\n\
        腰部: {} 个\n",
        total_actions,
        completed_actions, (completed_actions as f64 / total_actions as f64 * 100.0),
        pending_actions, (pending_actions as f64 / total_actions as f64 * 100.0),
        all_actions.iter().filter(|a| a.action_type == "导航").count(),
        all_actions.iter().filter(|a| a.action_type == "机械臂").count(),
        all_actions.iter().filter(|a| a.action_type == "头部").count(),
        all_actions.iter().filter(|a| a.action_type == "腰部").count(),
    );

    std::fs::write(&stats_path, stats_content)?;
    println!("Action timeline stats exported to {}", stats_path);

    Ok(())
}
