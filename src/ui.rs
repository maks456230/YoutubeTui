//! Отрисовка интерфейса.

use ratatui::layout::{Alignment, Constraint, Flex, Layout, Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, BorderType, Gauge, HighlightSpacing, List, ListItem, Paragraph, Wrap,
};
use ratatui::Frame;
use ratatui_image::protocol::StatefulProtocol;
use ratatui_image::{FilterType, Resize, StatefulImage};

use crate::app::{App, Focus};
use crate::yt::{fmt_time, Track};

pub const ACCENT: Color = Color::Rgb(255, 60, 82);
const DIM: Color = Color::Rgb(126, 129, 142);
const BORDER: Color = Color::Rgb(58, 58, 70);
const TEXT: Color = Color::Rgb(232, 232, 238);
const BAR_BG: Color = Color::Rgb(42, 42, 52);
const SEL_BG: Color = Color::Rgb(52, 26, 36);

pub fn draw(f: &mut Frame, app: &mut App) {
    let area = f.area();
    if area.width < 46 || area.height < 14 {
        let v = Layout::vertical([
            Constraint::Fill(1),
            Constraint::Length(2),
            Constraint::Fill(1),
        ])
        .flex(Flex::Center)
        .split(area);
        f.render_widget(
            Paragraph::new(vec![
                Line::from(Span::styled(
                    "окно слишком маленькое",
                    Style::new().fg(ACCENT).add_modifier(Modifier::BOLD),
                )),
                Line::from(Span::styled(
                    format!("{}x{} · нужно минимум 46x14", area.width, area.height),
                    Style::new().fg(DIM),
                )),
            ])
            .alignment(Alignment::Center),
            v[1],
        );
        return;
    }

    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(3),
        Constraint::Length(1),
    ])
    .split(area);

    draw_header(f, app, rows[0]);

    let left_w = ((area.width as u32 * 44 / 100) as u16).clamp(26, 70);
    let cols = Layout::horizontal([Constraint::Length(left_w), Constraint::Fill(1)]).split(rows[1]);
    draw_now_playing(f, app, cols[0]);
    draw_lists(f, app, cols[1]);

    draw_search(f, app, rows[2]);
    draw_hints(f, app, rows[3]);
}

// ------------------------------------------------------------------ шапка

fn draw_header(f: &mut Frame, app: &App, area: Rect) {
    let mut left = vec![
        Span::styled(
            if app.mode.allows_video() {
                " ▶ "
            } else {
                " ♫ "
            },
            Style::new().fg(ACCENT).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            app.mode.bin(),
            Style::new().fg(TEXT).add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!(" · {} ", app.mode.subtitle()), Style::new().fg(DIM)),
        Span::raw(" "),
    ];

    if app.searching {
        left.push(Span::styled(
            format!("{} поиск…", app.spinner()),
            Style::new().fg(ACCENT),
        ));
    } else if let Some(err) = &app.error {
        left.push(Span::styled(
            format!("! {err}"),
            Style::new().fg(Color::Rgb(255, 110, 110)),
        ));
    } else {
        left.push(Span::styled(app.status.clone(), Style::new().fg(DIM)));
    }
    let proto = app
        .proto
        .split_whitespace()
        .next()
        .unwrap_or("?")
        .to_string();
    let video_badge = if app.want_video() { "VID · " } else { "" };
    let right = Line::from(vec![
        Span::styled(
            video_badge,
            Style::new().fg(ACCENT).add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!("{} · ", app.repeat.label()), Style::new().fg(DIM)),
        Span::styled(format!("vol {}%", app.volume), Style::new().fg(DIM)),
        Span::raw(" "),
        Span::styled(proto, Style::new().fg(BORDER)),
        Span::raw(" "),
    ]);
    let right_w = if area.width > 80 {
        right.width() as u16
    } else {
        0
    };
    let lrect = Rect {
        width: area.width.saturating_sub(right_w + 1),
        ..area
    };
    f.render_widget(Paragraph::new(Line::from(left)), lrect);
    if right_w > 0 {
        f.render_widget(Paragraph::new(right).alignment(Alignment::Right), area);
    }
}

// ------------------------------------------------- панель «сейчас играет»

fn draw_now_playing(f: &mut Frame, app: &mut App, area: Rect) {
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(BORDER))
        .title(Line::from(vec![
            Span::styled(" ♪ ", Style::new().fg(ACCENT)),
            Span::styled(
                "сейчас играет ",
                Style::new().fg(TEXT).add_modifier(Modifier::BOLD),
            ),
        ]));
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.height < 5 || inner.width < 8 {
        return;
    }

    let rows = Layout::vertical([
        Constraint::Min(3),
        Constraint::Length(2),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .split(inner);

    draw_cover(f, app, rows[0]);

    let (title, artist) = match &app.now {
        Some(t) => (t.title.clone(), t.artist.clone()),
        None => (
            "ничего не играет".to_string(),
            "выбери трек справа →".to_string(),
        ),
    };
    let title_style = if app.now.is_some() {
        Style::new().fg(TEXT).add_modifier(Modifier::BOLD)
    } else {
        Style::new().fg(DIM)
    };
    let onair = |icon: &str, text: String| {
        Line::from(vec![
            Span::styled(format!("{icon} "), Style::new().fg(ACCENT)),
            Span::styled(text, Style::new().fg(DIM)),
        ])
    };
    let artist_line = if app.now.is_some() && app.paused {
        onair("⏸", artist)
    } else if app.now.is_some() && app.buffering {
        onair(app.spinner(), "получаю поток…".to_string())
    } else if app.now.is_some() && app.playing {
        onair("▶", artist)
    } else {
        Line::from(Span::styled(artist, Style::new().fg(DIM)))
    };
    f.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(title, title_style)),
            artist_line,
        ])
        .wrap(Wrap { trim: true }),
        rows[1],
    );

    let ratio = if app.dur > 0.5 {
        (app.pos / app.dur).clamp(0.0, 1.0)
    } else {
        0.0
    };
    f.render_widget(
        Gauge::default()
            .ratio(ratio)
            .label("")
            .gauge_style(Style::new().fg(ACCENT).bg(BAR_BG)),
        rows[2],
    );

    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(fmt_time(app.pos), Style::new().fg(DIM)),
            Span::styled(" / ", Style::new().fg(BORDER)),
            Span::styled(fmt_time(app.dur), Style::new().fg(DIM)),
        ])),
        rows[3],
    );
    let state = match (app.playing, app.paused) {
        (true, true) => "пауза",
        (true, false) => {
            if app.buffering {
                "буферизация"
            } else {
                "играет"
            }
        }
        _ => "",
    };
    if !state.is_empty() {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                format!("{state} "),
                Style::new().fg(BORDER).add_modifier(Modifier::ITALIC),
            )))
            .alignment(Alignment::Right),
            rows[3],
        );
    }
}

/// Обложка: вписываем картинку с сохранением пропорций 16:9 по центру области.
fn draw_cover(f: &mut Frame, app: &mut App, area: Rect) {
    if area.width < 2 || area.height < 1 {
        return;
    }
    if app.cover.is_none() {
        let hint = if app.cover_id.is_some() {
            "нет обложки".to_string()
        } else {
            format!("{} загружаю обложку", app.spinner())
        };
        let v = Layout::vertical([
            Constraint::Fill(1),
            Constraint::Length(1),
            Constraint::Fill(1),
        ])
        .flex(Flex::Center)
        .split(area);
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(hint, Style::new().fg(BORDER))))
                .alignment(Alignment::Center),
            v[1],
        );
        return;
    }

    let target = fit_aspect(area, 16.0 / 9.0, app.picker.font_size());
    if let Some(proto) = app.cover.as_mut() {
        f.render_stateful_widget(
            StatefulImage::<StatefulProtocol>::new()
                .resize(Resize::Fit(Some(FilterType::Triangle))),
            target,
            proto,
        );
    }
}

fn fit_aspect(area: Rect, aspect: f32, font: ratatui_image::FontSize) -> Rect {
    let fw = font.width.max(1) as f32;
    let fh = font.height.max(1) as f32;
    let pw = area.width as f32 * fw;
    let ph = area.height as f32 * fh;

    let (w_px, h_px) = if pw / ph > aspect {
        (ph * aspect, ph)
    } else {
        (pw, pw / aspect)
    };

    let w = ((w_px / fw).round() as u16).clamp(1, area.width);
    let h = ((h_px / fh).round() as u16).clamp(1, area.height);
    Rect {
        x: area.x + (area.width.saturating_sub(w)) / 2,
        y: area.y + (area.height.saturating_sub(h)) / 2,
        width: w,
        height: h,
    }
}

// ------------------------------------------------------------------- списки

fn draw_lists(f: &mut Frame, app: &mut App, area: Rect) {
    let queue_h = ((area.height as u32) / 3) as u16;
    let queue_h = queue_h.clamp(4, area.height.saturating_sub(6).max(4));
    let rows = Layout::vertical([Constraint::Min(3), Constraint::Length(queue_h)]).split(area);
    draw_results(f, app, rows[0]);
    draw_queue(f, app, rows[1]);
}

fn panel_block(title: Vec<Span<'static>>, focused: bool) -> Block<'static> {
    Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(if focused { ACCENT } else { BORDER }))
        .title(Line::from(title))
}

fn draw_results(f: &mut Frame, app: &mut App, area: Rect) {
    let focused = app.focus == Focus::Results;
    let block = panel_block(
        vec![
            Span::styled(
                " ▤ результаты ",
                Style::new()
                    .fg(if focused { ACCENT } else { DIM })
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!("{} ", app.results.len()), Style::new().fg(BORDER)),
        ],
        focused,
    );
    let inner = block.inner(area);
    let budget = (inner.width as usize).saturating_sub(2);

    if app.results.is_empty() {
        f.render_widget(block, area);
        let hint = if app.searching {
            format!("{} ищу…", app.spinner())
        } else {
            "нажми / и введи запрос".to_string()
        };
        render_hint(f, inner, &hint);
        return;
    }

    let now_id = app.now.as_ref().map(|t| t.id.clone());
    let items: Vec<ListItem> = app
        .results
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let is_now = now_id.as_deref() == Some(t.id.as_str());
            ListItem::new(row_line(Some(i + 1), t, is_now, budget))
        })
        .collect();

    let list = List::new(items)
        .block(block)
        .highlight_spacing(HighlightSpacing::Always)
        .highlight_style(Style::new().bg(SEL_BG).add_modifier(Modifier::BOLD))
        .highlight_symbol(Span::styled("▶", Style::new().fg(ACCENT)));
    f.render_stateful_widget(list, area, &mut app.results_state);
}

fn draw_queue(f: &mut Frame, app: &mut App, area: Rect) {
    let focused = app.focus == Focus::Queue;
    let block = panel_block(
        vec![
            Span::styled(
                " ☰ очередь ",
                Style::new()
                    .fg(if focused { ACCENT } else { DIM })
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!("{} ", app.queue.len()), Style::new().fg(BORDER)),
        ],
        focused,
    );
    let inner = block.inner(area);
    let budget = (inner.width as usize).saturating_sub(2);

    if app.queue.is_empty() {
        f.render_widget(block, area);
        render_hint(f, inner, "пусто · a — добавить выбранное");
        return;
    }

    let items: Vec<ListItem> = app
        .queue
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let is_now = app.qpos == Some(i);
            let mut line = row_line(Some(i + 1), t, is_now, budget);
            if is_now && !line.spans.is_empty() {
                line.spans[0] = Span::styled("♪  ", Style::new().fg(ACCENT));
            }
            ListItem::new(line)
        })
        .collect();

    let list = List::new(items)
        .block(block)
        .highlight_spacing(HighlightSpacing::Always)
        .highlight_style(Style::new().bg(SEL_BG))
        .highlight_symbol(Span::styled("▶", Style::new().fg(ACCENT)));
    f.render_stateful_widget(list, area, &mut app.queue_state);
}

/// Одна строка списка: [номер] название  исполнитель  длительность,
/// аккуратно обрезанная по доступной ширине.
fn row_line(idx: Option<usize>, t: &Track, is_now: bool, budget: usize) -> Line<'static> {
    let num = match idx {
        Some(i) => format!("{i:>2} "),
        None => "   ".to_string(),
    };
    let dur = if t.duration > 0.0 {
        fmt_time(t.duration)
    } else {
        String::new()
    };
    let dur_w = if dur.is_empty() { 0 } else { cells(&dur) + 2 };
    let title_style = if is_now {
        Style::new().fg(ACCENT).add_modifier(Modifier::BOLD)
    } else {
        Style::new().fg(TEXT)
    };

    let mut left = budget.saturating_sub(cells(&num) + dur_w);
    // исполнитель показываем, только если под название остаётся хотя бы 12 символов
    let artist_w = cells(&t.artist);
    let show_artist = artist_w > 0 && left > artist_w + 14;
    let artist = if show_artist {
        left -= artist_w + 2;
        format!("  {}", t.artist)
    } else {
        String::new()
    };

    Line::from(vec![
        Span::styled(num, Style::new().fg(BORDER)),
        Span::styled(elide(&t.title, left.max(1)), title_style),
        Span::styled(artist, Style::new().fg(DIM)),
        Span::styled(
            if dur.is_empty() {
                String::new()
            } else {
                format!("  {dur}")
            },
            Style::new().fg(BORDER),
        ),
    ])
}

/// Ширина символа в терминальных ячейках (CJK и эмодзи занимают две).
fn char_cells(c: char) -> usize {
    let u = c as u32;
    let wide = matches!(u,
        0x1100..=0x115F
        | 0x2E80..=0x303E
        | 0x3041..=0x33FF
        | 0x3400..=0x4DBF
        | 0x4E00..=0x9FFF
        | 0xA000..=0xA4CF
        | 0xA960..=0xA97F
        | 0xAC00..=0xD7A3
        | 0xF900..=0xFAFF
        | 0xFE30..=0xFE6F
        | 0xFF00..=0xFF60
        | 0xFFE0..=0xFFE6
        | 0x1F000..=0x1FAFF
        | 0x20000..=0x3FFFD);
    if wide {
        2
    } else {
        1
    }
}

fn cells(s: &str) -> usize {
    s.chars().map(char_cells).sum()
}

/// Обрезает строку по ширине в ячейках, добавляя «…».
pub(crate) fn elide(s: &str, max: usize) -> String {
    if max == 0 {
        return String::new();
    }
    if cells(s) <= max {
        return s.to_string();
    }
    let mut out = String::new();
    let mut used = 0;
    for c in s.chars() {
        let w = char_cells(c);
        if used + w > max.saturating_sub(1) {
            break;
        }
        out.push(c);
        used += w;
    }
    out.push('…');
    out
}

fn render_hint(f: &mut Frame, inner: Rect, text: &str) {
    if inner.height == 0 || inner.width == 0 {
        return;
    }
    let v = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(1),
        Constraint::Fill(1),
    ])
    .flex(Flex::Center)
    .split(inner);
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            text.to_string(),
            Style::new().fg(BORDER),
        )))
        .alignment(Alignment::Center),
        v[1],
    );
}

// -------------------------------------------------------------------- поиск

fn draw_search(f: &mut Frame, app: &mut App, area: Rect) {
    let focused = app.editing;
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(if focused { ACCENT } else { BORDER }))
        .title(Line::from(vec![
            Span::styled(
                " / ",
                Style::new()
                    .fg(if focused { ACCENT } else { DIM })
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("поиск ", Style::new().fg(if focused { TEXT } else { DIM })),
        ]));
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.height == 0 {
        return;
    }

    let text = if app.query.is_empty() {
        Span::styled(
            "название, исполнитель или ссылка на YouTube Music".to_string(),
            Style::new().fg(BORDER).add_modifier(Modifier::ITALIC),
        )
    } else {
        Span::styled(app.query.clone(), Style::new().fg(TEXT))
    };
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("❯ ", Style::new().fg(if focused { ACCENT } else { BORDER })),
            text,
        ])),
        inner,
    );

    if focused {
        let cursor_x = inner.x + 2 + app.query.chars().count() as u16;
        f.set_cursor_position(Position::new(
            cursor_x.min(inner.x + inner.width.saturating_sub(1)),
            inner.y,
        ));
    }
}

fn draw_hints(f: &mut Frame, app: &App, area: Rect) {
    let key = Style::new().fg(DIM).add_modifier(Modifier::BOLD);
    let mut items: Vec<(&str, &str)> = vec![("/", "поиск"), ("⏎", "играть"), ("space", "пауза")];
    if app.mode.allows_video() {
        items.push(("v", "видео"));
    }
    items.extend([
        ("←/→", "±5с"),
        ("+/−", "звук"),
        ("n/p", "переключить"),
        ("a", "в очередь"),
        ("tab", "фокус"),
        ("r", "повтор"),
        ("q", "выход"),
    ]);
    let mut spans = Vec::new();
    let mut used = 0usize;
    let limit = area.width as usize;
    for (k, d) in items.iter() {
        let w = k.chars().count() + 1 + d.chars().count() + 3;
        if used + w > limit {
            break;
        }
        used += w;
        if !spans.is_empty() {
            spans.push(Span::styled(" · ", Style::new().fg(BORDER)));
        }
        spans.push(Span::styled(*k, key));
        spans.push(Span::raw(" "));
        spans.push(Span::styled(*d, Style::new().fg(BORDER)));
    }
    spans.push(Span::raw(" "));
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}
