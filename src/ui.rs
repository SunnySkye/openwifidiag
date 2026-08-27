use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState};
use ratatui::Frame;

use crate::app::App;

const SPINNER: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

pub fn draw(f: &mut Frame, app: &mut App) {
    let [header, table_area, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(2),
    ])
    .areas(f.area());

    render_header(f, app, header);
    render_table(f, app, table_area);
    render_footer(f, app, footer);
}

fn render_header(f: &mut Frame, app: &App, area: Rect) {
    let status = SPINNER[app.spinner_tick % SPINNER.len()];
    let right = if app.scanning {
        format!("{} scanning… ", status)
    } else {
        match app.countdown_secs() {
            Some(secs) => format!("refresh in {}s ", secs),
            None => String::new(),
        }
    };
    let left = format!(
        " openwifidiag • backend: {} • {} networks • sort: {} ",
        app.backend,
        app.networks.len(),
        app.sort.label()
    );
    let pad_chars = area.width as i32 - left.len() as i32 - right.len() as i32;
    let line = Line::from(vec![
        Span::styled(left, Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Span::raw(if pad_chars > 0 { " ".repeat(pad_chars as usize) } else { String::new() }),
        Span::styled(right, Style::default().fg(Color::Yellow)),
    ]);
    f.render_widget(Paragraph::new(line), area);
}

fn render_table(f: &mut Frame, app: &mut App, area: Rect) {
    let header_cells = ["SSID", "SIGNAL", "CH", "BAND", "BSSID", "SECURITY"]
        .iter()
        .map(|h| Cell::from(*h).style(Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD)));
    let header = Row::new(header_cells);

    let rows = app.networks.iter().map(|net| {
        let (bar, color) = signal_bar(net.rssi);
        let sec = security_cell(net.security.label());
        Row::new(vec![
            Cell::from(net.ssid.clone()),
            Cell::from(Line::from(vec![
                Span::styled(bar, Style::default().fg(color)),
                Span::raw(format!(" {:>4} dBm", net.rssi)),
            ])),
            Cell::from(net.channel.map(|c| c.to_string()).unwrap_or_else(|| "?".into())),
            Cell::from(net.band.label()),
            Cell::from(net.bssid.clone()),
            sec,
        ])
    });

    let widths = [
        Constraint::Min(20),
        Constraint::Length(11),
        Constraint::Length(5),
        Constraint::Length(5),
        Constraint::Length(19),
        Constraint::Length(10),
    ];

    let mut state = TableState::default();
    if !app.networks.is_empty() {
        state.select(Some(app.selected));
    }

    let table = Table::new(rows, widths)
        .header(header)
        .block(Block::default().borders(Borders::TOP))
        .row_highlight_style(Style::default().add_modifier(Modifier::REVERSED));

    f.render_stateful_widget(table, area, &mut state);
}

fn render_footer(f: &mut Frame, app: &App, area: Rect) {
    let keys = " q quit │ r refresh │ s sort │ ↑/↓/j/k navigate │ g top │ G bottom ";
    let text = match (&app.last_error, &app.advisory) {
        (Some(e), _) => Line::from(vec![
            Span::styled(keys, Style::default().fg(Color::DarkGray)),
            Span::raw("\n"),
            Span::styled(format!(" ⚠ {}", e), Style::default().fg(Color::Red)),
        ]),
        (None, Some(hint)) => Line::from(vec![
            Span::styled(keys, Style::default().fg(Color::DarkGray)),
            Span::raw("\n"),
            Span::styled(format!(" ⓘ {}", hint), Style::default().fg(Color::Yellow)),
        ]),
        (None, None) => Line::from(Span::styled(keys, Style::default().fg(Color::DarkGray))),
    };
    f.render_widget(Paragraph::new(text), area);
}

fn signal_bar(rssi: i32) -> (String, Color) {
    const LEVELS: &[char] = &['▁', '▂', '▃', '▄', '▅', '▆'];
    let clamped = (rssi.max(-90).min(-30) - (-90)) as usize; // 0..60
    let level = (clamped / 12).min(5); // 0..5
    let color = match rssi {
        r if r >= -50 => Color::Green,
        r if r >= -65 => Color::Yellow,
        r if r >= -75 => Color::LightRed,
        _ => Color::Red,
    };
    (LEVELS.iter().take(level + 1).collect(), color)
}

fn security_cell(label: &str) -> Cell<'static> {
    let color = match label {
        "Open" => Color::Red,
        "WEP" => Color::LightRed,
        "WPA" | "Encrypted" => Color::Yellow,
        "WPA2" => Color::Green,
        "WPA3" => Color::LightGreen,
        _ => Color::Gray,
    };
    Cell::from(Span::styled(label.to_owned(), Style::default().fg(color)))
}
