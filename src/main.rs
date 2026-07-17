mod app;
mod generate;
mod search;
mod ui;

use std::io::{self, stdout};
use std::path::PathBuf;
use std::time::Duration;

use anyhow::Result;
use app::{App, Focus};
use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, EventStream, KeyCode, KeyEventKind,
    KeyModifiers,
};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use futures::StreamExt;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use search::SearchClient;
use tokio::sync::mpsc;

enum Msg {
    SearchDone {
        generation: u64,
        packages: Vec<search::Package>,
        elapsed: Duration,
        total: u64,
    },
    SearchErr {
        generation: u64,
        error: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let target = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().expect("cwd"));

    let mut client = SearchClient::new(None);
    let channel = client.channel().to_string();
    let mut app = App::new(target, channel);

    enable_raw_mode()?;
    let mut out = stdout();
    execute!(out, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(out);
    let mut terminal = Terminal::new(backend)?;

    let (tx, mut rx) = mpsc::unbounded_channel::<Msg>();
    let mut events = EventStream::new();
    let mut debounce = tokio::time::interval(Duration::from_millis(50));
    debounce.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut last_query_change = tokio::time::Instant::now();
    let mut dirty = false;

    let result = run_loop(
        &mut terminal,
        &mut app,
        &mut client,
        &tx,
        &mut rx,
        &mut events,
        &mut debounce,
        &mut last_query_change,
        &mut dirty,
    )
    .await;

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    result
}

#[allow(clippy::too_many_arguments)]
async fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    client: &mut SearchClient,
    tx: &mpsc::UnboundedSender<Msg>,
    rx: &mut mpsc::UnboundedReceiver<Msg>,
    events: &mut EventStream,
    debounce: &mut tokio::time::Interval,
    last_query_change: &mut tokio::time::Instant,
    dirty: &mut bool,
) -> Result<()> {
    loop {
        terminal.draw(|f| ui::draw(f, app))?;

        if app.should_quit {
            break;
        }

        tokio::select! {
            maybe_event = events.next() => {
                match maybe_event {
                    Some(Ok(Event::Key(key))) if key.kind == KeyEventKind::Press => {
                        handle_key(app, client, key.code, key.modifiers, last_query_change, dirty)?;
                    }
                    Some(Ok(Event::Resize(_, _))) => {}
                    Some(Err(_)) => break,
                    None => break,
                    _ => {}
                }
            }
            msg = rx.recv() => {
                if let Some(msg) = msg {
                    match msg {
                        Msg::SearchDone { generation, packages, elapsed, total } => {
                            app.apply_results(packages, elapsed, total, generation);
                            if app.focus == Focus::Search && !app.results.is_empty() {
                                // keep focus on search so typing continues smoothly
                            }
                        }
                        Msg::SearchErr { generation, error } => {
                            if generation == app.search_generation {
                                app.set_error(error);
                            }
                        }
                    }
                }
            }
            _ = debounce.tick() => {
                if *dirty && last_query_change.elapsed() >= Duration::from_millis(280) {
                    *dirty = false;
                    spawn_search(app, client, tx);
                }
            }
        }
    }
    Ok(())
}

fn spawn_search(app: &mut App, client: &SearchClient, tx: &mpsc::UnboundedSender<Msg>) {
    let query = match app.pending_query.take() {
        Some(q) => q,
        None => return,
    };
    if query.trim().is_empty() {
        return;
    }
    let generation = app.search_generation;
    app.set_searching();

    // Clone what the worker needs
    let mut worker_client = SearchClient::new(Some(client.channel().to_string()));
    let tx = tx.clone();
    tokio::spawn(async move {
        match worker_client.search(&query, 30).await {
            Ok((packages, elapsed, total)) => {
                let _ = tx.send(Msg::SearchDone {
                    generation,
                    packages,
                    elapsed,
                    total,
                });
            }
            Err(e) => {
                let _ = tx.send(Msg::SearchErr {
                    generation,
                    error: e.to_string(),
                });
            }
        }
    });
}

fn handle_key(
    app: &mut App,
    client: &mut SearchClient,
    code: KeyCode,
    modifiers: KeyModifiers,
    last_query_change: &mut tokio::time::Instant,
    dirty: &mut bool,
) -> Result<()> {
    if app.show_help {
        match code {
            KeyCode::Char('?') | KeyCode::Esc | KeyCode::Char('q') => app.show_help = false,
            _ => {}
        }
        return Ok(());
    }

    // Global keys
    match code {
        KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => {
            app.should_quit = true;
            return Ok(());
        }
        KeyCode::Char('q') if app.focus != Focus::Search => {
            app.should_quit = true;
            return Ok(());
        }
        KeyCode::Esc => {
            if app.focus != Focus::Search {
                app.focus = Focus::Search;
            } else if !app.query.is_empty() {
                app.query.clear();
                app.request_search();
                *dirty = true;
                *last_query_change = tokio::time::Instant::now();
            } else {
                app.should_quit = true;
            }
            return Ok(());
        }
        KeyCode::Char('?') if app.focus != Focus::Search => {
            app.show_help = true;
            return Ok(());
        }
        KeyCode::Tab => {
            app.cycle_focus();
            return Ok(());
        }
        KeyCode::Char('g') if app.focus != Focus::Search => {
            do_generate(app, false);
            return Ok(());
        }
        KeyCode::Char('G') if app.focus != Focus::Search => {
            do_generate(app, true);
            return Ok(());
        }
        KeyCode::Char('c') if app.focus != Focus::Search => {
            cycle_channel(app, client);
            return Ok(());
        }
        KeyCode::Char('/') if app.focus != Focus::Search => {
            app.focus = Focus::Search;
            return Ok(());
        }
        _ => {}
    }

    match app.focus {
        Focus::Search => match code {
            KeyCode::Char(c) if !modifiers.contains(KeyModifiers::CONTROL) => {
                app.query.push(c);
                app.request_search();
                *dirty = true;
                *last_query_change = tokio::time::Instant::now();
            }
            KeyCode::Backspace => {
                app.query.pop();
                app.request_search();
                *dirty = true;
                *last_query_change = tokio::time::Instant::now();
            }
            KeyCode::Enter => {
                // immediate search
                app.request_search();
                *dirty = false;
                // force spawn on next tick by setting dirty false and calling directly
                // but we need the tx — so mark dirty with zero delay
                *last_query_change =
                    tokio::time::Instant::now() - Duration::from_millis(500);
                *dirty = true;
            }
            KeyCode::Down | KeyCode::Char('j') if modifiers.contains(KeyModifiers::CONTROL) => {
                if !app.results.is_empty() {
                    app.focus = Focus::Results;
                }
            }
            KeyCode::Down => {
                if !app.results.is_empty() {
                    app.focus = Focus::Results;
                }
            }
            _ => {}
        },
        Focus::Results | Focus::Basket => match code {
            KeyCode::Char('j') | KeyCode::Down => app.move_cursor(1),
            KeyCode::Char('k') | KeyCode::Up => app.move_cursor(-1),
            KeyCode::Char(' ') => app.toggle_selected(),
            KeyCode::Enter => app.toggle_selected(),
            KeyCode::Char('g') => do_generate(app, false),
            KeyCode::Char('G') => do_generate(app, true),
            KeyCode::Backspace | KeyCode::Delete if app.focus == Focus::Basket => {
                app.toggle_selected();
            }
            _ => {}
        },
    }
    Ok(())
}

fn do_generate(app: &mut App, force: bool) {
    let result = if force {
        generate::merge_into_existing(&app.target_dir, &app.basket, &app.channel)
    } else {
        generate::generate(&app.target_dir, &app.basket, &app.channel, false)
    };

    match result {
        Ok(r) => {
            let pkgs = r.packages.join(", ");
            let msg = format!(
                "Wrote {} packages to {} · {}",
                r.packages.len(),
                r.flake_path.display(),
                if r.direnv_allowed {
                    "direnv allow ✓"
                } else {
                    r.direnv_message.as_str()
                }
            );
            // keep pkgs in status for feedback
            let _ = pkgs;
            app.set_ok(msg);
        }
        Err(e) => app.set_error(e.to_string()),
    }
}

fn cycle_channel(app: &mut App, client: &mut SearchClient) {
    let next = match app.channel.as_str() {
        "unstable" => "25.11",
        "25.11" => "24.11",
        _ => "unstable",
    };
    app.channel = next.to_string();
    client.set_channel(next.to_string());
    app.set_ok(format!("Channel → {next}"));
    if !app.query.trim().is_empty() {
        app.request_search();
    }
}

// silence unused import warning path for event poll if any
#[allow(dead_code)]
fn _poll_hint() {
    let _ = event::poll(Duration::from_millis(0));
}
