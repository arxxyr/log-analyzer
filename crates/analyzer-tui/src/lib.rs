//! # Analyzer TUI
//!
//! 基于 ratatui 的终端用户界面模块
//! 提供交互式的实时日志分析界面

pub mod app;
pub mod events;
pub mod state;
pub mod ui;
pub mod widgets;

// 重新导出主要类型
pub use app::{App, AppResult};
pub use events::{Event, EventHandler};
pub use state::{AppState, WorkflowPhase};

use anyhow::Result;
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::io;

/// TUI 终端类型
pub type Tui = Terminal<CrosstermBackend<io::Stdout>>;

/// 初始化终端
pub fn init_terminal() -> Result<Tui> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let terminal = Terminal::new(backend)?;
    Ok(terminal)
}

/// 恢复终端
pub fn restore_terminal(terminal: &mut Tui) -> Result<()> {
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;
    Ok(())
}

/// 运行 TUI 应用
pub async fn run_tui(app: &mut App) -> Result<()> {
    let mut terminal = init_terminal()?;

    // 清空屏幕
    terminal.clear()?;

    // 主循环
    let res = app.run(&mut terminal).await;

    // 恢复终端
    restore_terminal(&mut terminal)?;

    res
}
