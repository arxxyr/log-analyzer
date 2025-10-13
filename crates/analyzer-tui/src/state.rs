//! 应用状态管理

use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::Arc;
use parking_lot::RwLock;

/// 工作流阶段
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkflowPhase {
    /// 初始化
    Initializing,
    /// 连接远程
    Connecting,
    /// 发现文件
    Discovering,
    /// 下载文件
    Downloading,
    /// 选择插件
    SelectingPlugin,
    /// 分析中
    Analyzing,
    /// 完成
    Completed,
    /// 错误
    Error,
}

impl WorkflowPhase {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Initializing => "初始化",
            Self::Connecting => "连接远程",
            Self::Discovering => "发现文件",
            Self::Downloading => "下载文件",
            Self::SelectingPlugin => "选择插件",
            Self::Analyzing => "分析中",
            Self::Completed => "完成",
            Self::Error => "错误",
        }
    }
}

/// 进度信息
#[derive(Debug, Clone)]
pub struct ProgressInfo {
    /// 当前值
    pub current: u64,
    /// 总值
    pub total: u64,
    /// 速度（字节/秒）
    pub speed: Option<f64>,
    /// 百分比
    pub percentage: f32,
}

impl ProgressInfo {
    pub fn new(current: u64, total: u64) -> Self {
        let percentage = if total > 0 {
            (current as f32 / total as f32 * 100.0).min(100.0)
        } else {
            0.0
        };

        Self {
            current,
            total,
            speed: None,
            percentage,
        }
    }

    pub fn with_speed(mut self, speed: f64) -> Self {
        self.speed = Some(speed);
        self
    }
}

/// 日志条目
#[derive(Debug, Clone)]
pub struct LogEntry {
    /// 时间戳
    pub timestamp: DateTime<Local>,
    /// 日志级别
    pub level: LogLevel,
    /// 消息
    pub message: String,
}

/// 日志级别
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl LogLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Trace => "TRACE",
            Self::Debug => "DEBUG",
            Self::Info => "INFO ",
            Self::Warn => "WARN ",
            Self::Error => "ERROR",
        }
    }
}

/// 插件显示信息（用于 TUI 显示）
#[derive(Debug, Clone)]
pub struct PluginDisplayInfo {
    /// 插件名称
    pub name: String,
    /// 版本
    pub version: String,
    /// 描述
    pub description: String,
    /// 是否已启用
    pub enabled: bool,
    /// 是否必需（必需的插件不能被禁用）
    pub required: bool,
}

/// 焦点区域
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusArea {
    /// 主区域（日志查看）
    Main,
    /// 插件面板
    PluginPanel,
}

/// 应用状态（线程安全）
#[derive(Clone)]
pub struct AppState {
    inner: Arc<RwLock<AppStateInner>>,
}

struct AppStateInner {
    /// 当前工作流阶段
    phase: WorkflowPhase,
    /// 当前焦点区域
    focus: FocusArea,
    /// 插件选择：当前选中的索引
    selected_plugin_index: usize,
    /// 是否请求重启工作流
    restart_requested: bool,
    /// 进度信息
    progress: Option<ProgressInfo>,
    /// 日志缓冲区（最多保留1000条）
    logs: VecDeque<LogEntry>,
    /// 当前文件名
    current_file: Option<String>,
    /// 当前插件名
    current_plugin: Option<String>,
    /// 可用插件列表
    available_plugins: Vec<PluginDisplayInfo>,
    /// 错误信息
    error_message: Option<String>,
    /// 开始时间
    start_time: DateTime<Local>,
    /// 结束时间
    end_time: Option<DateTime<Local>>,
    /// 是否暂停
    paused: bool,
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

impl AppState {
    /// 创建新的应用状态
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(AppStateInner {
                phase: WorkflowPhase::Initializing,
                focus: FocusArea::Main,
                selected_plugin_index: 0,
                restart_requested: false,
                progress: None,
                logs: VecDeque::with_capacity(1000),
                current_file: None,
                current_plugin: None,
                available_plugins: Vec::new(),
                error_message: None,
                start_time: Local::now(),
                end_time: None,
                paused: false,
            })),
        }
    }

    /// 设置工作流阶段
    pub fn set_phase(&self, phase: WorkflowPhase) {
        self.inner.write().phase = phase;
    }

    /// 获取工作流阶段
    pub fn phase(&self) -> WorkflowPhase {
        self.inner.read().phase
    }

    /// 设置进度
    pub fn set_progress(&self, progress: Option<ProgressInfo>) {
        self.inner.write().progress = progress;
    }

    /// 获取进度
    pub fn progress(&self) -> Option<ProgressInfo> {
        self.inner.read().progress.clone()
    }

    /// 添加日志
    pub fn add_log(&self, level: LogLevel, message: impl Into<String>) {
        let mut inner = self.inner.write();
        let entry = LogEntry {
            timestamp: Local::now(),
            level,
            message: message.into(),
        };

        inner.logs.push_back(entry);

        // 保持最多 1000 条日志
        if inner.logs.len() > 1000 {
            inner.logs.pop_front();
        }
    }

    /// 获取日志（最新的 N 条）
    pub fn logs(&self, count: usize) -> Vec<LogEntry> {
        let inner = self.inner.read();
        inner.logs.iter()
            .rev()
            .take(count)
            .rev()
            .cloned()
            .collect()
    }

    /// 获取所有日志
    pub fn all_logs(&self) -> Vec<LogEntry> {
        self.inner.read().logs.iter().cloned().collect()
    }

    /// 设置当前文件
    pub fn set_current_file(&self, file: Option<String>) {
        self.inner.write().current_file = file;
    }

    /// 获取当前文件
    pub fn current_file(&self) -> Option<String> {
        self.inner.read().current_file.clone()
    }

    /// 设置当前插件
    pub fn set_current_plugin(&self, plugin: Option<String>) {
        self.inner.write().current_plugin = plugin;
    }

    /// 获取当前插件
    pub fn current_plugin(&self) -> Option<String> {
        self.inner.read().current_plugin.clone()
    }

    /// 设置错误信息
    pub fn set_error(&self, error: Option<String>) {
        let mut inner = self.inner.write();
        inner.error_message = error.clone();
        if error.is_some() {
            inner.phase = WorkflowPhase::Error;
        }
    }

    /// 获取错误信息
    pub fn error_message(&self) -> Option<String> {
        self.inner.read().error_message.clone()
    }

    /// 标记完成
    pub fn mark_completed(&self) {
        let mut inner = self.inner.write();
        inner.phase = WorkflowPhase::Completed;
        inner.end_time = Some(Local::now());
    }

    /// 获取运行时长（秒）
    pub fn elapsed_seconds(&self) -> i64 {
        let inner = self.inner.read();
        let end = inner.end_time.unwrap_or_else(Local::now);
        (end - inner.start_time).num_seconds()
    }

    /// 切换暂停状态
    pub fn toggle_pause(&self) {
        let mut inner = self.inner.write();
        inner.paused = !inner.paused;
    }

    /// 是否暂停
    pub fn is_paused(&self) -> bool {
        self.inner.read().paused
    }

    /// 设置可用插件列表
    pub fn set_available_plugins(&self, plugins: Vec<PluginDisplayInfo>) {
        self.inner.write().available_plugins = plugins;
    }

    /// 获取可用插件列表
    pub fn available_plugins(&self) -> Vec<PluginDisplayInfo> {
        self.inner.read().available_plugins.clone()
    }

    /// 设置焦点区域
    pub fn set_focus(&self, focus: FocusArea) {
        self.inner.write().focus = focus;
    }

    /// 获取焦点区域
    pub fn focus(&self) -> FocusArea {
        self.inner.read().focus
    }

    /// 切换焦点
    pub fn toggle_focus(&self) {
        let mut inner = self.inner.write();
        inner.focus = match inner.focus {
            FocusArea::Main => FocusArea::PluginPanel,
            FocusArea::PluginPanel => FocusArea::Main,
        };
    }

    /// 请求重启工作流
    pub fn request_restart(&self) {
        let mut inner = self.inner.write();
        inner.restart_requested = true;
    }

    /// 检查是否请求了重启
    pub fn is_restart_requested(&self) -> bool {
        self.inner.read().restart_requested
    }

    /// 清除重启请求
    pub fn clear_restart_request(&self) {
        let mut inner = self.inner.write();
        inner.restart_requested = false;
    }

    /// 重置状态（用于重启）
    pub fn reset_for_restart(&self) {
        let mut inner = self.inner.write();
        inner.phase = WorkflowPhase::Initializing;
        inner.progress = None;
        inner.logs.clear();
        inner.current_file = None;
        inner.current_plugin = None;
        inner.error_message = None;
        inner.start_time = Local::now();
        inner.end_time = None;
        inner.paused = false;
        inner.restart_requested = false;
    }

    /// 获取选中的插件索引
    pub fn selected_plugin_index(&self) -> usize {
        self.inner.read().selected_plugin_index
    }

    /// 设置选中的插件索引
    pub fn set_selected_plugin_index(&self, index: usize) {
        let mut inner = self.inner.write();
        if index < inner.available_plugins.len() {
            inner.selected_plugin_index = index;
        }
    }

    /// 选择上一个插件
    pub fn select_previous_plugin(&self) {
        let mut inner = self.inner.write();
        if !inner.available_plugins.is_empty() {
            if inner.selected_plugin_index > 0 {
                inner.selected_plugin_index -= 1;
            } else {
                inner.selected_plugin_index = inner.available_plugins.len() - 1;
            }
        }
    }

    /// 选择下一个插件
    pub fn select_next_plugin(&self) {
        let mut inner = self.inner.write();
        if !inner.available_plugins.is_empty() {
            inner.selected_plugin_index = (inner.selected_plugin_index + 1) % inner.available_plugins.len();
        }
    }

    /// 切换当前选中插件的启用状态
    pub fn toggle_selected_plugin(&self) {
        let mut inner = self.inner.write();
        let index = inner.selected_plugin_index;
        if index < inner.available_plugins.len() {
            let plugin = &mut inner.available_plugins[index];
            // 必需的插件不能被禁用
            if !plugin.required {
                plugin.enabled = !plugin.enabled;
            }
        }
    }

    /// 获取已启用的插件名称列表
    pub fn enabled_plugin_names(&self) -> Vec<String> {
        self.inner.read()
            .available_plugins
            .iter()
            .filter(|p| p.enabled)
            .map(|p| p.name.clone())
            .collect()
    }
}
