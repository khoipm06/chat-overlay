# Specification: VS Code Copilot Chat Architecture for GTK4 Sidebar Canvas

## 1. Context & Background
This specification defines the architecture, design patterns, and implementation plan for transforming the GTK4 Wayland overlay assistant application (`pi-overlay`) into a desktop sidebar chat interface modeled directly after VS Code's Copilot Chat (`src/vs/workbench/contrib/chat`).

The system integrates with local and remote CLI agent backends (such as `pi` and ACP JSON-RPC) to provide an ergonomic floating canvas sidebar chat with:
- Sequential, inline stream rendering matching VS Code's `chatContentParts`.
- Complete isolation from desktop theme accent color bleeding (preventing Adwaita cyan/blue bleed).
- Bounded dropdown popovers without layout ballooning.
- Responsive, horizontally scrollable structured grid tables (`GtkGrid` inside `GtkScrolledWindow`) with column character-width preservation.
- Safe destructive actions (conversation reset and session item deletions) guarded by confirmation popovers.
- Zero dead code across `src/`.

---

## 2. Requirements & User Stories

### 2.1 Visual Isolation & Theme Fidelity
- **FR-1**: Header buttons (`btn_session`, `btn_settings`, `btn_clear`, `btn_pin`, `btn_min`, `btn_close`) must never display desktop system accent colors (e.g. Adwaita cyan or teal) during hover, active, focus, or checked states.
- **FR-2**: Settings flyout dropdown menus (`Model`, `Reasoning Depth`, `Theme`) must remain bounded to the popover's width (`max-width: 240px`) and never balloon, warp, or render system focus rings when expanded.
- **FR-3**: Dark surface design palette (Radix Dark Zinc / VS Code Editor Dark) must govern all UI surfaces and child controls via `@define-color` variable overrides.

### 2.2 Rich Structured Table Rendering
- **FR-4**: Markdown tables must be parsed into structured column and row matrices (`TableData`), supporting both enclosed pipe syntax (`| a | b |`) and compact GFM syntax (`a | b`).
- **FR-5**: Each table must render as an interactive widget:
  - Header row with bold text, distinct background, and subtle bottom border.
  - Body rows with alternating backgrounds (zebra striping) and cell padding.
  - Cell contents formatted with inline markup (bold, italics, monospace code pills, links).
  - Measured character widths (`col_char_widths`) enforcing column minimum widths (`clamp(64, 360)`), preventing narrow columns from being squished.
  - Enclosed in a `GtkScrolledWindow` with horizontal automatic scrolling to prevent canvas width stretching.

### 2.3 Integrated Tool Invocation Architecture
- **FR-6**: Tool invocations must never leak outside or render in a detached top-level container away from the assistant turn.
- **FR-7**: The backend runner must process both `tool_execution_start` and `tool_execution_end` from the agent CLI, tracking status (`Running`, `Done`, `Error`), input arguments, tool output, and execution duration.
- **FR-8**: Within each assistant card, stream parts must render in chronological order:
  1. Thinking collapsible disclosure (`thinking-disclosure`).
  2. Tool invocation card with tool icon, human-readable action label (e.g. `Terminal: cargo test ›`), status badge, and collapsible input/output.
  3. Formatted markdown response prose, code blocks, and structured tables.
- **FR-9**: Tool outputs must be enclosed in bounded `GtkScrolledWindow` widgets (`max-content-height: 200px`) to prevent unbounded canvas vertical explosion.

### 2.4 Destructive Action Confirmations & Session Controls
- **FR-10**: The "Clear Conversation" action (button and `Ctrl+L` shortcut) must prompt the user with a confirmation dialog before resetting chat history.
- **FR-11**: Each item in the Session History popover must feature a prompt snippet, prompt index, and an explicit Delete icon button (`🗑`).
- **FR-12**: Clicking the delete button must trigger a confirmation popover before removing the targeted turn and its associated widgets from active chat memory and UI.
- **FR-13**: If the active streaming turn is deleted, the subprocess must be immediately aborted and streaming targets unmounted.

---

## 3. Architecture & Component Mapping

| VS Code Chat Component (`contrib/chat`) | `pi-overlay` Rust GTK4 Equivalent | File Reference |
|---|---|---|
| `ChatWidget` / `ChatListWidget` | Main sidebar canvas container (`messages_box`, `chat-scroll`) | `src/main.rs` |
| `ChatThinkingContentPart` (`chatThinkingContentPart.ts`) | `gtk4::Expander` (`thinking-disclosure`) with duration timer | `src/main.rs` |
| `ChatToolInputOutputContentPart` (`chatToolInputOutputContentPart.ts`) | Structured tool invocation card (`tool-call-card`) with status badge & output disclosure | `src/main.rs`, `src/format.rs` |
| `ChatMarkdownContentPart` (`chatMarkdownContentPart.ts`) | Rich Pango markdown block parser (`render::MarkdownBlock::Text`) | `src/render/mod.rs` |
| `ChatMarkdownTableScrolling` (`chatMarkdownTableScrolling.ts`) | `gtk4::ScrolledWindow` wrapping `gtk4::Grid` (`render::MarkdownBlock::Table`) | `src/render/table.rs` |
| `CodeBlockPart` (`codeBlockPart.ts`) | `code-block-container` with language badge & copy button | `src/render/mod.rs` |
| `ChatConfirmationWidget` (`chatConfirmationWidget.ts`) | Confirmation dialog overlay (`GtkPopover` / `.confirm-popover`) | `src/main.rs` |
| `chatSessions.contribution.ts` | Session item row with jump button, delete confirmation popover, and `🗑` icon | `src/main.rs` |

---

## 4. Verification & Testing Plan

### 4.1 Unit Tests
- Stream event JSONL parser tests for `tool_execution_start`, `tool_execution_end`, `thinking_delta`, and edge cases.
- Markdown table parser tests for enclosed and un-enclosed table rows, column alignments, and uneven row padding.
- Slash command expansion tests for `/explain`, `/refactor`, `/fix`, `/test`, `/review`, `/doc`.
- Duration and tool call formatting tests.
- Settings clamping and default values tests.

### 4.2 Integration Verification
- Verify header button hover and active states produce zero cyan accent bleed.
- Verify settings dropdowns remain constrained to 240px width without ballooning.
- Verify clear confirmation modal flow and session prompt deletion with safety confirmation.
- Build release binary via toolbox and verify clean compilation under `cargo clippy -- -D warnings`.
