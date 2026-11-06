# Specification: Pi-Overlay Antigravity Chat Canvas Redesign

## 1. Architectural Principles
- **Pure Rust Native**: GTK4 (`gtk4` crate v0.9) + `gtk4-layer-shell` (v0.4). No Webview, No Tauri, No Electron.
- **Wayland Layer-Shell Overlay**: Floating window docked to screen edge/center, supporting `Ctrl+Alt+L` toggle and instant keyboard grab.
- **Composite Card Architecture**: Replace legacy monolithic Pango-markup chat bubbles with modular `GtkBox` composite widgets.
- **Antigravity Dark Canvas**: Dark theme (#121214) with card hierarchy, subtle borders (#27272a), and clear status badges.

---

## 2. Design System & CSS Tokens (`resources/style.css`)
- **Canvas Background**: `#121214`
- **User Card**: Background `#1e1e24`, border `1px solid #2d2d35`, border-radius `10px`.
- **Assistant Card**: Background `#18181b`, border `1px solid #27272a`, border-radius `10px`.
- **Thinking Accordion**:
  - Header: `#1f1f23`, border-radius `6px`.
  - Pulse Dot: `@keyframes pulse-dot`, color `#60a5fa`.
  - Chevron: `▾` (expanded), `›` (collapsed).
  - Body: Font monospace, color `#a1a1aa`.
- **Tool Call Box**:
  - Status RUNNING: Badge `#1e3a8a` / text `#60a5fa`.
  - Status DONE: Badge `#14532d` / text `#4ade80`.
  - Status ERROR: Badge `#7f1d1d` / text `#f87171`.
  - Terminal Output: `#0d1117`, font monospace, collapsible via `GtkRevealer`.
- **Code Block**:
  - Header Bar: `#202024`, language label left, Copy Button right.
  - Body: Monospace `#0d1117`, border-radius `0 0 6px 6px`.
- **Token HUD**:
  - Badge: `#1f1f23`, text `#a1a1aa`.
  - Progress Bar: Colored by percentage (green < 60%, yellow 60-80%, red > 80%).

---

## 3. Widget Module Architecture (`src/widgets/`)
All widgets reside in `src/widgets/` and export through `src/widgets/mod.rs`:

1. `thinking.rs` (`ThinkingAccordion`):
   - Manages start time, duration counter (`Thinking... (1.2s)` -> `Thought for 1.2s`).
   - Toggles body reveal on header button click.
2. `tool_call.rs` (`ToolCallCard`):
   - Displays tool title (e.g. `bash > cargo check`, `read src/main.rs`).
   - Manages status transitions: `Running` -> `Done` or `Error`.
   - Collapsible stdout/stderr terminal viewer.
3. `code_block.rs` (`CodeBlock`):
   - Top bar with language tag.
   - One-click copy to Wayland clipboard using `gdk::Display::default().clipboard()`.
4. `token_hud.rs` (`TokenHud`):
   - Updates total token count, latency, and context window % bar.
5. `message_card.rs` (`UserMessageCard`, `AssistantMessageCard`):
   - Containers appending text segments, code blocks, thinking accordions, and tool calls.

---

## 4. Backend Stream Pipeline (`src/main.rs`)
Events from `src/backend.rs` (`StreamEvent`) must be dispatched to the active assistant card:
- `StreamEvent::ThinkingDelta(text)` -> Route to active `ThinkingAccordion`.
- `StreamEvent::ToolCall(event)` -> Instantiate or update `ToolCallCard`.
- `StreamEvent::TextDelta(text)` -> Route to active `AssistantMessageCard`.
- `StreamEvent::Metrics(metrics)` -> Route to `TokenHud`.
- `StreamEvent::Done` -> Finalize thinking timer, auto-scroll `GtkScrolledWindow` to bottom.

All legacy copy text mentioning "VS Code", "Copilot", or old bubble classes must be removed.

---

## 5. Herdr Multi-Agent Collaboration Protocol

### Roles & Responsibilities:
1. **Coordinator (Antigravity)**:
   - Holds this Spec as Single Source of Truth.
   - Creates worktrees, assigns prompts, and oversees Git merges.
2. **`overlay-ui` (UI & Component Specialist)**:
   - Scope: `resources/style.css` and `src/widgets/`.
   - Responsibility: Implement and test widget components in isolation.
3. **`overlay-lead` (Architecture & Integration Lead)**:
   - Scope: `src/main.rs`, `src/backend.rs`, clean up legacy rendering.
   - Responsibility: Wire the composite widgets into the event loop, ensure Wayland layer-shell stability.
4. **`overlay-qa` (Verification & Code Reviewer)**:
   - Scope: `cargo check`, `cargo test`, `cargo build --release`, reviewing Git diff.
   - Responsibility: Verify zero compiler warnings, test regressions, and runtime styling.

### Operational Rules for Agents:
1. **Worktree Isolation**: Parallel agents must NEVER edit the same working tree concurrently. Each agent operates in its assigned Git worktree or branch.
2. **Artifact-Driven Handoff**: Agents do not chatter randomly in shell. Handoffs occur when code compiles and commits are pushed.
3. **Structured Notifications**: When an agent completes a milestone, it notifies its peer via Herdr CLI:
   `herdr agent prompt <target> "<Summary of changes + Git commit hash>"`
4. **Definition of Done (DoD)**:
   - `cargo check` passes with 0 errors.
   - `cargo test` passes all tests.
   - `cargo build --release` produces `target/release/pi-overlay`.
   - All criteria in Sections 1-4 satisfied.
