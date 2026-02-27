//! 文件发现模块
//!
//! 提供本地和远程文件发现、排序和选择功能

use crate::config::{AutoSelect, FileDiscoveryConfig, SortBy, SortOrder};
use analyzer_remote::{RemoteFileInfo, SshConnection};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use thiserror::Error;
use tracing::{debug, info, warn};

/// 文件发现错误
#[derive(Error, Debug)]
pub enum DiscovererError {
    #[error("目录不存在: {0}")]
    DirectoryNotFound(String),

    #[error("没有找到匹配的文件")]
    NoFilesFound,

    #[error("远程操作失败: {0}")]
    RemoteError(String),

    #[error("IO 错误: {0}")]
    IoError(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, DiscovererError>;

/// 统一的文件信息（本地和远程）
#[derive(Debug, Clone)]
pub struct FileInfo {
    pub path: PathBuf,
    pub name: String,
    pub size: u64,
    pub mtime: u64,
    pub is_remote: bool,
}

impl FileInfo {
    /// 从本地文件创建
    pub fn from_local_path<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        let metadata = fs::metadata(path)?;

        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();

        let size = metadata.len();
        let mtime = metadata
            .modified()?
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Ok(Self {
            path: path.to_path_buf(),
            name,
            size,
            mtime,
            is_remote: false,
        })
    }

    /// 从远程文件信息创建
    pub fn from_remote_file_info(info: RemoteFileInfo) -> Self {
        Self {
            path: info.path,
            name: info.name,
            size: info.size,
            mtime: info.mtime,
            is_remote: true,
        }
    }
}

/// 文件发现器
pub struct FileDiscoverer {
    config: FileDiscoveryConfig,
}

impl FileDiscoverer {
    /// 创建文件发现器
    pub fn new(config: FileDiscoveryConfig) -> Self {
        Self { config }
    }

    /// 发现本地文件
    pub fn discover_local<P: AsRef<Path>>(&self, dir: P, pattern: &str) -> Result<Vec<FileInfo>> {
        let dir = dir.as_ref();

        if !dir.exists() {
            return Err(DiscovererError::DirectoryNotFound(
                dir.display().to_string(),
            ));
        }

        info!("搜索本地文件: {} 模式: {}", dir.display(), pattern);

        // 使用 glob 匹配
        let glob_pattern = dir.join(pattern);
        let glob_str = glob_pattern.to_str().ok_or_else(|| {
            DiscovererError::IoError(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "无效的路径",
            ))
        })?;

        let mut files = Vec::new();

        match glob::glob(glob_str) {
            Ok(paths) => {
                for entry in paths.flatten() {
                    if entry.is_file() {
                        match FileInfo::from_local_path(&entry) {
                            Ok(info) => files.push(info),
                            Err(e) => warn!("读取文件信息失败: {:?} - {}", entry, e),
                        }
                    }
                }
            }
            Err(e) => {
                warn!("Glob 模式错误: {}", e);
            }
        }

        if files.is_empty() {
            debug!("没有找到匹配的本地文件");
            return Err(DiscovererError::NoFilesFound);
        }

        info!("找到 {} 个本地文件", files.len());
        Ok(files)
    }

    /// 发现远程文件
    pub fn discover_remote(
        &self,
        connection: &mut SshConnection,
        remote_dir: &str,
        pattern: &str,
    ) -> Result<Vec<FileInfo>> {
        info!("搜索远程文件: {} 模式: {}", remote_dir, pattern);

        let remote_files = connection
            .list_files(remote_dir, pattern)
            .map_err(|e| DiscovererError::RemoteError(e.to_string()))?;

        if remote_files.is_empty() {
            debug!("没有找到匹配的远程文件");
            return Err(DiscovererError::NoFilesFound);
        }

        let files: Vec<FileInfo> = remote_files
            .into_iter()
            .map(FileInfo::from_remote_file_info)
            .collect();

        info!("找到 {} 个远程文件", files.len());
        Ok(files)
    }

    /// 根据配置排序文件
    pub fn sort_files(&self, files: &mut [FileInfo]) {
        match self.config.sort_by {
            SortBy::Mtime => {
                files.sort_by_key(|f| f.mtime);
            }
            SortBy::Name => {
                files.sort_by(|a, b| a.name.cmp(&b.name));
            }
            SortBy::Size => {
                files.sort_by_key(|f| f.size);
            }
        }

        if self.config.sort_order == SortOrder::Desc {
            files.reverse();
        }

        debug!(
            "文件排序: {:?} {:?}",
            self.config.sort_by, self.config.sort_order
        );
    }

    /// 自动选择文件
    pub fn auto_select(&self, mut files: Vec<FileInfo>) -> Option<FileInfo> {
        if files.is_empty() {
            return None;
        }

        // 先排序
        self.sort_files(&mut files);

        match self.config.auto_select {
            AutoSelect::Latest => {
                // 如果按 mtime desc 排序，第一个就是最新的
                if self.config.sort_by == SortBy::Mtime && self.config.sort_order == SortOrder::Desc
                {
                    Some(files.into_iter().next().unwrap())
                } else {
                    // 手动查找最新的
                    files.into_iter().max_by_key(|f| f.mtime)
                }
            }
            AutoSelect::Oldest => {
                // 找最旧的
                files.into_iter().min_by_key(|f| f.mtime)
            }
            AutoSelect::None => None,
        }
    }

    /// 选择排序后的前 N 个文件（不足 N 则返回全部）
    pub fn select_top_n(&self, mut files: Vec<FileInfo>, n: usize) -> Vec<FileInfo> {
        if files.is_empty() || n == 0 {
            return Vec::new();
        }
        self.sort_files(&mut files);
        files.truncate(n);
        files
    }

    /// 发现并选择前 N 个文件（远程）
    pub fn discover_and_select_top_n_remote(
        &self,
        connection: &mut SshConnection,
        remote_dir: &str,
        pattern: &str,
        n: usize,
    ) -> Result<Vec<FileInfo>> {
        let files = self.discover_remote(connection, remote_dir, pattern)?;
        let selected = self.select_top_n(files, n);
        if selected.is_empty() {
            return Err(DiscovererError::NoFilesFound);
        }
        Ok(selected)
    }

    /// 发现并选择前 N 个文件（本地）
    pub fn discover_and_select_top_n_local<P: AsRef<Path>>(
        &self,
        dir: P,
        pattern: &str,
        n: usize,
    ) -> Result<Vec<FileInfo>> {
        let files = self.discover_local(dir, pattern)?;
        let selected = self.select_top_n(files, n);
        if selected.is_empty() {
            return Err(DiscovererError::NoFilesFound);
        }
        Ok(selected)
    }

    /// 发现并自动选择文件（本地）
    pub fn discover_and_select_local<P: AsRef<Path>>(
        &self,
        dir: P,
        pattern: &str,
    ) -> Result<FileInfo> {
        let files = self.discover_local(dir, pattern)?;
        self.auto_select(files).ok_or(DiscovererError::NoFilesFound)
    }

    /// 发现并自动选择文件（远程）
    pub fn discover_and_select_remote(
        &self,
        connection: &mut SshConnection,
        remote_dir: &str,
        pattern: &str,
    ) -> Result<FileInfo> {
        let files = self.discover_remote(connection, remote_dir, pattern)?;
        self.auto_select(files).ok_or(DiscovererError::NoFilesFound)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sort_by_mtime() {
        let config = FileDiscoveryConfig {
            sort_by: SortBy::Mtime,
            sort_order: SortOrder::Desc,
            auto_select: AutoSelect::Latest,
        };

        let discoverer = FileDiscoverer::new(config);

        let mut files = vec![
            FileInfo {
                path: PathBuf::from("file1.log"),
                name: "file1.log".to_string(),
                size: 100,
                mtime: 1000,
                is_remote: false,
            },
            FileInfo {
                path: PathBuf::from("file2.log"),
                name: "file2.log".to_string(),
                size: 200,
                mtime: 2000,
                is_remote: false,
            },
            FileInfo {
                path: PathBuf::from("file3.log"),
                name: "file3.log".to_string(),
                size: 150,
                mtime: 1500,
                is_remote: false,
            },
        ];

        discoverer.sort_files(&mut files);

        assert_eq!(files[0].name, "file2.log");
        assert_eq!(files[1].name, "file3.log");
        assert_eq!(files[2].name, "file1.log");
    }

    #[test]
    fn test_auto_select_latest() {
        let config = FileDiscoveryConfig {
            sort_by: SortBy::Mtime,
            sort_order: SortOrder::Desc,
            auto_select: AutoSelect::Latest,
        };

        let discoverer = FileDiscoverer::new(config);

        let files = vec![
            FileInfo {
                path: PathBuf::from("file1.log"),
                name: "file1.log".to_string(),
                size: 100,
                mtime: 1000,
                is_remote: false,
            },
            FileInfo {
                path: PathBuf::from("file2.log"),
                name: "file2.log".to_string(),
                size: 200,
                mtime: 2000,
                is_remote: false,
            },
        ];

        let selected = discoverer.auto_select(files).unwrap();
        assert_eq!(selected.name, "file2.log");
    }
}
