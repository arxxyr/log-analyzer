//! 状态栏组件

use crate::state::AppState;
use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

/// 渲染状态栏
pub fn render(frame: &mut Frame, area: Rect, _state: &AppState) {
    let mut spans = vec![
        Span::styled(
            " 快捷键: ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
    ];

    // 快捷键提示
    let shortcuts = vec![
        ("Q", "退出"),
        ("P/Space", "暂停"),
        ("↑/↓", "滚动"),
        ("PgUp/PgDn", "翻页"),
        ("Home/End", "首尾"),
    ];

    for (i, (key, desc)) in shortcuts.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled(" | ", Style::default().fg(Color::DarkGray)));
        }
        spans.push(Span::styled(
            key.to_string(),
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::raw(" "));
        spans.push(Span::raw(desc.to_string()));
    }

    // 右侧显示版本信息
    spans.push(Span::raw(" ".repeat(5))); // 间隔
    spans.push(Span::styled(
        "v0.3.0 ",
        Style::default().fg(Color::DarkGray),
    ));

    let status_bar = Paragraph::new(Line::from(spans))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan)),
        )
        .alignment(Alignment::Left);

    frame.render_widget(status_bar, area);
}
