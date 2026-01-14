//! CSV导出模块
//!
//! 本模块负责将分析结果导出为CSV文件

use anyhow::Result;

use crate::models::{CsvRecord, NavigationFlow, Round};
use crate::utils::timestamp_to_beijing_time;

/// 构建CSV记录
///
/// 将导航流程和轮次信息转换为CSV记录
///
/// # 参数
/// * `flows` - 导航流程切片
/// * `rounds` - 轮次切片
/// * `t0` - 起始时间戳（用于计算相对时间）
///
/// # 返回
/// CSV记录向量
pub fn build_csv_records(flows: &[NavigationFlow], rounds: &[Round], t0: f64) -> Vec<CsvRecord> {
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

/// 导出CSV文件
///
/// # 参数
/// * `records` - CSV记录切片
/// * `outdir` - 输出目录
pub fn export_csv(records: &[CsvRecord], outdir: &str) -> Result<()> {
    let mut wtr = csv::Writer::from_path(format!("{}/analysis.csv", outdir))?;

    for record in records {
        wtr.serialize(record)?;
    }

    wtr.flush()?;
    // 静默输出，避免在 TUI 模式下刷屏
    // println!("CSV exported to {}/analysis.csv", outdir);
    Ok(())
}

/// 生成动作时间汇总表CSV
///
/// # 参数
/// * `flows` - 导航流程切片
/// * `_rounds` - 轮次切片（未使用但保留接口兼容性）
/// * `outdir` - 输出目录
/// * `t0` - 起始时间戳
pub fn generate_action_timeline_csv(
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
        phase: String, // 阶段名称（空表示主动作）
        start_time_abs: f64,
        start_time_rel: f64,
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

            let action_label = match flow.nav_target_pos.as_deref() {
                Some(pos) => format!("导航→{}", pos),
                None => "导航".to_string(),
            };

            all_actions.push(ActionRecord {
                round_id,
                flow_id,
                action_type: "导航".to_string(),
                action_label: action_label.clone(),
                phase: String::new(),
                start_time_abs: nav_start,
                start_time_rel: nav_start - t0,
                end_time_abs: nav_end,
                end_time_rel: nav_end.map(|e| e - t0),
                duration,
                status: flow.nav_status.clone(),
            });

            // 添加导航的子阶段
            for (i, sub_step) in flow.nav_sub_steps.iter().enumerate() {
                let sub_end = if i + 1 < flow.nav_sub_steps.len() {
                    Some(flow.nav_sub_steps[i + 1].timestamp)
                } else {
                    nav_end
                };
                let sub_duration = sub_end.map(|e| e - sub_step.timestamp);

                all_actions.push(ActionRecord {
                    round_id,
                    flow_id,
                    action_type: "导航".to_string(),
                    action_label: action_label.clone(),
                    phase: sub_step.name.clone(),
                    start_time_abs: sub_step.timestamp,
                    start_time_rel: sub_step.timestamp - t0,
                    end_time_abs: sub_end,
                    end_time_rel: sub_end.map(|e| e - t0),
                    duration: sub_duration,
                    status: "phase".to_string(),
                });
            }
        }

        // 添加其他动作
        for operation in &flow.operations {
            // 如果这个 navigation 已经从 nav_start_ts 添加了，跳过（按时间戳匹配避免重复）
            if operation.action_type == "navigation" {
                if let (Some(nav_start), Some(op_start)) = (flow.nav_start_ts, operation.start_ts) {
                    // 时间戳差异小于 0.01 秒认为是同一个导航
                    if (nav_start - op_start).abs() < 0.01 {
                        continue;
                    }
                }
            }
            if let Some(start) = operation.start_ts {
                let end = operation.end_ts;
                let duration = end.map(|e| e - start);

                let action_label = match operation.action_type.as_str() {
                    "navigation" => "导航".to_string(),
                    "arm" => format!(
                        "机械臂:{}(code:{})",
                        operation.label,
                        operation.action_code.unwrap_or(0)
                    ),
                    "head" => "头部控制".to_string(),
                    "waist" => "腰部控制".to_string(),
                    "preplan" => operation.label.clone(),
                    _ => operation.label.clone(),
                };

                let action_type_display = match operation.action_type.as_str() {
                    "navigation" => "导航".to_string(),
                    "arm" => "机械臂".to_string(),
                    "head" => "头部".to_string(),
                    "waist" => "腰部".to_string(),
                    "preplan" => "预打舵".to_string(),
                    _ => operation.action_type.clone(),
                };

                all_actions.push(ActionRecord {
                    round_id,
                    flow_id,
                    action_type: action_type_display.clone(),
                    action_label: action_label.clone(),
                    phase: String::new(),
                    start_time_abs: start,
                    start_time_rel: start - t0,
                    end_time_abs: end,
                    end_time_rel: end.map(|e| e - t0),
                    duration,
                    status: operation.status.clone(),
                });

                // 添加动作的子阶段
                for (i, sub_step) in operation.sub_steps.iter().enumerate() {
                    let sub_end = if i + 1 < operation.sub_steps.len() {
                        Some(operation.sub_steps[i + 1].timestamp)
                    } else {
                        end
                    };
                    let sub_duration = sub_end.map(|e| e - sub_step.timestamp);

                    all_actions.push(ActionRecord {
                        round_id,
                        flow_id,
                        action_type: action_type_display.clone(),
                        action_label: action_label.clone(),
                        phase: sub_step.name.clone(),
                        start_time_abs: sub_step.timestamp,
                        start_time_rel: sub_step.timestamp - t0,
                        end_time_abs: sub_end,
                        end_time_rel: sub_end.map(|e| e - t0),
                        duration: sub_duration,
                        status: "phase".to_string(),
                    });
                }
            }
        }
    }

    // 按开始时间排序
    all_actions.sort_by(|a, b| a.start_time_rel.partial_cmp(&b.start_time_rel).unwrap());

    // 写入CSV文件
    let file_path = format!("{}/action_timeline.csv", outdir);
    let mut wtr = csv::Writer::from_path(&file_path)?;

    wtr.write_record([
        "序号",
        "轮次",
        "流程",
        "动作类型",
        "动作详情",
        "阶段",
        "开始时间(秒)",
        "结束时间(秒)",
        "持续时间(秒)",
        "开始时间(北京)",
        "结束时间(北京)",
        "状态",
    ])?;

    for (idx, action) in all_actions.iter().enumerate() {
        let seq_num = (idx + 1).to_string();
        let round_str = format!("Round {}", action.round_id);
        let flow_str = format!("Flow {}", action.flow_id);

        let start_rel_str = format!("{:.3}", action.start_time_rel);
        let end_rel_str = action
            .end_time_rel
            .map(|e| format!("{:.3}", e))
            .unwrap_or_else(|| "N/A".to_string());
        let duration_str = action
            .duration
            .map(|d| format!("{:.3}", d))
            .unwrap_or_else(|| "N/A".to_string());

        let start_beijing = timestamp_to_beijing_time(action.start_time_abs);
        let end_beijing = action
            .end_time_abs
            .map(timestamp_to_beijing_time)
            .unwrap_or_else(|| "未完成".to_string());

        wtr.write_record(&[
            seq_num,
            round_str,
            flow_str,
            action.action_type.clone(),
            action.action_label.clone(),
            action.phase.clone(),
            start_rel_str,
            end_rel_str,
            duration_str,
            start_beijing,
            end_beijing,
            action.status.clone(),
        ])?;
    }

    wtr.flush()?;
    // 静默输出，避免在 TUI 模式下刷屏
    // println!("Action timeline exported to {}", file_path);

    // 生成简要统计
    let total_actions = all_actions.len();
    let completed_actions = all_actions
        .iter()
        .filter(|a| a.end_time_abs.is_some())
        .count();
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
        completed_actions,
        (completed_actions as f64 / total_actions as f64 * 100.0),
        pending_actions,
        (pending_actions as f64 / total_actions as f64 * 100.0),
        all_actions
            .iter()
            .filter(|a| a.action_type == "导航")
            .count(),
        all_actions
            .iter()
            .filter(|a| a.action_type == "机械臂")
            .count(),
        all_actions
            .iter()
            .filter(|a| a.action_type == "头部")
            .count(),
        all_actions
            .iter()
            .filter(|a| a.action_type == "腰部")
            .count(),
    );

    std::fs::write(&stats_path, stats_content)?;
    // 静默输出，避免在 TUI 模式下刷屏
    // println!("Action timeline stats exported to {}", stats_path);

    Ok(())
}
