//! 工作流编排器
//!
//! 编排整个分析流程：连接、发现、下载、选择插件、执行分析

use crate::config::AnalyzerConfig;
use crate::discoverer::{DiscovererError, FileDiscoverer, FileInfo};
use crate::selector::PluginSelector;
use analyzer_remote::{FileTransfer, SshConfig, SshConnection};
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;
use tracing::{debug, info, warn};

/// 工作流错误
#[derive(Error, Debug)]
pub enum WorkflowError {
    #[error("配置错误: {0}")]
    ConfigError(String),

    #[error("连接失败: {0}")]
    ConnectionError(String),

    #[error("文件发现失败: {0}")]
    DiscoveryError(#[from] DiscovererError),

    #[error("下载失败: {0}")]
    DownloadError(String),

    #[error("没有找到匹配的插件")]
    NoPluginFound,

    #[error("插件加载失败: {0}")]
    PluginLoadError(String),

    #[error("分析失败: {0}")]
    AnalysisError(String),

    #[error("IO 错误: {0}")]
    IoError(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, WorkflowError>;

/// 工作流步骤（用于跟踪）
#[derive(Debug, Clone)]
pub enum WorkflowStep {
    ConnectRemote,
    DiscoverFiles { pattern: String },
    DownloadFile { remote: String, local: String },
    SelectPlugin { file_name: String },
    LoadPlugin { plugin_name: String },
    RunAnalysis { input: String, output: String },
    Cleanup,
}

/// 工作流结果
#[derive(Debug)]
pub struct WorkflowResult {
    pub success: bool,
    pub steps_completed: Vec<WorkflowStep>,
    pub output_files: Vec<PathBuf>,
    pub errors: Vec<String>,
    pub selected_plugin: Option<String>,
    pub analyzed_file: Option<PathBuf>,
}

impl WorkflowResult {
    fn new() -> Self {
        Self {
            success: false,
            steps_completed: Vec::new(),
            output_files: Vec::new(),
            errors: Vec::new(),
            selected_plugin: None,
            analyzed_file: None,
        }
    }

    fn add_step(&mut self, step: WorkflowStep) {
        self.steps_completed.push(step);
    }

    fn add_error(&mut self, error: String) {
        self.errors.push(error);
    }
}

/// 工作流编排器
pub struct WorkflowOrchestrator {
    config: AnalyzerConfig,
    discoverer: FileDiscoverer,
    selector: PluginSelector,
    remote_connection: Option<SshConnection>,
}

impl WorkflowOrchestrator {
    /// 创建工作流编排器
    pub fn new(config: AnalyzerConfig) -> Result<Self> {
        let discoverer = FileDiscoverer::new(config.file_discovery.clone());
        let selector = PluginSelector::new(config.analyzers.clone());

        Ok(Self {
            config,
            discoverer,
            selector,
            remote_connection: None,
        })
    }

    /// 自动模式：获取最新日志并分析
    pub fn run_auto(&mut self) -> Result<WorkflowResult> {
        info!("启动自动模式");
        let mut result = WorkflowResult::new();

        // 1. 连接远程（如果启用）
        if self.config.remote.enabled {
            if let Err(e) = self.connect_remote() {
                let error = format!("远程连接失败: {}", e);
                warn!("{}", error);
                result.add_error(error);
                return Ok(result);
            }
            result.add_step(WorkflowStep::ConnectRemote);
        }

        // 2. 发现并选择文件
        let file_info = self.discover_and_select_file(&mut result)?;

        // 3. 下载文件（如果是远程）
        let local_file = if file_info.is_remote {
            self.download_file(&file_info, &mut result)?
        } else {
            file_info.path.clone()
        };

        // 4. 选择插件
        let plugin_mapping = self
            .selector
            .select_plugin(&file_info.name)
            .ok_or(WorkflowError::NoPluginFound)?;

        result.selected_plugin = Some(plugin_mapping.plugin.clone());
        result
            .add_step(WorkflowStep::SelectPlugin {
                file_name: file_info.name.clone(),
            });

        // 5. 分析文件
        info!("准备分析文件: {:?}", local_file);
        result.analyzed_file = Some(local_file.clone());

        // 注意：实际的插件加载和执行需要在 CLI 层完成
        // 这里只返回需要分析的文件和插件信息

        result.success = true;
        Ok(result)
    }

    /// 指定文件模式
    pub fn run_with_file(&mut self, file_path: &str) -> Result<WorkflowResult> {
        info!("分析指定文件: {}", file_path);
        let mut result = WorkflowResult::new();

        let local_path = PathBuf::from(file_path);

        // 检查本地文件是否存在
        let file_to_analyze = if local_path.exists() {
            info!("使用本地文件: {:?}", local_path);
            local_path
        } else if self.config.remote.enabled {
            // 尝试从远程下载
            if let Err(e) = self.connect_remote() {
                let error = format!("远程连接失败: {}", e);
                result.add_error(error);
                return Ok(result);
            }
            result.add_step(WorkflowStep::ConnectRemote);

            // 创建临时 FileInfo
            let file_name = local_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown")
                .to_string();

            let remote_path = PathBuf::from(self.config.remote.log_dir.clone()).join(&file_name);

            let file_info = FileInfo {
                path: remote_path,
                name: file_name,
                size: 0,
                mtime: 0,
                is_remote: true,
            };

            self.download_file(&file_info, &mut result)?
        } else {
            return Err(WorkflowError::DiscoveryError(
                DiscovererError::NoFilesFound,
            ));
        };

        // 选择插件
        let file_name = file_to_analyze
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown");

        let plugin_mapping = self
            .selector
            .select_plugin(file_name)
            .ok_or(WorkflowError::NoPluginFound)?;

        result.selected_plugin = Some(plugin_mapping.plugin.clone());
        result.analyzed_file = Some(file_to_analyze);
        result.success = true;

        Ok(result)
    }

    /// 列出远程可用日志
    pub fn list_remote_logs(&mut self, pattern: Option<&str>) -> Result<Vec<FileInfo>> {
        info!("列出远程日志文件");

        if !self.config.remote.enabled {
            return Err(WorkflowError::ConfigError("远程功能未启用".to_string()));
        }

        self.connect_remote()?;

        let pattern = pattern.unwrap_or("*.log");
        let connection = self.remote_connection.as_mut().unwrap();

        self.discoverer
            .discover_remote(
                connection,
                self.config.remote.log_dir.to_str().unwrap(),
                pattern,
            )
            .map_err(WorkflowError::from)
    }

    /// 仅下载模式
    pub fn download_only(&mut self, file_name: &str) -> Result<PathBuf> {
        info!("下载文件: {}", file_name);

        if !self.config.remote.enabled {
            return Err(WorkflowError::ConfigError("远程功能未启用".to_string()));
        }

        self.connect_remote()?;

        let remote_path = PathBuf::from(self.config.remote.log_dir.clone()).join(file_name);

        let file_info = FileInfo {
            path: remote_path,
            name: file_name.to_string(),
            size: 0,
            mtime: 0,
            is_remote: true,
        };

        let mut result = WorkflowResult::new();
        self.download_file(&file_info, &mut result)
    }

    /// 获取插件选择器引用
    pub fn selector(&self) -> &PluginSelector {
        &self.selector
    }

    /// 获取配置引用
    pub fn config(&self) -> &AnalyzerConfig {
        &self.config
    }

    // ===== 内部辅助方法 =====

    /// 连接远程
    fn connect_remote(&mut self) -> Result<()> {
        if self.remote_connection.is_some() {
            debug!("远程连接已存在");
            return Ok(());
        }

        info!("建立远程连接");

        let ssh_config = SshConfig {
            host: self.config.remote.host.clone(),
            port: self.config.remote.port,
            user: self.config.remote.user.clone(),
            auth: self.config.remote.auth.to_auth_method(),
            timeouts: self.config.remote.timeouts.clone(),
        };

        let connection = SshConnection::connect(ssh_config)
            .map_err(|e| WorkflowError::ConnectionError(e.to_string()))?;

        self.remote_connection = Some(connection);
        Ok(())
    }

    /// 发现并选择文件
    fn discover_and_select_file(&mut self, result: &mut WorkflowResult) -> Result<FileInfo> {
        // 获取第一个启用的分析器模式
        let pattern = self
            .selector
            .list_enabled()
            .first()
            .map(|m| m.pattern.clone())
            .unwrap_or_else(|| "*.log".to_string());

        info!("搜索文件，模式: {}", pattern);
        result.add_step(WorkflowStep::DiscoverFiles {
            pattern: pattern.clone(),
        });

        // 优先搜索本地
        if let Ok(file) = self
            .discoverer
            .discover_and_select_local(&self.config.local.log_dir, &pattern)
        {
            info!("使用本地文件: {:?}", file.path);
            return Ok(file);
        }

        // 搜索远程
        if self.config.remote.enabled && self.remote_connection.is_some() {
            let connection = self.remote_connection.as_mut().unwrap();
            let file = self
                .discoverer
                .discover_and_select_remote(
                    connection,
                    self.config.remote.log_dir.to_str().unwrap(),
                    &pattern,
                )
                .map_err(WorkflowError::from)?;

            info!("使用远程文件: {:?}", file.path);
            return Ok(file);
        }

        Err(WorkflowError::DiscoveryError(
            DiscovererError::NoFilesFound,
        ))
    }

    /// 下载文件
    fn download_file(&mut self, file_info: &FileInfo, result: &mut WorkflowResult) -> Result<PathBuf> {
        if !file_info.is_remote {
            return Ok(file_info.path.clone());
        }

        info!("下载远程文件: {:?}", file_info.path);

        let connection = self
            .remote_connection
            .take()
            .ok_or_else(|| WorkflowError::ConnectionError("未建立连接".to_string()))?;

        let mut transfer = FileTransfer::new(connection);

        // 确保本地目录存在
        fs::create_dir_all(&self.config.local.log_dir)?;

        let local_path = self.config.local.log_dir.join(&file_info.name);

        transfer
            .download(
                &file_info.path,
                &local_path,
                self.config.workflow.progress.show_progress_bar,
            )
            .map_err(|e| WorkflowError::DownloadError(e.to_string()))?;

        // 恢复连接
        self.remote_connection = Some(transfer.into_connection());

        result.add_step(WorkflowStep::DownloadFile {
            remote: file_info.path.display().to_string(),
            local: local_path.display().to_string(),
        });

        info!("下载完成: {:?}", local_path);
        Ok(local_path)
    }
}

impl Drop for WorkflowOrchestrator {
    fn drop(&mut self) {
        if let Some(conn) = self.remote_connection.take() {
            let _ = conn.disconnect();
        }
    }
}
