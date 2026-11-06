use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use rune::tok::type_;
use rune::{
    AppSpec, Color, Cx, El, Key, KeyEvent, ScrollSpec, TextEdit, Tx,
    col, el, row, spacer, text, text_input,
};
use tokio::sync::mpsc;

mod acp;
mod backend;
mod commands;
mod ipc;

use backend::{PiRunner, StreamEvent, UsageMetrics};

#[derive(Clone, Debug, PartialEq)]
pub enum ToolStatus {
    Running,
    Done,
    Error,
}

#[derive(Clone, Debug)]
pub struct ToolCallItem {
    pub name: String,
    pub args: String,
    pub status: ToolStatus,
    pub output: String,
}

#[derive(Clone, Debug)]
pub enum ChatMessage {
    User(String),
    Assistant {
        text: String,
        thinking: String,
        thinking_start: Instant,
        thinking_duration_s: Option<f32>,
        tool_calls: Vec<ToolCallItem>,
        done: bool,
    },
}

pub struct State {
    pub input: TextEdit,
    pub messages: Vec<ChatMessage>,
    pub metrics: Option<UsageMetrics>,
    pub is_streaming: bool,
    pub runner: Arc<Mutex<PiRunner>>,
    pub rx: Arc<Mutex<Option<mpsc::UnboundedReceiver<(u64, StreamEvent)>>>>,
    pub active_gen: u64,
    pub collapsed_thinking: HashSet<usize>,
    pub model: String,
    pub thinking_level: String,
}

impl Default for State {
    fn default() -> Self {
        let mut input = TextEdit::new("");
        input.focused = true;
        Self {
            input,
            messages: Vec::new(),
            metrics: None,
            is_streaming: false,
            runner: Arc::new(Mutex::new(PiRunner::new())),
            rx: Arc::new(Mutex::new(None)),
            active_gen: 0,
            collapsed_thinking: HashSet::new(),
            model: "default".to_string(),
            thinking_level: "medium".to_string(),
        }
    }
}

impl State {
    pub fn submit(&mut self) {
        let prompt = self.input.text.trim().to_string();
        if prompt.is_empty() || self.is_streaming {
            return;
        }

        self.input.clear();
        self.messages.push(ChatMessage::User(prompt.clone()));

        // Initial assistant message
        self.messages.push(ChatMessage::Assistant {
            text: String::new(),
            thinking: String::new(),
            thinking_start: Instant::now(),
            thinking_duration_s: None,
            tool_calls: Vec::new(),
            done: false,
        });

        self.is_streaming = true;

        let (tx, rx) = mpsc::unbounded_channel();
        *self.rx.lock().unwrap() = Some(rx);

        let mut runner = self.runner.lock().unwrap();
        let model = if self.model == "default" { None } else { Some(self.model.clone()) };
        let thinking = Some(self.thinking_level.clone());

        if let Ok(()) = runner.spawn_prompt(prompt, model, thinking, tx) {
            self.active_gen = runner.generation();
        } else {
            self.is_streaming = false;
        }
    }
}

fn poll(s: &mut State, _now: f64, _tx: &mut Tx) -> bool {
    let mut updated = false;
    let mut rx_guard = s.rx.lock().unwrap();

    if let Some(rx) = rx_guard.as_mut() {
        while let Ok((gen, event)) = rx.try_recv() {
            if gen != s.active_gen {
                continue;
            }
            updated = true;

            let last_idx = s.messages.len().saturating_sub(1);
            if let Some(ChatMessage::Assistant {
                text,
                thinking,
                thinking_start,
                thinking_duration_s,
                tool_calls,
                done,
            }) = s.messages.get_mut(last_idx)
            {
                match event {
                    StreamEvent::ThinkingDelta(chunk) => {
                        thinking.push_str(&chunk);
                    }
                    StreamEvent::ToolCall { name, args } => {
                        // Mark thinking duration if first tool call
                        if thinking_duration_s.is_none() && !thinking.is_empty() {
                            *thinking_duration_s = Some(thinking_start.elapsed().as_secs_f32());
                        }
                        tool_calls.push(ToolCallItem {
                            name,
                            args,
                            status: ToolStatus::Done,
                            output: String::new(),
                        });
                    }
                    StreamEvent::TextDelta(chunk) => {
                        if thinking_duration_s.is_none() && !thinking.is_empty() {
                            *thinking_duration_s = Some(thinking_start.elapsed().as_secs_f32());
                        }
                        text.push_str(&chunk);
                    }
                    StreamEvent::Metrics(m) => {
                        s.metrics = Some(m);
                    }
                    StreamEvent::Error(err) => {
                        text.push_str(&format!("\n[Error: {}]", err));
                        *done = true;
                        s.is_streaming = false;
                    }
                    StreamEvent::Done => {
                        if thinking_duration_s.is_none() && !thinking.is_empty() {
                            *thinking_duration_s = Some(thinking_start.elapsed().as_secs_f32());
                        }
                        *done = true;
                        s.is_streaming = false;
                    }
                    StreamEvent::Content(c) => {
                        text.push_str(&c);
                    }
                }
            }
        }
    }

    updated
}

fn view(s: &State, _: &mut Cx<State>) -> El {
    // Theme palette: Antigravity dark canvas
    let bg_canvas = Color { r: 0.07, g: 0.07, b: 0.08, a: 1.0 };       // #121214
    let bg_card = Color { r: 0.09, g: 0.09, b: 0.11, a: 1.0 };         // #18181b
    let bg_user = Color { r: 0.12, g: 0.12, b: 0.15, a: 1.0 };         // #1e1e24
    let border_subtle = Color { r: 0.15, g: 0.15, b: 0.17, a: 1.0 };   // #27272a
    let text_primary = Color { r: 0.94, g: 0.94, b: 0.96, a: 1.0 };    // #f4f4f6
    let text_muted = Color { r: 0.55, g: 0.55, b: 0.60, a: 1.0 };      // #8c8c99
    let accent_blue = Color { r: 0.23, g: 0.51, b: 0.96, a: 1.0 };     // #3b82f6
    let green_done = Color { r: 0.13, g: 0.77, b: 0.37, a: 1.0 };      // #22c55e

    // 1. Header Bar
    let title_label = row()
        .gap(8.0)
        .center_y()
        .child(
            el()
                .size(10.0, 10.0)
                .radius(5.0)
                .fill(if s.is_streaming { accent_blue } else { green_done }),
        )
        .child(
            text("Antigravity Canvas")
                .font(type_::TITLE.lh(20.0))
                .color(text_primary),
        );

    let hud_text = if let Some(m) = &s.metrics {
        let pct = (m.total_tokens as f32 / 1_000_000.0) * 100.0;
        format!("{} tok • {:.1}%/1.0M • {:.1}s", m.total_tokens, pct, m.duration_ms as f32 / 1000.0)
    } else {
        "1.0M Context • Ready".to_string()
    };

    let hud_badge = el()
        .px(10.0)
        .py(4.0)
        .radius(6.0)
        .fill(bg_card)
        .border(1.0, border_subtle)
        .child(
            text(&hud_text)
                .font(type_::LABEL)
                .color(text_muted),
        );

    let header_bar = row()
        .w_full()
        .h(44.0)
        .px(16.0)
        .center_y()
        .border(1.0, border_subtle)
        .fill(bg_canvas)
        .child(title_label)
        .child(spacer())
        .child(hud_badge);

    // 2. Messages List (Scrollable)
    let mut message_list = col()
        .key("messages_viewport")
        .w_full()
        .grow(1.0)
        .padding(16.0)
        .gap(14.0)
        .scroll(ScrollSpec { stick_bottom: true });

    if s.messages.is_empty() {
        // Welcome empty state
        let welcome_box = col()
            .w_full()
            .grow(1.0)
            .center()
            .gap(12.0)
            .child(
                text("Pi Overlay")
                    .font(type_::TITLE.lh(28.0))
                    .color(text_primary),
            )
            .child(
                text("GPU-rendered native Rust overlay • Powered by Rune & Antigravity Canvas")
                    .font(type_::BODY)
                    .color(text_muted),
            )
            .child(
                el()
                    .px(12.0)
                    .py(6.0)
                    .radius(8.0)
                    .fill(bg_card)
                    .border(1.0, border_subtle)
                    .child(
                        text("Type a message below or use /explain, /refactor, /review")
                            .font(type_::LABEL)
                            .color(text_muted),
                    ),
            );
        message_list = message_list.child(welcome_box);
    } else {
        for (idx, msg) in s.messages.iter().enumerate() {
            match msg {
                ChatMessage::User(user_text) => {
                    let card = col()
                        .key(format!("msg-user-{idx}"))
                        .w_full()
                        .padding(12.0)
                        .gap(6.0)
                        .radius(10.0)
                        .fill(bg_user)
                        .border(1.0, Color { r: 0.18, g: 0.18, b: 0.22, a: 1.0 })
                        .child(
                            text("You")
                                .font(type_::LABEL)
                                .color(accent_blue),
                        )
                        .child(
                            text(user_text)
                                .font(type_::BODY.lh(22.0))
                                .color(text_primary),
                        );
                    message_list = message_list.child(card);
                }
                ChatMessage::Assistant {
                    text: assistant_text,
                    thinking,
                    thinking_duration_s,
                    tool_calls,
                    ..
                } => {
                    let mut card = col()
                        .key(format!("msg-assistant-{idx}"))
                        .w_full()
                        .padding(14.0)
                        .gap(10.0)
                        .radius(10.0)
                        .fill(bg_card)
                        .border(1.0, border_subtle)
                        .child(
                            text("Antigravity")
                                .font(type_::LABEL)
                                .color(green_done),
                        );

                    // Thinking Accordion
                    if !thinking.is_empty() {
                        let is_collapsed = s.collapsed_thinking.contains(&idx);
                        let duration_str = match thinking_duration_s {
                            Some(dur) => format!("Thought for {:.1}s", dur),
                            None => "Thinking...".to_string(),
                        };
                        let chevron = if is_collapsed { "›" } else { "▾" };

                        let accordion_header = row()
                            .key(format!("thinking-hdr-{idx}"))
                            .h(28.0)
                            .px(8.0)
                            .radius(6.0)
                            .gap(8.0)
                            .center_y()
                            .fill(Color { r: 0.12, g: 0.12, b: 0.14, a: 1.0 })
                            .child(
                                text(chevron)
                                    .font(type_::LABEL)
                                    .color(text_muted),
                            )
                            .child(
                                text(&duration_str)
                                    .font(type_::LABEL)
                                    .color(text_muted),
                            )
                            .on_press(move |s: &mut State| {
                                if s.collapsed_thinking.contains(&idx) {
                                    s.collapsed_thinking.remove(&idx);
                                } else {
                                    s.collapsed_thinking.insert(idx);
                                }
                            });

                        card = card.child(accordion_header);

                        if !is_collapsed {
                            let thinking_body = el()
                                .padding(10.0)
                                .radius(6.0)
                                .fill(Color { r: 0.05, g: 0.06, b: 0.07, a: 1.0 })
                                .child(
                                    text(thinking)
                                        .font(type_::BODY.lh(20.0))
                                        .color(text_muted),
                                );
                            card = card.child(thinking_body);
                        }
                    }

                    // Tool Calls
                    for (tool_idx, tool) in tool_calls.iter().enumerate() {
                        let tool_box = row()
                            .key(format!("tool-{idx}-{tool_idx}"))
                            .px(10.0)
                            .py(6.0)
                            .radius(6.0)
                            .gap(8.0)
                            .center_y()
                            .fill(Color { r: 0.06, g: 0.08, b: 0.10, a: 1.0 })
                            .border(1.0, border_subtle)
                            .child(
                                el()
                                    .px(6.0)
                                    .py(2.0)
                                    .radius(4.0)
                                    .fill(accent_blue.alpha(0.2))
                                    .child(
                                        text("TOOL")
                                            .font(type_::LABEL)
                                            .color(accent_blue),
                                    ),
                            )
                            .child(
                                text(&format!("{}: {}", tool.name, tool.args))
                                    .font(type_::LABEL)
                                    .color(text_primary),
                            );
                        card = card.child(tool_box);
                    }

                    // Assistant Answer Text
                    if !assistant_text.is_empty() {
                        let answer = text(assistant_text)
                            .font(type_::BODY.lh(24.0))
                            .color(text_primary);
                        card = card.child(answer);
                    }

                    message_list = message_list.child(card);
                }
            }
        }
    }

    // 3. Bottom Input Box
    let input_field = text_input(&s.input, type_::BODY.lh(24.0))
        .key("main_input")
        .single_line()
        .grow(1.0)
        .color(text_primary)
        .placeholder("Ask pi or type / for commands...", text_muted)
        .on_edit(|s: &mut State| &mut s.input)
        .on_input_key(|s: &mut State, ev: &KeyEvent, _tx: &mut Tx| {
            if ev.key == Key::Enter {
                s.submit();
                true
            } else {
                false
            }
        });

    let send_button = el()
        .px(14.0)
        .py(8.0)
        .radius(6.0)
        .fill(if s.is_streaming { border_subtle } else { accent_blue })
        .child(
            text(if s.is_streaming { "..." } else { "Send" })
                .font(type_::LABEL)
                .color(Color::WHITE),
        )
        .on_press(|s: &mut State| s.submit());

    let input_container = row()
        .w_full()
        .px(12.0)
        .py(8.0)
        .radius(10.0)
        .center_y()
        .gap(10.0)
        .fill(bg_card)
        .border(1.0, border_subtle)
        .child(input_field)
        .child(send_button);

    let mut bottom_bar = col()
        .w_full()
        .px(16.0)
        .py(12.0)
        .gap(8.0)
        .fill(bg_canvas)
        .border(1.0, border_subtle);

    // Slash command suggestion chips
    if s.input.text.starts_with('/') {
        let matches = commands::match_commands(&s.input.text);
        if !matches.is_empty() {
            let mut chip_row = row().gap(6.0).w_full();
            for cmd in matches.iter().take(4) {
                let template = cmd.template;
                let chip = el()
                    .px(8.0)
                    .py(4.0)
                    .radius(6.0)
                    .fill(bg_card)
                    .border(1.0, border_subtle)
                    .child(
                        text(&format!("{} - {}", cmd.name, cmd.description))
                            .font(type_::LABEL)
                            .color(accent_blue),
                    )
                    .on_press(move |s: &mut State| {
                        s.input = TextEdit::new(template);
                        s.input.focused = true;
                    });
                chip_row = chip_row.child(chip);
            }
            bottom_bar = bottom_bar.child(chip_row);
        }
    }

    let bottom_bar = bottom_bar.child(input_container);

    // Root Container
    col()
        .w_full()
        .h_full()
        .fill(bg_canvas)
        .child(header_bar)
        .child(message_list)
        .child(bottom_bar)
}

fn main() {
    let is_cli_tooling = std::env::args().any(|a| matches!(a.as_str(), "--snap" | "--tree" | "--frames" | "--help" | "-h" | "--list-scenes"));
    // If an existing overlay instance is running, signal it to toggle and exit cleanly.
    if !is_cli_tooling && ipc::try_send_toggle() {
        return;
    }

    // Initialize ambient Tokio runtime for IPC and async runner tasks
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("Failed to create Tokio runtime");
    let _guard = rt.enter();

    if !is_cli_tooling {
        // Start IPC listener for instant toggle from CLI or shortcut
        let (ipc_tx, mut ipc_rx) = mpsc::unbounded_channel();
        ipc::start_ipc_listener(ipc_tx);

        std::thread::spawn(move || {
            while let Some(()) = ipc_rx.blocking_recv() {
                // Signal to toggle or refocus
            }
        });
    }

    let spec = AppSpec {
        poll: Some(poll),
        ..AppSpec::new(
            "Pi Overlay",
            (820.0, 580.0),
            State::default,
            view,
        )
    };

    rune::run(spec);
}
