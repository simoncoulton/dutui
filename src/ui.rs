use crate::{
    app::App,
    scan::Node,
    theme::{Theme, mix},
    treemap::{self, Cell},
};
use ratatui::{
    Frame,
    buffer::Buffer,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

const MAX_DEPTH: u8 = 3;
const SPINNER: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

static IEC: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn set_iec(on: bool) {
    IEC.store(on, std::sync::atomic::Ordering::Relaxed);
}

pub fn human(b: u64) -> String {
    let iec = IEC.load(std::sync::atomic::Ordering::Relaxed);
    let (base, units): (f64, [&str; 6]) = if iec {
        (1024.0, ["B", "KiB", "MiB", "GiB", "TiB", "PiB"])
    } else {
        (1000.0, ["B", "kB", "MB", "GB", "TB", "PB"])
    };
    let mut v = b as f64;
    let mut i = 0;
    while v >= base && i < units.len() - 1 {
        v /= base;
        i += 1;
    }
    if i == 0 {
        format!("{b} B")
    } else {
        format!("{v:.1} {}", units[i])
    }
}

fn duration(d: std::time::Duration) -> String {
    let s = d.as_secs_f64();
    if s < 1.0 {
        format!("{} ms", d.as_millis())
    } else if s < 60.0 {
        format!("{s:.1}s")
    } else {
        format!("{}m {:02}s", d.as_secs() / 60, d.as_secs() % 60)
    }
}

fn thousands(n: u64) -> String {
    let digits = n.to_string();
    let groups: Vec<&str> = digits
        .as_bytes()
        .rchunks(3)
        .rev()
        .map(|c| std::str::from_utf8(c).unwrap_or(""))
        .collect();
    groups.join(",")
}

/// Write `s` clipped to `maxw` columns. Returns columns used.
fn put(buf: &mut Buffer, x: u16, y: u16, maxw: u16, s: &str, style: Style) -> u16 {
    // Callers lay out for a nominal width; never write outside the buffer.
    if y >= buf.area.bottom() || x < buf.area.x {
        return 0;
    }
    let maxw = maxw.min(buf.area.right().saturating_sub(x));
    let mut cx = 0;
    for ch in s.chars() {
        let ch = if ch.is_control() { '?' } else { ch };
        let cw = ch.width().unwrap_or(0) as u16;
        if cw == 0 {
            continue;
        }
        if cx + cw > maxw {
            break;
        }
        let c = &mut buf[(x + cx, y)];
        c.set_char(ch);
        c.set_style(style);
        for e in 1..cw {
            let c = &mut buf[(x + cx + e, y)];
            c.set_char(' ');
            c.set_style(style);
        }
        cx += cw;
    }
    cx
}

fn fill(buf: &mut Buffer, c: Cell, bg: Color) {
    for y in c.y..c.y + c.h {
        for x in c.x..c.x + c.w {
            let cell = &mut buf[(x, y)];
            cell.set_char(' ');
            cell.set_bg(bg);
        }
    }
}

pub fn draw(f: &mut Frame, app: &mut App) {
    // Cloned so the app can be borrowed mutably (hit-test rects) while drawing.
    let th = &app.theme().clone();
    let area = f.area();
    f.render_widget(
        Block::default().style(
            Style::default()
                .bg(th.page_bg(app.cfg.transparent))
                .fg(th.fg),
        ),
        area,
    );

    let [head, body, foot] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(3),
        Constraint::Length(1),
    ])
    .areas(area);
    let left_w = (area.width * 2 / 5).clamp(30.min(area.width), 60);
    let [left, right] =
        Layout::horizontal([Constraint::Length(left_w), Constraint::Min(10)]).areas(body);

    draw_header(f, app, th, head);

    let border = Style::default().fg(th.border);
    let blk = |title: String| {
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(border)
            .title(Line::styled(
                title,
                Style::default().fg(th.accent).add_modifier(Modifier::BOLD),
            ))
    };

    let cur = crate::app::cur(&app.root, &app.path);
    let sel = app.sel.min(cur.children.len().saturating_sub(1));
    let total = cur.size.max(1);

    // Left: file list.
    let lb = blk(" Files ".into());
    let li = lb.inner(left);
    f.render_widget(lb, left);
    app.list_area = li;
    if li.height > 0 {
        let h = li.height as usize;
        if sel < app.scroll {
            app.scroll = sel;
        } else if sel >= app.scroll + h {
            app.scroll = sel + 1 - h;
        }
        draw_list(f.buffer_mut(), li, th, cur, sel, app.scroll, total);
    }

    // Right: treemap.
    let title = match cur.children.get(sel) {
        Some(n) => format!(
            " {}{} · {} · {:.1}% ",
            n.name,
            if n.is_dir { "/" } else { "" },
            human(n.size),
            n.size as f64 * 100.0 / total as f64
        ),
        None => " Treemap ".into(),
    };
    let rb = blk(title);
    let ri = rb.inner(right);
    f.render_widget(rb, right);
    app.map_hits.clear();
    if ri.width > 0 && ri.height > 0 {
        let cell = Cell {
            x: ri.x,
            y: ri.y,
            w: ri.width,
            h: ri.height,
        };
        if cur.children.is_empty() {
            let msg = if app.job.is_some() {
                "scanning…"
            } else {
                "empty"
            };
            put(
                f.buffer_mut(),
                ri.x + 1,
                ri.y,
                ri.width.saturating_sub(1),
                msg,
                Style::default().fg(th.muted),
            );
        } else {
            let mut p = Painter {
                buf: f.buffer_mut(),
                th,
                hits: &mut app.map_hits,
                trail: Vec::new(),
            };
            p.top(cur, cell, sel);
        }
    }

    draw_footer(f, app, th, foot);
    if app.help {
        draw_help(f, th, app.cfg.transparent, area);
    }
    if let Some(c) = &app.confirm {
        draw_confirm(f, th, app.cfg.transparent, area, c);
    }
    if let Some(m) = &app.modal {
        (app.modal_area, app.modal_top) =
            draw_theme_modal(f, th, &app.themes, area, m.sel, app.cfg.transparent);
    }
}

/// A clock face; one cell wide and not an emoji, so it lines up everywhere.
const TIME_ICON: &str = "◷";

/// Keep the end of `s` (the interesting part of a path) when it must fit in `max` cells.
fn ellipsize_left(s: &str, max: usize) -> String {
    if s.width() <= max {
        return s.to_string();
    }
    let mut tail: Vec<char> = Vec::new();
    let mut w = 1; // the ellipsis
    for ch in s.chars().rev() {
        let cw = ch.width().unwrap_or(0);
        if w + cw > max {
            break;
        }
        w += cw;
        tail.push(ch);
    }
    if max == 0 {
        return String::new();
    }
    tail.push('…');
    tail.iter().rev().collect()
}

fn draw_header(f: &mut Frame, app: &App, th: &Theme, area: Rect) {
    let cur = app.current();
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(th.border))
        .title(Line::styled(" dutui ", key_style(th)));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let spin = if app.scanning() {
        format!(
            "{} ",
            SPINNER[(app.started.elapsed().as_millis() / 80) as usize % SPINNER.len()]
        )
    } else {
        String::new()
    };
    let verb = if app.scanning() {
        "scanning"
    } else {
        "scanned in"
    };
    let took = app.scan_elapsed().map_or(String::new(), duration);
    let size = format!("{spin}{}", human(cur.size));
    let files = format!(" · {} files · ", thousands(cur.count));
    let time = format!("{TIME_ICON} {verb} {took} ");
    let stats_w = (size.width() + files.width() + time.width()) as u16;
    let [path, stats] =
        Layout::horizontal([Constraint::Min(0), Constraint::Length(stats_w)]).areas(inner);

    f.render_widget(
        Paragraph::new(Line::styled(
            format!(
                " {}",
                ellipsize_left(
                    &app.current_path().display().to_string(),
                    (path.width as usize).saturating_sub(2)
                )
            ),
            Style::default().fg(th.fg).add_modifier(Modifier::BOLD),
        )),
        path,
    );
    let dim = Style::default().fg(th.muted);
    f.render_widget(
        Paragraph::new(Line::from(vec![
            ratatui::text::Span::styled(size, Style::default().fg(th.fg)),
            ratatui::text::Span::styled(files, dim),
            ratatui::text::Span::styled(time, Style::default().fg(th.accent)),
        ])),
        stats,
    );
}

/// Clear `r` and paint the page style, so a popup matches the page in both the
/// opaque and the transparent mode.
fn popup(f: &mut Frame, th: &Theme, transparent: bool, r: Rect) {
    f.render_widget(Clear, r);
    f.render_widget(
        Block::default().style(Style::default().bg(th.page_bg(transparent)).fg(th.fg)),
        r,
    );
}

fn key_style(th: &Theme) -> Style {
    Style::default().fg(th.accent).add_modifier(Modifier::BOLD)
}

/// Key hints: items are separated by two spaces and the first word of each is the key.
fn hint_line<'a>(th: &Theme, items: &str) -> Line<'a> {
    let mut spans = vec![ratatui::text::Span::raw(" ")];
    for item in items.split("  ") {
        let (k, d) = item.split_once(' ').unwrap_or((item, ""));
        spans.push(ratatui::text::Span::styled(k.to_string(), key_style(th)));
        spans.push(ratatui::text::Span::styled(
            format!(" {d}  "),
            Style::default().fg(th.muted),
        ));
    }
    Line::from(spans)
}

fn draw_footer(f: &mut Frame, app: &App, th: &Theme, area: Rect) {
    let dim = Style::default().fg(th.muted);
    let spin = SPINNER[(app.started.elapsed().as_millis() / 80) as usize % SPINNER.len()];
    let line = if app.scanning() {
        let (files, bytes) = app.scan_stats();
        Line::styled(
            format!(
                " {spin} scanning… {} files · {}",
                thousands(files),
                human(bytes)
            ),
            dim,
        )
    } else if app.deleting() {
        Line::styled(format!(" {spin} working…"), dim)
    } else if !app.status.is_empty() {
        Line::styled(format!(" {}", app.status), dim)
    } else {
        let xfs = format!(
            "other drives/mounts: {}",
            if app.cfg.cross_filesystems {
                "included"
            } else {
                "skipped"
            }
        );
        hint_line(
            th,
            &format!(
                "j/k move  l open  h up  d trash  D delete  r rescan  x {xfs}  ? help  q quit"
            ),
        )
    };
    f.render_widget(Paragraph::new(line), area);
}

/// Returns the modal's rect and how many terminal rows each theme entry occupies.
fn draw_theme_modal(
    f: &mut Frame,
    th: &Theme,
    themes: &[Theme],
    area: Rect,
    sel: usize,
    transparent: bool,
) -> (Rect, usize) {
    let n = themes.len() as u16;
    let w = area.width.saturating_sub(4).clamp(1, 62);
    let h = (n + 2).clamp(3, area.height.saturating_sub(2).max(3));
    let r = Rect::new(
        area.x + (area.width.saturating_sub(w)) / 2,
        area.y + (area.height.saturating_sub(h)) / 2,
        w,
        h,
    );
    popup(f, th, transparent, r);

    let key = key_style(th);
    // Scroll just far enough to keep the selection in view.
    let rows = h.saturating_sub(2).max(1) as usize;
    let top = (sel + 1).saturating_sub(rows);
    let name_w = themes
        .iter()
        .map(|t| t.name.chars().count())
        .max()
        .unwrap_or(0);
    let lines: Vec<Line> = themes
        .iter()
        .enumerate()
        .skip(top)
        .take(rows)
        .map(|(i, t)| {
            let mut spans = vec![ratatui::text::Span::raw(if i == sel {
                "▶ "
            } else {
                "  "
            })];
            spans.push(ratatui::text::Span::raw(format!("{:<name_w$}  ", t.name)));
            for c in t.swatch() {
                spans.push(ratatui::text::Span::styled("██ ", Style::default().fg(c)));
            }
            let line = Line::from(spans);
            if i == sel {
                line.style(th.selection())
            } else {
                line
            }
        })
        .collect();
    let hints = hint_line(
        th,
        &format!(
            "↑↓ preview  Enter save  Esc cancel  b transparent: {}",
            if transparent { "on" } else { "off" }
        ),
    );
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(th.accent))
        .title(Line::styled(" Theme ", key))
        .title_bottom(hints)
        .style(Style::default().fg(th.fg));
    f.render_widget(Paragraph::new(lines).block(block), r);
    (r, top)
}

fn draw_confirm(f: &mut Frame, th: &Theme, transparent: bool, area: Rect, c: &crate::app::Confirm) {
    let red = th.error;
    let tone = if c.permanent { red } else { th.accent };
    let w = 64.min(area.width);
    let h = 8.min(area.height);
    let r = Rect::new(
        area.x + (area.width - w) / 2,
        area.y + (area.height - h) / 2,
        w,
        h,
    );
    popup(f, th, transparent, r);
    let what = if c.is_dir {
        format!(
            "{} · {} files, recursively",
            human(c.size),
            thousands(c.count)
        )
    } else {
        human(c.size)
    };
    let note = if c.permanent {
        Line::styled(
            " Permanently deleted: this cannot be undone.",
            Style::default().fg(red),
        )
    } else {
        Line::styled(
            " Moves to the Trash; you can restore it from there.",
            Style::default().fg(th.muted),
        )
    };
    let lines = vec![
        Line::styled(
            format!(" {}", c.path.display()),
            Style::default().fg(th.fg).add_modifier(Modifier::BOLD),
        ),
        Line::styled(format!(" {what}"), Style::default().fg(th.muted)),
        Line::raw(""),
        note,
        Line::raw(""),
        Line::from(vec![
            ratatui::text::Span::styled(
                " y ",
                Style::default()
                    .fg(th.dark)
                    .bg(tone)
                    .add_modifier(Modifier::BOLD),
            ),
            ratatui::text::Span::styled(
                if c.permanent {
                    " delete forever    "
                } else {
                    " move to Trash    "
                },
                Style::default().fg(th.fg),
            ),
            ratatui::text::Span::styled(
                " n/esc ",
                Style::default()
                    .fg(th.dark)
                    .bg(th.muted)
                    .add_modifier(Modifier::BOLD),
            ),
            ratatui::text::Span::styled(" cancel", Style::default().fg(th.fg)),
        ]),
    ];
    let verb = if c.permanent {
        "Permanently delete"
    } else {
        "Trash"
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(tone))
        .title(format!(
            " {verb} {}{}? ",
            c.name,
            if c.is_dir { "/" } else { "" }
        ))
        .style(Style::default().fg(th.fg));
    f.render_widget(Paragraph::new(lines).block(block), r);
}

fn draw_help(f: &mut Frame, th: &Theme, transparent: bool, area: Rect) {
    let rows = [
        ("↑ ↓ j k", "move selection"),
        ("→ l ⏎", "enter directory"),
        ("← h ⌫", "go up (scans the parent if above the start)"),
        ("g G / PgUp PgDn", "jump"),
        ("d", "move selected to the Trash (asks first)"),
        (
            "D",
            "permanently delete selected (asks first; folders recursively)",
        ),
        ("r", "rescan"),
        ("u", "toggle units: GB (decimal) / GiB (binary)"),
        ("t", "choose theme (live preview)"),
        ("b", "toggle transparent background"),
        ("E", "edit the config file in $VISUAL / $EDITOR"),
        (
            "x",
            "include / skip other mounted drives and volumes (rescans)",
        ),
        ("mouse", "click select · double-click open · right-click up"),
        ("q esc", "quit"),
    ];
    let w = 72.min(area.width);
    let h = (rows.len() as u16 + 2).min(area.height);
    let r = Rect::new(
        area.x + (area.width - w) / 2,
        area.y + (area.height - h) / 2,
        w,
        h,
    );
    popup(f, th, transparent, r);
    let lines: Vec<Line> = rows
        .iter()
        .map(|(k, d)| {
            Line::from(vec![
                ratatui::text::Span::styled(format!(" {k:<17}"), key_style(th)),
                ratatui::text::Span::styled(*d, Style::default().fg(th.fg)),
            ])
        })
        .collect();
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(th.accent))
        .title(Line::styled(" Help ", key_style(th)))
        .style(Style::default().fg(th.fg));
    f.render_widget(Paragraph::new(lines).block(block), r);
}

fn draw_list(
    buf: &mut Buffer,
    a: Rect,
    th: &Theme,
    dir: &Node,
    sel: usize,
    scroll: usize,
    total: u64,
) {
    let size_w = 10u16;
    let show_bar = a.width >= 44;
    let bar_w: u16 = if show_bar { 10 } else { 0 };
    let pct_w: u16 = if show_bar { 5 } else { 0 };
    let fixed = 2 + size_w + if show_bar { bar_w + pct_w + 2 } else { 0 };
    let name_w = a.width.saturating_sub(fixed + 1);

    for (row, (i, n)) in dir
        .children
        .iter()
        .enumerate()
        .skip(scroll)
        .take(a.height as usize)
        .enumerate()
    {
        let y = a.y + row as u16;
        let selected = i == sel;
        let st = |s: Style| if selected { s.patch(th.selection()) } else { s };
        if selected {
            for x in a.x..a.x + a.width {
                buf[(x, y)].set_style(th.selection());
            }
        }
        let color = th.palette[i % th.palette.len()];
        put(buf, a.x, y, 2, "■", st(Style::default().fg(color)));
        let (name_style, suffix) = if n.is_dir {
            (Style::default().fg(th.accent), "/")
        } else {
            (Style::default().fg(th.fg), "")
        };
        let name_style = if selected {
            name_style.add_modifier(Modifier::BOLD)
        } else {
            name_style
        };
        let label = format!("{}{suffix}", n.name);
        put(buf, a.x + 2, y, name_w, &label, st(name_style));

        let sx = a.x + 2 + name_w + 1;
        let size = format!("{:>w$}", human(n.size), w = size_w as usize);
        put(buf, sx, y, size_w, &size, st(Style::default().fg(th.fg)));
        if show_bar {
            let frac = (n.size as f64 / total as f64).clamp(0.0, 1.0);
            let eighths = (frac * bar_w as f64 * 8.0).round() as u32;
            let bx = sx + size_w + 1;
            for c in 0..bar_w as u32 {
                let e = eighths.saturating_sub(c * 8).min(8);
                let ch = ['·', '▏', '▎', '▍', '▌', '▋', '▊', '▉', '█'][e as usize];
                let fg = if e == 0 { th.muted } else { color };
                put(
                    buf,
                    bx + c as u16,
                    y,
                    1,
                    &ch.to_string(),
                    st(Style::default().fg(fg)),
                );
            }
            let pct = format!("{:>4.0}%", frac * 100.0);
            put(
                buf,
                bx + bar_w + 1 - 1,
                y,
                pct_w,
                &pct,
                st(Style::default().fg(th.muted)),
            );
        }
    }
}

struct Painter<'a> {
    buf: &'a mut Buffer,
    th: &'a Theme,
    hits: &'a mut Vec<(Cell, Vec<usize>)>,
    /// Child indices from the shown directory to the block being painted.
    trail: Vec<usize>,
}

impl Painter<'_> {
    fn top(&mut self, dir: &Node, area: Cell, sel: usize) {
        let sizes: Vec<u64> = dir.children.iter().map(|c| c.size).take(256).collect();
        let sizes: Vec<u64> = sizes.into_iter().take_while(|&s| s > 0).collect();
        let cells = treemap::layout(&sizes, area);
        for (i, c) in cells.iter().enumerate() {
            let Some(c) = *c else { continue };
            self.trail.push(i);
            let mut color = self.th.palette[i % self.th.palette.len()];
            if i == sel {
                color = mix(color, self.th.fg, 0.35);
            }
            self.node(&dir.children[i], c, 0, color, i == sel);
            self.trail.pop();
        }
    }

    fn node(&mut self, n: &Node, c: Cell, depth: u8, color: Color, selected: bool) {
        self.hits.push((c, self.trail.clone()));
        fill(self.buf, c, color);
        let text = Style::default().fg(self.th.dark);
        let bold = if depth == 0 || selected {
            text.add_modifier(Modifier::BOLD)
        } else {
            text
        };
        let can_nest =
            n.is_dir && !n.children.is_empty() && depth < MAX_DEPTH && c.w >= 8 && c.h >= 4;
        if c.w >= 3 && c.h >= 1 {
            let label = format!(
                "{}{} {}",
                n.name,
                if n.is_dir { "/" } else { "" },
                human(n.size)
            );
            put(self.buf, c.x + 1, c.y, c.w - 1, &label, bold);
        }
        if can_nest {
            let inner = Cell {
                x: c.x,
                y: c.y + 1,
                w: c.w,
                h: c.h - 1,
            };
            let cap = 64;
            let sizes: Vec<u64> = n
                .children
                .iter()
                .take(cap)
                .map(|c| c.size)
                .take_while(|&s| s > 0)
                .collect();
            let cells = treemap::layout(&sizes, inner);
            for (k, cc) in cells.iter().enumerate() {
                let Some(cc) = *cc else { continue };
                let shade = [0.14, 0.28, 0.42][k % 3] + 0.04 * depth as f32;
                let col = mix(color, self.th.dark, shade);
                self.trail.push(k);
                self.child(&n.children[k], cc, depth + 1, col, selected);
                self.trail.pop();
            }
        } else if c.h >= 2 && c.w >= 3 && n.is_dir && n.count > 0 {
            let s = format!("{} files", thousands(n.count));
            put(self.buf, c.x + 1, c.y + 1, c.w - 1, &s, text);
        }
    }

    /// Nested blocks sit on a darkened fill, so their label needs a light colour.
    fn child(&mut self, n: &Node, c: Cell, depth: u8, color: Color, selected: bool) {
        self.hits.push((c, self.trail.clone()));
        fill(self.buf, c, color);
        let fg = if selected {
            self.th.fg
        } else {
            mix(self.th.fg, color, 0.15)
        };
        let style = Style::default().fg(fg);
        if c.w >= 3 {
            let label = format!("{}{}", n.name, if n.is_dir { "/" } else { "" });
            let used = put(self.buf, c.x + 1, c.y, c.w - 1, &label, style);
            if c.h >= 2 && c.w >= 6 && used + 1 < c.w {
                put(self.buf, c.x + 1, c.y + 1, c.w - 1, &human(n.size), style);
            }
        }
        let can_nest =
            n.is_dir && !n.children.is_empty() && depth < MAX_DEPTH && c.w >= 10 && c.h >= 5;
        if can_nest {
            let inner = Cell {
                x: c.x,
                y: c.y + 2,
                w: c.w,
                h: c.h - 2,
            };
            let sizes: Vec<u64> = n
                .children
                .iter()
                .take(32)
                .map(|c| c.size)
                .take_while(|&s| s > 0)
                .collect();
            let cells = treemap::layout(&sizes, inner);
            for (k, cc) in cells.iter().enumerate() {
                let Some(cc) = *cc else { continue };
                let col = mix(color, self.th.dark, [0.18, 0.32, 0.46][k % 3]);
                self.trail.push(k);
                self.child(&n.children[k], cc, depth + 1, col, selected);
                self.trail.pop();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{app::App, config::Config};
    use ratatui::{Terminal, backend::TestBackend, buffer::Buffer, crossterm::event::*};
    use std::sync::Mutex;

    /// The unit system is a process-wide global; `App::new` resets it.
    static UNITS: Mutex<()> = Mutex::new(());

    /// An app over a small throwaway directory (never the real config or home).
    fn app(transparent: bool) -> App {
        let dir = std::env::temp_dir().join(format!("dutui-ui-{}", std::process::id()));
        let _ = std::fs::create_dir_all(dir.join("sub"));
        let _ = std::fs::write(dir.join("a.bin"), vec![0u8; 4096]);
        let _ = std::fs::write(dir.join("sub/b.bin"), vec![0u8; 8192]);
        let cfg = Config {
            transparent,
            ..Config::default()
        };
        let mut app = {
            let _g = UNITS.lock().unwrap();
            App::new(cfg, crate::theme::builtin_themes(), None, dir)
        };
        let t = std::time::Instant::now();
        while app.scanning() && t.elapsed().as_secs() < 5 {
            app.poll_scan();
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        app
    }

    fn press(app: &mut App, c: char) {
        app.on_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
    }

    fn render(app: &mut App, w: u16, h: u16) -> Buffer {
        let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
        term.draw(|f| draw(f, app)).unwrap();
        term.backend().buffer().clone()
    }

    fn row(buf: &Buffer, y: u16) -> String {
        (0..buf.area.width).map(|x| buf[(x, y)].symbol()).collect()
    }

    #[test]
    fn nothing_panics_on_tiny_terminals() {
        let mut a = app(false);
        for popup in ["", "?", "t", "d", "D"] {
            let mut a2 = app(false);
            if let Some(c) = popup.chars().next() {
                press(&mut a2, c);
            }
            for (w, h) in [
                (0, 0),
                (1, 1),
                (3, 2),
                (10, 4),
                (30, 8),
                (60, 14),
                (200, 60),
            ] {
                render(&mut a2, w, h);
            }
        }
        render(&mut a, 80, 24);
    }

    #[test]
    fn popups_follow_the_page_background() {
        for transparent in [false, true] {
            let mut a = app(transparent);
            press(&mut a, 't');
            let buf = render(&mut a, 100, 30);
            let r = a.modal_area;
            let want = a.theme().page_bg(transparent);
            // Bottom-right interior cell: inside the popup, not a selected row.
            assert_eq!(buf[(r.x + r.width - 2, r.y + r.height - 2)].bg, want);
        }
    }

    #[test]
    fn modal_hints_use_the_accent_colour() {
        let mut a = app(false);
        press(&mut a, 't');
        let buf = render(&mut a, 100, 30);
        let r = a.modal_area;
        let y = r.y + r.height - 1;
        let text = row(&buf, y);
        let col = text.find("Enter").expect("hint in the bottom border");
        let x = text[..col].chars().count() as u16;
        assert_eq!(buf[(x, y)].fg, a.theme().accent);
    }

    #[test]
    fn shift_e_asks_to_edit_the_config() {
        let mut a = app(false);
        a.on_key(KeyEvent::new(KeyCode::Char('E'), KeyModifiers::SHIFT));
        assert!(a.edit_config);
    }

    #[test]
    fn footer_stays_short_and_omits_rare_keys() {
        let mut a = app(false);
        let buf = render(&mut a, 100, 30);
        let foot = row(&buf, 29);
        assert!(foot.contains("quit"));
        assert!(!foot.contains("theme") && !foot.contains("config"));
    }

    #[test]
    fn formats_decimal_and_binary_units() {
        let _g = UNITS.lock().unwrap();
        set_iec(false);
        assert_eq!(human(999), "999 B");
        assert_eq!(human(1_500_000), "1.5 MB");
        set_iec(true);
        assert_eq!(human(1_048_576), "1.0 MiB");
        set_iec(false);
    }

    #[test]
    fn groups_thousands() {
        assert_eq!(thousands(0), "0");
        assert_eq!(thousands(999), "999");
        assert_eq!(thousands(1_000), "1,000");
        assert_eq!(thousands(1_234_567), "1,234,567");
    }
}
