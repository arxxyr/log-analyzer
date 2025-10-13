//! 配置管理模块
//!
//! 定义和管理分析器的所有配置

use analyzer_remote::{AuthMethod, Timeouts};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

/// 配置错误
#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("配置文件不存在: {0}")]
    FileNotFound(String),

    #[error("配置解析失败: {0}")]
    ParseError(String),

    #[error("配置验证失败: {0}")]
    ValidationError(String),

    #[error("IO 错误: {0}")]
    IoError(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, ConfigError>;

/// 主配置
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AnalyzerConfig {
    /// 远程连接配置
    #[serde(default)]
    pub remote: RemoteConfig,

    /// 本地路径配置
    #[serde(default)]
    pub local: LocalConfig,

    /// 文件发现配置
    #[serde(default)]
    pub file_discovery: FileDiscoveryConfig,

    /// 分析器映射
    #[serde(default)]
    pub analyzers: Vec<AnalyzerMapping>,

    /// 多文件分析配置（v0.4.0 新增）
    #[serde(default)]
    pub multi_file: MultiFileConfig,

    /// 工作流配置
    #[serde(default)]
    pub workflow: WorkflowConfig,

    /// 日志配置
    #[serde(default)]
    pub logging: LoggingConfig,

    /// 高级配置
    #[serde(default)]
    pub advanced: AdvancedConfig,
}

/// 远程连接配置
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RemoteConfig {
    /// 是否启用远程功能
    #[serde(default)]
    pub enabled: bool,

    /// 主机地址
    #[serde(default = "default_host")]
    pub host: String,

    /// 端口
    #[serde(default = "default_port")]
    pub port: u16,

    /// 用户名
    #[serde(default = "default_user")]
    pub user: String,

    /// 认证配置
    #[serde(default)]
    pub auth: AuthConfig,

    /// 日志目录
    #[serde(default = "default_remote_log_dir")]
    pub log_dir: PathBuf,

    /// 超时配置
    #[serde(default)]
    pub timeouts: Timeouts,
}

fn default_host() -> String {
    "192.168.4.69".to_string()
}

fn default_port() -> u16 {
    22
}

fn default_user() -> String {
    "firefly".to_string()
}

fn default_remote_log_dir() -> PathBuf {
    PathBuf::from("/home/firefly/.ros/log")
}

impl Default for RemoteConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            host: default_host(),
            port: default_port(),
            user: default_user(),
            auth: AuthConfig::default(),
            log_dir: default_remote_log_dir(),
            timeouts: Timeouts::default(),
        }
    }
}

/// 认证配置
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AuthConfig {
    /// SSH 密钥文件路径
    pub key_file: Option<PathBuf>,

    /// 使用 SSH Agent
    #[serde(default = "default_use_agent")]
    pub use_agent: bool,

    /// 密码（不推荐）
    pub password: Option<String>,
}

fn default_use_agent() -> bool {
    true
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            key_file: None,
            use_agent: true,
            password: None,
        }
    }
}

impl AuthConfig {
    /// 转换为 analyzer-remote 的 AuthMethod
    /// 优先级：密码 > 密钥文件 > SSH Agent > 交互式
    pub fn to_auth_method(&self) -> AuthMethod {
        // 如果明确提供了密码，优先使用密码（用户意图明确）
        if let Some(ref password) = self.password {
            AuthMethod::Password {
                password: password.clone(),
            }
        } else if let Some(ref key_file) = self.key_file {
            // 其次使用密钥文件
            AuthMethod::KeyFile {
                path: key_file.clone(),
                passphrase: None,
            }
        } else if self.use_agent {
            // 再使用 SSH Agent
            AuthMethod::Agent
        } else {
            // 最后使用交互式
            AuthMethod::Interactive
        }
    }
}

/// 本地路径配置
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LocalConfig {
    /// 本地日志目录
    #[serde(default = "default_local_log_dir")]
    pub log_dir: PathBuf,

    /// 输出目录
    #[serde(default = "default_output_dir")]
    pub output_dir: PathBuf,

    /// 插件目录
    #[serde(default = "default_plugin_dir")]
    pub plugin_dir: PathBuf,

    /// 是否在分析前清理旧输出
    #[serde(default = "default_cleanup")]
    pub cleanup_old_output: bool,
}

fn default_local_log_dir() -> PathBuf {
    PathBuf::from("./logs")
}

fn default_output_dir() -> PathBuf {
    PathBuf::from("./output")
}

fn default_plugin_dir() -> PathBuf {
    PathBuf::from("./plugins")
}

fn default_cleanup() -> bool {
    true
}

impl Default for LocalConfig {
    fn default() -> Self {
        Self {
            log_dir: default_local_log_dir(),
            output_dir: default_output_dir(),
            plugin_dir: default_plugin_dir(),
            cleanup_old_output: true,
        }
    }
}

/// 文件发现配置
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FileDiscoveryConfig {
    /// 排序方式
    #[serde(default = "default_sort_by")]
    pub sort_by: SortBy,

    /// 排序顺序
    #[serde(default = "default_sort_order")]
    pub sort_order: SortOrder,

    /// 自动选择
    #[serde(default = "default_auto_select")]
    pub auto_select: AutoSelect,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum SortBy {
    Mtime,
    Name,
    Size,
}

fn default_sort_by() -> SortBy {
    SortBy::Mtime
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum SortOrder {
    Asc,
    Desc,
}

fn default_sort_order() -> SortOrder {
    SortOrder::Desc
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum AutoSelect {
    Latest,
    Oldest,
    None,
}

fn default_auto_select() -> AutoSelect {
    AutoSelect::Latest
}

impl Default for FileDiscoveryConfig {
    fn default() -> Self {
        Self {
            sort_by: SortBy::Mtime,
            sort_order: SortOrder::Desc,
            auto_select: AutoSelect::Latest,
        }
    }
}

/// 分析器映射规则
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AnalyzerMapping {
    /// 名称
    pub name: String,

    /// 文件匹配模式（glob）
    pub pattern: String,

    /// 插件名称
    pub plugin: String,

    /// 描述
    pub description: String,

    /// 是否启用
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    /// 是否必需（多文件模式下，必需的文件不存在会报错）
    #[serde(default)]
    pub required: bool,

    /// 是否为主时间轴（用于时间对齐）
    #[serde(default)]
    pub is_primary: bool,

    /// 优先级（数字越大越优先）
    #[serde(default)]
    pub priority: i32,

    /// 插件特定配置
    #[serde(default)]
    pub config: Option<serde_json::Value>,
}

fn default_enabled() -> bool {
    true
}

/// 工作流配置
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WorkflowConfig {
    /// 自动下载最新日志
    #[serde(default = "default_auto_download")]
    pub auto_download: bool,

    /// 下载后自动分析
    #[serde(default = "default_auto_analyze")]
    pub auto_analyze: bool,

    /// 重试配置
    #[serde(default)]
    pub retry: RetryConfig,

    /// 进度显示配置
    #[serde(default)]
    pub progress: ProgressConfig,
}

fn default_auto_download() -> bool {
    true
}

fn default_auto_analyze() -> bool {
    true
}

impl Default for WorkflowConfig {
    fn default() -> Self {
        Self {
            auto_download: true,
            auto_analyze: true,
            retry: RetryConfig::default(),
            progress: ProgressConfig::default(),
        }
    }
}

/// 重试配置
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RetryConfig {
    #[serde(default = "default_retry_enabled")]
    pub enabled: bool,

    #[serde(default = "default_max_attempts")]
    pub max_attempts: u32,

    #[serde(default = "default_delay_seconds")]
    pub delay_seconds: u64,
}

fn default_retry_enabled() -> bool {
    true
}

fn default_max_attempts() -> u32 {
    3
}

fn default_delay_seconds() -> u64 {
    5
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_attempts: 3,
            delay_seconds: 5,
        }
    }
}

/// 进度显示配置
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProgressConfig {
    #[serde(default = "default_show_progress")]
    pub show_progress_bar: bool,

    #[serde(default = "default_show_speed")]
    pub show_transfer_speed: bool,
}

fn default_show_progress() -> bool {
    true
}

fn default_show_speed() -> bool {
    true
}

impl Default for ProgressConfig {
    fn default() -> Self {
        Self {
            show_progress_bar: true,
            show_transfer_speed: true,
        }
    }
}

/// 日志配置
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LoggingConfig {
    #[serde(default = "default_log_level")]
    pub level: String,

    #[serde(default = "default_log_format")]
    pub format: String,
}

fn default_log_level() -> String {
    "info".to_string()
}

fn default_log_format() -> String {
    "compact".to_string()
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: "info".to_string(),
            format: "compact".to_string(),
        }
    }
}

/// 高级配置
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AdvancedConfig {
    /// 并发下载数量
    #[serde(default = "default_concurrent")]
    pub concurrent_downloads: usize,

    /// 缓存远程文件列表（秒）
    #[serde(default = "default_cache_duration")]
    pub cache_remote_list: u64,

    /// 跳过已分析的文件
    #[serde(default)]
    pub skip_analyzed: bool,
}

fn default_concurrent() -> usize {
    2
}

fn default_cache_duration() -> u64 {
    60
}

impl Default for AdvancedConfig {
    fn default() -> Self {
        Self {
            concurrent_downloads: 2,
            cache_remote_list: 60,
            skip_analyzed: false,
        }
    }
}

/// 多文件分析配置（v0.4.0 新增）
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MultiFileConfig {
    /// 是否启用多文件分析模式
    #[serde(default = "default_multifile_enabled")]
    pub enabled: bool,

    /// 自动模式下同时下载的文件模式（按优先级）
    #[serde(default)]
    pub auto_patterns: Vec<String>,

    /// 时间对齐配置
    #[serde(default)]
    pub alignment: AlignmentConfig,

    /// 泳道优先级（控制甘特图中的显示顺序）
    #[serde(default)]
    pub track_priority: std::collections::HashMap<String, i32>,
}

fn default_multifile_enabled() -> bool {
    false
}

impl Default for MultiFileConfig {
    fn default() -> Self {
        let mut track_priority = std::collections::HashMap::new();
        track_priority.insert("RoundMarker".to_string(), 0);
        track_priority.insert("Navigation".to_string(), 1);
        track_priority.insert("Arm".to_string(), 2);
        track_priority.insert("Head".to_string(), 3);
        track_priority.insert("Waist".to_string(), 4);

        Self {
            enabled: false,
            auto_patterns: vec!["master_control_*.log".to_string()],
            alignment: AlignmentConfig::default(),
            track_priority,
        }
    }
}

/// 时间对齐配置
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AlignmentConfig {
    /// 主时间轴来源（必须匹配 analyzers 中标记 is_primary: true 的）
    #[serde(default = "default_primary_source")]
    pub primary_source: String,

    /// 时间容差（秒），用于对齐不同日志
    #[serde(default = "default_time_tolerance")]
    pub time_tolerance: f64,

    /// 对齐策略
    #[serde(default = "default_alignment_strategy")]
    pub strategy: AlignmentStrategy,
}

fn default_primary_source() -> String {
    "master-control".to_string()
}

fn default_time_tolerance() -> f64 {
    1.0
}

fn default_alignment_strategy() -> AlignmentStrategy {
    AlignmentStrategy::Timestamp
}

impl Default for AlignmentConfig {
    fn default() -> Self {
        Self {
            primary_source: "master-control".to_string(),
            time_tolerance: 1.0,
            strategy: AlignmentStrategy::Timestamp,
        }
    }
}

/// 对齐策略
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum AlignmentStrategy {
    /// 直接使用时间戳（假设时钟同步）
    Timestamp,
    /// 基于事件特征对齐（未来实现）
    EventBased,
}

impl Default for AnalyzerConfig {
    fn default() -> Self {
        Self {
            remote: RemoteConfig::default(),
            local: LocalConfig::default(),
            file_discovery: FileDiscoveryConfig::default(),
            analyzers: vec![
                AnalyzerMapping {
                    name: "master-control".to_string(),
                    pattern: "master_control_*.log".to_string(),
                    plugin: "master-control-analyzer".to_string(),
                    description: "机器人主控系统日志分析器".to_string(),
                    enabled: true,
                    required: true,
                    is_primary: true,
                    priority: 0,
                    config: None,
                },
            ],
            multi_file: MultiFileConfig::default(),
            workflow: WorkflowConfig::default(),
            logging: LoggingConfig::default(),
            advanced: AdvancedConfig::default(),
        }
    }
}

impl AnalyzerConfig {
    /// 从文件加载配置
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();

        if !path.exists() {
            return Err(ConfigError::FileNotFound(path.display().to_string()));
        }

        let content = fs::read_to_string(path)?;

        let config: AnalyzerConfig = serde_yaml::from_str(&content)
            .map_err(|e| ConfigError::ParseError(format!("YAML 解析失败: {}", e)))?;

        config.validate()?;
        Ok(config)
    }

    /// 保存配置到文件
    pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let content = serde_yaml::to_string(self)
            .map_err(|e| ConfigError::ParseError(format!("YAML 序列化失败: {}", e)))?;

        fs::write(path, content)?;
        Ok(())
    }

    /// 验证配置有效性
    pub fn validate(&self) -> Result<()> {
        // 验证远程配置
        if self.remote.enabled {
            if self.remote.host.is_empty() {
                return Err(ConfigError::ValidationError("远程主机不能为空".to_string()));
            }
            if self.remote.user.is_empty() {
                return Err(ConfigError::ValidationError("远程用户不能为空".to_string()));
            }
        }

        // 验证分析器映射
        if self.analyzers.is_empty() {
            return Err(ConfigError::ValidationError("至少需要一个分析器映射".to_string()));
        }

        for analyzer in &self.analyzers {
            if analyzer.pattern.is_empty() {
                return Err(ConfigError::ValidationError(
                    format!("分析器 '{}' 的模式不能为空", analyzer.name)
                ));
            }
            if analyzer.plugin.is_empty() {
                return Err(ConfigError::ValidationError(
                    format!("分析器 '{}' 的插件名称不能为空", analyzer.name)
                ));
            }
        }

        Ok(())
    }

    /// 合并配置（用于配置覆盖）
    pub fn merge(mut self, other: AnalyzerConfig) -> Self {
        // 简单的字段级合并
        if other.remote.enabled {
            self.remote = other.remote;
        }

        // 合并分析器（保留唯一的）
        for other_analyzer in other.analyzers {
            if !self.analyzers.iter().any(|a| a.name == other_analyzer.name) {
                self.analyzers.push(other_analyzer);
            }
        }

        self
    }
}
