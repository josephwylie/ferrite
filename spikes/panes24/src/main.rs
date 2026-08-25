// panes24 — Ferrite render spike.
// Gate: 24 panes streaming synthetic deltas concurrently, whole-window
// re-render per tick (worst case — no per-pane damage tracking), ≥60fps
// sustained (target 120) at sane RSS.
use std::collections::VecDeque;
use std::time::{Duration, Instant};

use gpui::*;

const PANES: usize = 24;
const COLS: usize = 6;
const TICK_MS: u64 = 8; // 125 ticks/sec; every pane appends every tick
const MAX_LINES: usize = 200; // per-pane ring
const VISIBLE_LINES: usize = 14;
const WRAP_COLS: usize = 72;

const WORDS: &[&str] = &[
    "wiring", "the", "joiner", "into", "canvas", "path", "atlas", "stays",
    "per-cell", "checks", "green", "vitest", "run", "passed", "resume",
    "session", "delta", "coalesce", "channel", "spawn", "parse", "commit",
    "ferrite", "pane", "stream", "tokens", "metal", "frame", "budget",
];

struct Pane {
    title: SharedString,
    lines: VecDeque<SharedString>,
    current: String,
    deltas: u64,
}

impl Pane {
    fn new(i: usize) -> Self {
        Self {
            title: format!("pane-{i:02}").into(),
            lines: VecDeque::new(),
            current: String::new(),
            deltas: 0,
        }
    }

    fn push_word(&mut self, word: &str) {
        self.deltas += 1;
        if !self.current.is_empty() {
            self.current.push(' ');
        }
        self.current.push_str(word);
        if self.current.len() >= WRAP_COLS {
            let done = std::mem::take(&mut self.current);
            self.lines.push_back(done.into());
            if self.lines.len() > MAX_LINES {
                self.lines.pop_front();
            }
        }
    }
}

struct Cockpit {
    panes: Vec<Pane>,
    word_ix: usize,
    frames: u64,
    ticks: u64,
    last_report: Instant,
    fps: f64,
    tps: f64,
    rss_mb: f64,
}

impl Cockpit {
    fn new(cx: &mut Context<Self>) -> Self {
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(TICK_MS))
                    .await;
                let alive = this.update(cx, |cockpit, cx| {
                    cockpit.tick();
                    cx.notify();
                });
                if alive.is_err() {
                    break;
                }
            }
        })
        .detach();
        Self {
            panes: (0..PANES).map(Pane::new).collect(),
            word_ix: 0,
            frames: 0,
            ticks: 0,
            last_report: Instant::now(),
            fps: 0.0,
            tps: 0.0,
            rss_mb: 0.0,
        }
    }

    fn tick(&mut self) {
        self.ticks += 1;
        for pane in &mut self.panes {
            let word = WORDS[self.word_ix % WORDS.len()];
            pane.push_word(word);
            self.word_ix += 1;
        }
    }

    fn report(&mut self) {
        let dt = self.last_report.elapsed().as_secs_f64();
        if dt >= 1.0 {
            self.fps = self.frames as f64 / dt;
            self.tps = self.ticks as f64 / dt;
            self.frames = 0;
            self.ticks = 0;
            self.last_report = Instant::now();
            self.rss_mb = rss_mb();
            println!(
                "fps {:>6.1} | ticks/s {:>6.1} | rss {:>7.1} MB | deltas/s {:>8.0}",
                self.fps,
                self.tps,
                self.rss_mb,
                self.tps * PANES as f64
            );
        }
    }

    fn render_pane(&self, pane: &Pane) -> impl IntoElement {
        let start = pane.lines.len().saturating_sub(VISIBLE_LINES);
        let mut body = div().flex().flex_col().flex_1().min_h_0().overflow_hidden();
        for line in pane.lines.iter().skip(start) {
            body = body.child(
                div()
                    .text_size(px(11.))
                    .text_color(rgb(0xa7abb4))
                    .child(line.clone()),
            );
        }
        if !pane.current.is_empty() {
            body = body.child(
                div()
                    .text_size(px(11.))
                    .text_color(rgb(0xf3f4f7))
                    .child(SharedString::from(pane.current.clone())),
            );
        }
        div()
            .flex()
            .flex_col()
            .bg(rgb(0x0e0e0e))
            .border_1()
            .border_color(rgb(0x232323))
            .rounded_sm()
            .overflow_hidden()
            .child(
                div()
                    .flex()
                    .justify_between()
                    .px_2()
                    .py_1()
                    .border_b_1()
                    .border_color(rgb(0x1a1a1a))
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(rgb(0xf3f4f7))
                            .child(pane.title.clone()),
                    )
                    .child(
                        div()
                            .text_size(px(10.))
                            .text_color(rgb(0x7f8187))
                            .child(SharedString::from(format!("{}", pane.deltas))),
                    ),
            )
            .child(body.px_2().py_1())
    }
}

impl Render for Cockpit {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        self.frames += 1;
        self.report();

        let hud = format!(
            "ferrite panes24 — fps {:.0} · ticks/s {:.0} · deltas/s {:.0} · rss {:.0} MB",
            self.fps,
            self.tps,
            self.tps * PANES as f64,
            self.rss_mb
        );

        let mut grid = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .gap(px(6.))
            .p(px(8.));
        let rows = PANES / COLS;
        for r in 0..rows {
            let mut row = div().flex().flex_row().flex_1().min_h_0().gap(px(6.));
            for c in 0..COLS {
                let pane = &self.panes[r * COLS + c];
                row = row.child(
                    div()
                        .flex()
                        .flex_col()
                        .flex_1()
                        .min_w_0()
                        .min_h_0()
                        .child(self.render_pane(pane)),
                );
            }
            grid = grid.child(row);
        }

        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(rgb(0x050505))
            .font_family("Menlo")
            .child(
                div()
                    .px(px(10.))
                    .py(px(6.))
                    .text_size(px(12.))
                    .text_color(rgb(0xc7ccd6))
                    .child(SharedString::from(hud)),
            )
            .child(grid)
    }
}

fn rss_mb() -> f64 {
    let pid = std::process::id().to_string();
    std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &pid])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| s.trim().parse::<f64>().ok())
        .map(|kb| kb / 1024.0)
        .unwrap_or(0.0)
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(1440.), px(900.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some("ferrite — panes24 spike".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            |_, cx| cx.new(Cockpit::new),
        )
        .unwrap();
        cx.activate(true);
    });
}
