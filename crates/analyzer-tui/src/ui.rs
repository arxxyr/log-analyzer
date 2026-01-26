//! UI 渲染逻辑

use crate::state::{AppState, FocusArea, WorkflowPhase};
use crate::widgets::{log_viewer, progress_bar, status_bar};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
};

/// 渲染主界面
pub fn render(frame: &mut Frame, state: &AppState, scroll_offset: u16) {
    let size = frame.area();

    // 主布局：标题栏 | 内容区 | 状态栏
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // 标题栏
            Constraint::Min(0),    // 内容区
            Constraint::Length(3), // 状态栏
        ])
        .split(size);

    // 渲染标题栏
    render_header(frame, chunks[0], state);

    // 内容区分为左右两部分：主内容 | 插件面板
    let content_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(40),    // 主内容区（最小40列）
            Constraint::Length(35), // 插件面板（固定35列）
        ])
        .split(chunks[1]);

    // 渲染主内容区
    render_content(frame, content_chunks[0], state, scroll_offset);

    // 渲染插件面板
    render_plugin_panel(frame, content_chunks[1], state);

    // 渲染状态栏
    status_bar::render(frame, chunks[2], state);
}

/// 渲染标题栏
fn render_header(frame: &mut Frame, area: Rect, state: &AppState) {
    let phase = state.phase();
    let phase_str = phase.as_str();

    let (phase_color, phase_symbol) = match phase {
        WorkflowPhase::Initializing => (Color::Yellow, "⏳"),
        WorkflowPhase::Connecting => (Color::Cyan, "🔗"),
        WorkflowPhase::Discovering => (Color::Blue, "🔍"),
        WorkflowPhase::Downloading => (Color::Magenta, "⬇️"),
        WorkflowPhase::SelectingPlugin => (Color::LightBlue, "🔌"),
        WorkflowPhase::Analyzing => (Color::Green, "⚙️"),
        WorkflowPhase::Completed => (Color::LightGreen, "✅"),
        WorkflowPhase::Error => (Color::Red, "❌"),
    };

    let title = vec![
        Span::styled(
            " 📊 日志分析器 TUI ",
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" | "),
        Span::raw(phase_symbol),
        Span::raw(" "),
        Span::styled(
            phase_str,
            Style::default()
                .fg(phase_color)
                .add_modifier(Modifier::BOLD),
        ),
    ];

    let header = Paragraph::new(Line::from(title))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan)),
        )
        .alignment(Alignment::Left);

    frame.render_widget(header, area);
}

/// 渲染内容区
fn render_content(frame: &mut Frame, area: Rect, state: &AppState, scroll_offset: u16) {
    let phase = state.phase();

    // 内容区分为上下两部分：信息面板 | 日志窗口
    // 信息面板高度根据插件数量动态调整
    let plugins_count = state.available_plugins().len();
    let info_panel_height = if plugins_count > 0 {
        // 基础4行 + 分隔1行 + 标题1行 + 插件行（最多4行：3个插件 + 省略）
        (4 + 2 + plugins_count.min(3) + if plugins_count > 3 { 1 } else { 0 }) as u16
    } else {
        8 // 没有插件时使用默认高度
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(info_panel_height), // 信息面板（动态高度）
            Constraint::Min(0),                    // 日志窗口
        ])
        .split(area);

    // 渲染信息面板
    render_info_panel(frame, chunks[0], state);

    // 渲染日志窗口
    log_viewer::render(frame, chunks[1], state, scroll_offset);

    // 如果有进度，在信息面板下方渲染进度条
    if let Some(progress) = state.progress() {
        let progress_area = Rect {
            x: chunks[0].x + 2,
            y: chunks[0].y + chunks[0].height - 2,
            width: chunks[0].width.saturating_sub(4),
            height: 1,
        };
        progress_bar::render(frame, progress_area, &progress);
    }

    // 如果是错误状态，显示错误信息
    if phase == WorkflowPhase::Error
        && let Some(error_msg) = state.error_message()
    {
        render_error_popup(frame, area, &error_msg);
    }
}

/// 渲染信息面板
fn render_info_panel(frame: &mut Frame, area: Rect, state: &AppState) {
    let mut lines = vec![];

    // 当前文件
    if let Some(file) = state.current_file() {
        lines.push(Line::from(vec![
            Span::styled("📄 文件: ", Style::default().fg(Color::Cyan)),
            Span::raw(file),
        ]));
    } else {
        lines.push(Line::from(vec![
            Span::styled("📄 文件: ", Style::default().fg(Color::Cyan)),
            Span::styled("(未指定)", Style::default().fg(Color::DarkGray)),
        ]));
    }

    // 当前插件
    if let Some(plugin) = state.current_plugin() {
        lines.push(Line::from(vec![
            Span::styled("🔌 插件: ", Style::default().fg(Color::Cyan)),
            Span::raw(plugin),
        ]));
    } else {
        lines.push(Line::from(vec![
            Span::styled("🔌 插件: ", Style::default().fg(Color::Cyan)),
            Span::styled("(未选择)", Style::default().fg(Color::DarkGray)),
        ]));
    }

    // 运行时间
    let elapsed = state.elapsed_seconds();
    let time_str = format_duration(elapsed);
    lines.push(Line::from(vec![
        Span::styled("⏱️  用时: ", Style::default().fg(Color::Cyan)),
        Span::raw(time_str),
    ]));

    // 暂停状态
    if state.is_paused() {
        lines.push(Line::from(vec![
            Span::styled("⏸️  ", Style::default().fg(Color::Yellow)),
            Span::styled("已暂停", Style::default().fg(Color::Yellow)),
        ]));
    }

    // 可用插件列表
    let plugins = state.available_plugins();
    if !plugins.is_empty() {
        lines.push(Line::from("")); // 空行分隔
        lines.push(Line::from(vec![
            Span::styled(
                "🔧 可用插件 ",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("({})", plugins.len()),
                Style::default().fg(Color::DarkGray),
            ),
        ]));

        // 显示前 3 个插件
        for plugin in plugins.iter().take(3) {
            let status_icon = if plugin.enabled { "✓" } else { "✗" };
            let status_color = if plugin.enabled {
                Color::Green
            } else {
                Color::DarkGray
            };

            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(status_icon, Style::default().fg(status_color)),
                Span::raw(" "),
                Span::styled(&plugin.name, Style::default().fg(Color::White)),
                Span::raw(" "),
                Span::styled(
                    format!("v{}", plugin.version),
                    Style::default().fg(Color::DarkGray),
                ),
            ]));
        }

        // 如果有更多插件，显示省略号
        if plugins.len() > 3 {
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    format!("... 还有 {} 个插件", plugins.len() - 3),
                    Style::default().fg(Color::DarkGray),
                ),
            ]));
        }
    }

    let panel = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Gray))
                .title(" 信息 "),
        )
        .alignment(Alignment::Left);

    frame.render_widget(panel, area);
}

/// 渲染错误弹窗
fn render_error_popup(frame: &mut Frame, area: Rect, error_msg: &str) {
    let popup_width = area.width.saturating_sub(10).min(80);
    let popup_height = 10;
    let popup_x = (area.width.saturating_sub(popup_width)) / 2;
    let popup_y = (area.height.saturating_sub(popup_height)) / 2;

    let popup_area = Rect {
        x: popup_x,
        y: popup_y,
        width: popup_width,
        height: popup_height,
    };

    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            "发生错误:",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::raw(error_msg)),
        Line::from(""),
        Line::from(Span::styled(
            "按 q 或 ESC 退出",
            Style::default().fg(Color::DarkGray),
        )),
    ];

    let popup = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Red))
                .title(" 错误 "),
        )
        .alignment(Alignment::Center);

    frame.render_widget(popup, popup_area);
}

/// 格式化时长
fn format_duration(seconds: i64) -> String {
    if seconds < 60 {
        format!("{}秒", seconds)
    } else if seconds < 3600 {
        format!("{}分{}秒", seconds / 60, seconds % 60)
    } else {
        format!("{}时{}分", seconds / 3600, (seconds % 3600) / 60)
    }
}

/// 渲染插件面板（右侧）
fn render_plugin_panel(frame: &mut Frame, area: Rect, state: &AppState) {
    let plugins = state.available_plugins();
    let selected_index = state.selected_plugin_index();
    let is_focused = state.focus() == FocusArea::PluginPanel;

    // 边框样式根据焦点状态变化
    let border_style = if is_focused {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    // 分为两个区域：插件列表 | 帮助提示
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(5),    // 插件列表
            Constraint::Length(6), // 帮助提示
        ])
        .split(area);

    // 渲染插件列表
    let items: Vec<ListItem> = plugins
        .iter()
        .enumerate()
        .map(|(i, plugin)| {
            let is_selected = i == selected_index && is_focused;
            let checkbox = if plugin.enabled {
                if plugin.required {
                    "✓*" // 必需
                } else {
                    "✓"
                }
            } else {
                "·"
            };

            let style = if is_selected {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else if plugin.enabled {
                Style::default().fg(Color::Green)
            } else {
                Style::default().fg(Color::DarkGray)
            };

            // 紧凑显示：只显示名称和状态
            let name = if plugin.name.len() > 28 {
                format!("{}...", &plugin.name[..25])
            } else {
                plugin.name.clone()
            };
            let content = format!("{} {}", checkbox, name);

            ListItem::new(content).style(style)
        })
        .collect();

    let enabled_count = plugins.iter().filter(|p| p.enabled).count();
    let title = if is_focused {
        format!(" 🔧 插件 ({}/{}) [活动] ", enabled_count, plugins.len())
    } else {
        format!(" 🔧 插件 ({}/{}) ", enabled_count, plugins.len())
    };

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(border_style)
            .title(title),
    );

    frame.render_widget(list, chunks[0]);

    // 渲染帮助提示
    let help_lines = if is_focused {
        vec![
            Line::from(vec![
                Span::styled("↑/↓", Style::default().fg(Color::Yellow)),
                Span::raw(" 选择"),
            ]),
            Line::from(vec![
                Span::styled("空格", Style::default().fg(Color::Yellow)),
                Span::raw(" 切换"),
            ]),
            Line::from(vec![
                Span::styled("Enter", Style::default().fg(Color::Green)),
                Span::raw(" 重启"),
            ]),
            Line::from(vec![
                Span::styled("Tab", Style::default().fg(Color::Cyan)),
                Span::raw(" 返回主区"),
            ]),
        ]
    } else {
        vec![
            Line::from(vec![
                Span::styled("Tab", Style::default().fg(Color::Cyan)),
                Span::raw(" 切换到插件面板"),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("*", Style::default().fg(Color::Yellow)),
                Span::raw(" = 必需插件"),
            ]),
        ]
    };

    let help = Paragraph::new(help_lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray))
            .title(" 快捷键 "),
    );

    frame.render_widget(help, chunks[1]);
}
