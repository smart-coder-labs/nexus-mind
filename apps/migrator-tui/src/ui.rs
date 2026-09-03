//! Rendering. Reads `App`, mutates nothing.

use crate::app::{is_active, ActivityView, App, FieldKind, LastCommand, Screen, STAGES};
use crate::config::{is_local, Source};
use crate::monorepo::Action;
use ratatui::prelude::*;
use ratatui::widgets::{
    BarChart, Block, Borders, Clear, Gauge, List, ListItem, ListState, Padding, Paragraph, Tabs,
    Wrap,
};

const ACCENT: Color = Color::Cyan;
const MUTED: Color = Color::DarkGray;
const DANGER: Color = Color::Red;
const CAUTION: Color = Color::Yellow;
const GOOD: Color = Color::Green;

pub fn draw(f: &mut Frame, app: &App) {
    let rows = Layout::vertical([
        Constraint::Length(4),
        Constraint::Min(10),
        Constraint::Length(3),
    ])
    .split(f.area());

    header(f, rows[0], app);
    match app.screen {
        Screen::Connection => connection(f, rows[1], app),
        Screen::Source => source(f, rows[1], app),
        Screen::Options => options(f, rows[1], app),
        Screen::Projects => projects(f, rows[1], app),
        Screen::Running => running(f, rows[1], app),
        Screen::Review => review(f, rows[1], app),
        Screen::Summary => summary(f, rows[1], app),
    }
    footer(f, rows[2], app);
    // Last, so it can see what the panels wrote and decline to cover any of it.
    overlay_mascot(f, rows[1], app);

    if app.show_help {
        help_overlay(f, app);
    }
}

// ── Chrome ───────────────────────────────────────────────────────────────────

fn header(f: &mut Frame, area: Rect, app: &App) {
    let block = Block::default().borders(Borders::BOTTOM).border_style(MUTED);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let parts =
        Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).split(inner);

    let tabs = Tabs::new(
        [
            Screen::Connection,
            Screen::Source,
            Screen::Options,
            Screen::Projects,
            Screen::Running,
            Screen::Review,
            Screen::Summary,
        ]
        .iter()
        .map(|s| s.title())
        .collect::<Vec<_>>(),
    )
    .select(match app.screen {
        Screen::Connection => 0,
        Screen::Source => 1,
        Screen::Options => 2,
        Screen::Projects => 3,
        Screen::Running => 4,
        Screen::Review => 5,
        Screen::Summary => 6,
    })
    .style(Style::default().fg(MUTED))
    .highlight_style(Style::default().fg(ACCENT).add_modifier(Modifier::BOLD))
    .divider(" ");
    f.render_widget(tabs, parts[0]);

    f.render_widget(Paragraph::new(pipeline(app)), parts[1]);
}

/// The migration pipeline, with the stage the operator is in lit up.
///
/// It is the answer to the question every step of this process raises — "is
/// anything permanent yet?" — kept on screen instead of in the guide.
fn pipeline(app: &App) -> Line<'static> {
    let active = if app.in_flight() && app.progress.total > 0 {
        1
    } else {
        app.screen.stage()
    };
    let mut spans = Vec::new();
    for (i, stage) in STAGES.iter().enumerate() {
        let done = i < active;
        let style = if i == active {
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
        } else if done {
            Style::default().fg(GOOD)
        } else {
            Style::default().fg(MUTED)
        };
        spans.push(Span::styled(
            format!("{} {stage}", if done { "✓" } else { "○" }),
            style,
        ));
        if i + 1 < STAGES.len() {
            let sep = if i + 1 == 4 { "  ══▶  " } else { "  ──▶  " };
            spans.push(Span::styled(sep, Style::default().fg(MUTED)));
        }
    }
    spans.push(Span::styled(
        "   (nothing is written until commit)",
        Style::default().fg(MUTED).add_modifier(Modifier::ITALIC),
    ));
    Line::from(spans)
}

fn footer(f: &mut Frame, area: Rect, app: &App) {
    let rows = Layout::vertical([Constraint::Length(1), Constraint::Length(1)])
        .split(Block::default().borders(Borders::TOP).border_style(MUTED).inner(area));
    f.render_widget(
        Block::default().borders(Borders::TOP).border_style(MUTED),
        area,
    );

    let tone = if app.status.starts_with('✗') || app.status.contains("cannot") {
        DANGER
    } else {
        Color::Reset
    };
    // A backend call runs on its own thread, so the terminal stays live while
    // it is out. The marker is what tells the operator that something is in
    // flight rather than finished.
    let status = if app.is_waiting() {
        format!("{} {}", spinner_frame(app.frame), app.status)
    } else {
        app.status.clone()
    };
    f.render_widget(
        Paragraph::new(Line::styled(status, Style::default().fg(tone))),
        rows[0],
    );
    f.render_widget(
        Paragraph::new(Line::styled(keys(app), Style::default().fg(MUTED))),
        rows[1],
    );
}

fn keys(app: &App) -> String {
    let common = "Tab/⇧Tab screen · ? help · q quit";
    match app.screen {
        Screen::Connection => format!("↑↓ field · Enter edit · t test connection · {common}"),
        Screen::Source => format!("↑↓ pick · Enter options · {common}"),
        Screen::Options => {
            format!("↑↓ field · Enter edit · Space toggle · d preview · r run · {common}")
        }
        Screen::Running if app.is_running() => format!(
            "x stop · e panel ({}) · ↑↓ inspect · f follow · {common}",
            app.activity.label()
        ),
        Screen::Running => format!(
            "R review queue · e panel ({}) · ↑↓ inspect · {common}",
            app.activity.label()
        ),
        Screen::Review if app.picking_run() => {
            format!("↑↓ move · Enter open · X cancel run · R reload · {common}")
        }
        Screen::Review => {
            let (n, whole_queue) = app.batch_target();
            let scope = if whole_queue { "queue" } else { "selected" };
            format!(
                "↑↓ move · Space select · a approve · j reject · s restage · A approve {n} \
                 ({scope}) · C commit · p other run · {common}"
            )
        }
        Screen::Projects if app.selecting_for.is_some() => {
            format!("↑↓ pick · Enter choose · Esc cancel · {common}")
        }
        Screen::Projects if app.plan.is_empty() => common.to_string(),
        Screen::Projects => {
            let (create, select, _) = app.plan_summary();
            format!(
                "↑↓ move · Enter cycle · s pick existing · r apply & run \
                 ({create} new, {select} existing) · {common}"
            )
        }
        Screen::Summary => format!("R review · r run again · {common}"),
    }
}

// ── Field screens ────────────────────────────────────────────────────────────

fn field_rows(f: &mut Frame, area: Rect, app: &App) {
    let fields = app.fields();
    let items: Vec<ListItem> = fields
        .iter()
        .enumerate()
        .map(|(i, field)| {
            let selected = i == app.cursor;
            let live = is_active(*field, &app.config);
            let value = match field.kind() {
                FieldKind::Toggle => {
                    if field.is_on(&app.config) {
                        "● on".to_string()
                    } else {
                        "○ off".to_string()
                    }
                }
                FieldKind::Secret => {
                    let v = field.value(&app.config);
                    if v.is_empty() {
                        "—".into()
                    } else {
                        format!("{}… ({} chars)", &v[..v.len().min(6)], v.len())
                    }
                }
                FieldKind::Text => {
                    let v = field.value(&app.config);
                    if v.is_empty() {
                        "—".into()
                    } else {
                        v
                    }
                }
            };
            let editing = selected && app.editing;
            let value = if editing { format!("{value}▏") } else { value };

            let label_style = if !live {
                Style::default().fg(MUTED).add_modifier(Modifier::DIM)
            } else if selected {
                Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            let value_style = if !live {
                Style::default().fg(MUTED)
            } else if editing {
                Style::default().fg(Color::Black).bg(ACCENT)
            } else {
                Style::default().fg(Color::White)
            };
            ListItem::new(Line::from(vec![
                Span::styled(if selected { "▸ " } else { "  " }, label_style),
                Span::styled(format!("{:<34}", field.label()), label_style),
                Span::styled(value, value_style),
            ]))
        })
        .collect();

    let mut state = ListState::default();
    state.select(Some(app.cursor));
    f.render_stateful_widget(
        List::new(items).block(
            Block::bordered()
                .title(format!(" {} ", app.screen.title()))
                .border_style(MUTED)
                .padding(Padding::horizontal(1)),
        ),
        area,
        &mut state,
    );
}

fn connection(f: &mut Frame, area: Rect, app: &App) {
    let cols = Layout::horizontal([Constraint::Percentage(58), Constraint::Percentage(42)])
        .split(area);
    field_rows(f, cols[0], app);

    let mut lines: Vec<Line> = Vec::new();
    if let Some(field) = app.current_field() {
        lines.push(Line::styled(
            field.help(),
            Style::default().add_modifier(Modifier::ITALIC),
        ));
        lines.push(Line::raw(""));
    }

    let local = is_local(&app.config.api_url);
    lines.push(Line::from(vec![
        Span::raw("target  "),
        Span::styled(
            if local { "local" } else { "REMOTE" },
            Style::default()
                .fg(if local { GOOD } else { DANGER })
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(app.config.api_url.clone(), Style::default().fg(MUTED)),
    ]));
    if let Ok(env) = std::env::var("NEXUSMIND_BASE_URL") {
        if env.trim_end_matches('/') != app.config.api_url.trim_end_matches('/') {
            lines.push(Line::raw(""));
            lines.push(Line::styled(
                "NEXUSMIND_BASE_URL in your shell points elsewhere:",
                Style::default().fg(CAUTION),
            ));
            lines.push(Line::styled(format!("  {env}"), Style::default().fg(MUTED)));
            lines.push(Line::styled(
                "  It is not used unless you type it above.",
                Style::default().fg(MUTED).add_modifier(Modifier::ITALIC),
            ));
        }
    }
    lines.push(Line::raw(""));
    lines.push(Line::styled(
        format!("runner  {}", app.binary().display()),
        Style::default().fg(if app.binary().exists() { MUTED } else { DANGER }),
    ));
    if !app.binary().exists() {
        lines.push(Line::styled(
            "  not built — cargo build --bin migrate-knowledge",
            Style::default().fg(DANGER),
        ));
    }

    f.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: true })
            .block(
                Block::bordered()
                    .title(" Where this goes ")
                    .border_style(MUTED)
                    .padding(Padding::horizontal(1)),
            ),
        cols[1],
    );
}

fn source(f: &mut Frame, area: Rect, app: &App) {
    let cols = Layout::horizontal([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(area);

    let selected = Source::ALL
        .iter()
        .position(|s| *s == app.config.source)
        .unwrap_or(0);
    let items: Vec<ListItem> = Source::ALL
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let style = if i == selected {
                Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            ListItem::new(Line::from(vec![
                Span::styled(if i == selected { "▸ " } else { "  " }, style),
                Span::styled(s.title(), style),
            ]))
        })
        .collect();
    let mut state = ListState::default();
    state.select(Some(selected));
    f.render_stateful_widget(
        List::new(items).block(
            Block::bordered()
                .title(" Source ")
                .border_style(MUTED)
                .padding(Padding::horizontal(1)),
        ),
        cols[0],
        &mut state,
    );

    let s = app.config.source;
    let mut lines = vec![
        Line::styled(
            s.title(),
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ),
        Line::styled(format!("--source {}", s.flag()), Style::default().fg(MUTED)),
        Line::raw(""),
    ];
    lines.extend(
        s.blurb()
            .split(". ")
            .filter(|p| !p.trim().is_empty())
            .map(|p| Line::raw(format!("• {}", p.trim_end_matches('.')))),
    );
    if s == Source::DbSchema {
        lines.push(Line::raw(""));
        lines.push(Line::styled(
            "Set NEXUSMIND_SOURCE_DSN before starting this TUI.",
            Style::default().fg(CAUTION),
        ));
        lines.push(Line::styled(
            "A DSN passed as an argument would survive in ps and shell history.",
            Style::default().fg(MUTED).add_modifier(Modifier::ITALIC),
        ));
    }
    f.render_widget(
        Paragraph::new(lines).wrap(Wrap { trim: true }).block(
            Block::bordered()
                .title(" What it reads ")
                .border_style(MUTED)
                .padding(Padding::horizontal(1)),
        ),
        cols[1],
    );
}

fn options(f: &mut Frame, area: Rect, app: &App) {
    let cols = Layout::horizontal([Constraint::Percentage(58), Constraint::Percentage(42)])
        .split(area);
    field_rows(f, cols[0], app);

    let right = Layout::vertical([Constraint::Min(6), Constraint::Length(7)]).split(cols[1]);

    let mut lines: Vec<Line> = Vec::new();
    if let Some(field) = app.current_field() {
        lines.push(Line::styled(
            field.help(),
            Style::default().add_modifier(Modifier::ITALIC),
        ));
        lines.push(Line::raw(""));
    }
    for b in app.blockers(false) {
        lines.push(Line::from(vec![
            Span::styled("✗ ", Style::default().fg(DANGER)),
            Span::styled(b.why, Style::default().fg(DANGER)),
        ]));
    }
    for w in app.warnings(false) {
        lines.push(Line::styled(
            format!("! {}", w.headline),
            Style::default().fg(CAUTION),
        ));
        lines.push(Line::styled(
            format!("  {}", w.detail),
            Style::default().fg(MUTED),
        ));
    }
    if lines.is_empty() {
        lines.push(Line::styled("Ready to run.", Style::default().fg(GOOD)));
    }
    f.render_widget(
        Paragraph::new(lines).wrap(Wrap { trim: true }).block(
            Block::bordered()
                .title(" Before you start ")
                .border_style(MUTED)
                .padding(Padding::horizontal(1)),
        ),
        right[0],
    );

    f.render_widget(
        Paragraph::new(app.config.display_command(false))
            .wrap(Wrap { trim: true })
            .style(Style::default().fg(MUTED))
            .block(
                Block::bordered()
                    .title(" Equivalent command ")
                    .border_style(MUTED)
                    .padding(Padding::horizontal(1)),
            ),
        right[1],
    );
}

// ── Run screen ───────────────────────────────────────────────────────────────

fn running(f: &mut Frame, area: Rect, app: &App) {
    if app.never_ran() {
        return idle(f, area, app);
    }
    let rows = Layout::vertical([
        Constraint::Length(6),
        Constraint::Min(8),
        Constraint::Length(8),
    ])
    .split(area);

    gauges(f, rows[0], app);

    let band = Layout::vertical([Constraint::Min(8), Constraint::Length(8)]).split(rows[1].union(rows[2]));

    match app.activity {
        // An expanded panel takes the charts' room. During a run that goes
        // wrong the charts say what happened; the panels say why.
        ActivityView::Both => {
            let charts =
                Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
                    .split(band[0]);
            histogram(
                f,
                charts[0],
                " Candidates by destination ",
                app.progress.destination_histogram(),
                ACCENT,
            );
            histogram(
                f,
                charts[1],
                " Skipped, by reason ",
                app.progress.exclusion_histogram(),
                MUTED,
            );
        }
        ActivityView::Agents => agents_panel(f, band[0], app),
        ActivityView::Logs => logs_panel(f, band[0], app, true),
    }

    match app.activity {
        ActivityView::Both => {
            let split =
                Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
                    .split(band[1]);
            agents_summary(f, split[0], app);
            logs_panel(f, split[1], app, false);
        }
        // The expanded panel already occupies the charts' row above; this row
        // continues it rather than repeating it.
        ActivityView::Agents => agents_detail(f, band[1], app),
        ActivityView::Logs => logs_panel(f, band[1], app, false),
    }
}

/// The list of exchanges: one line each, newest last.
fn agents_panel(f: &mut Frame, area: Rect, app: &App) {
    let items: Vec<ListItem> = app
        .progress
        .agents
        .iter()
        .map(|a| {
            let (mark, tone) = if a.ok {
                ("✓", GOOD)
            } else {
                ("✗", DANGER)
            };
            ListItem::new(Line::from(vec![
                Span::styled(format!("{mark} "), Style::default().fg(tone)),
                Span::styled(
                    format!("{:>5}/{:<5}", a.index, a.total),
                    Style::default().fg(MUTED),
                ),
                Span::styled(
                    format!("{:>7}t ", a.tokens_spent),
                    Style::default().fg(CAUTION),
                ),
                Span::styled(
                    format!("{:>6}ms  ", a.duration_ms),
                    Style::default().fg(MUTED),
                ),
                Span::raw(a.origin.clone()),
            ]))
        })
        .collect();

    let pinned = app.agent_cursor.is_some();
    let selected = app
        .agent_cursor
        .unwrap_or_else(|| app.progress.agents.len().saturating_sub(1));
    let title = format!(
        " Agents ({}) — {} · ↑↓ inspect · e panel ",
        app.progress.agents.len(),
        if pinned { "pinned" } else { "following" }
    );
    let mut state = ListState::default();
    state.select(Some(selected));
    f.render_stateful_widget(
        List::new(items)
            .block(
                Block::bordered()
                    .title(title)
                    .border_style(MUTED)
                    .padding(Padding::horizontal(1)),
            )
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED)),
        area,
        &mut state,
    );
}

/// What was asked, and what came back.
fn agents_detail(f: &mut Frame, area: Rect, app: &App) {
    let block = Block::bordered()
        .title(" Exchange — what was asked, and what Claude answered ")
        .border_style(MUTED)
        .padding(Padding::horizontal(1));

    let Some(a) = app.selected_agent() else {
        f.render_widget(
            Paragraph::new(Line::styled(
                "No exchange yet. Runs with --no-llm never ask anything.",
                Style::default().fg(MUTED),
            ))
            .block(block),
            area,
        );
        return;
    };

    let cols = Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(block.inner(area));
    f.render_widget(block, area);

    f.render_widget(
        Paragraph::new(
            a.prompt
                .lines()
                .map(|l| Line::raw(l.to_string()))
                .collect::<Vec<_>>(),
        )
        .wrap(Wrap { trim: false })
        .block(
            Block::bordered()
                .title(" asked ")
                .border_style(MUTED)
                .padding(Padding::horizontal(1)),
        ),
        cols[0],
    );

    let mut answer: Vec<Line> = Vec::new();
    if let Some(e) = &a.error {
        // The reason first: an answer that could not be used is exactly when
        // somebody opens this panel.
        answer.push(Line::styled(
            e.clone(),
            Style::default().fg(DANGER).add_modifier(Modifier::BOLD),
        ));
        answer.push(Line::raw(""));
    }
    answer.extend(a.response.lines().map(|l| Line::raw(l.to_string())));
    if a.response.trim().is_empty() {
        answer.push(Line::styled(
            "(the classifier never ran)",
            Style::default().fg(MUTED),
        ));
    }
    f.render_widget(
        Paragraph::new(answer).wrap(Wrap { trim: false }).block(
            Block::bordered()
                .title(format!(" answered · {}t · {}ms ", a.tokens_spent, a.duration_ms))
                .border_style(if a.ok { MUTED } else { DANGER })
                .padding(Padding::horizontal(1)),
        ),
        cols[1],
    );
}

/// A compact view of the exchanges, for when both panels share the row.
fn agents_summary(f: &mut Frame, area: Rect, app: &App) {
    let block = Block::bordered()
        .title(format!(" Agents ({}) — e expands ", app.progress.agents.len()))
        .border_style(MUTED)
        .padding(Padding::horizontal(1));
    if app.progress.agents.is_empty() {
        f.render_widget(
            Paragraph::new(Line::styled(
                if app.config.no_llm {
                    "--no-llm is on: nothing is asked of the model."
                } else {
                    "No exchange yet."
                },
                Style::default().fg(MUTED).add_modifier(Modifier::ITALIC),
            ))
            .wrap(Wrap { trim: true })
            .block(block),
            area,
        );
        return;
    }
    let rows = block.inner(area).height as usize;
    let lines: Vec<Line> = app
        .progress
        .agents
        .iter()
        .rev()
        .take(rows)
        .rev()
        .map(|a| {
            Line::from(vec![
                Span::styled(
                    if a.ok { "✓ " } else { "✗ " },
                    Style::default().fg(if a.ok { GOOD } else { DANGER }),
                ),
                Span::styled(format!("{:>6}t  ", a.tokens_spent), Style::default().fg(CAUTION)),
                Span::raw(a.origin.clone()),
            ])
        })
        .collect();
    f.render_widget(Paragraph::new(lines).block(block), area);
}

fn logs_panel(f: &mut Frame, area: Rect, app: &App, expanded: bool) {
    let block = Block::bordered()
        .title(if expanded {
            " Logs — e cycles panels ".to_string()
        } else {
            " Logs ".to_string()
        })
        .border_style(MUTED)
        .padding(Padding::horizontal(1));

    if app.progress.log.is_empty() {
        f.render_widget(
            Paragraph::new(Line::styled(
                if app.in_flight() {
                    "Walking the source tree. The scan is one pass and reports its unit \
                     count when it finishes; the counter above moves in the meantime."
                } else {
                    "Nothing logged."
                },
                Style::default().fg(MUTED).add_modifier(Modifier::ITALIC),
            ))
            .wrap(Wrap { trim: true })
            .block(block),
            area,
        );
        return;
    }
    let rows = block.inner(area).height as usize;
    let log: Vec<Line> = app
        .progress
        .log
        .iter()
        .rev()
        .take(rows)
        .rev()
        .map(|l| {
            let style = if l.starts_with('✗') {
                Style::default().fg(DANGER)
            } else if l.starts_with('?') {
                Style::default().fg(CAUTION)
            } else {
                Style::default().fg(MUTED)
            };
            Line::styled(l.clone(), style)
        })
        .collect();
    f.render_widget(Paragraph::new(log).wrap(Wrap { trim: true }).block(block), area);
}

/// Widths to try when the terminal can show a real image, widest first.
///
/// Wider than the quadrant sizes because it costs nothing here: the frames are
/// 192px and the terminal scales them, so more cells is more picture rather
/// than more blocks. A ladder rather than one value so a narrow terminal still
/// gets a robot instead of losing it to a size that never fits.
const GRAPHICS_CELLS: [u16; 3] = [40, 32, 24];

/// Where the mascot may stand, and where it would rather stand.
///
/// # Why this is a search and not a corner
///
/// It began as "bottom-right, and rise until something fits", which put the
/// robot in the corner of whatever panel happened to be empty rather than in
/// the middle of it. Scoring every legal position against a target instead
/// costs nothing measurable and places it where a decoration belongs.
///
/// Blankness is answered in constant time from a summed-area table, so scanning
/// a few thousand candidate positions per frame is a few thousand additions.
struct BlankMap {
    area: Rect,
    /// `sums[(y+1) * (w+1) + (x+1)]` = number of blank cells above and left.
    sums: Vec<u32>,
}

impl BlankMap {
    fn new(buf: &Buffer, area: Rect) -> Self {
        let (w, h) = (area.width as usize, area.height as usize);
        let mut sums = vec![0u32; (w + 1) * (h + 1)];
        for y in 0..h {
            for x in 0..w {
                let cell = &buf[(area.x + x as u16, area.y + y as u16)];
                // "Untouched" means a blank symbol and no colour set — a cell
                // no widget has written to. Borders, text and bar-chart fill
                // all fail this.
                let blank = cell.symbol() == " "
                    && cell.bg == Color::Reset
                    && cell.fg == Color::Reset;
                sums[(y + 1) * (w + 1) + x + 1] = blank as u32 + sums[y * (w + 1) + x + 1]
                    + sums[(y + 1) * (w + 1) + x]
                    - sums[y * (w + 1) + x];
            }
        }
        Self { area, sums }
    }

    /// Is every cell of this `w` x `h` box, at local `(x, y)`, blank?
    fn is_clear(&self, x: usize, y: usize, w: usize, h: usize) -> bool {
        let stride = self.area.width as usize + 1;
        let sum = self.sums[(y + h) * stride + x + w] + self.sums[y * stride + x]
            - self.sums[y * stride + x + w]
            - self.sums[(y + h) * stride + x];
        sum as usize == w * h
    }

    /// The blank box of this size whose centre lands nearest `target`.
    fn nearest_clear(&self, w: u16, h: u16, target: (i32, i32)) -> Option<Rect> {
        let (aw, ah) = (self.area.width as usize, self.area.height as usize);
        let (bw, bh) = (w as usize, h as usize);
        if bw > aw || bh > ah {
            return None;
        }
        let mut best: Option<(i64, Rect)> = None;
        for y in 0..=(ah - bh) {
            for x in 0..=(aw - bw) {
                if !self.is_clear(x, y, bw, bh) {
                    continue;
                }
                let cx = self.area.x as i32 + x as i32 + w as i32 / 2;
                let cy = self.area.y as i32 + y as i32 + h as i32 / 2;
                // Cells are about twice as tall as they are wide, so a vertical
                // cell of distance is worth two horizontal ones. Without this
                // the robot drifts up and down looking for a "closer" spot that
                // is visibly further away.
                let dx = (cx - target.0) as i64;
                let dy = (cy - target.1) as i64 * 2;
                let score = dx * dx + dy * dy;
                if best.as_ref().is_none_or(|(b, _)| score < *b) {
                    best = Some((
                        score,
                        Rect {
                            x: self.area.x + x as u16,
                            y: self.area.y + y as u16,
                            width: w,
                            height: h,
                        },
                    ));
                }
            }
        }
        best.map(|(_, r)| r)
    }
}

/// Draws the robot in the empty space on the right, centred in it.
///
/// # Why an overlay and not a reserved column
///
/// It used to be given a column of its own, which cost every panel beside it
/// 28 characters of width whether or not the robot was worth that much. As an
/// overlay it costs nothing: it goes where the screen is already empty, and if
/// the screen is full it simply does not appear. That also lets it use a bigger
/// frame — quality is bought with cells, and free cells are free.
///
/// The whole box must be empty before anything is drawn. Checking cell by cell
/// and skipping the occupied ones would produce a half-eaten robot; more to the
/// point, a decoration may never paint over something that means something.
fn overlay_mascot(f: &mut Frame, area: Rect, app: &App) {
    // Kept off the border so it reads as sitting in the space, not glued to it.
    const MARGIN: u16 = 1;
    if area.width <= MARGIN * 2 || area.height <= MARGIN * 2 {
        return;
    }
    let room = Rect {
        x: area.x + MARGIN,
        y: area.y + MARGIN,
        width: area.width - MARGIN * 2,
        height: area.height - MARGIN * 2,
    };

    let Some(robot) = app.mascot(room.width, room.height) else {
        return;
    };
    // The graphics path gets a box sized from the terminal's own font aspect,
    // and a wider one: it is drawing a real image, so the extra cells buy real
    // detail rather than bigger blocks.
    let (w, h) = match app.graphics.as_ref() {
        Some(g) => {
            let mut fits = None;
            for wide in GRAPHICS_CELLS {
                let (w, h) = g.box_for(wide);
                if w <= room.width && h <= room.height {
                    fits = Some((w, h));
                    break;
                }
            }
            match fits {
                Some(box_) => box_,
                None => return,
            }
        }
        None => robot.cells(),
    };
    if w > room.width || h > room.height {
        return;
    }

    // Aim at the bottom-right corner. The scoring then settles on the free box
    // nearest it, so the robot sits low and right *inside whatever space is
    // actually empty* rather than being jammed into the literal corner, which
    // is usually somebody's border.
    let target = (room.right() as i32, room.bottom() as i32);
    let map = BlankMap::new(f.buffer_mut(), room);
    let Some(box_) = map.nearest_clear(w, h, target) else {
        return;
    };

    // The real image where the terminal can draw one, the quadrant rendering
    // everywhere else. Any failure encoding the image falls through to the
    // quadrants rather than leaving a hole.
    if let Some(gfx) = app.graphics.as_ref() {
        if let Some(protocol) = gfx.protocol(&robot, box_) {
            f.render_widget(ratatui_image::Image::new(&protocol), box_);
            return;
        }
    }
    f.render_widget(robot, box_);
}

/// The Run screen before anything has run./// The Run screen before anything has run./// The Run screen before anything has run.
///
/// This screen is reachable with Tab, so arriving here having started nothing
/// is normal. It used to render the progress gauges regardless, which labelled
/// an empty run "scanning…" — a screen that claims to be working when it is
/// doing nothing is worse than a blank one, because the operator waits on it.
fn idle(f: &mut Frame, area: Rect, app: &App) {
    let ready = app.blockers(false).is_empty();
    let mut lines = vec![
        Line::styled(
            "Nothing has run yet.",
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Line::raw(""),
        Line::from(vec![
            Span::styled("d", Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
            Span::raw("  preview — scans and reports what it found, posts nothing"),
        ]),
        Line::from(vec![
            Span::styled("r", Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
            Span::raw("  run — scans, classifies, and stages for review"),
        ]),
        Line::from(vec![
            Span::styled("R", Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
            Span::raw("  open a queue from an earlier run"),
        ]),
        Line::raw(""),
        Line::from(vec![
            Span::styled("source  ", Style::default().fg(MUTED)),
            Span::raw(app.config.source.title()),
            Span::styled("   path  ", Style::default().fg(MUTED)),
            Span::raw(if app.config.source.takes_path() {
                app.config.path.clone()
            } else {
                "—".into()
            }),
        ]),
    ];
    if !ready {
        lines.push(Line::raw(""));
        for b in app.blockers(false) {
            lines.push(Line::styled(
                format!("✗ {}", b.why),
                Style::default().fg(DANGER),
            ));
        }
        lines.push(Line::styled(
            "A preview (d) needs none of these — it posts nothing.",
            Style::default().fg(MUTED).add_modifier(Modifier::ITALIC),
        ));
    }
    f.render_widget(
        Paragraph::new(lines).wrap(Wrap { trim: true }).block(
            Block::bordered()
                .title(" Run ")
                .border_style(MUTED)
                .padding(Padding::new(2, 2, 1, 1)),
        ),
        area,
    );
}

const SPINNER: [&str; 8] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧"];

pub fn spinner_frame(frame: u64) -> &'static str {
    SPINNER[(frame / 2) as usize % SPINNER.len()]
}

/// A frame of the spinner, or a settled marker once the run is over.
fn spinner(app: &App) -> &'static str {
    if !app.in_flight() {
        return "·";
    }
    spinner_frame(app.frame)
}

fn clock(app: &App) -> String {
    match app.elapsed() {
        Some(d) => {
            let s = d.as_secs();
            format!("{:02}:{:02}", s / 60, s % 60)
        }
        None => "--:--".to_string(),
    }
}

fn gauges(f: &mut Frame, area: Rect, app: &App) {
    let p = &app.progress;
    let block = Block::bordered()
        .title(if p.dry_run {
            " Preview (nothing is posted) "
        } else {
            " Progress "
        })
        .border_style(MUTED)
        .padding(Padding::horizontal(1));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .split(inner);

    f.render_widget(
        Gauge::default()
            .gauge_style(Style::default().fg(ACCENT))
            .ratio(p.classify_ratio())
            .label(if p.total == 0 {
                // No unit count exists until the walk ends, so the only honest
                // things to show are the clock and how many sources it has
                // opened. Both keep moving; a bare "scanning…" does not, and
                // a frozen label is what reads as a hang.
                format!(
                    "{} scanning — {} source(s) seen · {}",
                    spinner(app),
                    p.scanning_seen,
                    clock(app)
                )
            } else {
                format!(
                    "{} {}/{} units · {}",
                    spinner(app),
                    p.current,
                    p.total,
                    clock(app)
                )
            }),
        rows[0],
    );

    match p.budget_ratio(app.max_tokens()) {
        Some(ratio) => f.render_widget(
            Gauge::default()
                .gauge_style(Style::default().fg(if ratio > 0.85 { DANGER } else { GOOD }))
                .ratio(ratio)
                .label(format!(
                    "{} / {} tokens",
                    p.tokens,
                    app.max_tokens().unwrap_or_default()
                )),
            rows[1],
        ),
        None => f.render_widget(
            Paragraph::new(Line::styled(
                format!("{} tokens spent · no budget set", p.tokens),
                Style::default().fg(MUTED),
            )),
            rows[1],
        ),
    }

    f.render_widget(
        Paragraph::new(Line::from(vec![
            stat("documents", p.documents.to_string(), Color::Reset),
            stat("units", p.units.to_string(), Color::Reset),
            stat("classified", p.classified.to_string(), GOOD),
            stat("fallback", p.fallbacks.to_string(), CAUTION),
            stat(
                "failed",
                p.failed.to_string(),
                if p.failed > 0 { DANGER } else { MUTED },
            ),
            stat("skipped", p.excluded_total().to_string(), MUTED),
        ])),
        rows[2],
    );

    f.render_widget(
        Paragraph::new(Line::styled(
            if p.current_origin.is_empty() {
                String::new()
            } else {
                format!("→ {}", p.current_origin)
            },
            Style::default().fg(MUTED).add_modifier(Modifier::ITALIC),
        )),
        rows[3],
    );
}

fn stat(label: &str, value: String, color: Color) -> Span<'static> {
    Span::styled(
        format!("{label} {value}   "),
        Style::default().fg(color),
    )
}

/// A horizontal bar chart. Horizontal because the labels are sentences —
/// "not engineering knowledge" does not fit under a vertical bar.
///
/// # Why the labels are truncated here rather than left to the widget
///
/// `BarChart`'s horizontal renderer computes `area.width - label_size - margin`
/// without checking the order, so a label wider than the area underflows and
/// panics (ratatui 0.29, barchart.rs:433). A narrow pane is not an exotic case:
/// it is any tmux split, and a panic mid-draw leaves the terminal in raw mode.
/// So the labels are cut to fit and a pane too narrow for a chart falls back to
/// text.
fn histogram(f: &mut Frame, area: Rect, title: &str, data: Vec<(String, u64)>, color: Color) {
    let block = Block::bordered()
        .title(title.to_string())
        .border_style(MUTED)
        .padding(Padding::horizontal(1));
    let inner = block.inner(area);

    if data.is_empty() {
        f.render_widget(
            Paragraph::new(Line::styled("nothing yet", Style::default().fg(MUTED))).block(block),
            area,
        );
        return;
    }

    // A bar needs room for its label, a gap, and something to draw.
    const MIN_BAR: u16 = 6;
    const MIN_LABEL: u16 = 6;
    if inner.height == 0 || inner.width < MIN_LABEL + MIN_BAR + 1 {
        let lines: Vec<Line> = data
            .iter()
            .take(inner.height as usize)
            .map(|(k, v)| Line::styled(format!("{v} {k}"), Style::default().fg(color)))
            .collect();
        f.render_widget(Paragraph::new(lines).block(block), area);
        return;
    }

    let max_label = inner.width.saturating_sub(MIN_BAR + 1) as usize;
    let labels: Vec<(String, u64)> = data
        .iter()
        .take(inner.height as usize)
        .map(|(k, v)| {
            let label = if k.chars().count() > max_label {
                k.chars().take(max_label.saturating_sub(1)).collect::<String>() + "…"
            } else {
                k.clone()
            };
            (label, *v)
        })
        .collect();
    let shown: Vec<(&str, u64)> = labels.iter().map(|(k, v)| (k.as_str(), *v)).collect();

    f.render_widget(
        BarChart::default()
            .block(block)
            .direction(Direction::Horizontal)
            .bar_width(1)
            .bar_gap(0)
            .bar_style(Style::default().fg(color))
            .value_style(Style::default().fg(Color::Black).bg(color))
            .label_style(Style::default().fg(Color::Reset))
            .data(&shown),
        area,
    );
}

// ── Review ───────────────────────────────────────────────────────────────────

fn review(f: &mut Frame, area: Rect, app: &App) {
    if app.picking_run() {
        return run_picker(f, area, app);
    }
    let rows = Layout::vertical([Constraint::Percentage(55), Constraint::Percentage(45)])
        .split(area);

    let items: Vec<ListItem> = app
        .candidates
        .iter()
        .map(|c| {
            let picked = app.selected.contains(&c.id);
            let locked = c.needs_individual_review();
            let conf = c.confidence.unwrap_or(0.0);
            let conf_color = if conf < 0.5 {
                DANGER
            } else if conf < 0.8 {
                CAUTION
            } else {
                GOOD
            };
            ListItem::new(Line::from(vec![
                Span::raw(if picked { "[x] " } else { "[ ] " }),
                Span::styled(
                    format!("{:>4.0}%  ", conf * 100.0),
                    Style::default().fg(conf_color),
                ),
                Span::styled(
                    format!("{:<12}", c.destination_kind),
                    Style::default().fg(ACCENT),
                ),
                Span::raw(c.title()),
                Span::styled(
                    if locked { "  ⚠ needs its own decision" } else { "" },
                    Style::default().fg(CAUTION),
                ),
            ]))
        })
        .collect();

    let title = format!(
        " Queue — {} staged, {} selected ",
        app.candidates.len(),
        app.selected.len()
    );
    let mut state = ListState::default();
    state.select(Some(app.review_cursor));
    f.render_stateful_widget(
        List::new(items)
            .block(
                Block::bordered()
                    .title(title)
                    .border_style(MUTED)
                    .padding(Padding::horizontal(1)),
            )
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED)),
        rows[0],
        &mut state,
    );

    let detail = match app.candidates.get(app.review_cursor) {
        None => vec![Line::styled(
            "Nothing staged. Press R on the run screen to reload the queue.",
            Style::default().fg(MUTED),
        )],
        Some(c) => {
            let mut lines = vec![
                Line::styled(c.title(), Style::default().add_modifier(Modifier::BOLD)),
                Line::styled(
                    format!("{}  ·  {}  ·  v{}", c.source_identity, c.provenance_kind, c.version),
                    Style::default().fg(MUTED),
                ),
            ];
            if c.needs_individual_review() {
                lines.push(Line::styled(
                    "This candidate is excluded from batch approval by design.",
                    Style::default().fg(CAUTION),
                ));
            }
            if app.conflicts.contains(&c.id) {
                lines.push(Line::styled(
                    "Changed under you since the queue was loaded — your decision was not \
                     applied. Read it again before deciding.",
                    Style::default().fg(DANGER),
                ));
            }
            lines.push(Line::raw(""));
            lines.extend(c.content.lines().take(11).map(|l| Line::raw(l.to_string())));
            if let Some(excerpt) = &c.source_excerpt {
                lines.push(Line::raw(""));
                lines.push(Line::styled(
                    format!("from the source ({}):", c.status),
                    Style::default().fg(MUTED),
                ));
                lines.extend(
                    excerpt
                        .lines()
                        .take(3)
                        .map(|l| Line::styled(format!("  {l}"), Style::default().fg(MUTED))),
                );
            }
            lines
        }
    };
    f.render_widget(
        Paragraph::new(detail).wrap(Wrap { trim: false }).block(
            Block::bordered()
                .title(" Candidate ")
                .border_style(MUTED)
                .padding(Padding::horizontal(1)),
        ),
        rows[1],
    );
}

/// The list of runs on this backend.
///
/// A review queue outlives the process that filled it, so the TUI has to be
/// able to open one it did not create — otherwise an interrupted review is
/// unfinishable from here.
fn run_picker(f: &mut Frame, area: Rect, app: &App) {
    // After a monorepo run, offer this session's per-project runs first — they
    // are labelled by project and are exactly what was just staged. `R` still
    // loads the full backend history over this view.
    if app.showing_session_runs() {
        return session_run_picker(f, area, app);
    }
    if app.runs.is_empty() {
        f.render_widget(
            Paragraph::new(vec![
                Line::styled(
                    "No run selected.",
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Line::raw(""),
                Line::raw("Press R to list the runs on this backend, or start one with r."),
                Line::styled(
                    "A queue lives in the backend, so a review can always be finished later.",
                    Style::default().fg(MUTED).add_modifier(Modifier::ITALIC),
                ),
            ])
            .wrap(Wrap { trim: true })
            .block(
                Block::bordered()
                    .title(" Review ")
                    .border_style(MUTED)
                    .padding(Padding::new(2, 2, 1, 1)),
            ),
            area,
        );
        return;
    }

    let items: Vec<ListItem> = app
        .runs
        .iter()
        .map(|r| {
            let status_color = match r.status.as_str() {
                "completed" => GOOD,
                "failed" => DANGER,
                _ => CAUTION,
            };
            ListItem::new(Line::from(vec![
                Span::styled(format!("{:<11}", r.status), Style::default().fg(status_color)),
                Span::styled(format!("{:<16}", r.source_kind), Style::default().fg(ACCENT)),
                Span::styled(format!("{:<22}", r.created_at), Style::default().fg(MUTED)),
                Span::raw(r.source_ref.clone().unwrap_or_default()),
            ]))
        })
        .collect();
    let rows = Layout::vertical([Constraint::Percentage(55), Constraint::Percentage(45)])
        .split(area);

    let mut state = ListState::default();
    state.select(Some(app.run_cursor));
    f.render_stateful_widget(
        List::new(items)
            .block(
                Block::bordered()
                    .title(format!(" Runs ({}) — Enter opens · X cancels ", app.runs.len()))
                    .border_style(MUTED)
                    .padding(Padding::horizontal(1)),
            )
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED)),
        rows[0],
        &mut state,
    );

    let detail: Vec<Line> = match app.runs.get(app.run_cursor) {
        None => vec![Line::styled("—", Style::default().fg(MUTED))],
        Some(r) => {
            let mut lines = vec![Line::styled(
                r.id.clone(),
                Style::default().add_modifier(Modifier::BOLD),
            )];
            for (label, value) in [
                ("source", r.source_kind.clone()),
                ("status", r.status.clone()),
                ("source ref", r.source_ref.clone().unwrap_or_else(|| "—".into())),
                ("client", r.client_id.clone().unwrap_or_else(|| "—".into())),
                ("project", r.project_id.clone().unwrap_or_else(|| "—".into())),
                (
                    "runner",
                    r.runner_version.clone().unwrap_or_else(|| "—".into()),
                ),
                ("created by", r.created_by.clone()),
                ("created", r.created_at.clone()),
                ("updated", r.updated_at.clone()),
            ] {
                lines.push(Line::from(vec![
                    Span::styled(format!("  {label:<12}"), Style::default().fg(MUTED)),
                    Span::raw(value),
                ]));
            }
            if !r.attestation.is_null()
                && r.attestation.as_object().map(|o| !o.is_empty()) == Some(true)
            {
                lines.push(Line::styled(
                    format!("  attestation  {}", r.attestation),
                    Style::default().fg(CAUTION),
                ));
            }
            lines.push(Line::raw(""));
            lines.push(Line::styled(
                "X cancels this run's pending candidates. There is no delete: removing a \
                 run would cascade away the provenance of everything it already committed.",
                Style::default().fg(MUTED).add_modifier(Modifier::ITALIC),
            ));
            lines
        }
    };
    f.render_widget(
        Paragraph::new(detail).wrap(Wrap { trim: true }).block(
            Block::bordered()
                .title(" Run ")
                .border_style(MUTED)
                .padding(Padding::horizontal(1)),
        ),
        rows[1],
    );
}

/// This session's per-project runs, from a monorepo migration. Each row is one
/// project's queue; opening one reviews it exactly like any other run.
fn session_run_picker(f: &mut Frame, area: Rect, app: &App) {
    let created = &app.session_runs;
    let items: Vec<ListItem> = created
        .iter()
        .map(|r| {
            ListItem::new(Line::from(vec![
                Span::styled(format!("{:<20}", r.alias), Style::default().fg(ACCENT)),
                Span::styled(
                    format!("{:<26}", r.project_id),
                    Style::default().fg(MUTED),
                ),
                Span::raw(r.run_id.clone()),
            ]))
        })
        .collect();
    let mut state = ListState::default();
    state.select(Some(app.run_cursor.min(created.len().saturating_sub(1))));
    f.render_stateful_widget(
        List::new(items)
            .block(
                Block::bordered()
                    .title(format!(
                        " This run's projects ({}) — Enter reviews · R backend history ",
                        created.len()
                    ))
                    .border_style(ACCENT)
                    .padding(Padding::horizontal(1)),
            )
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED)),
        area,
        &mut state,
    );
}

// ── Projects (monorepo plan) ─────────────────────────────────────────────────

fn projects(f: &mut Frame, area: Rect, app: &App) {
    if app.plan.is_empty() {
        let note = if app.plan_note.is_empty() {
            "Set a path on the Options screen, then return here."
        } else {
            &app.plan_note
        };
        f.render_widget(
            Paragraph::new(vec![
                Line::styled(
                    "Monorepo plan",
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Line::raw(""),
                Line::styled(note.to_string(), Style::default().fg(MUTED)),
                Line::raw(""),
                Line::styled(
                    "A monorepo routes each package into its own project. When one is \
                     detected, you decide here — create a new project, route into an \
                     existing one, or skip it — before anything is created.",
                    Style::default().fg(MUTED),
                ),
            ])
            .wrap(Wrap { trim: true })
            .block(
                Block::bordered()
                    .title(" Projects ")
                    .border_style(MUTED)
                    .padding(Padding::horizontal(1)),
            ),
            area,
        );
        return;
    }

    let rows = Layout::vertical([Constraint::Min(6), Constraint::Length(6)]).split(area);

    let items: Vec<ListItem> = app
        .plan
        .iter()
        .map(|row| {
            let (badge, badge_color) = match &row.action {
                Action::Create => ("＋ create ", GOOD),
                Action::Select(_) => ("→ existing", ACCENT),
                Action::Skip => ("∅ skip   ", MUTED),
            };
            let target = match &row.action {
                Action::Select(id) => {
                    let name = row
                        .matched
                        .as_ref()
                        .map(|p| p.name.clone())
                        .unwrap_or_else(|| id.clone());
                    format!("  ▸ {name}")
                }
                Action::Create => format!("  ▸ {} (new)", row.detected.name),
                Action::Skip => String::new(),
            };
            let where_ = if row.detected.rel_dir.is_empty() {
                "· repository root".to_string()
            } else {
                row.detected.rel_dir.clone()
            };
            ListItem::new(Line::from(vec![
                Span::styled(format!("{badge}  "), Style::default().fg(badge_color)),
                Span::styled(
                    format!("{where_:<22}"),
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::styled(target, Style::default().fg(MUTED)),
            ]))
        })
        .collect();

    let (create, select, skip) = app.plan_summary();
    let title = format!(
        " Sub-projects — {} create, {} existing, {} skip ",
        create, select, skip
    );
    let mut state = ListState::default();
    state.select(Some(app.plan_cursor));
    f.render_stateful_widget(
        List::new(items)
            .block(
                Block::bordered()
                    .title(title)
                    .border_style(MUTED)
                    .padding(Padding::horizontal(1)),
            )
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED)),
        rows[0],
        &mut state,
    );

    // The footer of the screen: what the focused row was matched on, plus the
    // one thing the operator must know before confirming — that a config will be
    // written, and whether it overwrites one.
    let mut detail: Vec<Line> = Vec::new();
    if let Some(row) = app.plan.get(app.plan_cursor) {
        detail.push(Line::from(vec![
            Span::styled(
                format!("{} ", row.detected.name),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("· found via {} · routes {}", row.detected.via, row.detected.route_glob()),
                Style::default().fg(MUTED),
            ),
        ]));
        match &row.matched {
            Some(p) => detail.push(Line::styled(
                format!("name matches existing project “{}”", p.name),
                Style::default().fg(ACCENT),
            )),
            None => detail.push(Line::styled(
                "no project of this name — Enter creates one, s picks another",
                Style::default().fg(MUTED),
            )),
        }
    }
    if create + select == 0 {
        detail.push(Line::styled(
            "everything is skipped — nothing would be migrated",
            Style::default().fg(CAUTION),
        ));
    } else {
        // The two layouts execute differently, and the line has to say which:
        // one routed run over a checkout, or one run per repository.
        let (text, tone) = match app.plan_layout {
            crate::monorepo::Layout::Monorepo => (
                format!(
                    "r writes .nexusmind.yaml and runs {create} new + {select} existing project(s){}",
                    if app.existing_config {
                        " — overwrites the one already here"
                    } else {
                        ""
                    }
                ),
                if app.existing_config { CAUTION } else { GOOD },
            ),
            crate::monorepo::Layout::RepoCollection => (
                format!(
                    "r runs {} repositor{} one at a time ({create} new project(s), {select} existing) \
                     — no config file, each repo is its own run",
                    create + select,
                    if create + select == 1 { "y" } else { "ies" }
                ),
                GOOD,
            ),
        };
        detail.push(Line::styled(text, Style::default().fg(tone)));
    }
    f.render_widget(
        Paragraph::new(detail).wrap(Wrap { trim: true }).block(
            Block::bordered()
                .title(" Plan ")
                .border_style(MUTED)
                .padding(Padding::horizontal(1)),
        ),
        rows[1],
    );

    if app.selecting_for.is_some() {
        existing_project_picker(f, area, app);
    }
}

/// The overlay for routing a row into an existing backend project.
fn existing_project_picker(f: &mut Frame, area: Rect, app: &App) {
    let area = centered(60, 60, area);
    f.render_widget(Clear, area);
    let items: Vec<ListItem> = app
        .existing_projects
        .iter()
        .map(|p| {
            let client = p
                .client_id
                .as_deref()
                .map(|c| format!("  ({c})"))
                .unwrap_or_else(|| "  (internal)".to_string());
            ListItem::new(Line::from(vec![
                Span::raw(p.name.clone()),
                Span::styled(client, Style::default().fg(MUTED)),
            ]))
        })
        .collect();
    let mut state = ListState::default();
    state.select(Some(app.select_cursor));
    f.render_stateful_widget(
        List::new(items)
            .block(
                Block::bordered()
                    .title(" Route into which project? ")
                    .border_style(ACCENT)
                    .padding(Padding::horizontal(1)),
            )
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED)),
        area,
        &mut state,
    );
}

// ── Summary ──────────────────────────────────────────────────────────────────

fn summary(f: &mut Frame, area: Rect, app: &App) {
    let p = &app.progress;
    let mut lines: Vec<Line> = Vec::new();

    let (headline, tone) = match (&app.last_command, &p.finished) {
        (Some(LastCommand::Commit(r)), _) => (
            format!(
                "Committed {} · skipped {} · failed {} · indexed {} · awaiting indexing {}",
                r.committed, r.skipped, r.failed, r.indexed, r.pending_index
            ),
            if r.failed > 0 { CAUTION } else { GOOD },
        ),
        (Some(LastCommand::Review { applied, conflicts }), _) => (
            format!("{applied} decision(s) applied, {conflicts} conflict(s)"),
            if *conflicts > 0 { CAUTION } else { GOOD },
        ),
        (_, Some(fin)) if !fin.ok => (
            format!(
                "Run failed — {}",
                fin.error.clone().unwrap_or_else(|| "no reason given".into())
            ),
            DANGER,
        ),
        (_, Some(fin)) if fin.aborted_on_budget => (
            "Token budget reached — the run stopped cleanly and is resumable".to_string(),
            CAUTION,
        ),
        (Some(LastCommand::Preview), _) => (
            format!(
                "Preview — {} document(s), {} unit(s), ≈{} tokens to classify",
                p.documents, p.units, p.estimated_tokens
            ),
            ACCENT,
        ),
        // Report what actually reached the queue, not how many units were
        // classified. A rescan of an already-migrated source classifies every
        // unit and stages none of them — saying "12 staged" there is the exact
        // lie that makes an empty review screen look like a bug.
        (_, Some(_)) => match p.staged {
            Some((0, skipped, rejected)) if skipped + rejected > 0 => (
                format!(
                    "Run finished — nothing new to review ({skipped} already migrated \
                     or previously rejected, {rejected} rejected now)"
                ),
                CAUTION,
            ),
            Some((staged, _, _)) => (
                format!("Run finished — {staged} candidate(s) staged for review"),
                GOOD,
            ),
            None => (
                format!("Run finished — {} unit(s) classified", p.classified + p.fallbacks),
                GOOD,
            ),
        },
        _ => ("Nothing has been run yet.".to_string(), MUTED),
    };
    lines.push(Line::styled(
        headline,
        Style::default().fg(tone).add_modifier(Modifier::BOLD),
    ));
    lines.push(Line::raw(""));

    let rows: Vec<(&str, String)> = vec![
        ("source", p.source.clone()),
        ("documents scanned", p.documents.to_string()),
        ("units found", p.units.to_string()),
        ("bytes read", p.bytes.to_string()),
        ("skipped", p.excluded_total().to_string()),
        ("classified by the model", p.classified.to_string()),
        ("deterministic fallback", p.fallbacks.to_string()),
        ("failed", p.failed.to_string()),
        ("tokens spent", p.tokens.to_string()),
    ];
    for (k, v) in rows {
        lines.push(Line::from(vec![
            Span::styled(format!("  {k:<26}"), Style::default().fg(MUTED)),
            Span::raw(v),
        ]));
    }

    if !p.excluded.is_empty() {
        lines.push(Line::raw(""));
        lines.push(Line::styled("  skipped, by reason", Style::default().fg(MUTED)));
        for (reason, count, sample) in &p.excluded {
            lines.push(Line::from(vec![
                Span::styled(format!("    {count:>7}  "), Style::default().fg(CAUTION)),
                Span::raw(reason.clone()),
            ]));
            // One concrete path per reason: a count nobody can check is a count
            // nobody believes.
            lines.push(Line::styled(
                format!("             e.g. {sample}"),
                Style::default().fg(MUTED).add_modifier(Modifier::ITALIC),
            ));
        }
    }

    if let Some((staged, skipped, rejected)) = p.staged {
        lines.push(Line::raw(""));
        lines.push(Line::from(vec![
            Span::styled("  staged into the queue    ", Style::default().fg(MUTED)),
            Span::styled(staged.to_string(), Style::default().fg(GOOD)),
            Span::styled(
                format!("   (skipped {skipped}, rejected {rejected})"),
                Style::default().fg(MUTED),
            ),
        ]));
    }
    // A monorepo run staged one queue per project. List them so the operator
    // knows there is more than one to review, and that R opens the picker.
    if app.session_runs.len() > 1 {
        lines.push(Line::raw(""));
        lines.push(Line::styled(
            format!("  {} project queue(s) staged — R opens the picker", app.session_runs.len()),
            Style::default().fg(ACCENT),
        ));
        for r in &app.session_runs {
            lines.push(Line::from(vec![
                Span::styled(format!("    {:<20}", r.alias), Style::default().fg(ACCENT)),
                Span::styled(r.run_id.clone(), Style::default().fg(MUTED)),
            ]));
        }
    } else if let Some(id) = &p.run_id {
        lines.push(Line::from(vec![
            Span::styled("  run id                   ", Style::default().fg(MUTED)),
            Span::raw(id.clone()),
        ]));
    }
    if p.unknown_events > 0 {
        lines.push(Line::styled(
            format!(
                "  {} event(s) came from a newer runner than this TUI understands",
                p.unknown_events
            ),
            Style::default().fg(CAUTION),
        ));
    }

    let next = match (&app.last_command, &p.finished) {
        (Some(LastCommand::Preview), _) => {
            "Next: press r to run for real, or narrow the scan with Include on the options screen."
        }
        (Some(LastCommand::Commit(_)), _) => {
            "Committed knowledge is vectorized and searchable. Nothing else is pending."
        }
        // A finished run that staged nothing has an empty queue by design — a
        // rescan of an already-migrated source. Do not send the operator to a
        // review screen that will correctly show zero.
        (_, Some(fin)) if fin.ok && matches!(p.staged, Some((0, s, r)) if s + r > 0) => {
            "This source was already migrated — its units are committed or were rejected \
             before, so there is nothing new to review. Change or add sources, or edit the \
             source to produce new units."
        }
        (_, Some(fin)) if fin.ok && p.run_id.is_some() => {
            "Next: press R to open the review queue. Nothing is written until you commit."
        }
        (_, Some(fin)) if !fin.ok => {
            "Anything already staged survived. Fix the cause and run again — identities dedupe."
        }
        _ => "Next: pick a source and press d to preview it.",
    };
    // The next step is pinned to its own row rather than appended to the body.
    // The body grows with the number of exclusion reasons, and the one line the
    // operator needs is the one that must never be the one pushed off screen.
    let rows = Layout::vertical([Constraint::Min(4), Constraint::Length(3)]).split(area);
    f.render_widget(
        Paragraph::new(lines).wrap(Wrap { trim: true }).block(
            Block::bordered()
                .title(" Summary ")
                .border_style(MUTED)
                .padding(Padding::new(2, 2, 1, 0)),
        ),
        rows[0],
    );
    f.render_widget(
        Paragraph::new(Line::styled(
            next,
            Style::default().fg(ACCENT).add_modifier(Modifier::ITALIC),
        ))
        .wrap(Wrap { trim: true })
        .block(
            Block::bordered()
                .title(" Next ")
                .border_style(ACCENT)
                .padding(Padding::horizontal(1)),
        ),
        rows[1],
    );
}

// ── Help ─────────────────────────────────────────────────────────────────────

fn help_overlay(f: &mut Frame, app: &App) {
    let area = centered(72, 74, f.area());
    f.render_widget(Clear, area);
    let lines = vec![
        Line::styled(
            "What this does",
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ),
        Line::raw("Scans a source, classifies each unit locally, and stages the result"),
        Line::raw("for review. Staging is not writing: nothing reaches the knowledge"),
        Line::raw("base until you approve candidates and press C to commit."),
        Line::raw(""),
        Line::styled(
            "Three things that cannot be turned off",
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ),
        Line::raw("• Redaction runs before staging, so secrets never reach the queue."),
        Line::raw("• Third-party plugin assets and session transcripts are never read."),
        Line::raw("• Reading client rows needs an allowlist, a row cap, redaction and"),
        Line::raw("  a recorded attestation — all four, every time."),
        Line::raw(""),
        Line::styled(
            "Monorepos",
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ),
        Line::raw("The Projects screen detects each package under the path and routes it"),
        Line::raw("into its own NexusMind project — create a new one, pick an existing"),
        Line::raw("one, or skip it. Inside one checkout that means a .nexusmind.yaml and"),
        Line::raw("a single routed run; a folder holding separate repos runs one per repo."),
        Line::raw("Nothing is created on the backend until you confirm."),
        Line::raw(""),
        Line::styled(
            "Keys",
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ),
        Line::raw("Tab / ⇧Tab   move between screens"),
        Line::raw("↑ ↓          move between fields or candidates"),
        Line::raw("Enter        edit a field (Esc or Enter to leave)"),
        Line::raw("Space        toggle a switch, or select a candidate"),
        Line::raw("Enter / s    (Projects) cycle a row's action / pick an existing project"),
        Line::raw("d            dry run — scan and report, post nothing"),
        Line::raw("r            run — scan, classify, stage for review"),
        Line::raw("x            stop a run (staged work survives)"),
        Line::raw("e            cycle the activity panel: both / agents / logs"),
        Line::raw("↑ ↓ / f      inspect an exchange / resume following the newest"),
        Line::raw("R            load the review queue"),
        Line::raw("a / j        approve / reject the candidate under the cursor"),
        Line::raw("A            approve everything selected"),
        Line::raw("C            commit approved candidates"),
        Line::raw("t            test the backend connection"),
        Line::raw("m            hide or show the mascot (decoration only)"),
        Line::raw(""),
        Line::raw("? / q        this help / quit"),
    ];
    let lines = {
        let mut lines = lines;
        // Says which of the two renderers is in use, because "why is the robot
        // blocky" is otherwise unanswerable from inside the app.
        lines.push(Line::styled(
            format!("mascot renderer: {}", app.mascot_backend()),
            Style::default().fg(MUTED).add_modifier(Modifier::ITALIC),
        ));
        lines
    };
    f.render_widget(
        Paragraph::new(lines).wrap(Wrap { trim: true }).block(
            Block::bordered()
                .title(" nexusmind-migrate ")
                .border_style(ACCENT)
                .padding(Padding::new(2, 2, 1, 1)),
        ),
        area,
    );
}

fn centered(pct_x: u16, pct_y: u16, area: Rect) -> Rect {
    let v = Layout::vertical([
        Constraint::Percentage((100 - pct_y) / 2),
        Constraint::Percentage(pct_y),
        Constraint::Percentage((100 - pct_y) / 2),
    ])
    .split(area);
    Layout::horizontal([
        Constraint::Percentage((100 - pct_x) / 2),
        Constraint::Percentage(pct_x),
        Constraint::Percentage((100 - pct_x) / 2),
    ])
    .split(v[1])[1]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::{Candidate, Run};
    use crate::app::{LastCommand, Screen};
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn render(app: &App, w: u16, h: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
        terminal.draw(|f| draw(f, app)).unwrap();
        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect::<Vec<_>>()
            .chunks(w as usize)
            .map(|row| row.concat())
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn populated() -> App {
        let mut app = App::new();
        app.progress.source = "repo_docs".into();
        app.progress.documents = 3;
        app.progress.units = 69;
        app.progress.total = 69;
        app.progress.current = 41;
        app.progress.tokens = 9_100;
        app.progress.classified = 38;
        app.progress.fallbacks = 3;
        app.progress.run_id = Some("run-7f2".into());
        app.progress.staged = Some((69, 0, 0));
        *app.progress
            .by_destination
            .entry("memory".into())
            .or_default() = 40;
        *app.progress
            .by_destination
            .entry("convention".into())
            .or_default() = 12;
        app.progress.excluded = vec![
            ("not engineering knowledge".into(), 2, "docs/marketing/a.md".into()),
            ("third-party asset".into(), 1, "node_modules/x/LICENSE.md".into()),
        ];
        app.config.max_tokens = "50000".into();
        app.candidates = vec![Candidate {
            id: "c1".into(),
            source_identity: "repo-docs:nexusmind:docs/adr/ADR-001.md#context:9ab".into(),
            destination_kind: "memory".into(),
            content: "The backend never calls a model.".into(),
            destination_hint: serde_json::json!({ "title": "BYOM is non-negotiable" }),
            source_excerpt: Some("Nunca dependemos de un proveedor de LLM.".into()),
            confidence: Some(0.42),
            attestation: serde_json::json!({}),
            provenance_kind: "migrated".into(),
            status: "staged".into(),
            version: 1,
        }];
        app.runs = vec![Run {
            id: "run-7f2".into(),
            source_kind: "repo_docs".into(),
            status: "completed".into(),
            source_ref: Some("./nexusmind".into()),
            created_at: "2026-08-15T09:00:00Z".into(),
            client_id: Some("ba3ba5e9-7f81-4f9b-b8d1-46a3d3a6439a".into()),
            project_id: None,
            runner_version: Some("claude 2.1".into()),
            created_by: "cesar@u2s.local".into(),
            updated_at: "2026-08-15T09:04:00Z".into(),
            attestation: serde_json::json!({}),
        }];
        app
    }

    const SCREENS: [Screen; 7] = [
        Screen::Connection,
        Screen::Source,
        Screen::Options,
        Screen::Projects,
        Screen::Running,
        Screen::Review,
        Screen::Summary,
    ];

    /// Layout arithmetic panics on small terminals — a split that assumes more
    /// rows than exist, or `Block::inner` on a zero-height rect. Nobody resizes
    /// their terminal to 20×6 on purpose, but a tmux pane during a resize is
    /// exactly that for one frame, and a panic there leaves the terminal in raw
    /// mode with no way back.
    #[test]
    fn every_screen_renders_at_every_size_without_panicking() {
        for size in [(20, 6), (40, 12), (80, 24), (200, 60)] {
            for screen in SCREENS {
                for help in [false, true] {
                    let mut app = populated();
                    app.goto(screen);
                    app.show_help = help;
                    render(&app, size.0, size.1);
                }
            }
        }
    }

    #[test]
    fn an_empty_app_renders_too() {
        for screen in SCREENS {
            let mut app = App::new();
            app.goto(screen);
            render(&app, 80, 24);
        }
    }

    /// The API key must not be legible on any screen, at any width.
    #[test]
    fn the_api_key_is_never_drawn_in_full() {
        let mut app = populated();
        app.config.api_key = "nm_0000000000000000000000000000000000".into();
        for screen in SCREENS {
            app.goto(screen);
            let screen_text = render(&app, 200, 60);
            assert!(
                !screen_text.contains("nm_0000000000000000000000000000000000"),
                "the key appeared on {screen:?}"
            );
        }
    }

    /// The DSN is the one secret that would be catastrophic on a shared screen.
    #[test]
    fn the_dsn_is_never_drawn() {
        let mut app = populated();
        app.config.source = Source::DbSchema;
        app.config.dsn = "postgres://ro:hunter2@db.internal/app".into();
        for screen in SCREENS {
            app.goto(screen);
            let text = render(&app, 200, 60);
            assert!(!text.contains("hunter2"), "the DSN leaked onto {screen:?}");
        }
    }

    #[test]
    fn a_remote_backend_is_labelled_on_the_connection_screen() {
        let mut app = populated();
        app.config.api_url = "https://api.nexusmind.smartcoderlabs.com".into();
        app.goto(Screen::Connection);
        let text = render(&app, 120, 24);
        assert!(text.contains("REMOTE"), "{text}");

        app.config.api_url = "http://localhost:8080".into();
        let text = render(&app, 120, 24);
        assert!(text.contains("local"), "{text}");
        assert!(!text.contains("REMOTE"));
    }

    #[test]
    fn the_pipeline_diagram_marks_commit_as_the_point_of_no_return() {
        let app = populated();
        let text = render(&app, 160, 24);
        assert!(text.contains("scan"), "{text}");
        assert!(text.contains("commit"), "{text}");
        assert!(
            text.contains("nothing is written until commit"),
            "the one thing an operator needs to know is always on screen: {text}"
        );
    }

    #[test]
    fn the_locked_sampling_gates_are_shown_greyed_rather_than_hidden() {
        let mut app = populated();
        app.config.source = Source::DbSchema;
        app.goto(Screen::Options);
        let text = render(&app, 140, 30);
        for label in ["Table allowlist", "Rows per table", "Redact PII", "attestation"] {
            assert!(
                text.contains(label),
                "{label} must stay visible so its cost is known before unlocking: {text}"
            );
        }
    }

    #[test]
    fn a_low_confidence_candidate_shows_its_score_and_source() {
        let mut app = populated();
        app.picked_run = Some("run-7f2".into());
        app.goto(Screen::Review);
        let text = render(&app, 140, 30);
        assert!(text.contains("42%"), "{text}");
        assert!(text.contains("BYOM is non-negotiable"), "{text}");
        assert!(text.contains("Nunca dependemos"), "the excerpt grounds the decision: {text}");
    }

    #[test]
    fn the_review_screen_offers_the_run_list_when_nothing_is_selected() {
        let mut app = populated();
        app.progress.run_id = None;
        app.goto(Screen::Review);
        assert!(app.picking_run());
        let text = render(&app, 140, 30);
        assert!(text.contains("repo_docs"), "{text}");
        assert!(text.contains("completed"), "{text}");
    }

    /// The screenshot bug: Tab reaches the Run screen without starting
    /// anything, and the gauge label said "scanning…" over an empty run. An
    /// operator waits on a screen that claims to be working.
    #[test]
    fn the_run_screen_does_not_claim_to_be_scanning_before_anything_started() {
        let mut app = App::new();
        app.goto(Screen::Running);
        assert!(app.never_ran());
        let text = render(&app, 120, 30);
        assert!(!text.contains("scanning"), "it must not fake progress: {text}");
        assert!(text.contains("Nothing has run yet"), "{text}");
        assert!(text.contains("preview"), "and it must say how to start: {text}");
    }

    /// A preview needs no credentials, so an operator with none must still be
    /// told they can press `d`.
    #[test]
    fn the_idle_screen_names_the_blockers_but_still_offers_a_preview() {
        let mut app = App::new();
        app.config.api_key.clear();
        app.config.api_url.clear();
        app.goto(Screen::Running);
        let text = render(&app, 120, 30);
        assert!(text.contains("API key is required"), "{text}");
        assert!(text.contains("posts nothing"), "{text}");
    }

    /// While the walk is running there are no units yet, so the counter and the
    /// clock are the only proof it is alive. Both must be on screen.
    #[test]
    fn a_scan_in_flight_shows_a_moving_counter_and_a_clock() {
        let mut app = App::new();
        app.last_command = Some(LastCommand::Run);
        app.started_at = Some(std::time::Instant::now());
        app.progress.scanning_seen = 137;
        app.progress.current_origin = "docs/API_SPEC.md".into();
        app.goto(Screen::Running);
        let text = render(&app, 120, 30);
        assert!(text.contains("137 source(s) seen"), "{text}");
        assert!(text.contains("00:00"), "the clock must render: {text}");
        assert!(text.contains("docs/API_SPEC.md"), "{text}");
        assert!(
            text.contains("Walking the source tree"),
            "an empty Activity pane must explain itself: {text}"
        );
    }

    #[test]
    fn the_runs_screen_shows_a_run_in_full_and_offers_cancel_not_delete() {
        let mut app = populated();
        app.progress.run_id = None;
        app.goto(Screen::Review);
        assert!(app.picking_run());
        let text = render(&app, 150, 40);
        for expected in [
            "repo_docs",
            "completed",
            "./nexusmind",
            "ba3ba5e9",           // client
            "claude 2.1",         // runner version
            "cesar@u2s.local",    // created by
        ] {
            assert!(text.contains(expected), "{expected} missing from:\n{text}");
        }
        assert!(text.contains("X cancel"), "cancel must be discoverable: {text}");
        assert!(
            text.contains("provenance"),
            "and the reason there is no delete must be on screen: {text}"
        );
    }

    /// How many cells the mascot painted, measured by rendering the same state
    /// with it off and counting the difference.
    ///
    /// Scanning for block glyphs does not work: gauges and bar charts are made
    /// of `█`, and the mascot's own alphabet is the same range. The difference
    /// against a mascot-free render is the only honest measure.
    fn robot_cells(app: &App, w: u16, h: u16) -> usize {
        let mut off = App::new();
        // Clone the parts that decide what is drawn. `App` is not `Clone`
        // because it owns a child process handle.
        off.screen = app.screen;
        off.config = app.config.clone();
        off.progress = app.progress.clone();
        off.candidates = app.candidates.clone();
        off.runs = app.runs.clone();
        off.picked_run = app.picked_run.clone();
        off.last_command = app.last_command.clone();
        off.started_at = app.started_at;
        off.activity = app.activity;
        off.selected = app.selected.clone();
        off.mascot_on = false;

        let with = render(app, w, h);
        let without = render(&off, w, h);
        with.chars()
            .zip(without.chars())
            .filter(|(a, b)| a != b)
            .count()
    }

    fn with_exchanges(app: &mut App) {
        // Without this the Run screen is idle by definition and renders the
        // "nothing has run yet" panel instead of any of this.
        app.last_command = Some(LastCommand::Run);
        app.started_at = Some(std::time::Instant::now());
        for (i, ok) in [(1usize, true), (2, false)] {
            app.progress
                .apply(crate::runner::RunnerMsg::Line(crate::protocol::ParsedLine::Event(
                    crate::protocol::RunEvent::Agent {
                        index: i,
                        total: 9,
                        origin: format!("docs/adr/ADR-00{i}.md"),
                        prompt: "Classify this file so it can be PROPOSED".into(),
                        response: if ok {
                            r#"{"destination_kind":"memory"}"#.into()
                        } else {
                            "No store_memory call: this turn's job was to propose".into()
                        },
                        ok,
                        error: (!ok).then(|| "no parseable candidate JSON".to_string()),
                        tokens_spent: 1081,
                        duration_ms: 4200,
                    },
                )));
        }
    }

    #[test]
    fn the_agents_panel_shows_the_question_and_the_answer() {
        let mut app = populated();
        with_exchanges(&mut app);
        app.activity = ActivityView::Agents;
        app.goto(Screen::Running);
        let text = render(&app, 160, 40);
        assert!(text.contains("asked"), "{text}");
        assert!(text.contains("answered"), "{text}");
        assert!(text.contains("Classify this file"), "{text}");
        assert!(text.contains("No store_memory call"), "{text}");
        assert!(
            text.contains("no parseable candidate JSON"),
            "a failed answer must lead with why: {text}"
        );
    }

    #[test]
    fn the_logs_panel_takes_the_whole_area_when_expanded() {
        let mut app = populated();
        with_exchanges(&mut app);
        app.progress.log.push_back("· scanned 3 document(s)".into());
        app.activity = ActivityView::Logs;
        app.goto(Screen::Running);
        let text = render(&app, 160, 40);
        assert!(text.contains("scanned 3 document(s)"), "{text}");
        assert!(
            !text.contains("Candidates by destination"),
            "an expanded panel takes the charts' room: {text}"
        );
    }

    #[test]
    fn both_panels_share_the_row_by_default() {
        let mut app = populated();
        with_exchanges(&mut app);
        app.goto(Screen::Running);
        let text = render(&app, 160, 40);
        assert!(text.contains("Agents"), "{text}");
        assert!(text.contains("Logs"), "{text}");
        assert!(text.contains("Candidates by destination"), "{text}");
    }

    /// Every panel mode must survive every size, like every other screen.
    #[test]
    fn the_activity_panels_render_at_every_size() {
        for view in [ActivityView::Both, ActivityView::Agents, ActivityView::Logs] {
            for size in [(20, 6), (40, 12), (80, 24), (200, 60)] {
                let mut app = populated();
                with_exchanges(&mut app);
                app.activity = view;
                app.goto(Screen::Running);
                render(&app, size.0, size.1);
            }
        }
    }

    // ── The mascot's degradation contract ────────────────────────────────

    /// The rule, as a test — and the overlay makes it stronger than it was.
    /// The mascot reserves nothing, so turning it on changes *only* the cells
    /// the robot occupies. Every panel keeps the width it had.
    #[test]
    fn the_mascot_changes_nothing_but_its_own_cells() {
        let frame = |on: bool| {
            let mut app = populated();
            with_exchanges(&mut app);
            app.goto(Screen::Summary);
            app.mascot_on = on;
            render(&app, 160, 44)
        };
        let off = frame(false);

        if !crate::mascot::Mascot::compiled_in() {
            return;
        }
        let on = frame(true);
        assert_ne!(on, off, "the test is meaningless if it draws nothing");

        // Every difference must be a cell the robot painted — never a panel
        // that moved or narrowed.
        let differing: Vec<(usize, char, char)> = off
            .chars()
            .zip(on.chars())
            .enumerate()
            .filter(|(_, (a, b))| a != b)
            .map(|(i, (a, b))| (i, a, b))
            .collect();
        assert!(!differing.is_empty());
        for (i, was, now) in &differing {
            assert_eq!(*was, ' ', "the mascot painted over content at {i}");
            assert!(
                crate::mascot::QUADRANTS.contains(&now.to_string().as_str()),
                "cell {i} became {now:?}, which the mascot does not draw"
            );
        }
    }

    /// It never paints over anything, so a screen with no free corner simply
    /// does not get a robot.
    /// Low and right, in whatever space is actually free.
    #[test]
    fn the_robot_settles_low_and_right() {
        if !crate::mascot::Mascot::compiled_in() {
            return;
        }
        let mut app = populated();
        with_exchanges(&mut app);
        app.goto(Screen::Summary);
        app.mascot_on = true;

        let (w, h) = (160u16, 44u16);
        let mut off = App::new();
        off.screen = app.screen;
        off.config = app.config.clone();
        off.progress = app.progress.clone();
        off.last_command = app.last_command.clone();
        off.started_at = app.started_at;
        off.mascot_on = false;

        let with: Vec<char> = render(&app, w, h).chars().collect();
        let without: Vec<char> = render(&off, w, h).chars().collect();
        // The rendered string carries a newline per row, so a row is w + 1.
        let stride = w as usize + 1;
        let painted: Vec<(usize, usize)> = with
            .iter()
            .zip(&without)
            .enumerate()
            .filter(|(_, (a, b))| a != b)
            .map(|(i, _)| (i % stride, i / stride))
            .collect();
        assert!(!painted.is_empty(), "nothing was drawn");

        let left = painted.iter().map(|c| c.0).min().unwrap();
        let right = painted.iter().map(|c| c.0).max().unwrap();
        let top = painted.iter().map(|c| c.1).min().unwrap();
        let bottom = painted.iter().map(|c| c.1).max().unwrap();

        // Low and right: in the bottom-right quadrant of the screen, and not
        // painted over the border it sits beside.
        let centre_x = (left + right) / 2;
        let centre_y = (top + bottom) / 2;
        assert!(
            centre_x > w as usize / 2,
            "it wandered into the left half: {centre_x}"
        );
        assert!(
            centre_y > h as usize / 2,
            "it should sit in the lower half: {centre_y} of {h}"
        );
        assert!(right < w as usize - 1, "flush against the right edge: {right}");
    }

    /// Reported: no robot on the Connection screen in VS Code. It is drawn on
    /// every screen, so any screen with a free corner must get one.
    #[test]
    fn every_screen_with_room_gets_a_robot() {
        if !crate::mascot::Mascot::compiled_in() {
            return;
        }
        for screen in SCREENS {
            let mut app = populated();
            with_exchanges(&mut app);
            app.goto(screen);
            app.mascot_on = true;
            let n = robot_cells(&app, 150, 44);
            assert!(n > 40, "{screen:?} drew only {n} robot cells at 150x44");
        }
    }

    #[test]
    fn a_full_corner_is_left_alone() {
        let mut app = populated();
        with_exchanges(&mut app);
        app.activity = ActivityView::Logs;
        // Lines long enough to reach the panel's right edge. Short ones leave
        // the right-hand side blank, and blank is exactly where the robot is
        // entitled to sit — the test would then be measuring nothing.
        for _ in 0..60 {
            app.progress.log.push_back("·".to_string() + &"x".repeat(400));
        }
        app.goto(Screen::Running);
        app.mascot_on = true;
        assert_eq!(
            robot_cells(&app, 160, 44),
            0,
            "every cell of the lower half is occupied; the robot must stand down"
        );
    }

    /// A terminal that cannot draw it is not a fault, so nothing says so.
    #[test]
    fn nothing_announces_that_the_mascot_is_missing() {
        let mut app = populated();
        with_exchanges(&mut app);
        app.mascot_on = false;
        for screen in SCREENS {
            app.goto(screen);
            let text = render(&app, 160, 40).to_lowercase();
            for word in ["mascot", "robot", "unavailable", "unsupported", "not supported"] {
                assert!(!text.contains(word), "{screen:?} mentions {word:?}");
            }
        }
    }

    /// Too little room and it declines rather than drawing a smudge —
    /// the same code path as being switched off.
    #[test]
    fn the_mascot_declines_when_there_is_no_room_for_it() {
        let mut app = populated();
        with_exchanges(&mut app);
        app.goto(Screen::Summary);
        app.mascot_on = true;
        for (w, h) in [(20, 6), (40, 12), (25, 20)] {
            assert_eq!(
                robot_cells(&app, w, h),
                0,
                "it drew into a {w}x{h} screen, which has no free corner for it"
            );
        }
        if crate::mascot::Mascot::compiled_in() {
            assert!(
                robot_cells(&app, 160, 44) > 40,
                "and it does draw when there is room"
            );
        }
    }

    /// The whole point: a mascot that cannot render costs no information.
    /// Every number and label on the run screen survives without it.
    #[test]
    fn every_fact_on_the_run_screen_survives_without_the_mascot() {
        let mut app = populated();
        with_exchanges(&mut app);
        app.goto(Screen::Running);
        app.mascot_on = false;
        let text = render(&app, 160, 40);
        for fact in [
            "41/69 units",
            "documents 3",
            "classified 38",
            "fallback 3",
            "skipped",
            "Candidates by destination",
            "Skipped, by reason",
            "Agents",
            "Logs",
        ] {
            assert!(text.contains(fact), "{fact:?} vanished with the mascot: {text}");
        }
    }

    #[test]
    fn the_mascot_renders_on_every_screen_and_size_without_panicking() {
        for screen in SCREENS {
            for size in [(20, 6), (80, 24), (160, 40), (240, 80)] {
                let mut app = populated();
                with_exchanges(&mut app);
                app.goto(screen);
                app.mascot_on = true;
                render(&app, size.0, size.1);
            }
        }
    }

    #[test]
    fn a_preview_summary_reports_the_cost_and_the_next_step() {
        let mut app = populated();
        app.last_command = Some(LastCommand::Preview);
        app.progress.estimated_tokens = 14_716;
        app.goto(Screen::Summary);
        let text = render(&app, 140, 30);
        assert!(text.contains("14716"), "{text}");
        assert!(text.contains("press r to run for real"), "{text}");
    }

    #[test]
    fn the_summary_backs_every_exclusion_count_with_a_concrete_example() {
        let mut app = populated();
        app.last_command = Some(LastCommand::Preview);
        app.goto(Screen::Summary);
        let text = render(&app, 140, 40);
        assert!(text.contains("third-party asset"), "{text}");
        assert!(
            text.contains("node_modules/x/LICENSE.md"),
            "a count with no example cannot be checked: {text}"
        );
    }

    /// Pins a plan onto an app without re-detecting: goto(Projects) would
    /// otherwise wipe it and scan the filesystem.
    fn with_plan(app: &mut App, rows: Vec<crate::monorepo::PlanRow>) {
        app.plan = rows;
        app.plan_detected = true;
        app.plan_path = app.config.path.clone();
        app.plan_note = format!("{} sub-project(s) detected", app.plan.len());
        app.goto(Screen::Projects);
    }

    fn plan_row_ui(rel: &str, action: Action, matched: Option<&str>) -> crate::monorepo::PlanRow {
        let name = rel.rsplit('/').next().unwrap().to_string();
        crate::monorepo::PlanRow {
            detected: crate::monorepo::Detected {
                alias: name.clone(),
                name: name.clone(),
                rel_dir: rel.into(),
                via: "test",
            },
            matched: matched.map(|id| crate::api::Project {
                id: id.into(),
                name,
                client_id: None,
                archived_at: None,
            }),
            action,
            resolved_project_id: None,
        }
    }

    #[test]
    fn the_projects_screen_lists_each_subproject_with_its_action() {
        let mut app = populated();
        with_plan(
            &mut app,
            vec![
                plan_row_ui("apps/web", Action::Create, None),
                plan_row_ui("packages/ui", Action::Select("p_ui".into()), Some("p_ui")),
                plan_row_ui("apps/api", Action::Skip, None),
            ],
        );
        let text = render(&app, 140, 40);
        assert!(text.contains("apps/web"), "{text}");
        assert!(text.contains("packages/ui"), "{text}");
        assert!(text.contains("create"), "{text}");
        assert!(text.contains("existing"), "{text}");
        assert!(text.contains("skip"), "{text}");
        // The counts the confirmation states plainly: 1 create, 1 existing.
        assert!(text.contains("1 create") || text.contains("1 new"), "{text}");
    }

    #[test]
    fn the_summary_lists_every_project_queue_after_a_monorepo_run() {
        let mut app = populated();
        app.session_runs = vec![
            crate::app::CreatedRun {
                alias: "web".into(),
                project_id: "p_web".into(),
                run_id: "run-web".into(),
            },
            crate::app::CreatedRun {
                alias: "api".into(),
                project_id: "p_api".into(),
                run_id: "run-api".into(),
            },
        ];
        app.progress.finished = Some(crate::app::FinishedRun {
            ok: true,
            aborted_on_budget: false,
            error: None,
        });
        app.goto(Screen::Summary);
        let text = render(&app, 140, 40);
        assert!(text.contains("2 project queue(s) staged"), "{text}");
        assert!(text.contains("web") && text.contains("api"), "{text}");
    }

    /// The reported confusion: a rescan classifies every unit but stages none,
    /// and the summary must say so plainly instead of claiming N were staged.
    #[test]
    fn a_rescan_that_stages_nothing_says_so_instead_of_claiming_candidates() {
        let mut app = populated();
        app.progress.classified = 0;
        app.progress.fallbacks = 12;
        app.progress.staged = Some((0, 12, 0));
        app.progress.run_id = Some("01ec".into());
        app.progress.finished = Some(crate::app::FinishedRun {
            ok: true,
            aborted_on_budget: false,
            error: None,
        });
        app.goto(Screen::Summary);
        let text = render(&app, 140, 40);
        assert!(text.contains("nothing new to review"), "{text}");
        assert!(
            !text.contains("12 candidate(s) staged"),
            "the misleading headline must be gone: {text}"
        );
        assert!(text.contains("already migrated"), "{text}");
    }

    #[test]
    fn a_failed_run_says_why_and_what_survived() {
        let mut app = populated();
        app.progress.finished = Some(crate::app::FinishedRun {
            ok: false,
            aborted_on_budget: false,
            error: Some("the runner exited with status 101".into()),
        });
        app.goto(Screen::Summary);
        let text = render(&app, 140, 30);
        assert!(text.contains("status 101"), "{text}");
        assert!(text.contains("already staged survived"), "{text}");
    }
    /// Prints every screen for a human to look at: `DUMP_FRAMES=1 cargo test
    /// dump_frames -- --nocapture`.
    ///
    /// Assertions catch panics and leaked secrets; they cannot tell you that a
    /// panel is empty or that a label collides with its value. Gated by an
    /// environment variable so it costs nothing in a normal run.
    #[test]
    fn dump_frames_for_visual_review() {
        if std::env::var("DUMP_FRAMES").is_err() {
            return;
        }
        let mut mas = populated();
        with_exchanges(&mut mas);
        mas.progress.staged = Some((69, 0, 0));
        mas.goto(Screen::Summary);
        mas.mascot_on = true;
        println!("\n===== mascot =====\n{}", render(&mas, 150, 44));

        let mut agents = populated();
        with_exchanges(&mut agents);
        agents.activity = ActivityView::Agents;
        agents.goto(Screen::Running);
        println!("\n===== agents =====\n{}", render(&agents, 120, 34));

        for (name, screen) in [
            ("connection", Screen::Connection),
            ("source", Screen::Source),
            ("options", Screen::Options),
            ("running", Screen::Running),
            ("review", Screen::Review),
            ("summary", Screen::Summary),
        ] {
            let mut app = populated();
            app.picked_run = Some("run-7f2".into());
            app.last_command = Some(LastCommand::Preview);
            app.goto(screen);
            println!("\n===== {name} =====\n{}", render(&app, 110, 30));
        }
    }
}
