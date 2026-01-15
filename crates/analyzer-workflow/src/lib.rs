//! analyzer-workflow - 工作流编排模块
//!
//! 提供配置管理、文件发现、插件选择和工作流编排功能

pub mod config;
pub mod discoverer;
pub mod orchestrator;
pub mod selector;

// 重新导出常用类型
pub use config::{AnalyzerConfig, AnalyzerMapping, ConfigError};
pub use discoverer::{DiscovererError, FileDiscoverer, FileInfo};
pub use orchestrator::{
    AnalysisTask, WorkflowError, WorkflowOrchestrator, WorkflowResult, WorkflowStep,
};
pub use selector::PluginSelector;
