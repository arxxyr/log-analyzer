//! 事件处理

use anyhow::Result;
use crossterm::event::{self, Event as CrosstermEvent, KeyCode, KeyEvent, KeyModifiers};
use std::time::Duration;
use tokio::sync::mpsc;

/// TUI 事件
#[derive(Debug, Clone)]
pub enum Event {
    /// 键盘事件
    Key(KeyEvent),
    /// 鼠标事件
    Mouse,
    /// 调整大小
    Resize,
    /// Tick（定时刷新）
    Tick,
    /// 退出
    Quit,
}

/// 事件处理器
pub struct EventHandler {
    /// 事件接收器
    receiver: mpsc::UnboundedReceiver<Event>,
    /// 事件发送器（用于复制）
    _sender: mpsc::UnboundedSender<Event>,
    /// 处理句柄
    _handle: tokio::task::JoinHandle<()>,
}

impl EventHandler {
    /// 创建新的事件处理器
    pub fn new(tick_rate: Duration) -> Self {
        let (sender, receiver) = mpsc::unbounded_channel();
        let _sender = sender.clone();

        let _handle = tokio::spawn(async move {
            loop {
                // 等待 crossterm 事件
                if event::poll(tick_rate).unwrap_or(false) {
                    match event::read() {
                        Ok(CrosstermEvent::Key(key)) => {
                            // Ctrl+C, Ctrl+D, 或 q 退出
                            let is_ctrl_quit = key.modifiers.contains(KeyModifiers::CONTROL)
                                && matches!(key.code, KeyCode::Char('c') | KeyCode::Char('d'));
                            let is_q_quit = key.code == KeyCode::Char('q');

                            if is_ctrl_quit || is_q_quit {
                                let _ = sender.send(Event::Quit);
                            } else {
                                let _ = sender.send(Event::Key(key));
                            }
                        }
                        Ok(CrosstermEvent::Mouse(_)) => {
                            let _ = sender.send(Event::Mouse);
                        }
                        Ok(CrosstermEvent::Resize(_, _)) => {
                            let _ = sender.send(Event::Resize);
                        }
                        _ => {}
                    }
                } else {
                    // 超时，发送 Tick
                    let _ = sender.send(Event::Tick);
                }

                // 小延迟避免 CPU 占用过高
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        });

        Self {
            receiver,
            _sender,
            _handle,
        }
    }

    /// 异步接收下一个事件
    pub async fn next(&mut self) -> Result<Event> {
        self.receiver
            .recv()
            .await
            .ok_or_else(|| anyhow::anyhow!("事件通道已关闭"))
    }
}
