//! 流程检测模块
//!
//! 本模块负责从日志中检测导航流程和各类动作操作

use anyhow::Result;
use regex::Regex;

use crate::models::{ActionOperation, LogLine, NavigationFlow, Round, SubStep};
use crate::round_detector::ts_to_round_id;

/// 检测日志中的导航流程和动作操作
///
/// 从日志行中提取所有导航流程，包括：
/// - 导航动作（NavAction/NavAction2）
/// - 机械臂动作（DoubleArmAction）
/// - 头部控制动作（HeadControlAction/HeadControlAction2）
/// - 腰部控制动作（WaistAction/WaistAction2）
///
/// # 参数
/// * `lines` - 日志行切片
/// * `rounds` - 轮次切片
///
/// # 返回
/// 包含所有检测到的导航流程的向量
pub fn detect_flows(lines: &[LogLine], rounds: &[Round]) -> Result<Vec<NavigationFlow>> {
    // 导航相关正则 (支持 NavAction 和 NavAction2)
    let nav_start_regex = Regex::new(r"\[导航\]:\s*NavAction2?\[NavAction2?\]\s*-\s*开始执行")?;
    let nav_target_regex = Regex::new(r"设置导航目标:\s*pos\(([^)]+)\),\s*ori\(([^)]+)\)")?;
    let nav_send_regex = Regex::new(r"\[导航\]:\s*发送导航目标")?;
    let nav_response_regex =
        Regex::new(r"\[导航\]:\s*\[RESPONSE CALLBACK\]\s*-\s*目标已被服务端接受")?;
    let nav_result_regex = Regex::new(r"\[导航\]:\s*\[RESULT CALLBACK\]")?;
    let nav_end_regex =
        Regex::new(r"\[导航\]:\s*NavAction2?\[NavAction2?\]\s*-\s*执行完成，结果:")?;

    // 导航完成的正则（匹配日志中的实际格式）
    let nav_complete_regex =
        Regex::new(r"\[导航\]:\s*\[RESULT CALLBACK\]\s*-\s*导航完成，结果代码:")?;

    // 额外添加对 NavAction[NavAction] 格式的支持
    let nav_start_alt_regex = Regex::new(r"\[导航\]:\s*NavAction\[NavAction\]\s*-\s*开始执行")?;

    // 机械臂相关正则
    let arm_start_regex = Regex::new(r"\[机械臂\]:\s*DoubleArmAction\[([^\]]+)\]\s*-\s*开始执行")?;
    let arm_setgoal_regex =
        Regex::new(r"\[机械臂\]:\s*DoubleArmAction\s+setGoal\s+action_type_code:\s*(\d+)")?;
    let arm_send_regex = Regex::new(r"\[机械臂\]:\s*发送机械臂控制目标")?;
    let arm_response_regex =
        Regex::new(r"\[机械臂\]:\s*\[RESPONSE CALLBACK\]\s*-\s*目标已被服务端接受")?;
    let arm_result_regex =
        Regex::new(r"\[机械臂\]:\s*\[RESULT CALLBACK\]\s*-\s*机械臂动作完成，状态:\s*(\d+)")?;
    let arm_complete_regex =
        Regex::new(r"\[机械臂\]:\s*DoubleArmAction\[([^\]]+)\]\s*-\s*执行完成，结果:")?;

    // 头部控制相关正则 (支持 HeadControlAction 和 HeadControlAction2)
    let head_start_regex =
        Regex::new(r"\[头部控制\]:\s*HeadControlAction2?\[head_control\]\s*-\s*开始执行")?;
    let head_send_regex = Regex::new(r"\[头部控制\]:\s*发送头部控制目标")?;
    let head_response_regex =
        Regex::new(r"\[头部控制\]:\s*\[RESPONSE CALLBACK\]\s*-\s*目标已被服务端接受")?;
    let head_result_regex = Regex::new(r"\[头部控制\]:\s*\[RESULT CALLBACK\]\s*-\s*头部动作完成")?;
    let head_end_regex =
        Regex::new(r"\[头部控制\]:\s*HeadControlAction2?\[head_control\]\s*-\s*执行完成")?;

    // 腰部控制相关正则 (支持 WaistAction 和 WaistAction2)
    let waist_start_regex =
        Regex::new(r"\[腰部\]:\s*WaistAction2?\[WaistAction2?\]\s*-\s*开始执行")?;
    let waist_send_regex = Regex::new(r"\[腰部\]:\s*发送腰部控制目标")?;
    let waist_response_regex =
        Regex::new(r"\[腰部\]:\s*\[RESPONSE CALLBACK\]\s*-\s*目标已被服务端接受")?;
    let waist_result_regex = Regex::new(r"\[腰部\]:\s*\[RESULT CALLBACK\]\s*-\s*腰部动作完成")?;
    let waist_end_regex =
        Regex::new(r"\[腰部\]:\s*WaistAction2?\[WaistAction2?\]\s*-\s*执行完成，结果:")?;

    // 预打舵相关正则
    let preplan_start_regex =
        Regex::new(r"\[预打舵\]:\s*PrePlanNavigation\[([^\]]+)\]\s*-\s*开始执行")?;
    let preplan_target_regex = Regex::new(
        r"\[预打舵\]:\s*设置预打舵目标:\s*pos\(([^)]+)\),\s*ori\(([^)]+)\)\s+action:\s*(\d+)",
    )?;
    let preplan_response_regex =
        Regex::new(r"\[预打舵\]:\s*PrePlanNavigation 响应:\s*error_code=(\d+)")?;

    let mut flows = Vec::new();
    let mut current_flow: Option<NavigationFlow> = None;

    for line in lines {
        // 导航开始
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

        // 导航目标设置
        if let Some(caps) = nav_target_regex.captures(&line.line) {
            if let Some(ref mut flow) = current_flow
                && flow.nav_target_pos.is_none()
            {
                flow.nav_target_pos = Some(caps[1].replace(' ', ""));
                flow.nav_target_ori = Some(caps[2].replace(' ', ""));
                flow.nav_sub_steps.push(SubStep {
                    name: "设置导航目标".to_string(),
                    timestamp: line.timestamp,
                });
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

        // 导航完成
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

        // 导航结束（旧格式兼容）
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

        // === 机械臂动作处理 ===
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

            if let Some(ref mut flow) = current_flow {
                flow.operations.push(arm_action);
            } else if let Some(flow) = flows.last_mut() {
                flow.operations.push(arm_action);
            }
            continue;
        }

        // 机械臂 - 发送目标
        if arm_send_regex.is_match(&line.line) {
            add_arm_substep(&mut current_flow, &mut flows, "发送目标", line.timestamp);
            continue;
        }

        // 机械臂 - Response callback
        if arm_response_regex.is_match(&line.line) {
            add_arm_substep(&mut current_flow, &mut flows, "服务端接受", line.timestamp);
            continue;
        }

        // 机械臂动作完成 - [RESULT CALLBACK]
        if let Some(caps) = arm_result_regex.captures(&line.line) {
            let status = caps[1].trim();
            finish_arm_action(&mut current_flow, &mut flows, status, line.timestamp);
            continue;
        }

        // 机械臂动作完成 - 执行完成
        if arm_complete_regex.is_match(&line.line) {
            add_arm_complete_substep(&mut current_flow, &mut flows, line.timestamp);
            continue;
        }

        // === 头部控制处理 ===
        if head_start_regex.is_match(&line.line) {
            let head_action = create_action("head", "头部控制", line.timestamp);
            add_action_to_flow(&mut current_flow, &mut flows, head_action);
            continue;
        }

        if head_send_regex.is_match(&line.line) {
            add_action_substep(
                &mut current_flow,
                &mut flows,
                "head",
                "发送目标",
                line.timestamp,
            );
            continue;
        }

        if head_response_regex.is_match(&line.line) {
            add_action_substep(
                &mut current_flow,
                &mut flows,
                "head",
                "服务端接受",
                line.timestamp,
            );
            continue;
        }

        if head_result_regex.is_match(&line.line) {
            add_action_substep(
                &mut current_flow,
                &mut flows,
                "head",
                "动作完成",
                line.timestamp,
            );
            continue;
        }

        if head_end_regex.is_match(&line.line) {
            finish_action(&mut current_flow, &mut flows, "head", line.timestamp);
            continue;
        }

        // === 腰部控制处理 ===
        if waist_start_regex.is_match(&line.line) {
            let waist_action = create_action("waist", "腰部控制", line.timestamp);
            add_action_to_flow(&mut current_flow, &mut flows, waist_action);
            continue;
        }

        if waist_send_regex.is_match(&line.line) {
            add_action_substep(
                &mut current_flow,
                &mut flows,
                "waist",
                "发送目标",
                line.timestamp,
            );
            continue;
        }

        if waist_response_regex.is_match(&line.line) {
            add_action_substep(
                &mut current_flow,
                &mut flows,
                "waist",
                "服务端接受",
                line.timestamp,
            );
            continue;
        }

        if waist_result_regex.is_match(&line.line) {
            add_action_substep(
                &mut current_flow,
                &mut flows,
                "waist",
                "动作完成",
                line.timestamp,
            );
            continue;
        }

        if waist_end_regex.is_match(&line.line) {
            finish_action(&mut current_flow, &mut flows, "waist", line.timestamp);
            continue;
        }

        // === 预打舵处理 ===
        if let Some(caps) = preplan_start_regex.captures(&line.line) {
            let action_label = caps[1].to_string();
            let preplan_action = ActionOperation {
                action_type: "preplan".to_string(),
                action_code: None,
                label: format!("预打舵[{}]", action_label),
                start_ts: Some(line.timestamp),
                end_ts: None,
                status: "pending".to_string(),
                sub_steps: vec![SubStep {
                    name: "开始执行".to_string(),
                    timestamp: line.timestamp,
                }],
            };
            add_action_to_flow(&mut current_flow, &mut flows, preplan_action);
            continue;
        }

        if let Some(caps) = preplan_target_regex.captures(&line.line) {
            let pos = caps[1].replace(' ', "");
            let _ori = caps[2].replace(' ', "");
            let action_code: u32 = caps[3].parse().unwrap_or(0);

            // 更新最近的预打舵动作的 action_code 和添加子步骤
            if let Some(flow) = current_flow.as_mut() {
                for op in flow.operations.iter_mut().rev() {
                    if op.action_type == "preplan" && op.end_ts.is_none() {
                        op.action_code = Some(action_code);
                        op.sub_steps.push(SubStep {
                            name: format!("设置目标→{}", pos),
                            timestamp: line.timestamp,
                        });
                        break;
                    }
                }
            } else if let Some(flow) = flows.last_mut() {
                for op in flow.operations.iter_mut().rev() {
                    if op.action_type == "preplan" && op.end_ts.is_none() {
                        op.action_code = Some(action_code);
                        op.sub_steps.push(SubStep {
                            name: format!("设置目标→{}", pos),
                            timestamp: line.timestamp,
                        });
                        break;
                    }
                }
            }
            continue;
        }

        if let Some(caps) = preplan_response_regex.captures(&line.line) {
            let error_code = caps[1].trim();
            // 预打舵响应即为完成（它是异步请求，不等待结果）
            if let Some(flow) = current_flow.as_mut() {
                for op in flow.operations.iter_mut().rev() {
                    if op.action_type == "preplan" && op.end_ts.is_none() {
                        op.sub_steps.push(SubStep {
                            name: format!("响应(code={})", error_code),
                            timestamp: line.timestamp,
                        });
                        op.end_ts = Some(line.timestamp);
                        op.status = if error_code == "10" {
                            "ok".to_string()
                        } else {
                            format!("error_{}", error_code)
                        };
                        break;
                    }
                }
            } else if let Some(flow) = flows.last_mut() {
                for op in flow.operations.iter_mut().rev() {
                    if op.action_type == "preplan" && op.end_ts.is_none() {
                        op.sub_steps.push(SubStep {
                            name: format!("响应(code={})", error_code),
                            timestamp: line.timestamp,
                        });
                        op.end_ts = Some(line.timestamp);
                        op.status = if error_code == "10" {
                            "ok".to_string()
                        } else {
                            format!("error_{}", error_code)
                        };
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

// === 辅助函数 ===

/// 创建一个动作操作
fn create_action(action_type: &str, label: &str, timestamp: f64) -> ActionOperation {
    ActionOperation {
        action_type: action_type.to_string(),
        action_code: None,
        label: label.to_string(),
        start_ts: Some(timestamp),
        end_ts: None,
        status: "pending".to_string(),
        sub_steps: vec![SubStep {
            name: "开始执行".to_string(),
            timestamp,
        }],
    }
}

/// 将动作添加到流程中
fn add_action_to_flow(
    current_flow: &mut Option<NavigationFlow>,
    flows: &mut Vec<NavigationFlow>,
    action: ActionOperation,
) {
    if let Some(flow) = current_flow {
        flow.operations.push(action);
    } else if let Some(flow) = flows.last_mut() {
        flow.operations.push(action);
    }
}

/// 为机械臂动作添加子步骤
fn add_arm_substep(
    current_flow: &mut Option<NavigationFlow>,
    flows: &mut Vec<NavigationFlow>,
    step_name: &str,
    timestamp: f64,
) {
    if let Some(flow) = current_flow {
        for op in flow.operations.iter_mut().rev() {
            if op.action_type == "arm" && op.end_ts.is_none() {
                op.sub_steps.push(SubStep {
                    name: step_name.to_string(),
                    timestamp,
                });
                break;
            }
        }
    } else if let Some(flow) = flows.last_mut() {
        for op in flow.operations.iter_mut().rev() {
            if op.action_type == "arm" && op.end_ts.is_none() {
                op.sub_steps.push(SubStep {
                    name: step_name.to_string(),
                    timestamp,
                });
                break;
            }
        }
    }
}

/// 完成机械臂动作
fn finish_arm_action(
    current_flow: &mut Option<NavigationFlow>,
    flows: &mut Vec<NavigationFlow>,
    status: &str,
    timestamp: f64,
) {
    let mut found = false;
    if let Some(flow) = current_flow {
        for op in flow.operations.iter_mut().rev() {
            if op.action_type == "arm" && op.end_ts.is_none() {
                op.sub_steps.push(SubStep {
                    name: format!("动作完成(状态:{})", status),
                    timestamp,
                });
                op.end_ts = Some(timestamp);
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

    if !found {
        for flow in flows.iter_mut().rev() {
            for op in flow.operations.iter_mut().rev() {
                if op.action_type == "arm" && op.end_ts.is_none() {
                    op.sub_steps.push(SubStep {
                        name: format!("动作完成(状态:{})", status),
                        timestamp,
                    });
                    op.end_ts = Some(timestamp);
                    op.status = if status == "0" {
                        "ok".to_string()
                    } else {
                        format!("status_{}", status)
                    };
                    break;
                }
            }
        }
    }
}

/// 为机械臂动作添加完成子步骤
fn add_arm_complete_substep(
    current_flow: &mut Option<NavigationFlow>,
    flows: &mut Vec<NavigationFlow>,
    timestamp: f64,
) {
    let mut found = false;
    if let Some(flow) = current_flow {
        for op in flow.operations.iter_mut().rev() {
            if op.action_type == "arm" {
                op.sub_steps.push(SubStep {
                    name: "执行完成".to_string(),
                    timestamp,
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
                        timestamp,
                    });
                    break;
                }
            }
        }
    }
}

/// 为指定类型的动作添加子步骤
fn add_action_substep(
    current_flow: &mut Option<NavigationFlow>,
    flows: &mut Vec<NavigationFlow>,
    action_type: &str,
    step_name: &str,
    timestamp: f64,
) {
    if let Some(flow) = current_flow {
        for op in flow.operations.iter_mut().rev() {
            if op.action_type == action_type && op.end_ts.is_none() {
                op.sub_steps.push(SubStep {
                    name: step_name.to_string(),
                    timestamp,
                });
                break;
            }
        }
    } else if let Some(flow) = flows.last_mut() {
        for op in flow.operations.iter_mut().rev() {
            if op.action_type == action_type && op.end_ts.is_none() {
                op.sub_steps.push(SubStep {
                    name: step_name.to_string(),
                    timestamp,
                });
                break;
            }
        }
    }
}

/// 完成指定类型的动作
fn finish_action(
    current_flow: &mut Option<NavigationFlow>,
    flows: &mut Vec<NavigationFlow>,
    action_type: &str,
    timestamp: f64,
) {
    if let Some(flow) = current_flow {
        for op in flow.operations.iter_mut().rev() {
            if op.action_type == action_type && op.end_ts.is_none() {
                op.sub_steps.push(SubStep {
                    name: "执行完成".to_string(),
                    timestamp,
                });
                op.end_ts = Some(timestamp);
                op.status = "ok".to_string();
                break;
            }
        }
    } else if let Some(flow) = flows.last_mut() {
        for op in flow.operations.iter_mut().rev() {
            if op.action_type == action_type && op.end_ts.is_none() {
                op.sub_steps.push(SubStep {
                    name: "执行完成".to_string(),
                    timestamp,
                });
                op.end_ts = Some(timestamp);
                op.status = "ok".to_string();
                break;
            }
        }
    }
}
