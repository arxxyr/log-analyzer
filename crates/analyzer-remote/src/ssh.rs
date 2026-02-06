//! SSH 连接管理模块
//!
//! 提供 SSH 连接、认证和远程命令执行功能

use serde::{Deserialize, Serialize};
use std::io::Read;
use std::net::TcpStream;
use std::path::PathBuf;
use std::time::Duration;
use thiserror::Error;
use tracing::{debug, info, warn};

/// 对 shell 参数进行转义，防止命令注入
///
/// 将字符串用单引号包裹，并对内部的单引号进行转义：
/// `it's` → `'it'\''s'`
fn shell_escape(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// 验证 glob 模式是否安全（不含命令注入字符）
///
/// 允许的字符：字母、数字、`_`、`-`、`.`、`*`、`?`、`[`、`]`
fn validate_glob_pattern(pattern: &str) -> std::result::Result<(), SshError> {
    if pattern.is_empty() {
        return Err(SshError::CommandFailed("glob 模式不能为空".to_string()));
    }
    for ch in pattern.chars() {
        if !(ch.is_alphanumeric() || matches!(ch, '_' | '-' | '.' | '*' | '?' | '[' | ']')) {
            return Err(SshError::CommandFailed(format!(
                "glob 模式包含不安全字符: '{ch}'"
            )));
        }
    }
    Ok(())
}

/// SSH 错误类型
#[derive(Error, Debug)]
pub enum SshError {
    #[error("连接失败: {0}")]
    ConnectionFailed(String),

    #[error("认证失败: {0}")]
    AuthenticationFailed(String),

    #[error("命令执行失败: {0}")]
    CommandFailed(String),

    #[error("IO 错误: {0}")]
    IoError(#[from] std::io::Error),

    #[error("SSH 错误: {0}")]
    Ssh2Error(#[from] ssh2::Error),
}

pub type Result<T> = std::result::Result<T, SshError>;

/// 认证方式
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum AuthMethod {
    /// SSH 密钥文件
    #[serde(rename = "key_file")]
    KeyFile {
        path: PathBuf,
        passphrase: Option<String>,
    },

    /// SSH Agent
    #[serde(rename = "agent")]
    Agent,

    /// 密码认证
    #[serde(rename = "password")]
    Password { password: String },

    /// 交互式（运行时输入）
    #[serde(rename = "interactive")]
    Interactive,
}

/// 超时配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Timeouts {
    /// 连接超时（秒）
    #[serde(default = "default_connect_timeout")]
    pub connect: u64,

    /// 传输超时（秒）
    #[serde(default = "default_transfer_timeout")]
    pub transfer: u64,
}

fn default_connect_timeout() -> u64 {
    30
}

fn default_transfer_timeout() -> u64 {
    300
}

impl Default for Timeouts {
    fn default() -> Self {
        Self {
            connect: 30,
            transfer: 300,
        }
    }
}

/// SSH 连接配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SshConfig {
    /// 主机地址
    pub host: String,

    /// 端口
    #[serde(default = "default_port")]
    pub port: u16,

    /// 用户名
    pub user: String,

    /// 认证方式
    pub auth: AuthMethod,

    /// 超时配置
    #[serde(default)]
    pub timeouts: Timeouts,
}

fn default_port() -> u16 {
    22
}

/// 远程文件信息
#[derive(Debug, Clone)]
pub struct RemoteFileInfo {
    pub path: PathBuf,
    pub name: String,
    pub size: u64,
    pub mtime: u64,
}

/// SSH 连接管理器
pub struct SshConnection {
    session: ssh2::Session,
    config: SshConfig,
}

impl SshConnection {
    /// 建立 SSH 连接
    pub fn connect(config: SshConfig) -> Result<Self> {
        info!("连接到 {}@{}:{}", config.user, config.host, config.port);

        // 建立 TCP 连接
        let tcp = TcpStream::connect(format!("{}:{}", config.host, config.port))
            .map_err(|e| SshError::ConnectionFailed(format!("TCP 连接失败: {}", e)))?;

        // 设置超时
        tcp.set_read_timeout(Some(Duration::from_secs(config.timeouts.connect)))?;
        tcp.set_write_timeout(Some(Duration::from_secs(config.timeouts.connect)))?;

        // 创建 SSH session
        let mut session = ssh2::Session::new()
            .map_err(|e| SshError::ConnectionFailed(format!("创建 session 失败: {}", e)))?;

        session.set_tcp_stream(tcp);
        session
            .handshake()
            .map_err(|e| SshError::ConnectionFailed(format!("SSH 握手失败: {}", e)))?;

        debug!("SSH 握手成功");

        // 认证
        let mut conn = Self { session, config };
        conn.authenticate()?;

        info!("SSH 连接成功");
        Ok(conn)
    }

    /// 认证
    fn authenticate(&mut self) -> Result<()> {
        info!("开始认证");

        match &self.config.auth {
            AuthMethod::KeyFile { path, passphrase } => {
                debug!("使用密钥文件认证: {:?}", path);
                self.session
                    .userauth_pubkey_file(&self.config.user, None, path, passphrase.as_deref())
                    .map_err(|e| SshError::AuthenticationFailed(format!("密钥认证失败: {}", e)))?;
            }
            AuthMethod::Agent => {
                debug!("使用 SSH Agent 认证");
                let mut agent = self.session.agent().map_err(|e| {
                    SshError::AuthenticationFailed(format!("无法连接 SSH Agent: {}", e))
                })?;

                agent.connect().map_err(|e| {
                    SshError::AuthenticationFailed(format!("连接 SSH Agent 失败: {}", e))
                })?;

                agent
                    .list_identities()
                    .map_err(|e| SshError::AuthenticationFailed(format!("列出身份失败: {}", e)))?;

                let identities = agent
                    .identities()
                    .map_err(|e| SshError::AuthenticationFailed(format!("获取身份失败: {}", e)))?;

                let mut authenticated = false;
                for identity in identities {
                    if agent.userauth(&self.config.user, &identity).is_ok() {
                        authenticated = true;
                        break;
                    }
                }

                if !authenticated {
                    return Err(SshError::AuthenticationFailed(
                        "所有密钥认证失败".to_string(),
                    ));
                }
            }
            AuthMethod::Password { password } => {
                debug!("使用密码认证");
                self.session
                    .userauth_password(&self.config.user, password)
                    .map_err(|e| SshError::AuthenticationFailed(format!("密码认证失败: {}", e)))?;
            }
            AuthMethod::Interactive => {
                return Err(SshError::AuthenticationFailed(
                    "交互式认证暂不支持，请使用密钥或密码".to_string(),
                ));
            }
        }

        if !self.session.authenticated() {
            return Err(SshError::AuthenticationFailed("认证失败".to_string()));
        }

        info!("认证成功");
        Ok(())
    }

    /// 执行远程命令
    pub fn execute(&mut self, command: &str) -> Result<String> {
        debug!("执行命令: {}", command);

        let mut channel = self
            .session
            .channel_session()
            .map_err(|e| SshError::CommandFailed(format!("创建 channel 失败: {}", e)))?;

        channel
            .exec(command)
            .map_err(|e| SshError::CommandFailed(format!("执行命令失败: {}", e)))?;

        let mut output = String::new();
        channel
            .read_to_string(&mut output)
            .map_err(|e| SshError::CommandFailed(format!("读取输出失败: {}", e)))?;

        channel
            .wait_close()
            .map_err(|e| SshError::CommandFailed(format!("等待关闭失败: {}", e)))?;

        let exit_status = channel.exit_status()?;
        if exit_status != 0 {
            warn!("命令返回非零状态码: {}", exit_status);
        }

        debug!("命令执行完成，输出长度: {}", output.len());
        Ok(output)
    }

    /// 列出远程目录文件
    pub fn list_files(&mut self, path: &str, pattern: &str) -> Result<Vec<RemoteFileInfo>> {
        debug!("列出远程文件: {} 模式: {}", path, pattern);

        // 验证 glob 模式安全性，防止命令注入
        validate_glob_pattern(pattern)?;

        // 构造命令：列出文件并获取详细信息
        // path 使用 shell_escape 转义，pattern 已经通过安全验证
        let escaped_path = shell_escape(path);
        let command = format!(
            "cd {escaped_path} && ls -t {pattern} 2>/dev/null | while read f; do \
             if [ -f \"$f\" ]; then \
               stat -c '%n|%s|%Y' \"$f\" 2>/dev/null || stat -f '%N|%z|%m' \"$f\" 2>/dev/null; \
             fi; \
             done"
        );

        let output = self.execute(&command)?;

        let mut files = Vec::new();
        for line in output.lines() {
            if line.trim().is_empty() {
                continue;
            }

            let parts: Vec<&str> = line.split('|').collect();
            if parts.len() >= 3 {
                let name = parts[0].to_string();
                let size = parts[1].parse().unwrap_or(0);
                let mtime = parts[2].parse().unwrap_or(0);

                files.push(RemoteFileInfo {
                    path: PathBuf::from(path).join(&name),
                    name,
                    size,
                    mtime,
                });
            }
        }

        info!("找到 {} 个文件", files.len());
        Ok(files)
    }

    /// 查找最新的日期目录（格式如 20251226）
    /// 返回完整路径，如 /home/linux/.ros/log/20251226
    pub fn find_latest_date_dir(&mut self, base_path: &str) -> Result<Option<String>> {
        debug!("查找最新日期目录: {}", base_path);

        // 列出所有以数字开头的目录，按名称降序排列（最新日期在前）
        // 日期格式：YYYYMMDD（8位数字）
        // 使用 shell_escape 防止命令注入
        let escaped_base = shell_escape(base_path);
        let command = format!(
            "ls -1d {escaped_base}/[0-9][0-9][0-9][0-9][0-9][0-9][0-9][0-9] 2>/dev/null | sort -r | head -1"
        );

        let output = self.execute(&command)?;
        let dir = output.trim();

        if dir.is_empty() {
            info!("未找到日期目录");
            Ok(None)
        } else {
            info!("找到最新日期目录: {}", dir);
            Ok(Some(dir.to_string()))
        }
    }

    /// 获取 SFTP session
    pub fn sftp(&self) -> Result<ssh2::Sftp> {
        self.session
            .sftp()
            .map_err(|e| SshError::ConnectionFailed(format!("创建 SFTP session 失败: {}", e)))
    }

    /// 获取底层 session 的引用
    pub fn session(&self) -> &ssh2::Session {
        &self.session
    }

    /// 关闭连接
    pub fn disconnect(self) -> Result<()> {
        info!("断开 SSH 连接");
        self.session
            .disconnect(None, "正常断开", None)
            .map_err(|e| SshError::ConnectionFailed(format!("断开连接失败: {}", e)))?;
        Ok(())
    }
}

impl Drop for SshConnection {
    fn drop(&mut self) {
        // 尝试断开连接，忽略错误
        let _ = self.session.disconnect(None, "连接关闭", None);
    }
}
