//! CSV 导出模块
//!
//! 将 arm_decision 任务数据导出为 CSV 格式

use anyhow::Result;
use csv::Writer;

use crate::models::{ArmDecisionTask, CsvRecord};

/// 构建 CSV 记录
pub fn build_csv_records(tasks: &[ArmDecisionTask], t0: f64) -> Vec<CsvRecord> {
    let mut records = Vec::new();

    for (task_idx, task) in tasks.iter().enumerate() {
        let task_start_rel = task.start_ts - t0;
        let task_end_rel = task.end_ts.map(|t| t - t0);
        let task_duration = task.end_ts.map(|t| t - task.start_ts);

        for (mod_idx, module) in task.modules.iter().enumerate() {
            let mod_start_rel = module.start_ts - t0;
            let mod_end_rel = module.end_ts.map(|t| t - t0);
            let mod_duration = module.end_ts.map(|t| t - module.start_ts);

            records.push(CsvRecord {
                task_index: task_idx,
                task_start_rel_s: task_start_rel,
                task_end_rel_s: task_end_rel,
                task_duration_s: task_duration,
                task_type: task.task_type,
                result_status: task.result_status,
                module_index: mod_idx,
                module_name: module.name.clone(),
                cmd_code: module.cmd_code,
                module_start_rel_s: mod_start_rel,
                module_end_rel_s: mod_end_rel,
                module_duration_s: mod_duration,
                module_status: module.status.clone(),
            });
        }

        // 如果任务没有模块，也要输出一条记录
        if task.modules.is_empty() {
            records.push(CsvRecord {
                task_index: task_idx,
                task_start_rel_s: task_start_rel,
                task_end_rel_s: task_end_rel,
                task_duration_s: task_duration,
                task_type: task.task_type,
                result_status: task.result_status,
                module_index: 0,
                module_name: "(no modules)".to_string(),
                cmd_code: None,
                module_start_rel_s: task_start_rel,
                module_end_rel_s: task_end_rel,
                module_duration_s: task_duration,
                module_status: "n/a".to_string(),
            });
        }
    }

    records
}

/// 导出分析结果为 CSV
pub fn export_csv(records: &[CsvRecord], output_dir: &str) -> Result<()> {
    let path = format!("{}/arm_decision_analysis.csv", output_dir);
    let mut writer = Writer::from_path(&path)?;

    for record in records {
        writer.serialize(record)?;
    }

    writer.flush()?;
    Ok(())
}

/// 导出任务汇总统计
pub fn export_task_summary(tasks: &[ArmDecisionTask], output_dir: &str, t0: f64) -> Result<()> {
    let path = format!("{}/arm_decision_summary.csv", output_dir);
    let mut writer = Writer::from_path(&path)?;

    // 写入表头
    writer.write_record([
        "task_index",
        "task_type",
        "start_rel_s",
        "end_rel_s",
        "duration_s",
        "module_count",
        "result_status",
        "result_message",
    ])?;

    for (idx, task) in tasks.iter().enumerate() {
        let start_rel = task.start_ts - t0;
        let end_rel = task.end_ts.map(|t| t - t0);
        let duration = task.end_ts.map(|t| t - task.start_ts);

        writer.write_record([
            idx.to_string(),
            task.task_type.map(|t| t.to_string()).unwrap_or_default(),
            format!("{:.3}", start_rel),
            end_rel
                .map(|t| format!("{:.3}", t))
                .unwrap_or_else(|| "-".to_string()),
            duration
                .map(|d| format!("{:.3}", d))
                .unwrap_or_else(|| "-".to_string()),
            task.modules.len().to_string(),
            task.result_status
                .map(|s| s.to_string())
                .unwrap_or_default(),
            task.result_message.clone().unwrap_or_default(),
        ])?;
    }

    writer.flush()?;
    Ok(())
}
