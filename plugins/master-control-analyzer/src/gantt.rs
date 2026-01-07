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
        let round_flows_data = round_flows
            .get(&round.id)
            .map(|v| v.as_slice())
            .unwrap_or(&[]);

        generate_round_gantt(round, round_flows_data, outdir, t0, width)?;
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
    _t0: f64,
    width: usize,
) -> Result<()> {
    // 创建字体加载器
    let font_loader = FontLoader::default();

    let _round_start = round.start_ts - _t0;
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

            let label = format!("F{}-nav", flow_id);
            let detail_info = match flow.nav_target_pos.as_deref() {
                Some(pos) => format!("导航→{}", pos),
                None => "导航".to_string(),
            };

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
            // 如果这个 navigation 已经从 nav_start_ts 添加了，跳过（按时间戳匹配避免重复）
            if operation.action_type == "navigation" {
                if let (Some(nav_start), Some(op_start)) = (flow.nav_start_ts, operation.start_ts) {
                    // 时间戳差异小于 0.01 秒认为是同一个导航
                    if (nav_start - op_start).abs() < 0.01 {
                        continue;
                    }
                }
            }
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
                            format!("手臂:{}{}", action_code, status_suffix),
                        )
                    }
                    "head" => (format!("F{}-head", flow_id), "头部控制".to_string()),
                    "waist" => (format!("F{}-waist", flow_id), "腰部控制".to_string()),
                    "preplan" => {
                        let action_code = operation.action_code.unwrap_or(0);
                        (
                            format!("F{}-preplan-{}", flow_id, action_code),
                            format!("预打舵(action={})", action_code),
                        )
                    }
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
        ("preplan", "预打舵"),
        ("arm", "手臂"),
        ("head", "头部"),
        ("waist", "腰部"),
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

    let filename = format!(
        "{}/round_{:0width$}_gantt.png",
        outdir,
        round.id,
        width = width
    );
    let canvas_width = 14400; // 超高分辨率 (4x)
    let bar_height = 720; // 增加条形高度 (4x)
    let canvas_height = (action_types.len() * bar_height + 1600) as u32;

    let root = BitMapBackend::new(&filename, (canvas_width, canvas_height)).into_drawing_area();
    root.fill(&WHITE)?;

    let max_time = round_duration.max(
        chart_data
            .iter()
            .map(|(_, _, start, dur, _, _)| start + dur)
            .fold(0.0, f64::max),
    );

    // 构建标题，包含循环类型
    let title = format!(
        "{} (Round {}) Timeline (Total: {:.3}s)\n层级{} | 北京时间: {} - {}",
        round.cycle_type,
        round.id,
        round_duration,
        round.layer_index,
        round_start_beijing,
        round_end_beijing
    );

    let mut chart = ChartBuilder::on(&root)
        .caption(&title, ("sans-serif", 192))
        .margin(200)
        .x_label_area_size(400)
        .y_label_area_size(800)
        .build_cartesian_2d(0.0..max_time * 1.1, 0.0..(action_types.len() as f64))?;

    // 配置网格和标签
    let mut mesh = chart.configure_mesh();
    mesh.y_desc("动作类型")
        .x_desc("时间 (相对于轮次开始的秒数)")
        .axis_desc_style(("sans-serif", 96))
        .label_style(("sans-serif", 72))
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
            "preplan" => RGBColor(255, 255, 150),            // 浅黄色 - 预打舵
            "arm" => RGBColor(144, 238, 144),                // 浅绿色 - 手臂
            "head" => RGBColor(255, 218, 185),               // 浅橙色 - 头部
            "waist" => RGBColor(221, 160, 221),              // 浅紫色 - 腰部
            _ => RGBColor(192, 192, 192),                    // 灰色 - 其他
        };

        // 获取该动作类型的Y轴位置
        let y_base = *action_type_map.get(step_type).unwrap() as f64;

        // 找到第一个可用的层（结束时间早于当前动作开始时间）
        let layers = type_action_layers.entry(step_type.clone()).or_default();

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
                &font_loader,
                max_time,
            )?;
        }

        // 在动作起点添加时间标注
        draw_time_label(&mut chart, &font_loader, *start, y_pos, y_height)?;

        // 在主方块顶部添加主标签
        draw_main_label(
            &mut chart,
            &font_loader,
            detail_info,
            *start,
            *duration,
            y_pos,
            step_type,
        )?;
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
    font_loader: &FontLoader,
    total_time_range: f64, // 总时间范围，用于判断子步骤是否足够宽
) -> Result<()>
where
    DB::ErrorType: 'static,
{
    // 为不同类型的子步骤定义颜色
    let get_sub_step_color = |name: &str, index: usize| -> RGBColor {
        // BehaviorTree 模块 - 使用交替颜色增强分隔
        if name.contains("GetReadyPose") {
            RGBColor(135, 206, 250) // 淡天蓝
        } else if name.contains("ModifyArmObstacle") {
            RGBColor(255, 182, 193) // 淡粉红
        } else if name.contains("GetGoalPose") {
            RGBColor(152, 251, 152) // 淡绿
        } else if name.contains("ArmTransitionPoint") {
            RGBColor(255, 218, 185) // 桃色
        } else if name.contains("ExecuteDoubleArmMove") {
            RGBColor(173, 216, 230) // 淡蓝
        } else if name.contains("节点开始") {
            RGBColor(100, 149, 237) // 矢车菊蓝
        } else if name.contains("节点结束") {
            RGBColor(34, 139, 34) // 森林绿
        }
        // ROS2ActionAdapter 阶段相关
        else if name == "开始执行" {
            RGBColor(100, 149, 237) // 矢车菊蓝
        } else if name.starts_with("等待服务器") {
            RGBColor(255, 215, 0) // 金色
        } else if name == "服务器已就绪" {
            RGBColor(50, 205, 50) // 酸橙绿
        } else if name == "发送目标" {
            RGBColor(255, 165, 0) // 橙色
        } else if name == "执行中" {
            RGBColor(135, 206, 250) // 淡天蓝 - 执行中
        } else if name.starts_with("[RESULT]") {
            RGBColor(147, 112, 219) // 中紫色
        } else if name.starts_with("执行完成") {
            RGBColor(34, 139, 34) // 森林绿
        }
        // 导航/master_control 旧格式
        else if name == "设置导航目标" {
            RGBColor(70, 130, 180) // 钢蓝
        } else if name == "发送导航目标"
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
        } else if name.contains("完成") {
            RGBColor(34, 139, 34) // 森林绿
        } else if name.contains("开始") {
            RGBColor(100, 149, 237) // 矢车菊蓝
        }
        // 默认颜色 - 使用交替颜色增强分隔
        else {
            // 根据索引使用不同的颜色
            let colors = [
                RGBColor(144, 238, 144), // 淡绿
                RGBColor(255, 218, 185), // 桃色
                RGBColor(173, 216, 230), // 淡蓝
                RGBColor(255, 182, 193), // 淡粉
                RGBColor(221, 160, 221), // 淡紫
                RGBColor(255, 255, 150), // 淡黄
            ];
            colors[index % colors.len()]
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
            let sub_color = get_sub_step_color(&sub_steps[i].name, i);

            // 绘制子步骤方块（只在主方块范围内）
            chart.draw_series(std::iter::once(Rectangle::new(
                [
                    (sub_start_clamped, sub_y_start),
                    (sub_end_clamped, sub_y_start + sub_y_height),
                ],
                sub_color.filled(),
            )))?;

            // 绘制边框分隔线（黑色边框增强分隔效果）
            chart.draw_series(std::iter::once(Rectangle::new(
                [
                    (sub_start_clamped, sub_y_start),
                    (sub_end_clamped, sub_y_start + sub_y_height),
                ],
                BLACK.stroke_width(2),
            )))?;

            // 计算子步骤在图表中的像素宽度比例
            // 如果子步骤持续时间足够长（超过总时间范围的0.5%），绘制文字标签
            let width_ratio = sub_duration / total_time_range;
            if width_ratio > 0.005 {
                // 提取简短的模块名称和时间
                let (module_name, time_str) = extract_short_module_name(&sub_steps[i].name);

                // 选择字体大小（根据宽度比例，4x 分辨率）
                let font_size = if width_ratio > 0.05 {
                    48
                } else if width_ratio > 0.02 {
                    40
                } else {
                    32
                };

                // 在子步骤方块中间绘制标签（分两行）
                let text_x = sub_start_clamped + sub_duration / 2.0;
                let text_y_upper = sub_y_start + sub_y_height * 0.35; // 上行：模块名
                let text_y_lower = sub_y_start + sub_y_height * 0.65; // 下行：时间

                // 绘制模块名称（上行）
                chart.draw_series(std::iter::once(Text::new(
                    module_name,
                    (text_x, text_y_upper),
                    font_loader
                        .font_desc(font_size)
                        .color(&BLACK)
                        .pos(Pos::new(HPos::Center, VPos::Center))
                        .transform(FontTransform::None),
                )))?;

                // 绘制时间（下行，如果有）
                if !time_str.is_empty() {
                    chart.draw_series(std::iter::once(Text::new(
                        time_str,
                        (text_x, text_y_lower),
                        font_loader
                            .font_desc(font_size - 8)
                            .color(&BLACK)
                            .pos(Pos::new(HPos::Center, VPos::Center))
                            .transform(FontTransform::None),
                    )))?;
                }
            }
        }
    }

    Ok(())
}

/// 从子步骤名称中提取简短的模块名称和时间（分开返回）
/// 例如: "ExecuteGripperMotionAction (0.51s)" -> ("夹爪", "0.51s")
fn extract_short_module_name(full_name: &str) -> (String, String) {
    // 提取耗时信息（如果有）
    let (name_part, time_part) = if let Some(paren_pos) = full_name.rfind(" (") {
        // 提取括号内的时间，去掉括号
        let time = &full_name[paren_pos + 2..full_name.len() - 1]; // 去掉 " (" 和 ")"
        let name = &full_name[..paren_pos];
        (name, time)
    } else {
        (full_name, "")
    };

    // 模块名称简化映射
    let short_name = if name_part.starts_with("ExecuteGripperMotion") {
        "夹爪"
    } else if name_part.starts_with("GetTaskType") {
        "获取类型"
    } else if name_part.starts_with("GetDetObjPose") {
        "检测位姿"
    } else if name_part.contains("ModifyArmObstacle") {
        "修改障碍"
    } else if name_part.contains("ArmTransitionPoint") {
        "过渡点"
    } else if name_part.contains("ExecuteDoubleArmMove") {
        "双臂运动"
    } else if name_part.contains("GetGoalPose") {
        "目标位姿"
    } else if name_part.starts_with("GetArmPose") {
        "手臂位姿"
    } else if name_part.starts_with("CalcPutDownPose") {
        "计算放置"
    } else if name_part.starts_with("CalcArmPose") {
        "计算位姿"
    } else if name_part.starts_with("CalcGripperPos") {
        "夹爪位置"
    } else if name_part.starts_with("CheckSafe") {
        "安全检查"
    } else if name_part.starts_with("ArmControl") {
        "手臂控制"
    } else if name_part.starts_with("ExecutePose") {
        "执行位姿"
    } else if name_part.starts_with("WaitForTrigger") {
        "等待触发"
    } else if name_part.contains("GetReadyPose") {
        "准备位姿"
    } else if name_part.starts_with("SendGoal") {
        "发送目标"
    // ROS2ActionAdapter 相关阶段
    } else if name_part == "开始执行" {
        "开始"
    } else if name_part.starts_with("等待服务器") {
        "等待服务器"
    } else if name_part == "服务器已就绪" {
        "就绪"
    } else if name_part == "发送目标" {
        "发送"
    } else if name_part == "执行中" {
        "执行中"
    } else if name_part.starts_with("[RESULT]") {
        "结果"
    } else if name_part.starts_with("执行完成") {
        "完成"
    // BehaviorTree 节点相关
    } else if name_part.contains("节点开始") {
        "开始"
    } else if name_part.contains("节点结束") {
        "结束"
    } else if name_part.ends_with(" 开始") {
        // "XXXAction 开始" -> "XXX开始"
        let module = name_part.trim_end_matches(" 开始");
        let short = if module.contains("GetReadyPose") {
            "准备"
        } else if module.contains("ModifyArmObstacle") {
            "障碍"
        } else if module.contains("GetGoalPose") {
            "目标"
        } else if module.contains("ArmTransitionPoint") {
            "过渡"
        } else if module.contains("ExecuteDoubleArmMove") {
            "运动"
        } else {
            module
        };
        return (format!("{}▶", short), time_part.to_string());
    } else if name_part.ends_with(" 完成") {
        // "XXXAction 完成" -> "XXX完成"
        let module = name_part.trim_end_matches(" 完成");
        let short = if module.contains("GetReadyPose") {
            "准备"
        } else if module.contains("ModifyArmObstacle") {
            "障碍"
        } else if module.contains("GetGoalPose") {
            "目标"
        } else if module.contains("ArmTransitionPoint") {
            "过渡"
        } else if module.contains("ExecuteDoubleArmMove") {
            "运动"
        } else {
            module
        };
        return (format!("{}✓", short), time_part.to_string());
    } else {
        // 如果名称太长，截取前6个字符
        if name_part.chars().count() > 8 {
            &name_part[..name_part.char_indices().nth(8).map(|(i, _)| i).unwrap_or(name_part.len())]
        } else {
            name_part
        }
    };

    (short_name.to_string(), time_part.to_string())
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
        font_loader
            .font_desc(56) // 4x 分辨率
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
    // 手臂动作使用特殊格式: 手臂:{action_code}-{duration}s
    let label_text = if step_type == "arm" {
        format!("{}-{:.1}s", detail_info, duration)
    } else if duration > 20.0 {
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
                "preplan" => "预打舵",
                "head" => "头部",
                "waist" => "腰部",
                _ => "动作",
            },
            duration
        )
    } else {
        format!("{:.1}s", duration)
    };

    // 选择字体大小（4x 分辨率）
    let font_size = if duration > 15.0 {
        88
    } else if duration > 8.0 {
        80
    } else if duration > 2.0 {
        72
    } else {
        64
    };

    // 绘制主标签
    if duration > 0.5 {
        chart.draw_series(std::iter::once(Text::new(
            label_text,
            (text_x, text_y),
            font_loader
                .font_desc(font_size)
                .color(&BLACK)
                .pos(Pos::new(HPos::Center, VPos::Top))
                .transform(FontTransform::None),
        )))?;
    }

    Ok(())
}
