//! analyzer-remote - 远程连接和文件传输模块
//!
//! 提供 SSH 连接管理和 SCP/SFTP 文件传输功能

// i18n 初始化
rust_i18n::i18n!("locales", fallback = "zh-CN");

pub mod ssh;
pub mod transfer;

// 重新导出常用类型
pub use ssh::{AuthMethod, RemoteFileInfo, SshConfig, SshConnection, SshError, Timeouts};
pub use transfer::{FileTransfer, ProgressBarCallback, TransferProgress};
