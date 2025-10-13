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

/// 应用状态（线程安全）
#[derive(Clone)]
pub struct AppState {
    inner: Arc<RwLock<AppStateInner>>,
}

struct AppStateInner {
    /// 当前工作流阶段
    phase: WorkflowPhase,
    /// 进度信息
    progress: Option<ProgressInfo>,
    /// 日志缓冲区（最多保留1000条）
    logs: VecDeque<LogEntry>,
    /// 当前文件名
    current_file: Option<String>,
    /// 当前插件名
    current_plugin: Option<String>,
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
                progress: None,
                logs: VecDeque::with_capacity(1000),
                current_file: None,
                current_plugin: None,
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
}
