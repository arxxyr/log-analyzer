//! 甘特图生成模块
//!
//! 本模块负责生成任务轮次的甘特图可视化

use std::collections::HashMap;

use anyhow::Result;
use plotters::prelude::*;
use plotters::style::text_anchor::{HPos, Pos, VPos};

use crate::font_loader::FontLoader;
use crate::models::{NavigationFlow, Round, SubStep};
use crate::utils::timestamp_to_beijing_time;

/// 生成所有轮次的甘特图
///
/// # 参数
/// * `flows` - 导航流程切片
/// * `rounds` - 轮次切片
/// * `outdir` - 输出目录
/// * `t0` - 起始时间戳
pub fn generate_gantt_charts(
    flows: &[NavigationFlow],
    rounds: &[Round],
    outdir: &str,
    t0: f64,
) -> Result<()> {
    // 按轮次分组flows
    let mut round_flows: HashMap<usize, Vec<&NavigationFlow>> = HashMap::new();
    for flow in flows {
        round_flows.entry(flow.round_id).or_default().push(flow);
    }

    // 计算需要的数字位数（根据轮次总数）
    let total_rounds = rounds.len();
    let width = if total_rounds >= 100 {
        3
    } else if total_rounds >= 10 {
        2
    } else {
        1
    };

    for round in rounds {
        if let Some(round_flows_data) = round_flows.get(&round.id) {
            generate_round_gantt(round, round_flows_data, outdir, t0, width)?;
        }
    }

    Ok(())
}

/// 生成单个轮次的甘特图
///
/// # 参数
/// * `round` - 轮次
/// * `flows` - 属于该轮次的导航流程列表
/// * `outdir` - 输出目录
/// * `t0` - 起始时间戳
/// * `width` - 编号宽度（用于前导零）
fn generate_round_gantt(
    round: &Round,
    flows: &[&NavigationFlow],
    outdir: &str,
    t0: f64,
    width: usize,
) -> Result<()> {
    // 创建字体加载器
    let font_loader = FontLoader::default();

    let _round_start = round.start_ts - t0;
    let round_duration = round.end_ts.map(|end| end - round.start_ts).unwrap_or(0.0);

    // 计算轮次开始和结束的北京时间
    let round_start_beijing = timestamp_to_beijing_time(round.start_ts);
    let round_end_beijing = round
        .end_ts
        .map(timestamp_to_beijing_time)
        .unwrap_or_else(|| "未结束".to_string());

    // 准备图表数据: (label, detail_info, start, duration, type, sub_steps)
    let mut chart_data = Vec::new();

    for (flow_idx, flow) in flows.iter().enumerate() {
        let flow_id = flow_idx + 1;

        // 添加导航动作
        if let Some(nav_start_ts) = flow.nav_start_ts {
            let nav_start = nav_start_ts - round.start_ts;
            let nav_duration = flow.nav_end_ts.map(|end| end - nav_start_ts).unwrap_or(1.0);

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
                let op_duration = operation.end_ts.map(|end| end - op_start_ts).unwrap_or(1.0);

                let (label, detail_info) = match operation.action_type.as_str() {
                    "arm" => {
                        let action_code = operation.action_code.unwrap_or(0);
                        let status_suffix = if operation.end_ts.is_none() {
                            "[未完成]"
                        } else {
                            ""
                        };
                        (
                            format!("F{}-arm-{}", flow_id, action_code),
                            format!(
                                "机械臂:{}({}){}",
                                operation.label, action_code, status_suffix
                            ),
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

    // 按开始时间排序chart_data，确保layer分配算法正确工作
    chart_data.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap());

    // 去重：移除完全相同的条目（基于label、start、duration、type）
    chart_data.dedup_by(|a, b| {
        a.0 == b.0 && // label
        (a.2 - b.2).abs() < 0.001 && // start (浮点数比较)
        (a.3 - b.3).abs() < 0.001 && // duration
        a.4 == b.4 // type
    });

    // 按动作类型分组并计算每种类型的总耗时
    let mut action_types: Vec<String> = Vec::new();
    let mut action_type_map: HashMap<String, usize> = HashMap::new();
    let mut action_type_durations: HashMap<String, f64> = HashMap::new();

    // 先计算每种动作类型的总耗时
    for (_, _, _, duration, step_type, _) in &chart_data {
        *action_type_durations
            .entry(step_type.clone())
            .or_insert(0.0) += duration;
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
            action_types.push(format!(
                "其他({}) (总计: {:.1}s)",
                action_type, total_duration
            ));
        }
    }

    let filename = format!("{}/round_{:0width$}_gantt.png", outdir, round.id, width = width);
    let canvas_width = 3600; // 更高分辨率
    let bar_height = 180; // 增加条形高度
    let canvas_height = (action_types.len() * bar_height + 400) as u32;

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
        .caption(&title, ("sans-serif", 48))
        .margin(50)
        .x_label_area_size(100)
        .y_label_area_size(200)
        .build_cartesian_2d(0.0..max_time * 1.1, 0.0..(action_types.len() as f64))?;

    // 配置网格和标签
    let mut mesh = chart.configure_mesh();
    mesh.y_desc("动作类型")
        .x_desc("时间 (相对于轮次开始的秒数)")
        .axis_desc_style(("sans-serif", 24))
        .label_style(("sans-serif", 18))
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
    // 每个动作类型维护多个层，每层记录其最后的结束时间
    let mut type_action_layers: HashMap<String, Vec<f64>> = HashMap::new();

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

        // 找到第一个可用的层（结束时间早于当前动作开始时间）
        let layers = type_action_layers
            .entry(step_type.clone())
            .or_default();

        let mut layer_idx = 0;
        let mut found_layer = false;

        // 遍历已有的层，找到第一个不冲突的层
        for (idx, &end_time) in layers.iter().enumerate() {
            if *start >= end_time {
                // 当前动作的开始时间晚于或等于这一层的结束时间，可以使用这一层
                layer_idx = idx;
                found_layer = true;
                break;
            }
        }

        // 如果没有找到可用的层，创建新层
        if !found_layer {
            layer_idx = layers.len();
            layers.push(*start + *duration);
        } else {
            // 更新该层的结束时间
            layers[layer_idx] = *start + *duration;
        }

        let y_offset = layer_idx as f64 * 0.25; // 每层偏移0.25
        let y_pos = y_base + y_offset;
        let y_height = 0.4; // 条形高度

        // 绘制主方块
        chart.draw_series(std::iter::once(Rectangle::new(
            [
                (*start, y_pos + 0.15),
                (*start + *duration, y_pos + y_height + 0.15),
            ],
            base_color.mix(0.3).filled(),
        )))?;

        // 绘制子步骤
        if !sub_steps.is_empty() && *duration > 0.0 {
            draw_sub_steps(
                &mut chart,
                sub_steps,
                round,
                *start,
                *duration,
                y_pos + 0.15,
                y_height,
            )?;
        }

        // 在动作起点添加时间标注
        draw_time_label(&mut chart, &font_loader, *start, y_pos, y_height)?;

        // 在主方块顶部添加主标签
        draw_main_label(&mut chart, &font_loader, detail_info, *start, *duration, y_pos, step_type)?;
    }

    chart
        .configure_series_labels()
        .background_style(WHITE.mix(0.8))
        .border_style(BLACK)
        .draw()?;
    root.present()?;

    // 不再输出每个甘特图的保存消息，避免在 TUI 模式下刷屏
    // 改为在分析结束时汇总显示
    // println!("Gantt chart saved: {}", filename);
    Ok(())
}

/// 绘制子步骤
fn draw_sub_steps<DB: DrawingBackend>(
    chart: &mut ChartContext<
        DB,
        Cartesian2d<plotters::coord::types::RangedCoordf64, plotters::coord::types::RangedCoordf64>,
    >,
    sub_steps: &[SubStep],
    round: &Round,
    start: f64,
    duration: f64,
    sub_y_start: f64,
    sub_y_height: f64,
) -> Result<()>
where
    DB::ErrorType: 'static,
{
    // 为不同类型的子步骤定义颜色
    let get_sub_step_color = |name: &str| -> RGBColor {
        if name == "开始执行" || name.starts_with("开始执行") {
            RGBColor(100, 149, 237) // 矢车菊蓝
        } else if name == "设置导航目标" {
            RGBColor(70, 130, 180) // 钢蓝
        } else if name == "发送目标"
            || name == "发送导航目标"
            || name == "发送头部控制目标"
            || name == "发送腰部控制目标"
        {
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
        let sub_start_abs = (sub_start_ts - round.start_ts).max(0.0);

        // 确定子步骤的结束时间（绝对坐标）
        let sub_end_abs = if i + 1 < sub_steps.len() {
            (sub_steps[i + 1].timestamp - round.start_ts).max(0.0)
        } else {
            start + duration // 最后一个子步骤延伸到主动作结束
        };

        // 将子步骤限制在主方块的范围内
        let sub_start_clamped = sub_start_abs.max(start).min(start + duration);
        let sub_end_clamped = sub_end_abs.max(start).min(start + duration);

        let sub_duration = sub_end_clamped - sub_start_clamped;

        if sub_duration > 0.0 {
            // 根据子步骤名称获取颜色
            let sub_color = get_sub_step_color(&sub_steps[i].name);

            // 绘制子步骤方块（只在主方块范围内）
            chart.draw_series(std::iter::once(Rectangle::new(
                [
                    (sub_start_clamped, sub_y_start),
                    (sub_end_clamped, sub_y_start + sub_y_height),
                ],
                sub_color.filled(),
            )))?;
        }
    }

    Ok(())
}

/// 绘制时间标注
fn draw_time_label<DB: DrawingBackend>(
    chart: &mut ChartContext<
        DB,
        Cartesian2d<plotters::coord::types::RangedCoordf64, plotters::coord::types::RangedCoordf64>,
    >,
    font_loader: &FontLoader,
    start: f64,
    y_pos: f64,
    y_height: f64,
) -> Result<()>
where
    DB::ErrorType: 'static,
{
    let start_time_text = format!("{:.1}s", start);
    chart.draw_series(std::iter::once(Text::new(
        start_time_text,
        (start, y_pos + y_height + 0.25),
        font_loader.font_desc(14)
            .color(&BLACK)
            .pos(Pos::new(HPos::Left, VPos::Top))
            .transform(FontTransform::None),
    )))?;

    Ok(())
}

/// 绘制主标签
fn draw_main_label<DB: DrawingBackend>(
    chart: &mut ChartContext<
        DB,
        Cartesian2d<plotters::coord::types::RangedCoordf64, plotters::coord::types::RangedCoordf64>,
    >,
    font_loader: &FontLoader,
    detail_info: &str,
    start: f64,
    duration: f64,
    y_pos: f64,
    step_type: &str,
) -> Result<()>
where
    DB::ErrorType: 'static,
{
    let text_x = start + duration / 2.0;
    let text_y = y_pos + 0.08;

    // 创建主标签文本
    let label_text = if duration > 20.0 {
        format!("{}\n总计:{:.1}s", detail_info, duration)
    } else if duration > 10.0 {
        format!("{} ({:.1}s)", detail_info, duration)
    } else if duration > 5.0 {
        let short_detail = if detail_info.chars().count() > 10 {
            let truncated: String = detail_info.chars().take(7).collect();
            format!("{}...", truncated)
        } else {
            detail_info.to_string()
        };
        format!("{} ({:.1}s)", short_detail, duration)
    } else if duration > 2.0 {
        format!(
            "{} ({:.1}s)",
            match step_type {
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

    // 选择字体大小
    let font_size = if duration > 15.0 {
        22
    } else if duration > 8.0 {
        20
    } else if duration > 2.0 {
        18
    } else {
        16
    };

    // 绘制主标签
    if duration > 0.5 {
        chart.draw_series(std::iter::once(Text::new(
            label_text,
            (text_x, text_y),
            font_loader.font_desc(font_size)
                .color(&BLACK)
                .pos(Pos::new(HPos::Center, VPos::Top))
                .transform(FontTransform::None),
        )))?;
    }

    Ok(())
}
