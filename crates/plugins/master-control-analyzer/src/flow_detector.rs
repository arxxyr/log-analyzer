//! 流程检测模块
//!
//! 本模块负责从日志中检测导航流程和各类动作操作
//!
//! 支持的日志格式：
//!
//! 1. BehaviorTree 节点格式：
//!    - 开始：`========== BehaviorTree 节点开始 ==========` + `节点ID: xxx`
//!    - 结束：`========== BehaviorTree 节点结束 ==========` + `结果: SUCCESS`
//!    - 中间动作：`[ModuleName] node=xxx phase=start/end ...`
//!
//! 2. ROS2ActionAdapter 格式：
//!    - 开始：`ROS2ActionAdapter[<type>] - 开始执行`
//!    - 结束：`ROS2ActionAdapter[<type>] - 执行完成，结果: 成功`
//!
//! 3. PrePlanNavigationNode 格式（预打舵）：
//!    - 开始：`PrePlanNavigationNode[xxx] - 开始执行`
//!    - 目标：`PrePlanNavigationNode[xxx] - 设置预规划目标:`
//!    - 结束：`PrePlanNavigationNode[xxx] - 执行成功`
//!
//! 支持的动作类型：
//! - navigation: 导航
//! - double_arm/arm: 双臂控制
//! - waist_control: 腰部控制
//! - head_control: 头部控制
//! - preplan: 预打舵
//! - behavior_tree: BehaviorTree 节点（包含多个子动作）

use std::collections::HashMap;

use anyhow::Result;
use regex::Regex;

use crate::models::{ActionOperation, LogLine, NavigationFlow, PauseEvent, Round, SubStep};
use crate::round_detector::ts_to_round_id;

/// 为指定类型的未完成 adapter 动作添加子步骤
fn add_substep_to_adapter(
    adapters: &mut HashMap<String, ActionOperation>,
    adapter_type: &str,
    step_name: String,
    timestamp: f64,
) {
    if let Some((_, action)) = adapters
        .iter_mut()
        .filter(|(k, a)| k.starts_with(adapter_type) && a.end_ts.is_none())
        .max_by(|(_, a1), (_, a2)| a1.start_ts.partial_cmp(&a2.start_ts).unwrap())
    {
        action.sub_steps.push(SubStep {
            name: step_name,
            timestamp,
        });
    }
}

/// 更新预打舵动作（在当前流程或已完成流程中查找）
fn update_preplan_action<F>(
    current_flow: &mut Option<NavigationFlow>,
    flows: &mut [NavigationFlow],
    mut updater: F,
) where
    F: FnMut(&mut ActionOperation),
{
    // 优先在当前流程中查找
    let flow_to_update = current_flow.as_mut().or_else(|| flows.last_mut());

    if let Some(flow) = flow_to_update {
        for op in flow.operations.iter_mut().rev() {
            if op.action_type == "preplan" && op.end_ts.is_none() {
                updater(op);
                break;
            }
        }
    }
}

/// 在当前 BT 节点上下文中，更新最近的指定类型子动作
fn update_bt_subaction<F>(bt_node: &mut Option<BtNodeContext>, action_type: &str, mut updater: F)
where
    F: FnMut(&mut ActionOperation),
{
    if let Some(ctx) = bt_node {
        for action in ctx.sub_actions.iter_mut().rev() {
            if action.action_type == action_type && action.end_ts.is_none() {
                updater(action);
                break;
            }
        }
    }
}

/// BehaviorTree 节点上下文
#[derive(Debug)]
struct BtNodeContext {
    node_id: String,
    action_code: Option<u32>,
    start_ts: f64,
    sub_actions: Vec<ActionOperation>,
}

/// 创建新的动作操作
fn new_action(
    action_type: &str,
    action_code: Option<u32>,
    label: String,
    start_ts: f64,
    initial_step_name: String,
) -> ActionOperation {
    ActionOperation {
        action_type: action_type.to_string(),
        action_code,
        label,
        start_ts: Some(start_ts),
        end_ts: None,
        status: "pending".to_string(),
        sub_steps: vec![SubStep {
            name: initial_step_name,
            timestamp: start_ts,
        }],
        pause_events: Vec::new(),
    }
}

/// 检测日志中的导航流程和动作操作
///
/// 支持的日志格式：
/// 1. ROS2ActionAdapter 格式：`ROS2ActionAdapter[type] - 开始执行/执行完成`
/// 2. BehaviorTree 节点格式
/// 3. PrePlanNavigationNode 格式（预打舵）
///
/// # 参数
/// * `lines` - 日志行切片
/// * `rounds` - 轮次切片
///
/// # 返回
/// 包含所有检测到的导航流程的向量
pub fn detect_flows(lines: &[LogLine], rounds: &[Round]) -> Result<Vec<NavigationFlow>> {
    // ============================================================
    // 新格式正则（ROS2ActionAdapter）
    // ============================================================
    // ROS2ActionAdapter[type] - 开始执行
    let adapter_start_regex = Regex::new(r"ROS2ActionAdapter\[(\w+)\]\s*-\s*开始执行")?;
    // ROS2ActionAdapter[type] - 等待服务器 'xxx'...
    let adapter_wait_regex =
        Regex::new(r"ROS2ActionAdapter\[(\w+)\]\s*-\s*等待服务器\s*'([^']+)'")?;
    // ROS2ActionAdapter[type] - 服务器已就绪
    let adapter_ready_regex = Regex::new(r"ROS2ActionAdapter\[(\w+)\]\s*-\s*服务器已就绪")?;
    // ROS2ActionAdapter[type] - 发送目标
    let adapter_send_regex = Regex::new(r"ROS2ActionAdapter\[(\w+)\]\s*-\s*发送目标")?;
    // ROS2ActionAdapter[type] - 执行完成，结果: (成功|失败)
    let adapter_end_regex =
        Regex::new(r"ROS2ActionAdapter\[(\w+)\]\s*-\s*执行完成，结果:\s*(\S+)")?;
    // ROS2ActionAdapter[xxx] - [RESPONSE] 目标已被接受
    let adapter_response_regex =
        Regex::new(r"ROS2ActionAdapter\[(\w+)\]\s*-\s*\[RESPONSE\]\s*目标已被接受")?;
    // ROS2ActionAdapter[xxx] - [RESULT] 完成，成功: 是, 消息: xxx
    let adapter_result_regex =
        Regex::new(r"ROS2ActionAdapter\[(\w+)\]\s*-\s*\[RESULT\]\s*完成，成功:\s*(\S+)")?;
    // ROS2ActionAdapter[xxx] - 暂停（动作被暂停）
    let adapter_pause_regex = Regex::new(r"ROS2ActionAdapter\[(\w+)\]\s*-\s*暂停")?;
    // ROS2ActionAdapter[xxx] - 恢复（动作恢复执行）
    let adapter_resume_regex = Regex::new(r"ROS2ActionAdapter\[(\w+)\]\s*-\s*恢复")?;

    // 预打舵相关正则（PrePlanNavigationNode 格式）
    // PrePlanNavigationNode[xxx] - 开始执行
    let preplan_start_regex = Regex::new(r"PrePlanNavigationNode\[([^\]]+)\]\s*-\s*开始执行")?;
    // PrePlanNavigationNode[xxx] - 设置预规划目标:
    //   位置: (x, y, z)
    //   action=N, first_dir=M, rotate_mode=K
    let preplan_pos_regex = Regex::new(r"PrePlanNavigationNode\[([^\]]+)\]\s*-\s*设置预规划目标:")?;
    let preplan_position_regex = Regex::new(r"位置:\s*\(([^)]+)\)")?;
    let preplan_action_regex = Regex::new(r"action=(\d+)")?;
    // PrePlanNavigationNode[xxx] - 服务响应: error_code=N
    let preplan_response_regex =
        Regex::new(r"PrePlanNavigationNode\[([^\]]+)\]\s*-\s*服务响应:\s*error_code=(\d+)")?;
    // PrePlanNavigationNode[xxx] - 执行成功
    let preplan_end_regex = Regex::new(r"PrePlanNavigationNode\[([^\]]+)\]\s*-\s*执行成功")?;

    // 新格式: action_type_code 提取
    let action_code_regex = Regex::new(r"action_type_code=(\d+)")?;

    // ============================================================
    // BehaviorTree 节点格式正则（最新格式）
    // ============================================================
    // BehaviorTreeNode xxx: 映射 X 个输入参数到黑板
    let bt_param_map_regex =
        Regex::new(r"BehaviorTreeNode\s+(\w+):\s*映射\s*\d+\s*个输入参数到黑板")?;
    // @gas_test_action_code = "2015" 提取 action_code
    let bt_action_code_regex = Regex::new(r#"@(\w+_action_code)\s*=\s*"(\d+)""#)?;
    // ========== BehaviorTree 节点开始 ==========
    let bt_start_marker_regex = Regex::new(r"=+\s*BehaviorTree\s*节点开始\s*=+")?;
    // 节点ID: normal_arm_leak_swap
    let bt_node_id_regex = Regex::new(r"节点ID:\s*(\S+)")?;
    // gas_test: gas_test start（预留，暂未使用）
    // 注：Rust regex 不支持反向引用，使用简化模式
    let _bt_test_start_regex = Regex::new(r"\w+:\s*\w+\s+start")?;
    // gas_test: gas_test finished（预留，暂未使用）
    let _bt_test_end_regex = Regex::new(r"\w+:\s*\w+\s+finished")?;
    // ========== BehaviorTree 节点结束 ==========
    let bt_end_marker_regex = Regex::new(r"=+\s*BehaviorTree\s*节点结束\s*=+")?;
    // 结果: SUCCESS / FAILURE
    let bt_result_regex = Regex::new(r"结果:\s*(\w+)")?;

    // BehaviorTree 中间动作模块正则
    // [ModuleName] node=ModuleName phase=start ...
    let bt_module_start_regex = Regex::new(r"\[(\w+)\]\s+node=\w+\s+phase=start")?;
    // [ModuleName] node=ModuleName phase=end status=success cost_ms=XXX
    let bt_module_end_regex =
        Regex::new(r"\[(\w+)\]\s+node=\w+\s+phase=end\s+status=(\w+)\s+cost_ms=(\d+)")?;
    // [ExecuteDoubleArmMoveAction] result received code=0 message=...
    let bt_arm_result_regex =
        Regex::new(r"\[ExecuteDoubleArmMoveAction\]\s+result\s+received\s+code=(\d+)")?;

    // ============================================================
    // 细化阶段检测正则（BehaviorTreeNode 内部子阶段）
    // ============================================================
    // GetReadyPoseAction phase=start/end
    let get_ready_pose_start_regex = Regex::new(r"node=GetReadyPoseAction\s+phase=start")?;
    let get_ready_pose_end_regex =
        Regex::new(r"node=GetReadyPoseAction\s+phase=end\s+status=(\w+)\s+cost_ms=(\d+)")?;
    // DetObjPose start/done
    let det_obj_pose_start_regex = Regex::new(r"DetObjPose start camera_id=(\w+).*obj_id=(\d+)")?;
    let det_obj_pose_response_regex = Regex::new(r"DetObjPose response ret=(\d+)")?;
    let det_obj_pose_done_regex = Regex::new(r"DetObjPose done goal_pose=")?;
    // gripper done
    let gripper_done_regex = Regex::new(r"gripper done status=(\w+) cost_ms=(\d+)")?;

    // ============================================================
    // 手臂动作子阶段正则（BehaviorTreeNode 内部）
    // ============================================================
    // gripper start arm=xxx open=xxx
    let gripper_start_regex = Regex::new(r"gripper start arm=(\w+) open=(\w+)")?;
    // gripper request: cmd_code=xxx arm=xxx
    let gripper_request_regex = Regex::new(r"gripper request: cmd_code=(\d+) arm=(\w+)")?;
    // ArmObstacle start cmd=xxx
    let arm_obstacle_start_regex = Regex::new(r"ArmObstacle start cmd=(\d+)")?;
    // ArmObstacle done resp_status=xxx
    let arm_obstacle_done_regex = Regex::new(r"ArmObstacle done resp_status=(\d+)")?;
    // arm_transition_point: 只匹配 "- start cmd=" 不匹配 "custom start cmd="（同一操作会输出两行）
    let arm_transition_start_regex =
        Regex::new(r"- start cmd=(\d+).*service=arm_transition_point")?;
    // arm_transition_point 完成: 匹配任意 "custom done" 格式
    // - custom done reference_state_n=... (ik=0)
    // - custom done resp_status=... (ik=1)
    let arm_transition_done_regex =
        Regex::new(r"custom done (?:resp_status=(\d+)|reference_state_n=)")?;
    // arm_move start cmd=xxx
    let arm_move_start_regex = Regex::new(r"arm_move start cmd=(\d+)")?;
    // arm_move response: success cmd=xxx / result code=xxx
    let arm_move_response_regex =
        Regex::new(r"arm_move response: (?:success cmd=(\d+)|result code=(\d+))")?;

    let mut flows = Vec::new();
    let mut current_flow: Option<NavigationFlow> = None;

    // 活跃的 ROS2ActionAdapter 动作（按类型跟踪）
    let mut active_adapters: HashMap<String, ActionOperation> = HashMap::new();

    // 活跃的 BehaviorTree 节点
    let mut active_bt_node: Option<BtNodeContext> = None;
    // 活跃的 BT 子动作（用于跟踪中间模块）
    let mut active_bt_subaction: Option<(String, f64)> = None; // (module_name, start_ts)

    // 准备阶段跟踪（合并多个连续的 GetReadyPoseAction）
    let mut ready_pose_phase: Option<(f64, u32)> = None; // (start_ts, count)

    for (line_idx, line) in lines.iter().enumerate() {
        // ============================================================
        // BehaviorTree 节点格式检测（最新格式）
        // ============================================================

        // BehaviorTreeNode 参数映射开始（预创建节点上下文）
        if let Some(caps) = bt_param_map_regex.captures(&line.line) {
            let node_name = caps[1].to_string();
            // 查找后续几行的 action_code
            let mut action_code = None;
            for next_line in lines.iter().skip(line_idx + 1).take(3) {
                if let Some(code_caps) = bt_action_code_regex.captures(&next_line.line) {
                    action_code = code_caps[2].parse().ok();
                    break;
                }
            }
            active_bt_node = Some(BtNodeContext {
                node_id: node_name,
                action_code,
                start_ts: line.timestamp,
                sub_actions: Vec::new(),
            });
            continue;
        }

        // BehaviorTree 节点开始标记
        if bt_start_marker_regex.is_match(&line.line) {
            // 查找下一行的节点ID
            if let Some(next_line) = lines.get(line_idx + 1)
                && let Some(caps) = bt_node_id_regex.captures(&next_line.line)
            {
                let node_id = caps[1].to_string();
                // 更新已有的节点上下文或创建新的
                if let Some(ref mut ctx) = active_bt_node {
                    if ctx.node_id != node_id {
                        ctx.node_id = node_id;
                    }
                    ctx.start_ts = line.timestamp;
                } else {
                    active_bt_node = Some(BtNodeContext {
                        node_id,
                        action_code: None,
                        start_ts: line.timestamp,
                        sub_actions: Vec::new(),
                    });
                }
            }
            continue;
        }

        // BehaviorTree 中间模块开始
        if let Some(caps) = bt_module_start_regex.captures(&line.line) {
            let module_name = caps[1].to_string();
            active_bt_subaction = Some((module_name, line.timestamp));
            continue;
        }

        // BehaviorTree 中间模块结束
        if let Some(caps) = bt_module_end_regex.captures(&line.line) {
            let module_name = caps[1].to_string();
            let status = caps[2].to_string();
            let cost_ms: u32 = caps[3].parse().unwrap_or(0);

            // 如果有活跃的 BT 节点，添加子动作
            if let Some(ref mut ctx) = active_bt_node {
                let start_ts = active_bt_subaction
                    .as_ref()
                    .filter(|(name, _)| *name == module_name)
                    .map(|(_, ts)| *ts)
                    .unwrap_or(line.timestamp - (cost_ms as f64 / 1000.0));

                let sub_action = ActionOperation {
                    action_type: "bt_module".to_string(),
                    action_code: None,
                    label: module_name.clone(),
                    start_ts: Some(start_ts),
                    end_ts: Some(line.timestamp),
                    status: status.clone(),
                    sub_steps: vec![
                        SubStep {
                            name: "开始".to_string(),
                            timestamp: start_ts,
                        },
                        SubStep {
                            name: format!("完成({}ms)", cost_ms),
                            timestamp: line.timestamp,
                        },
                    ],
                    pause_events: Vec::new(),
                };
                ctx.sub_actions.push(sub_action);
            }
            active_bt_subaction = None;
            continue;
        }

        // BehaviorTree 双臂结果（特殊处理）
        if let Some(caps) = bt_arm_result_regex.captures(&line.line) {
            let code = caps[1].trim();
            if let Some(ref mut ctx) = active_bt_node {
                // 为最后一个 ExecuteDoubleArmMoveAction 添加结果
                for action in ctx.sub_actions.iter_mut().rev() {
                    if action.label.contains("ExecuteDoubleArmMoveAction") {
                        action.sub_steps.push(SubStep {
                            name: format!("结果(code={})", code),
                            timestamp: line.timestamp,
                        });
                        break;
                    }
                }
            }
            continue;
        }

        // ============================================================
        // 手臂动作子阶段检测（BehaviorTreeNode 内部）
        // ============================================================

        // gripper 开始
        if let Some(caps) = gripper_start_regex.captures(&line.line) {
            let arm = caps[1].to_string();
            let open = caps[2].to_string();
            if let Some(ref mut ctx) = active_bt_node {
                ctx.sub_actions.push(new_action(
                    "gripper",
                    None,
                    format!("gripper({},{})", arm, open),
                    line.timestamp,
                    format!("gripper start arm={} open={}", arm, open),
                ));
            }
            continue;
        }

        // gripper 请求
        if let Some(caps) = gripper_request_regex.captures(&line.line) {
            let cmd = caps[1].to_string();
            let arm = caps[2].to_string();
            let ts = line.timestamp;
            update_bt_subaction(&mut active_bt_node, "gripper", |action| {
                action.sub_steps.push(SubStep {
                    name: format!("gripper request cmd={} arm={}", cmd, arm),
                    timestamp: ts,
                });
            });
            continue;
        }

        // gripper 完成
        if let Some(caps) = gripper_done_regex.captures(&line.line) {
            let status = caps[1].to_string();
            let cost_ms: u32 = caps[2].parse().unwrap_or(0);
            let ts = line.timestamp;
            update_bt_subaction(&mut active_bt_node, "gripper", |action| {
                action.sub_steps.push(SubStep {
                    name: format!("gripper done ({}ms)", cost_ms),
                    timestamp: ts,
                });
                action.end_ts = Some(ts);
                action.status = status.clone();
            });
            continue;
        }

        // ============================================================
        // 细化阶段检测（GetReadyPoseAction、DetObjPose 等）
        // ============================================================

        // GetReadyPoseAction 开始（合并连续的准备阶段）
        if get_ready_pose_start_regex.is_match(&line.line) {
            if let Some(ref mut ctx) = active_bt_node {
                // 如果还没有准备阶段，开始新的
                if ready_pose_phase.is_none() {
                    ready_pose_phase = Some((line.timestamp, 1));
                    ctx.sub_actions.push(new_action(
                        "ready_pose",
                        None,
                        "准备阶段".to_string(),
                        line.timestamp,
                        "GetReadyPose #1 开始".to_string(),
                    ));
                } else {
                    // 增加计数
                    if let Some((_, ref mut count)) = ready_pose_phase {
                        *count += 1;
                        // 为现有的准备阶段动作添加子步骤
                        for action in ctx.sub_actions.iter_mut().rev() {
                            if action.action_type == "ready_pose" && action.end_ts.is_none() {
                                action.sub_steps.push(SubStep {
                                    name: format!("GetReadyPose #{} 开始", count),
                                    timestamp: line.timestamp,
                                });
                                break;
                            }
                        }
                    }
                }
            }
            continue;
        }

        // GetReadyPoseAction 结束
        if let Some(caps) = get_ready_pose_end_regex.captures(&line.line) {
            let _status = caps[1].to_string();
            let cost_ms: u32 = caps[2].parse().unwrap_or(0);
            if let Some(ref mut ctx) = active_bt_node
                && let Some((_, count)) = ready_pose_phase
            {
                for action in ctx.sub_actions.iter_mut().rev() {
                    if action.action_type == "ready_pose" && action.end_ts.is_none() {
                        action.sub_steps.push(SubStep {
                            name: format!("GetReadyPose #{} 完成 ({}ms)", count, cost_ms),
                            timestamp: line.timestamp,
                        });
                        // 更新结束时间（每次都更新，最终为最后一个的结束时间）
                        action.end_ts = Some(line.timestamp);
                        action.status = "ok".to_string();
                        break;
                    }
                }
            }
            continue;
        }

        // DetObjPose 开始
        if let Some(caps) = det_obj_pose_start_regex.captures(&line.line) {
            // 结束准备阶段（如果有）
            ready_pose_phase = None;

            let camera_id = caps[1].to_string();
            let obj_id = caps[2].to_string();
            if let Some(ref mut ctx) = active_bt_node {
                ctx.sub_actions.push(new_action(
                    "det_obj_pose",
                    obj_id.parse().ok(),
                    format!("DetObjPose({},{})", camera_id, obj_id),
                    line.timestamp,
                    format!("DetObjPose start cam={} obj={}", camera_id, obj_id),
                ));
            }
            continue;
        }

        // DetObjPose 响应
        if let Some(caps) = det_obj_pose_response_regex.captures(&line.line) {
            let ret = caps[1].to_string();
            let ts = line.timestamp;
            update_bt_subaction(&mut active_bt_node, "det_obj_pose", |action| {
                action.sub_steps.push(SubStep {
                    name: format!("DetObjPose response ret={}", ret),
                    timestamp: ts,
                });
            });
            continue;
        }

        // DetObjPose 完成
        if det_obj_pose_done_regex.is_match(&line.line) {
            let ts = line.timestamp;
            update_bt_subaction(&mut active_bt_node, "det_obj_pose", |action| {
                action.sub_steps.push(SubStep {
                    name: "DetObjPose done".to_string(),
                    timestamp: ts,
                });
                action.end_ts = Some(ts);
                action.status = "ok".to_string();
            });
            continue;
        }

        // ArmObstacle 开始
        if let Some(caps) = arm_obstacle_start_regex.captures(&line.line) {
            // 结束准备阶段（如果有）
            ready_pose_phase = None;

            let cmd = caps[1].to_string();
            if let Some(ref mut ctx) = active_bt_node {
                ctx.sub_actions.push(new_action(
                    "obstacle",
                    cmd.parse().ok(),
                    format!("ArmObstacle({})", cmd),
                    line.timestamp,
                    format!("ArmObstacle start cmd={}", cmd),
                ));
            }
            continue;
        }

        // ArmObstacle 完成
        if let Some(caps) = arm_obstacle_done_regex.captures(&line.line) {
            let status = caps[1].to_string();
            let ts = line.timestamp;
            update_bt_subaction(&mut active_bt_node, "obstacle", |action| {
                action.sub_steps.push(SubStep {
                    name: format!("ArmObstacle done status={}", status),
                    timestamp: ts,
                });
                action.end_ts = Some(ts);
                action.status = if status == "0" {
                    "ok".to_string()
                } else {
                    format!("status_{}", status)
                };
            });
            continue;
        }

        // arm_transition_point 开始
        if let Some(caps) = arm_transition_start_regex.captures(&line.line) {
            let cmd = caps[1].to_string();
            if let Some(ref mut ctx) = active_bt_node {
                ctx.sub_actions.push(new_action(
                    "transition",
                    cmd.parse().ok(),
                    format!("transition({})", cmd),
                    line.timestamp,
                    format!("transition start cmd={}", cmd),
                ));
            }
            continue;
        }

        // arm_transition_point 完成
        if let Some(caps) = arm_transition_done_regex.captures(&line.line) {
            // caps[1] 可能为空（当匹配 reference_state_n= 时）
            let status = caps.get(1).map(|m| m.as_str()).unwrap_or("0").to_string();
            let ts = line.timestamp;
            update_bt_subaction(&mut active_bt_node, "transition", |action| {
                action.sub_steps.push(SubStep {
                    name: format!("transition done status={}", status),
                    timestamp: ts,
                });
                action.end_ts = Some(ts);
                action.status = if status == "0" {
                    "ok".to_string()
                } else {
                    format!("status_{}", status)
                };
            });
            continue;
        }

        // arm_move 开始
        if let Some(caps) = arm_move_start_regex.captures(&line.line) {
            let cmd = caps[1].to_string();
            if let Some(ref mut ctx) = active_bt_node {
                ctx.sub_actions.push(new_action(
                    "arm_move",
                    cmd.parse().ok(),
                    format!("arm_move({})", cmd),
                    line.timestamp,
                    format!("arm_move start cmd={}", cmd),
                ));
            }
            continue;
        }

        // arm_move 响应
        if arm_move_response_regex.is_match(&line.line) {
            let ts = line.timestamp;
            update_bt_subaction(&mut active_bt_node, "arm_move", |action| {
                action.sub_steps.push(SubStep {
                    name: "arm_move response".to_string(),
                    timestamp: ts,
                });
                action.end_ts = Some(ts);
                action.status = "ok".to_string();
            });
            continue;
        }

        // BehaviorTree 节点结束标记
        if bt_end_marker_regex.is_match(&line.line) {
            // 查找后续几行的节点ID和结果
            let mut result_status = "unknown".to_string();
            for next_line in lines.iter().skip(line_idx + 1).take(3) {
                if let Some(caps) = bt_result_regex.captures(&next_line.line) {
                    result_status = caps[1].to_string();
                    break;
                }
            }

            // 完成 BT 节点，将子动作作为独立操作添加到流程中
            if let Some(ctx) = active_bt_node.take() {
                // 重置准备阶段跟踪
                ready_pose_phase = None;

                let round_id = ts_to_round_id(ctx.start_ts, rounds);
                let label = if let Some(code) = ctx.action_code {
                    format!("{}({})", ctx.node_id, code)
                } else {
                    ctx.node_id.clone()
                };

                // 创建主 BT 节点动作（作为容器，用于显示整体时间范围）
                let bt_action = ActionOperation {
                    action_type: "arm".to_string(),
                    action_code: ctx.action_code,
                    label: label.clone(),
                    start_ts: Some(ctx.start_ts),
                    end_ts: Some(line.timestamp),
                    status: if result_status == "SUCCESS" {
                        "ok".to_string()
                    } else {
                        format!("failed_{}", result_status)
                    },
                    sub_steps: vec![
                        SubStep {
                            name: "节点开始".to_string(),
                            timestamp: ctx.start_ts,
                        },
                        SubStep {
                            name: format!("节点结束({})", result_status),
                            timestamp: line.timestamp,
                        },
                    ],
                    pause_events: Vec::new(),
                };

                // 收集所有要添加的操作（主动作 + 子动作）
                let mut operations_to_add = vec![bt_action];

                // 将子动作作为独立操作添加
                for sub_action in ctx.sub_actions {
                    // 只添加有有效时间范围的子动作
                    if sub_action.start_ts.is_some() {
                        operations_to_add.push(sub_action);
                    }
                }

                // 添加到流程中
                if current_flow.is_none()
                    || current_flow.as_ref().map(|f| f.round_id) != Some(round_id)
                {
                    // 保存当前流程
                    if let Some(flow) = current_flow.take() {
                        flows.push(flow);
                    }

                    // 创建新流程
                    current_flow = Some(NavigationFlow {
                        nav_start_ts: None,
                        nav_end_ts: None,
                        nav_target_pos: None,
                        nav_target_ori: None,
                        nav_status: "ok".to_string(),
                        nav_sub_steps: Vec::new(),
                        round_id,
                        operations: operations_to_add,
                    });
                } else if let Some(ref mut flow) = current_flow {
                    flow.operations.extend(operations_to_add);
                }
            }
            continue;
        }

        // ============================================================
        // 新格式检测（ROS2ActionAdapter）
        // ============================================================

        // ROS2ActionAdapter 开始
        if let Some(caps) = adapter_start_regex.captures(&line.line) {
            let adapter_type = caps[1].to_string();

            // 将 adapter_type 映射到 action_type
            let action_type = match adapter_type.as_str() {
                "navigation" => "navigation",
                "double_arm" => "arm",
                "waist_control" => "waist",
                "head_control" => "head",
                _ => &adapter_type,
            };

            // 查找后续几行的 action_type_code（如果有）
            let mut action_code = None;
            for next_line in lines.iter().skip(line_idx + 1).take(5) {
                if let Some(code_caps) = action_code_regex.captures(&next_line.line) {
                    action_code = code_caps[1].parse().ok();
                    break;
                }
            }

            let label = match action_type {
                "navigation" => "导航".to_string(),
                "arm" => {
                    if let Some(code) = action_code {
                        format!("双臂({})", code)
                    } else {
                        "双臂".to_string()
                    }
                }
                "waist" => "腰部".to_string(),
                "head" => "头部".to_string(),
                _ => adapter_type.clone(),
            };

            let action = new_action(
                action_type,
                action_code,
                label,
                line.timestamp,
                "开始执行".to_string(),
            );

            // 使用组合键：adapter_type + timestamp 以支持同类型的并发动作
            let key = format!("{}_{}", adapter_type, line.timestamp as u64);
            active_adapters.insert(key, action);
            continue;
        }

        // ROS2ActionAdapter 等待服务器
        if let Some(caps) = adapter_wait_regex.captures(&line.line) {
            let adapter_type = caps[1].to_string();
            let server_name = caps[2].to_string();
            add_substep_to_adapter(
                &mut active_adapters,
                &adapter_type,
                format!("等待服务器 '{}'...", server_name),
                line.timestamp,
            );
            continue;
        }

        // ROS2ActionAdapter 服务器已就绪
        if let Some(caps) = adapter_ready_regex.captures(&line.line) {
            let adapter_type = caps[1].to_string();
            add_substep_to_adapter(
                &mut active_adapters,
                &adapter_type,
                "服务器已就绪".to_string(),
                line.timestamp,
            );
            continue;
        }

        // ROS2ActionAdapter 发送目标
        if let Some(caps) = adapter_send_regex.captures(&line.line) {
            let adapter_type = caps[1].to_string();
            add_substep_to_adapter(
                &mut active_adapters,
                &adapter_type,
                "发送目标".to_string(),
                line.timestamp,
            );
            continue;
        }

        // ROS2ActionAdapter 响应（目标被接受，开始执行）
        if let Some(caps) = adapter_response_regex.captures(&line.line) {
            let adapter_type = caps[1].to_string();
            add_substep_to_adapter(
                &mut active_adapters,
                &adapter_type,
                "执行中".to_string(),
                line.timestamp,
            );
            continue;
        }

        // ROS2ActionAdapter 结果
        if let Some(caps) = adapter_result_regex.captures(&line.line) {
            let adapter_type = caps[1].to_string();
            let success = caps[2].to_string();
            add_substep_to_adapter(
                &mut active_adapters,
                &adapter_type,
                format!("[RESULT] 完成，成功: {}", success),
                line.timestamp,
            );
            continue;
        }

        // ROS2ActionAdapter 暂停（记录暂停开始时间）
        if let Some(caps) = adapter_pause_regex.captures(&line.line) {
            let adapter_type = caps[1].to_string();
            // 查找匹配类型的未完成动作，添加暂停事件
            for (key, action) in active_adapters.iter_mut() {
                if key.starts_with(&adapter_type) && action.end_ts.is_none() {
                    // 添加暂停事件（恢复时间待填充）
                    action.pause_events.push(PauseEvent {
                        pause_ts: line.timestamp,
                        resume_ts: None,
                    });
                    action.sub_steps.push(SubStep {
                        name: "暂停".to_string(),
                        timestamp: line.timestamp,
                    });
                    break;
                }
            }
            continue;
        }

        // ROS2ActionAdapter 恢复（记录恢复时间）
        if let Some(caps) = adapter_resume_regex.captures(&line.line) {
            let adapter_type = caps[1].to_string();
            // 查找匹配类型的未完成动作，更新最后一个暂停事件的恢复时间
            for (key, action) in active_adapters.iter_mut() {
                if key.starts_with(&adapter_type) && action.end_ts.is_none() {
                    // 更新最后一个暂停事件的恢复时间
                    if let Some(pause_event) = action.pause_events.last_mut()
                        && pause_event.resume_ts.is_none()
                    {
                        pause_event.resume_ts = Some(line.timestamp);
                    }
                    action.sub_steps.push(SubStep {
                        name: "恢复".to_string(),
                        timestamp: line.timestamp,
                    });
                    break;
                }
            }
            continue;
        }

        // ROS2ActionAdapter 结束
        if let Some(caps) = adapter_end_regex.captures(&line.line) {
            let adapter_type = caps[1].to_string();
            let result = caps[2].to_string();

            // 查找并完成匹配类型的最早未完成动作
            let mut completed_key = None;
            for (key, action) in active_adapters.iter_mut() {
                if key.starts_with(&adapter_type) && action.end_ts.is_none() {
                    action.end_ts = Some(line.timestamp);
                    action.status = if result == "成功" {
                        "ok".to_string()
                    } else {
                        format!("failed_{}", result)
                    };
                    action.sub_steps.push(SubStep {
                        name: format!("执行完成，结果: {}", result),
                        timestamp: line.timestamp,
                    });
                    completed_key = Some(key.clone());
                    break;
                }
            }

            // 将完成的动作添加到当前流程或创建新流程
            if let Some(key) = completed_key
                && let Some(action) = active_adapters.remove(&key)
            {
                let round_id = ts_to_round_id(action.start_ts.unwrap_or(line.timestamp), rounds);

                // 如果当前没有流程或轮次不同，创建新流程
                if current_flow.is_none()
                    || current_flow.as_ref().map(|f| f.round_id) != Some(round_id)
                {
                    // 保存当前流程
                    if let Some(flow) = current_flow.take() {
                        flows.push(flow);
                    }

                    // 创建新流程
                    // 如果第一个动作是导航，设置 nav_start_ts/nav_end_ts
                    let (nav_start, nav_end, nav_sub_steps) = if action.action_type == "navigation"
                    {
                        (action.start_ts, action.end_ts, action.sub_steps.clone())
                    } else {
                        (None, None, Vec::new())
                    };
                    current_flow = Some(NavigationFlow {
                        nav_start_ts: nav_start,
                        nav_end_ts: nav_end,
                        nav_target_pos: None,
                        nav_target_ori: None,
                        nav_status: "ok".to_string(),
                        nav_sub_steps,
                        round_id,
                        operations: vec![action],
                    });
                } else {
                    // 添加到当前流程
                    if let Some(ref mut flow) = current_flow {
                        // 如果是导航动作，更新导航信息
                        if action.action_type == "navigation" {
                            flow.nav_start_ts = action.start_ts;
                            flow.nav_end_ts = action.end_ts;
                            flow.nav_sub_steps = action.sub_steps.clone();
                        }
                        flow.operations.push(action);
                    }
                }
            }
            continue;
        }

        // === 预打舵处理（PrePlanNavigationNode 格式）===
        if let Some(caps) = preplan_start_regex.captures(&line.line) {
            let action_label = caps[1].to_string();
            let preplan_action = new_action(
                "preplan",
                None,
                format!("预打舵[{}]", action_label),
                line.timestamp,
                "开始执行".to_string(),
            );

            // 确保有流程可以添加动作
            let round_id = ts_to_round_id(line.timestamp, rounds);
            if current_flow.is_none() || current_flow.as_ref().map(|f| f.round_id) != Some(round_id)
            {
                // 保存当前流程
                if let Some(flow) = current_flow.take() {
                    flows.push(flow);
                }
                // 创建新流程
                current_flow = Some(NavigationFlow {
                    nav_start_ts: None,
                    nav_end_ts: None,
                    nav_target_pos: None,
                    nav_target_ori: None,
                    nav_status: "ok".to_string(),
                    nav_sub_steps: Vec::new(),
                    round_id,
                    operations: vec![preplan_action],
                });
            } else if let Some(ref mut flow) = current_flow {
                flow.operations.push(preplan_action);
            }
            continue;
        }

        // 设置预规划目标
        if preplan_pos_regex.is_match(&line.line) {
            // 查找后续几行的位置和action信息
            let mut pos_str = String::new();
            let mut action_code_val: Option<u32> = None;
            for next_line in lines.iter().skip(line_idx + 1).take(5) {
                if let Some(pos_caps) = preplan_position_regex.captures(&next_line.line) {
                    pos_str = pos_caps[1].replace(' ', "");
                }
                if let Some(action_caps) = preplan_action_regex.captures(&next_line.line) {
                    action_code_val = action_caps[1].parse().ok();
                }
            }

            // 更新最近的预打舵动作
            let ts = line.timestamp;
            update_preplan_action(&mut current_flow, &mut flows, |op| {
                if let Some(code) = action_code_val {
                    op.action_code = Some(code);
                }
                op.sub_steps.push(SubStep {
                    name: format!("设置目标→{}", pos_str),
                    timestamp: ts,
                });
            });
            continue;
        }

        // 服务响应
        if let Some(caps) = preplan_response_regex.captures(&line.line) {
            let error_code = caps[2].trim().to_string();
            let ts = line.timestamp;
            update_preplan_action(&mut current_flow, &mut flows, |op| {
                op.sub_steps.push(SubStep {
                    name: format!("响应(code={})", error_code),
                    timestamp: ts,
                });
            });
            continue;
        }

        // 执行成功
        if preplan_end_regex.is_match(&line.line) {
            let ts = line.timestamp;
            update_preplan_action(&mut current_flow, &mut flows, |op| {
                op.sub_steps.push(SubStep {
                    name: "执行成功".to_string(),
                    timestamp: ts,
                });
                op.end_ts = Some(ts);
                op.status = "ok".to_string();
            });
            continue;
        }
    }

    // 完成剩余的流程
    if let Some(mut flow) = current_flow {
        flow.nav_status = "incomplete".to_string();
        flows.push(flow);
    }

    // 处理未完成的 ROS2ActionAdapter 动作
    for (_, action) in active_adapters {
        if action.end_ts.is_none() {
            let round_id = ts_to_round_id(action.start_ts.unwrap_or(0.0), rounds);
            // 查找匹配轮次的流程并添加
            let mut found = false;
            for flow in flows.iter_mut() {
                if flow.round_id == round_id {
                    flow.operations.push(action.clone());
                    found = true;
                    break;
                }
            }
            if !found {
                // 创建新流程
                flows.push(NavigationFlow {
                    nav_start_ts: None,
                    nav_end_ts: None,
                    nav_target_pos: None,
                    nav_target_ori: None,
                    nav_status: "incomplete".to_string(),
                    nav_sub_steps: Vec::new(),
                    round_id,
                    operations: vec![action],
                });
            }
        }
    }

    Ok(flows)
}
