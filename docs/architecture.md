# Architecture & Protocol

## Surface Management
- Protocol: `zwlr_layer_surface_v1` on Wayland via `gtk4-layer-shell`.
- Non-Wayland / Non-layer-shell: Runtime detection via `gtk4_layer_shell::is_supported()` with fallback to borderless top-level window.
- Inactive state: Window hidden, keyboard mode none, zero input region blocking.
- Active state: Surface mapped on overlay layer, keyboard mode on-demand.

## Agent Process Execution
- Process: Invokes `pi -p --mode json` as a background child process.
- Streaming: Reads NDJSON line-by-line from child stdout.
- Lifecycle: Abort generation tracking terminates previous active processes on new query submission.

## IPC Mechanism
- Socket: `$XDG_RUNTIME_DIR/pi-ephemeral-overlay.sock` (fallback `/tmp/pi-ephemeral-overlay.sock`).
- Protocol: Unix Domain Socket. Secondary instance sends toggle command to primary instance and exits immediately.
