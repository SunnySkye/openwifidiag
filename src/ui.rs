use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Sparkline, Table, TableState};
use ratatui::Frame;

use crate::app::App;
use crate::diagnostics::{LiveDiagnostic, PROBE_TARGET};
use crate::stress::{STRESS_HOST, STRESS_WORKERS};

const SPINNER: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

pub fn draw(f: &mut Frame, app: &mut App) {
    if let Some(diagnostic) = &app.diagnostic {
        render_diagnostic(f, diagnostic, app.scanning, app.spinner_tick);
        return;
    }

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
    const FIXED_COLUMN_WIDTH: u16 = 5 + 5 + 19 + 10;
    const COLUMN_SPACING: u16 = 5;
    const SIGNAL_LABEL_WIDTH: u16 = 9; // Leading space, RSSI, and " dBm".

    let flexible_width = area
        .width
        .saturating_sub(FIXED_COLUMN_WIDTH + COLUMN_SPACING);
    let signal_width = flexible_width / 2;
    let signal_bar_width = signal_width.saturating_sub(SIGNAL_LABEL_WIDTH).max(1) as usize;

    let header_cells = ["SSID", "SIGNAL", "CH", "BAND", "BSSID", "SECURITY"]
        .iter()
        .map(|h| Cell::from(*h).style(Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD)));
    let header = Row::new(header_cells);

    let rows = app.networks.iter().map(|net| {
        let (bar, color) = signal_bar(net.rssi, signal_bar_width);
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
        Constraint::Fill(1),
        Constraint::Fill(1),
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
    let keys = " q quit │ enter/d diagnose │ r refresh │ s sort │ ↑/↓/j/k navigate │ g top │ G bottom ";
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

fn render_diagnostic(
    f: &mut Frame,
    diagnostic: &LiveDiagnostic,
    scanning: bool,
    spinner_tick: usize,
) {
    let [header, radio, network, signal_chart, latency_chart, stress_panel, note, footer] = Layout::vertical([
        Constraint::Length(2),
        Constraint::Length(3),
        Constraint::Length(3),
        Constraint::Fill(1),
        Constraint::Fill(1),
        Constraint::Length(4),
        Constraint::Length(2),
        Constraint::Length(1),
    ])
    .areas(f.area());

    let activity = if scanning || diagnostic.probing() {
        format!("{} sampling", SPINNER[spinner_tick % SPINNER.len()])
    } else {
        "live".into()
    };
    let heading = Line::from(vec![
        Span::styled(
            format!(" LIVE DIAGNOSTIC • {} ", diagnostic.target.ssid),
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        ),
        Span::styled(activity, Style::default().fg(Color::Yellow)),
    ]);
    let identity = format!(
        " BSSID {} • channel {} • {} GHz",
        if diagnostic.target.bssid.is_empty() {
            "<unavailable>"
        } else {
            &diagnostic.target.bssid
        },
        diagnostic
            .target
            .channel
            .map(|channel| channel.to_string())
            .unwrap_or_else(|| "?".into()),
        diagnostic.target.band.label()
    );
    f.render_widget(Paragraph::new(vec![heading, Line::from(identity)]), header);

    let signal_text = match (diagnostic.current_signal(), diagnostic.signal_stats()) {
        (Some(current), Some((min, max, average))) => format!(
            " Current: {current} dBm ({})   Average: {average:.1} dBm   Range: {min}…{max} dBm",
            signal_quality(current)
        ),
        _ => format!(
            " Access point not visible in latest scan{}",
            diagnostic
                .seconds_since_seen()
                .map(|seconds| format!(" • last seen {seconds}s ago"))
                .unwrap_or_default()
        ),
    };
    f.render_widget(
        Paragraph::new(signal_text).block(Block::default().borders(Borders::ALL).title(" Radio signal ")),
        radio,
    );

    let (sent, received) = diagnostic.packets();
    let latency_text = match diagnostic.latency_stats() {
        Some((min, max, average)) => format!(
            " Current: {}   Average: {average:.1} ms   Range: {min:.1}…{max:.1} ms   Loss: {:.1}% ({received}/{sent})",
            diagnostic
                .current_latency()
                .map(|latency| format!("{latency:.1} ms"))
                .unwrap_or_else(|| "timeout".into()),
            diagnostic.packet_loss()
        ),
        None if sent > 0 => format!(
            " No replies from {PROBE_TARGET}   Loss: {:.1}% ({received}/{sent})",
            diagnostic.packet_loss()
        ),
        None => format!(
            " Waiting for replies from {PROBE_TARGET}   Loss: {:.1}% ({received}/{sent})",
            diagnostic.packet_loss()
        ),
    };
    f.render_widget(
        Paragraph::new(latency_text).block(Block::default().borders(Borders::ALL).title(" Network path ")),
        network,
    );

    let signal_data = diagnostic.signal_data();
    let signal_color = diagnostic
        .current_signal()
        .map(signal_color)
        .unwrap_or(Color::DarkGray);
    f.render_widget(
        Sparkline::default()
            .block(Block::default().borders(Borders::ALL).title(" Signal history (−90 to −30 dBm) "))
            .data(&signal_data)
            .max(60)
            .style(Style::default().fg(signal_color)),
        signal_chart,
    );

    let latency_data = diagnostic.latency_data();
    let latency_max = latency_data.iter().copied().max().unwrap_or(100).max(20);
    f.render_widget(
        Sparkline::default()
            .block(Block::default().borders(Borders::ALL).title(format!(
                " Latency history (0–{latency_max} ms) "
            )))
            .data(&latency_data)
            .max(latency_max)
            .style(Style::default().fg(Color::Magenta)),
        latency_chart,
    );

    let stress = render_stress(f, diagnostic, stress_panel, spinner_tick);

    let note_text = match (&diagnostic.probe_error, &stress) {
        (Some(error), _) => Line::from(Span::styled(
            format!(" ⚠ {error}"),
            Style::default().fg(Color::Red),
        )),
        (None, Some(StressLine::Error(error))) => Line::from(Span::styled(
            format!(" ⚠ {error}"),
            Style::default().fg(Color::Red),
        )),
        (None, None) if diagnostic.stress.running() => Line::from(Span::styled(
            " ⚠ Stress test is saturating the downstream — latency/loss readings reflect that load.",
            Style::default().fg(Color::Yellow),
        )),
        (None, None) => {
            let tracking = if diagnostic.target.bssid.is_empty() {
                "BSSID unavailable: signal follows the SSID and may switch APs"
            } else {
                "Signal follows this BSSID"
            };
            Line::from(Span::styled(
                format!(
                    " {tracking}; latency/loss uses the active network route to {PROBE_TARGET}."
                ),
                Style::default().fg(Color::DarkGray),
            ))
        }
    };
    f.render_widget(Paragraph::new(note_text), note);
    f.render_widget(
        Paragraph::new(" esc/backspace return │ r sample now │ t stress test │ q quit ")
            .style(Style::default().fg(Color::DarkGray)),
        footer,
    );
}

/// Renders the stress-test panel; returns Some(error) when the load failed.
fn render_stress(
    f: &mut Frame,
    diagnostic: &LiveDiagnostic,
    area: Rect,
    spinner_tick: usize,
) -> Option<StressLine> {
    let stress = &diagnostic.stress;
    if let Some(error) = stress.error() {
        let line = StressLine::Error(error.to_owned());
        f.render_widget(
            Paragraph::new(vec![
                Line::from(Span::styled(
                    " Stress test failed ",
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                )),
                Line::from(Span::styled(
                    format!(" ⚠ {error}"),
                    Style::default().fg(Color::Red),
                )),
            ])
            .block(Block::default().borders(Borders::ALL).title(" Stress test ")),
            area,
        );
        return Some(line);
    }

    let running = stress.running();
    let status = if running {
        format!("running • {STRESS_WORKERS} workers")
    } else if stress.total_bytes() > 0 {
        "stopped".into()
    } else {
        "idle".into()
    };
    let megabytes = stress.total_bytes() as f64 / (1024.0 * 1024.0);
    let summary = if stress.total_bytes() > 0 {
        format!(
            " Transferred: {megabytes:.1} MB in {}s   Average: {:.1} Mbps{}",
            stress.elapsed_secs().unwrap_or(0),
            stress.average_mbps().unwrap_or(0.0),
            if running {
                stress
                    .current_mbps()
                    .map(|mbps| format!("   Current: {mbps} Mbps"))
                    .unwrap_or_default()
            } else {
                String::new()
            },
        )
    } else {
        format!(
            " press t to saturate the downstream with {STRESS_WORKERS} parallel downloads from {}",
            STRESS_HOST
        )
    };
    let status_span = if running {
        Span::styled(
            format!("{} {status}", SPINNER[spinner_tick % SPINNER.len()]),
            Style::default().fg(Color::Yellow),
        )
    } else {
        Span::styled(status, Style::default().fg(Color::DarkGray))
    };
    let mut lines = vec![Line::from(vec![
        Span::raw(" Status: "),
        status_span,
    ])];
    if stress.total_bytes() > 0 {
        lines.push(Line::from(Span::raw(summary)));
        let data = stress.throughput_data();
        let max = data.iter().copied().max().unwrap_or(10).max(10);
        f.render_widget(
            Sparkline::default()
                .data(&data)
                .max(max)
                .style(Style::default().fg(if running { Color::Cyan } else { Color::DarkGray })),
            inner(area, 2, 1),
        );
    } else {
        lines.push(Line::from(Span::styled(
            summary,
            Style::default().fg(Color::DarkGray),
        )));
    }
    f.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Stress test (downstream load) "),
        ),
        area,
    );
    None
}

fn inner(area: Rect, skip_top: u16, height: u16) -> Rect {
    Rect {
        x: area.x + 1,
        y: area.y + skip_top,
        width: area.width.saturating_sub(2),
        height: height.min(area.height.saturating_sub(skip_top + 1)),
    }
}

enum StressLine {
    Error(String),
}

fn signal_quality(rssi: i32) -> &'static str {
    match rssi {
        r if r >= -50 => "excellent",
        r if r >= -65 => "good",
        r if r >= -75 => "fair",
        _ => "poor",
    }
}

fn signal_color(rssi: i32) -> Color {
    match rssi {
        r if r >= -50 => Color::Green,
        r if r >= -65 => Color::Yellow,
        r if r >= -75 => Color::LightRed,
        _ => Color::Red,
    }
}

fn signal_bar(rssi: i32, width: usize) -> (String, Color) {
    let strength = (rssi.clamp(-90, -30) + 90) as usize; // 0..60
    let filled = (strength * width).div_ceil(60);
    let bar = format!("{}{}", "█".repeat(filled), "░".repeat(width - filled));
    let color = signal_color(rssi);
    (bar, color)
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
