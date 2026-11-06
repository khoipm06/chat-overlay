# Specification: VS Code Copilot Chat Architecture for GTK4 Sidebar

## 1. Context & Background
This specification defines the architecture, design tokens, and implementation of the GTK4 desktop sidebar chat interface modeled directly after VS Code's Copilot Chat (`src/vs/workbench/contrib/chat`, commit `75486a6da05b6d5cb589dcce15f222e4d3106121`).

The system integrates with local agent CLI runners to provide a sleek sidebar chat interface with:
- Five distinct regions: Panel Header, Session Sub-Header, Messages Stream, Bottom Pinned Composer, and Status Footer.
- Exact dark charcoal / purple-gray palette tokens (`--chat-bg: #1E1E2A`, `--chat-surface: #292936`, `--chat-composer: #343644`, `--chat-border: #424252`, `--chat-accent: #4FC1FF`).
- Compact pill controls (`@ Agent`, `Auto`, `Balance`) integrated directly into the composer's bottom toolbar, eliminating bulky flyouts.
- Multi-line composer text input (Enter to send, Shift+Enter for newline, draft retention on error).
- Context chips row (`+ ··· spec.md`) at the top of the composer.
- Collapsible progress disclosures (`Completed N steps in Xs ▾`) and assistant feedback action bar (`↻`, `📋`, `👍`, `👎`).
- Safe destructive actions with confirmation popovers.

---

## 2. Palette & Design Tokens

| Token | Color | Role |
|---|---|---|
| `--chat-bg` | `#1E1E2A` | Main sidebar canvas background |
| `--chat-surface` | `#292936` | User message bubbles and elevated surfaces |
| `--chat-composer` | `#343644` | Composer input background and control surfaces |
| `--chat-border` | `#424252` | Window borders, dividers, cell outlines |
| `--chat-text` | `#D4D4E0` | Primary readable text |
| `--chat-muted` | `#A7A8BA` | Secondary text, placeholders, metadata |
| `--chat-accent` | `#4FC1FF` | Icons, active focus, links, send actions |
| `--chat-success` | `#89D185` | Done / successful execution badge |
| `--chat-danger` | `#F48771` | Error badges, stop execution button |

---

## 3. UI Regions & Component Structure

### 3.1 Panel Header (Thanh Panel)
- Area title `Chat` on left.
- Action icons on right: `+` (New Chat), `⌄` (Session History), `···` (More Actions), `◫` (Toggle Layout), `✕` (Close Overlay).
- Zero system accent bleed; pure dark neutral hover and active states.

### 3.2 Session Sub-Header (Thanh Phiên)
- `←` Back navigation button.
- Title: `Local  •  <Session Title>` (with end ellipsizing).
- `◫` Side-by-side split button on right.

### 3.3 Messages Stream (Vùng Hội Thoại)
- Scrolled window independent of panel header and composer.
- User messages: Right-aligned bubble (`--chat-surface` `#292936`, border `#424252`).
- Assistant messages: Transparent background, structured markdown, code blocks, tables.
- Progress steps disclosure: Collapsible `Completed N steps in Xs ▾` showing individual tool steps (`Ran ...`, `Read ...`).
- Assistant feedback bar: Retry (`↻`), Copy (`📋`), Thumbs Up (`👍`), Thumbs Down (`👎`).
- Floating scroll-to-bottom circular button (`↓`).

### 3.4 Pinned Composer (pinned at bottom)
- Background: `--chat-composer` (`#343644`), border `#424252`, border-radius 8px.
- Attached context chips row (`+ ··· spec.md` with delete `✕`).
- Multi-line text view with Enter to send and Shift+Enter for newlines.
- Bottom toolbar:
  - `+` (Attach context chip)
  - `@ Agent` (Mode selector pill)
  - `Auto` (Model selector pill)
  - `Balance` (Reasoning depth pill)
  - `⇄` (Mode toggle)
  - `🎙` (Voice input)
  - Circular send/stop button (`↑` in idle, `⏹` in streaming)

### 3.5 Status Footer (Chân Trạng Thái)
- Left: `Local  •  Autopilot (Preview)`
- Right: Status dot indicator.

---

## 4. Verification & Testing Plan
- `cargo test` verifying all unit tests.
- `cargo clippy -- -D warnings` verifying zero compiler or linter warnings.
- Release binary compilation and installation to `~/.local/bin/pi-overlay`.
