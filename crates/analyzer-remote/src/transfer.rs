//! 文件传输模块
//!
//! 提供 SCP/SFTP 文件传输功能，支持进度回调

use crate::ssh::{RemoteFileInfo, SshConnection, SshError};
use indicatif::{ProgressBar, ProgressStyle};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

pub type Result<T> = std::result::Result<T, SshError>;

/// 传输进度回调 trait
pub trait TransferProgress: Send {
    /// 进度更新
    fn on_progress(&mut self, bytes_transferred: u64, total_bytes: u64);

    /// 传输完成
    fn on_complete(&mut self);

    /// 传输失败
    fn on_error(&mut self, error: &str);
}

/// 简单的进度条实现
pub struct ProgressBarCallback {
    bar: ProgressBar,
}

impl ProgressBarCallback {
    pub fn new(total_bytes: u64, file_name: &str) -> Self {
        let bar = ProgressBar::new(total_bytes);
        bar.set_style(
            ProgressStyle::default_bar()
                .template("{msg} [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})")
                .unwrap()
                .progress_chars("#>-"),
        );
        bar.set_message(format!("下载 {}", file_name));

        Self { bar }
    }
}

impl TransferProgress for ProgressBarCallback {
    fn on_progress(&mut self, bytes_transferred: u64, _total_bytes: u64) {
        self.bar.set_position(bytes_transferred);
    }

    fn on_complete(&mut self) {
        self.bar.finish_with_message("下载完成");
    }

    fn on_error(&mut self, error: &str) {
        self.bar
            .abandon_with_message(format!("下载失败: {}", error));
    }
}

/// 文件传输器
pub struct FileTransfer {
    connection: SshConnection,
}

impl FileTransfer {
    /// 创建文件传输器
    pub fn new(connection: SshConnection) -> Self {
        Self { connection }
    }

    /// 下载单个文件（带自定义进度回调）
    pub fn download_with_callback<P: TransferProgress>(
        &mut self,
        remote_path: &Path,
        local_path: &Path,
        mut progress_callback: Option<P>,
    ) -> Result<()> {
        info!("下载文件: {:?} -> {:?}", remote_path, local_path);

        // 确保本地目录存在
        if let Some(parent) = local_path.parent() {
            fs::create_dir_all(parent)?;
        }

        // 获取 SFTP session
        let sftp = self.connection.sftp()?;

        // 获取远程文件信息
        let remote_stat = sftp.stat(remote_path).map_err(|e| {
            SshError::IoError(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("无法获取远程文件信息: {}", e),
            ))
        })?;

        let file_size = remote_stat.size.unwrap_or(0);

        debug!("远程文件大小: {} 字节", file_size);

        // 打开远程文件
        let mut remote_file = sftp.open(remote_path).map_err(|e| {
            SshError::IoError(std::io::Error::other(format!("打开远程文件失败: {}", e)))
        })?;

        // 创建本地文件
        let mut local_file = File::create(local_path)?;

        // 传输文件
        let mut buffer = vec![0u8; 8192]; // 8KB 缓冲区
        let mut total_bytes = 0u64;

        loop {
            match remote_file.read(&mut buffer) {
                Ok(0) => break, // EOF
                Ok(n) => {
                    local_file.write_all(&buffer[..n])?;
                    total_bytes += n as u64;

                    if let Some(ref mut callback) = progress_callback {
                        callback.on_progress(total_bytes, file_size);
                    }
                }
                Err(e) => {
                    if let Some(ref mut callback) = progress_callback {
                        callback.on_error(&e.to_string());
                    }
                    return Err(SshError::IoError(e));
                }
            }
        }

        if let Some(ref mut callback) = progress_callback {
            callback.on_complete();
        }

        info!("文件下载完成: {} 字节", total_bytes);
        Ok(())
    }

    /// 下载单个文件（使用默认进度条）
    pub fn download(
        &mut self,
        remote_path: &Path,
        local_path: &Path,
        show_progress: bool,
    ) -> Result<()> {
        if show_progress {
            let file_name = remote_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown");

            // 获取文件大小（用于创建进度条）
            let sftp = self.connection.sftp()?;
            let file_size = sftp
                .stat(remote_path)
                .map_err(|e| {
                    SshError::IoError(std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        format!("无法获取远程文件信息: {}", e),
                    ))
                })?
                .size
                .unwrap_or(0);

            let progress = ProgressBarCallback::new(file_size, file_name);
            self.download_with_callback(remote_path, local_path, Some(progress))
        } else {
            self.download_with_callback::<ProgressBarCallback>(remote_path, local_path, None)
        }
    }

    /// 批量下载文件
    pub fn download_batch(
        &mut self,
        files: Vec<(PathBuf, PathBuf)>,
        show_progress: bool,
    ) -> Result<Vec<std::result::Result<PathBuf, String>>> {
        info!("批量下载 {} 个文件", files.len());

        let mut results = Vec::new();

        for (remote_path, local_path) in files {
            match self.download(&remote_path, &local_path, show_progress) {
                Ok(_) => {
                    results.push(Ok(local_path));
                }
                Err(e) => {
                    warn!("下载失败: {:?} - {}", remote_path, e);
                    results.push(Err(format!("{:?}: {}", remote_path, e)));
                }
            }
        }

        Ok(results)
    }

    /// 下载远程文件信息对象
    pub fn download_file_info(
        &mut self,
        file_info: &RemoteFileInfo,
        local_dir: &Path,
        show_progress: bool,
    ) -> Result<PathBuf> {
        let local_path = local_dir.join(&file_info.name);
        self.download(&file_info.path, &local_path, show_progress)?;
        Ok(local_path)
    }

    /// 上传文件（可选功能，用于回传结果）
    pub fn upload(&mut self, local_path: &Path, remote_path: &Path) -> Result<()> {
        info!("上传文件: {:?} -> {:?}", local_path, remote_path);

        let sftp = self.connection.sftp()?;

        // 打开本地文件
        let mut local_file = File::open(local_path)?;

        // 创建远程文件
        let mut remote_file = sftp.create(remote_path).map_err(|e| {
            SshError::IoError(std::io::Error::other(format!("创建远程文件失败: {}", e)))
        })?;

        // 传输
        let mut buffer = vec![0u8; 8192];
        let mut total_bytes = 0u64;

        loop {
            match local_file.read(&mut buffer) {
                Ok(0) => break,
                Ok(n) => {
                    remote_file.write_all(&buffer[..n])?;
                    total_bytes += n as u64;
                }
                Err(e) => return Err(SshError::IoError(e)),
            }
        }

        info!("文件上传完成: {} 字节", total_bytes);
        Ok(())
    }

    /// 消耗 self，返回底层连接
    pub fn into_connection(self) -> SshConnection {
        self.connection
    }

    /// 获取连接的可变引用
    pub fn connection_mut(&mut self) -> &mut SshConnection {
        &mut self.connection
    }
}
