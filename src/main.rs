use std::collections::HashMap;
use std::fs;

use anyhow::Result;
use chrono::{DateTime, FixedOffset};
use clap::Parser;
use plotters::prelude::*;
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
    start_ts: f64,
    end_ts: Option<f64>,
    pose0: Option<String>,
    pose6: Option<String>,
}

#[derive(Debug, Clone)]
struct ActionOperation {
    action_type: String, // "arm", "navigation", "head", "waist"
    action_code: Option<u32>,
    label: String,
    start_ts: Option<f64>,
    end_ts: Option<f64>,
    status: String,
}

#[derive(Debug, Clone)]
struct NavigationFlow {
    nav_start_ts: Option<f64>,
    nav_end_ts: Option<f64>,
    nav_target_pos: Option<String>,
    nav_target_ori: Option<String>,
    nav_status: String,
    round_id: usize,
    operations: Vec<ActionOperation>, // 支持所有类型的动作
}

#[derive(Debug, Serialize)]
struct CsvRecord {
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
    let records = build_csv_records(&flows, &rounds, t0);
    println!("Generated {} CSV records", records.len());

    // Create output directory
    std::fs::create_dir_all(&args.outdir)?;

    // Export CSV
    export_csv(&records, &args.outdir)?;

    // Generate Gantt charts
    generate_gantt_charts(&records, &rounds, &args.outdir, t0)?;

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
    // 循环标记作为轮次边界
    let loop_start_regex = Regex::new(r"\[发布日志节点\]:\s*\[INFO\]\s*loop:\s*开始循环")?;
    let loop_end_regex = Regex::new(r"\[发布日志节点\]:\s*\[INFO\]\s*loop:\s*结束当前循环")?;
    let pose_regex = Regex::new(r"\[master_control\]:\s*姿态字符串:\s*(\{.*\})")?;

    let mut rounds = Vec::new();
    let mut current: Option<Round> = None;

    for line in lines {
        // 检测循环开始
        if loop_start_regex.is_match(&line.line) {
            // 结束上一轮（如果存在且未正常结束）
            if let Some(mut round) = current {
                if round.end_ts.is_none() {
                    round.end_ts = Some(line.timestamp);
                }
                rounds.push(round);
            }

            // 开始新轮次
            let id = rounds.len() + 1;
            current = Some(Round {
                id,
                start_ts: line.timestamp,
                end_ts: None,
                pose0: None,
                pose6: None,
            });
            continue;
        }

        // 检测循环结束
        if loop_end_regex.is_match(&line.line) {
            if let Some(ref mut round) = current {
                round.end_ts = Some(line.timestamp);
            }
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
    // 导航相关正则
    let nav_start_regex = Regex::new(r"\[导航\]:\s*NavAction2\[NavAction2\]\s*-\s*开始执行")?;
    let nav_target_regex = Regex::new(r"设置导航目标:\s*pos\(([^)]+)\),\s*ori\(([^)]+)\)")?;
    let nav_end_regex = Regex::new(r"\[导航\]:\s*NavAction2\[NavAction2\]\s*-\s*执行完成，结果:")?;

    // 机械臂相关正则
    // DoubleArmAction[动作名称] - 开始执行
    let arm_start_regex =
        Regex::new(r"\[机械臂\]:\s*DoubleArmAction\[([^\]]+)\]\s*-\s*开始执行")?;
    // setGoal action_type_code
    let arm_setgoal_regex =
        Regex::new(r"\[机械臂\]:\s*DoubleArmAction\s+setGoal\s+action_type_code:\s*(\d+)")?;
    // [RESULT CALLBACK] - 机械臂动作完成
    let arm_result_regex =
        Regex::new(r"\[机械臂\]:\s*\[RESULT CALLBACK\]\s*-\s*机械臂动作完成，状态:\s*(\d+)")?;
    // 执行完成
    let arm_complete_regex =
        Regex::new(r"\[机械臂\]:\s*DoubleArmAction\[([^\]]+)\]\s*-\s*执行完成，结果:")?;

    // 头部控制相关正则
    let head_start_regex =
        Regex::new(r"\[头部控制\]:\s*HeadControlAction2\[head_control\]\s*-\s*开始执行")?;
    let head_end_regex =
        Regex::new(r"\[头部控制\]:\s*HeadControlAction2\[head_control\]\s*-\s*执行完成")?;

    // 腰部控制相关正则
    let waist_start_regex = Regex::new(r"\[腰部\]:\s*WaistAction2\[WaistAction2\]\s*-\s*开始执行")?;
    let waist_end_regex =
        Regex::new(r"\[腰部\]:\s*WaistAction2\[WaistAction2\]\s*-\s*执行完成，结果:")?;

    let mut flows = Vec::new();
    let mut current_flow: Option<NavigationFlow> = None;

    for line in lines {
        // 导航开始 - NavAction2[NavAction2] - 开始执行
        if nav_start_regex.is_match(&line.line) {
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
                }
            }
            continue;
        }

        // 导航结束 - 导航完成，结果代码: 0
        if nav_end_regex.is_match(&line.line) {
            if let Some(mut flow) = current_flow.take() {
                flow.nav_end_ts = Some(line.timestamp);
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
                label: action_label,
                start_ts: Some(line.timestamp),
                end_ts: None,
                status: "pending".to_string(),
            };

            // 添加到当前流程
            if let Some(ref mut flow) = current_flow {
                flow.operations.push(arm_action);
            } else if let Some(flow) = flows.last_mut() {
                flow.operations.push(arm_action);
            }
            continue;
        }

        // 机械臂动作完成 - [RESULT CALLBACK]
        if let Some(caps) = arm_result_regex.captures(&line.line) {
            let status = caps[1].trim();

            // 找到最近的未完成的机械臂动作
            if let Some(ref mut flow) = current_flow {
                for op in flow.operations.iter_mut().rev() {
                    if op.action_type == "arm" && op.end_ts.is_none() {
                        op.end_ts = Some(line.timestamp);
                        op.status = if status == "0" {
                            "ok".to_string()
                        } else {
                            format!("status_{}", status)
                        };
                        break;
                    }
                }
            } else if let Some(flow) = flows.last_mut() {
                for op in flow.operations.iter_mut().rev() {
                    if op.action_type == "arm" && op.end_ts.is_none() {
                        op.end_ts = Some(line.timestamp);
                        op.status = if status == "0" {
                            "ok".to_string()
                        } else {
                            format!("status_{}", status)
                        };
                        break;
                    }
                }
            }
            continue;
        }

        // 机械臂动作完成 - 执行完成
        if arm_complete_regex.is_match(&line.line) {
            // 这个日志通常在 RESULT CALLBACK 之后，不需要特别处理
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
            };

            if let Some(ref mut flow) = current_flow {
                flow.operations.push(head_action);
            } else if let Some(flow) = flows.last_mut() {
                flow.operations.push(head_action);
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
            };

            if let Some(ref mut flow) = current_flow {
                flow.operations.push(waist_action);
            } else if let Some(flow) = flows.last_mut() {
                flow.operations.push(waist_action);
            }
            continue;
        }


        // 头部控制完成 - 找到对应的pending头部动作并更新
        if head_end_regex.is_match(&line.line) {
            // 在当前流程或最后一个流程中找到未完成的头部动作
            if let Some(ref mut flow) = current_flow {
                for op in flow.operations.iter_mut().rev() {
                    if op.action_type == "head" && op.end_ts.is_none() {
                        op.end_ts = Some(line.timestamp);
                        op.status = "ok".to_string();
                        break;
                    }
                }
            } else if let Some(flow) = flows.last_mut() {
                for op in flow.operations.iter_mut().rev() {
                    if op.action_type == "head" && op.end_ts.is_none() {
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
                        op.end_ts = Some(line.timestamp);
                        op.status = "ok".to_string();
                        break;
                    }
                }
            } else if let Some(flow) = flows.last_mut() {
                for op in flow.operations.iter_mut().rev() {
                    if op.action_type == "waist" && op.end_ts.is_none() {
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

fn build_csv_records(flows: &[NavigationFlow], rounds: &[Round], t0: f64) -> Vec<CsvRecord> {
    let mut records = Vec::new();

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
        records.push(CsvRecord {
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

            records.push(CsvRecord {
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

fn generate_gantt_charts(
    records: &[CsvRecord],
    rounds: &[Round],
    outdir: &str,
    t0: f64,
) -> Result<()> {
    // Group records by round
    let mut round_records: HashMap<usize, Vec<&CsvRecord>> = HashMap::new();
    for record in records {
        round_records
            .entry(record.round_id)
            .or_default()
            .push(record);
    }

    for round in rounds {
        if let Some(round_data) = round_records.get(&round.id) {
            generate_round_gantt(round, round_data, outdir, t0)?;
        }
    }

    Ok(())
}

fn generate_round_gantt(
    round: &Round,
    records: &[&CsvRecord],
    outdir: &str,
    t0: f64,
) -> Result<()> {
    let round_start = round.start_ts - t0;
    let round_duration = round.end_ts.map(|end| end - round.start_ts).unwrap_or(0.0);

    // Calculate Beijing time for round start and end
    let round_start_beijing = timestamp_to_beijing_time(round.start_ts);
    let round_end_beijing = round
        .end_ts
        .map(|end| timestamp_to_beijing_time(end))
        .unwrap_or_else(|| "未结束".to_string());

    // Filter and prepare data
    let mut chart_data = Vec::new();
    for record in records {
        if let (Some(start), Some(duration)) = (record.start_rel_s, record.duration_s) {
            let relative_start = start - round_start;
            let (label, detail_info) = match record.step_type.as_str() {
                "nav" | "navigation" => {
                    let target_pos = record.nav_target_pos.as_deref().unwrap_or("unknown");
                    (
                        format!("F{}-nav", record.flow_id),
                        format!("导航→{}", target_pos),
                    )
                }
                "arm" => {
                    let action_label = record.action_label.as_deref().unwrap_or("unknown");
                    let action_code = record.action_code.unwrap_or(0);
                    (
                        format!("F{}-arm-{}", record.flow_id, action_code),
                        format!("机械臂:{}({})", action_label, action_code),
                    )
                }
                "head" => (format!("F{}-head", record.flow_id), "头部控制".to_string()),
                "waist" => (format!("F{}-waist", record.flow_id), "腰部控制".to_string()),
                _ => {
                    let action_label = record.action_label.as_deref().unwrap_or("unknown");
                    (
                        format!("F{}-{}", record.flow_id, record.step_type),
                        action_label.to_string(),
                    )
                }
            };

            chart_data.push((
                label,
                detail_info,
                relative_start.max(0.0),
                duration.max(0.0),
                record.step_type.clone(),
            ));
        }
    }

    if chart_data.is_empty() {
        return Ok(());
    }

    let filename = format!("{}/round_{}_gantt.png", outdir, round.id);
    // 增加分辨率，使用更大的画布和更清晰的渲染
    let canvas_width = 2800; // 双倍宽度
    let bar_height = 80; // 增加条形高度
    let canvas_height = (chart_data.len() * bar_height + 250) as u32; // 增加高度以容纳时间信息

    let root = BitMapBackend::new(&filename, (canvas_width, canvas_height)).into_drawing_area();
    root.fill(&WHITE)?;

    let max_time = round_duration.max(
        chart_data
            .iter()
            .map(|(_, _, start, dur, _)| start + dur)
            .fold(0.0, f64::max),
    );

    let mut chart = ChartBuilder::on(&root)
        .caption(
            &format!(
                "Round {} Timeline (Total: {:.3}s)\n北京时间: {} - {}",
                round.id, round_duration, round_start_beijing, round_end_beijing
            ),
            ("sans-serif", 42),
        ) // 保持字体大小，增加时间信息
        .margin(40) // 增加边距
        .x_label_area_size(80) // 增加标签区域
        .y_label_area_size(120)
        .build_cartesian_2d(0.0..max_time * 1.1, 0.0..(chart_data.len() as f64))?;

    chart
        .configure_mesh()
        .y_desc("Operations")
        .x_desc("Time (seconds relative to round start)")
        .axis_desc_style(("sans-serif", 20)) // 增大轴标签字体
        .label_style(("sans-serif", 16)) // 增大刻度标签字体
        .draw()?;

    for (idx, (_label, detail_info, start, duration, step_type)) in chart_data.iter().enumerate() {
        let color = match step_type.as_str() {
            "nav" | "navigation" => RGBColor(173, 216, 230).filled(), // 浅蓝色 - 导航
            "arm" => RGBColor(144, 238, 144).filled(),                // 浅绿色 - 机械臂
            "head" => RGBColor(255, 218, 185).filled(),               // 浅橙色 - 头部控制
            "waist" => RGBColor(221, 160, 221).filled(),              // 浅紫色 - 腰部控制
            _ => RGBColor(192, 192, 192).filled(),                    // 灰色 - 其他
        };

        let y_pos = idx as f64;
        let y_height = 0.7; // 调整条形高度比例

        // Draw the rectangle with better spacing
        chart.draw_series(std::iter::once(Rectangle::new(
            [
                (*start, y_pos + 0.15),
                (*start + *duration, y_pos + y_height + 0.15),
            ],
            color,
        )))?;

        // Add text label in the middle of the rectangle
        let text_x = *start + *duration / 2.0;
        let text_y = y_pos + y_height / 2.0 + 0.15;

        // Create label text with duration - always show duration with >time< format
        let label_text = if *duration > 20.0 {
            // For very long operations, show detail and duration on separate lines
            format!("{}\n>{:.1}s<", detail_info, duration)
        } else if *duration > 10.0 {
            // For long operations, show detail and duration
            format!("{} >{:.1}s<", detail_info, duration)
        } else if *duration > 5.0 {
            // For medium operations, show abbreviated detail and duration
            let short_detail = if detail_info.chars().count() > 10 {
                let truncated: String = detail_info.chars().take(7).collect();
                format!("{}...", truncated)
            } else {
                detail_info.clone()
            };
            format!("{} >{:.1}s<", short_detail, duration)
        } else if *duration > 2.0 {
            // For short operations, show type and duration
            format!(
                "{} >{:.1}s<",
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
            // For very short operations, just show duration
            format!(">{:.1}s<", duration)
        };

        // Choose font size based on rectangle width and height - 增大字体以适应高分辨率
        let font_size = if *duration > 15.0 {
            24
        } else if *duration > 8.0 {
            20
        } else if *duration > 2.0 {
            16
        } else {
            14
        };

        // Only draw text if there's enough space (minimum 1 second width)
        if *duration > 1.0 {
            chart.draw_series(std::iter::once(Text::new(
                label_text,
                (text_x, text_y),
                ("sans-serif", font_size)
                    .into_font()
                    .color(&BLACK)
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
