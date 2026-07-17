use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::{App, Focus, StatusKind};

pub fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();

    if app.show_help {
        draw_help(frame, area);
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // search
            Constraint::Min(5),    // body
            Constraint::Length(1), // status
            Constraint::Length(1), // help bar
        ])
        .split(area);

    draw_search(frame, chunks[0], app);

    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(65), Constraint::Percentage(35)])
        .split(chunks[1]);

    draw_results(frame, body[0], app);
    draw_basket(frame, body[1], app);
    draw_status(frame, chunks[2], app);
    draw_footer(frame, chunks[3]);
}

fn draw_search(frame: &mut Frame, area: Rect, app: &App) {
    let focused = app.focus == Focus::Search;
    let border = if focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let title = format!(" search  ·  channel: {} ", app.channel);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border)
        .title(title);

    let cursor = if focused { "▌" } else { "" };
    let content = format!("{}{}", app.query, cursor);
    let para = Paragraph::new(content)
        .style(Style::default().fg(Color::White))
        .block(block);
    frame.render_widget(para, area);
}

fn draw_results(frame: &mut Frame, area: Rect, app: &App) {
    let focused = app.focus == Focus::Results;
    let border = if focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let title = if app.total_hits > 0 {
        format!(
            " results ({}/{}) ",
            app.results.len().min(app.total_hits as usize),
            app.total_hits
        )
    } else {
        " results ".into()
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border)
        .title(title);

    if app.results.is_empty() {
        let msg = if app.query.trim().is_empty() {
            "Type a query and press Enter (or wait for debounce)"
        } else {
            "No results"
        };
        let para = Paragraph::new(msg)
            .style(Style::default().fg(Color::DarkGray))
            .block(block);
        frame.render_widget(para, area);
        return;
    }

    let items: Vec<ListItem> = app
        .results
        .iter()
        .enumerate()
        .map(|(i, pkg)| {
            let selected = app.is_in_basket(&pkg.attr_name);
            let mark = if selected { "●" } else { " " };
            let cursor = if focused && i == app.result_cursor {
                "›"
            } else {
                " "
            };

            let name_style = if focused && i == app.result_cursor {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else if selected {
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };

            let desc = if pkg.description.is_empty() {
                String::new()
            } else {
                format!("  {}", truncate(&pkg.description, 60))
            };

            let line = Line::from(vec![
                Span::styled(format!("{cursor}{mark} "), Style::default().fg(Color::Green)),
                Span::styled(pkg.attr_name.clone(), name_style),
                Span::styled(
                    format!(" {}", pkg.version),
                    Style::default().fg(Color::Yellow),
                ),
                Span::styled(desc, Style::default().fg(Color::DarkGray)),
            ]);
            ListItem::new(line)
        })
        .collect();

    let list = List::new(items).block(block);
    frame.render_widget(list, area);
}

fn draw_basket(frame: &mut Frame, area: Rect, app: &App) {
    let focused = app.focus == Focus::Basket;
    let border = if focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let title = format!(" basket ({}) ", app.basket.len());
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border)
        .title(title);

    if app.basket.is_empty() {
        let para = Paragraph::new("Empty\n\nSpace on a result\nto add packages")
            .style(Style::default().fg(Color::DarkGray))
            .wrap(Wrap { trim: true })
            .block(block);
        frame.render_widget(para, area);
        return;
    }

    let items: Vec<ListItem> = app
        .basket
        .iter()
        .enumerate()
        .map(|(i, pkg)| {
            let style = if focused && i == app.basket_cursor {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Green)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Green)
            };
            let cursor = if focused && i == app.basket_cursor {
                "› "
            } else {
                "  "
            };
            ListItem::new(Line::from(vec![
                Span::raw(cursor),
                Span::styled(pkg.attr_name.clone(), style),
            ]))
        })
        .collect();

    let list = List::new(items).block(block);
    frame.render_widget(list, area);
}

fn draw_status(frame: &mut Frame, area: Rect, app: &App) {
    let (fg, prefix) = match app.status.kind {
        StatusKind::Info => (Color::Gray, ""),
        StatusKind::Ok => (Color::Green, "✓ "),
        StatusKind::Error => (Color::Red, "✗ "),
        StatusKind::Searching => (Color::Yellow, "… "),
    };
    let text = format!("{prefix}{}", app.status.message);
    let para = Paragraph::new(text).style(Style::default().fg(fg));
    frame.render_widget(para, area);
}

fn draw_footer(frame: &mut Frame, area: Rect) {
    let text = " Tab focus · j/k move · Space toggle · g generate · G force · / search · ? help · q quit ";
    let para = Paragraph::new(text).style(Style::default().fg(Color::DarkGray));
    frame.render_widget(para, area);
}

fn draw_help(frame: &mut Frame, area: Rect) {
    let help = r#"
 nixpick — pick nixpkgs packages into a direnv flake

 KEYS
   / or Tab (from results)   focus search input
   Enter                     run search immediately
   j / k / ↓ / ↑             move cursor
   Tab                       cycle focus: search → results → basket
   Space                     add/remove package from basket
   g                         generate flake.nix + .envrc + direnv allow
   G                         force overwrite existing files
   c                         cycle channel (unstable ↔ 25.11 ↔ 24.11)
   ?                         toggle this help
   q / Esc                   quit (Esc first closes help)

 OUTPUT
   Writes to the current working directory (or path given as arg):
     flake.nix   — multi-system devShell with selected packages
     .envrc      — `use flake`

   Then runs `direnv allow` if direnv is on PATH.

 NOTES
   Global node/bun/npm from home-manager stay available outside projects.
   Inside a project with a generated flake, direnv layers the local shell.
"#;
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title(" help ");
    let para = Paragraph::new(help.trim_start())
        .style(Style::default().fg(Color::White))
        .block(block);
    frame.render_widget(para, area);
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}
