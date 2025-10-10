//! Master Control Analyzer - 主程序
//!
//! 用于分析机器人控制系统日志的命令行工具

use anyhow::Result;
use clap::Parser;

use master_control_analyzer::csv_exporter::{
    build_csv_records, export_csv, export_major_flow_stats, generate_action_timeline_csv,
};
use master_control_analyzer::flow_detector::detect_flows;
use master_control_analyzer::gantt::generate_gantt_charts;
use master_control_analyzer::models::Args;
use master_control_analyzer::parser::load_log_lines;
use master_control_analyzer::round_detector::{detect_major_flows, detect_rounds};

fn main() -> Result<()> {
    let args = Args::parse();

    // 加载日志行
    let lines = load_log_lines(&args.log)?;
    if lines.is_empty() {
        anyhow::bail!("No timestamped lines found in log file");
    }

    let t0 = lines[0].timestamp;
    let t_last = lines.last().unwrap().timestamp;

    // 检测轮次
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

    // 检测大流程
    let major_flows = detect_major_flows(&rounds);
    println!("Detected {} major flows", major_flows.len());
    for major_flow in &major_flows {
        let status = if major_flow.is_complete {
            "完整流程"
        } else {
            &format!(
                "不完整流程 ({})",
                major_flow
                    .failure_point
                    .as_ref()
                    .unwrap_or(&"未知".to_string())
            )
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

    // 检测导航流程
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

    // 构建CSV记录
    let records = build_csv_records(&flows, &rounds, &major_flows, t0);
    println!("Generated {} CSV records", records.len());

    // 创建输出目录
    std::fs::create_dir_all(&args.outdir)?;

    // 导出CSV
    export_csv(&records, &args.outdir)?;

    // 导出大流程统计
    export_major_flow_stats(&major_flows, &args.outdir, t0)?;

    // 生成甘特图
    generate_gantt_charts(&flows, &rounds, &args.outdir, t0)?;

    // 生成动作时间轴汇总表
    generate_action_timeline_csv(&flows, &rounds, &args.outdir, t0)?;

    println!("Analysis complete! Output in: {}", args.outdir);

    Ok(())
}
