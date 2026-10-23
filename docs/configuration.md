# Configuration

Settings are stored at `$XDG_CONFIG_HOME/pi-overlay/config.json` (defaults to `~/.config/pi-overlay/config.json`).

## Example Configuration
```json
{
  "model": "claude-3-7-sonnet",
  "theme": "catppuccin-mocha",
  "width": 520,
  "height": 720,
  "auto_hide_on_blur": true,
  "thinking_level": "medium",
  "sound_bell_on_done": true,
  "ephemeral": true
}
```

## Options
- `model`: Target AI model passed to `pi -p` (e.g. `default`, `claude-3-7-sonnet`, `gpt-4o`).
- `theme`: UI visual theme.
- `width`: Overlay panel width in pixels (bounds: 320 to 1200).
- `height`: Overlay panel height in pixels (bounds: 360 to 1600).
- `auto_hide_on_blur`: Automatically hide the overlay when focus is lost.
- `thinking_level`: Reasoning depth (`off`, `low`, `medium`, `high`).
- `sound_bell_on_done`: Emit system beep upon query completion.
- `ephemeral`: Clear conversation history upon overlay dismissal.

## Slash Commands
Commands expand into prompt templates automatically when entered in the prompt input:
- `/explain <query>`: Structured conceptual or code breakdown.
- `/refactor <query>`: Optimization and clean architecture refactoring.
- `/test <query>`: Comprehensive test suite generation.
- `/fix <query>`: Minimal diff bug fixing.
- `/review <query>`: Security and architecture review.
- `/doc <query>`: API and specification documentation.
