//! 应用主逻辑

use crate::{
    events::{Event, EventHandler},
    state::{AppState, FocusArea},
    ui,
};
use anyhow::Result;
use crossterm::event::KeyCode;
use ratatui::backend::Backend;
use ratatui::Terminal;
use std::time::Duration;

/// 应用结果类型
pub type AppResult<T> = Result<T>;

/// TUI 应用
pub struct App {
    /// 应用状态
    pub state: AppState,
    /// 是否正在运行
    running: bool,
    /// 事件处理器（在运行时创建）
    events: Option<EventHandler>,
    /// 滚动偏移（日志查看器）
    scroll_offset: u16,
}

impl App {
    /// 创建新的应用实例
    pub fn new(state: AppState) -> Self {
        Self {
            state,
            running: true,
            events: None,
            scroll_offset: 0,
        }
    }

    /// 运行应用主循环
    pub async fn run<B: Backend>(&mut self, terminal: &mut Terminal<B>) -> AppResult<()> {
        // 在 async 上下文中创建事件处理器
        if self.events.is_none() {
            self.events = Some(EventHandler::new(Duration::from_millis(250)));
        }

        while self.running {
            // 渲染界面
            terminal.draw(|frame| ui::render(frame, &self.state, self.scroll_offset))?;

            // 获取事件（分离借用）
            if let Some(events) = &mut self.events {
                let event = events.next().await?;
                // 处理事件
                self.handle_event(event)?;
            }
        }
        Ok(())
    }

    /// 处理事件
    fn handle_event(&mut self, event: Event) -> AppResult<()> {
        // 根据当前焦点区域处理事件
        let focus = self.state.focus();

        match event {
            Event::Quit => {
                self.quit();
            }
            Event::Key(key) => {
                // 全局快捷键
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => {
                        self.quit();
                        return Ok(());
                    }
                    KeyCode::Tab => {
                        // Tab 切换焦点
                        self.state.toggle_focus();
                        return Ok(());
                    }
                    _ => {}
                }

                // 根据焦点区域处理按键
                match focus {
                    FocusArea::Main => {
                        // 主区域（日志查看）
                        match key.code {
                            KeyCode::Char('p') | KeyCode::Char(' ') => {
                                // 暂停/恢复
                                self.state.toggle_pause();
                            }
                            KeyCode::Up => {
                                // 向上滚动日志
                                self.scroll_offset = self.scroll_offset.saturating_sub(1);
                            }
                            KeyCode::Down => {
                                // 向下滚动日志
                                self.scroll_offset = self.scroll_offset.saturating_add(1);
                            }
                            KeyCode::PageUp => {
                                // 向上翻页
                                self.scroll_offset = self.scroll_offset.saturating_sub(10);
                            }
                            KeyCode::PageDown => {
                                // 向下翻页
                                self.scroll_offset = self.scroll_offset.saturating_add(10);
                            }
                            KeyCode::Home => {
                                // 回到顶部
                                self.scroll_offset = 0;
                            }
                            _ => {}
                        }
                    }
                    FocusArea::PluginPanel => {
                        // 插件面板
                        match key.code {
                            KeyCode::Up => {
                                self.state.select_previous_plugin();
                            }
                            KeyCode::Down => {
                                self.state.select_next_plugin();
                            }
                            KeyCode::Char(' ') => {
                                self.state.toggle_selected_plugin();
                            }
                            KeyCode::Enter => {
                                // 请求重启工作流
                                self.state.request_restart();
                            }
                            _ => {}
                        }
                    }
                }
            }
            Event::Tick => {
                // 定时刷新（UI 会自动重绘）
            }
            Event::Resize => {
                // 窗口大小改变（ratatui 会自动处理）
            }
            Event::Mouse => {
                // 暂不处理鼠标事件
            }
        }
        Ok(())
    }

    /// 退出应用
    fn quit(&mut self) {
        self.running = false;
    }

    /// 获取滚动偏移
    pub fn scroll_offset(&self) -> u16 {
        self.scroll_offset
    }
}
