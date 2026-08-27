mod app;
mod diagnostics;
mod model;
mod scanner;
mod ui;

use std::io::{self, Write};
use std::time::Duration;

use anyhow::{Context, Result};
use clap::{Parser, ValueEnum};
use crossterm::event::{Event, KeyCode, KeyModifiers};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use app::{App, SortMode};

#[derive(Parser)]
#[command(name = "openwifidiag", version, about = "Terminal WiFi diagnostics — scan and monitor near-by networks")]
struct Cli {
    /// Request macOS Location Services access and wait for a response.
    #[arg(long, hide = true)]
    request_location: bool,
    /// Print one scan as JSON and exit (no TUI).
    #[arg(long)]
    json: bool,
    /// Refresh interval for scans, in seconds.
    #[arg(short, long, default_value = "3")]
    interval: u64,
    /// Sort order.
    #[arg(short, long, default_value = "signal")]
    sort: SortArg,
    /// Specific interface to scan (e.g. wlan0, en0).
    #[arg(short = 'I', long)]
    iface: Option<String>,
}

#[derive(Clone, Copy, ValueEnum)]
enum SortArg {
    Signal,
    Ssid,
    Channel,
    Security,
}

impl From<SortArg> for SortMode {
    fn from(s: SortArg) -> Self {
        match s {
            SortArg::Signal => SortMode::Signal,
            SortArg::Ssid => SortMode::Ssid,
            SortArg::Channel => SortMode::Channel,
            SortArg::Security => SortMode::Security,
        }
    }
}

fn main() {
    if let Err(e) = run() {
        eprintln!("openwifidiag: {:#}", e);
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    let interval = Duration::from_secs(cli.interval.max(1));

    if cli.request_location {
        scanner::request_permissions_interactively();
        return Ok(());
    }
    scanner::prepare_permissions();

    if cli.json {
        return run_json(cli.iface.as_deref());
    }
    run_tui(interval, cli.sort.into(), cli.iface)
}

fn run_json(iface: Option<&str>) -> Result<()> {
    let scanner = scanner::platform_scanner(iface).context("failed to initialise scanner")?;
    let nets = scanner.scan().context("scan failed")?;
    let json = serde_json::to_string_pretty(&nets).map_err(io::Error::other)?;
    let mut out = io::stdout().lock();
    out.write_all(json.as_bytes())?;
    out.write_all(b"\n")?;
    out.flush()?;
    Ok(())
}

fn run_tui(interval: Duration, sort: SortMode, iface: Option<String>) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    crossterm::execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = event_loop(&mut terminal, interval, sort, iface);

    disable_raw_mode()?;
    crossterm::execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    result
}

fn event_loop(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, interval: Duration, sort: SortMode, iface: Option<String>) -> Result<()> {
    let mut app = App::new(interval, sort, iface);
    app.start_scan();

    loop {
        scanner::poll_platform_events();
        app.on_tick();
        terminal.draw(|f| ui::draw(f, &mut app))?;

        if app.should_quit {
            break;
        }

        if crossterm::event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = crossterm::event::read()? {
                if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
                    break;
                }
                handle_key(&mut app, key.code);
            }
        }
    }
    Ok(())
}

fn handle_key(app: &mut App, code: KeyCode) {
    match code {
        KeyCode::Char('q') => app.should_quit = true,
        KeyCode::Esc | KeyCode::Backspace if app.diagnostic.is_some() => app.stop_diagnostic(),
        KeyCode::Enter | KeyCode::Char('d') if app.diagnostic.is_none() => app.start_diagnostic(),
        KeyCode::Char('r') => app.start_scan(),
        _ if app.diagnostic.is_some() => {}
        KeyCode::Char('s') => {
            app.sort = app.sort.next();
            app.sort();
        }
        KeyCode::Char('g') => app.selected = 0,
        KeyCode::Char('G') => app.selected = app.networks.len().saturating_sub(1),
        KeyCode::Up | KeyCode::Char('k') => {
            app.selected = app.selected.saturating_sub(1);
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if app.selected < app.networks.len().saturating_sub(1) {
                app.selected += 1;
            }
        }
        _ => {}
    }
}
