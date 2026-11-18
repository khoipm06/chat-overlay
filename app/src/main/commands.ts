import type { DesktopAction, SlashCommandEntry } from '../shared/types';

export type LocalCommand =
  | 'new'
  | 'clear'
  | 'screen'
  | 'markup'
  | 'region'
  | 'window'
  | 'help';

export const LOCAL_COMMANDS: readonly SlashCommandEntry[] = [
  { name: '/new', description: 'Start a fresh conversation', isLocal: true },
  { name: '/clear', description: 'Clear current conversation', isLocal: true },
  { name: '/screen', description: 'Capture full screen & attach', isLocal: true },
  { name: '/markup', description: 'Capture & annotate screen before sending', isLocal: true },
  { name: '/region', description: 'Capture a screen region', isLocal: true },
  { name: '/window', description: 'Capture active window', isLocal: true },
  { name: '/theme dark', description: 'Switch desktop to dark theme', isLocal: true },
  { name: '/theme light', description: 'Switch desktop to light theme', isLocal: true },
  { name: '/snap left', description: 'Snap active window to left half', isLocal: true },
  { name: '/snap right', description: 'Snap active window to right half', isLocal: true },
  { name: '/mute', description: 'Toggle system audio mute', isLocal: true },
  { name: '/volume', description: 'Set audio volume (/volume 50)', isLocal: true },
  { name: '/help', description: 'Show keyboard shortcuts & commands', isLocal: true },
];

export const PROMPT_TEMPLATES: readonly SlashCommandEntry[] = [
  {
    name: '/explain',
    description: 'Explain clearly and concisely',
    template: 'Please explain the following clearly and concisely:\n\n',
  },
  {
    name: '/refactor',
    description: 'Refactor following clean code standards',
    template: 'Refactor the following code following clean code standards:\n\n',
  },
  {
    name: '/test',
    description: 'Generate unit tests covering edge cases',
    template: 'Generate comprehensive unit tests covering edge cases for:\n\n',
  },
  {
    name: '/fix',
    description: 'Find bugs and provide minimal fixes',
    template: 'Find bugs or edge cases in this code and provide minimal fixes:\n\n',
  },
  {
    name: '/review',
    description: 'Perform a thorough code review',
    template: 'Perform a thorough code review checking security and design:\n\n',
  },
  {
    name: '/doc',
    description: 'Write clean documentation',
    template: 'Write clean, accurate documentation for:\n\n',
  },
];

export function parseLocalCommand(input: string): LocalCommand | null {
  const firstToken = input.trim().split(/\s+/)[0] ?? '';
  if (!firstToken.startsWith('/')) return null;
  const cmd = firstToken.slice(1).toLowerCase();
  if (
    cmd === 'new' ||
    cmd === 'clear' ||
    cmd === 'screen' ||
    cmd === 'markup' ||
    cmd === 'region' ||
    cmd === 'window' ||
    cmd === 'help'
  ) {
    return cmd;
  }
  return null;
}

export function parseOsQuickCommand(input: string): DesktopAction | null {
  const parts = input.trim().split(/\s+/);
  const cmd = (parts[0] || '').toLowerCase();
  const arg1 = (parts[1] || '').toLowerCase();
  const rest = parts.slice(1).join(' ').trim();

  if (cmd === '/theme') {
    if (arg1 === 'dark') return { id: 'os_theme_dark', kind: 'os', action: 'theme_dark', label: 'Dark Theme' };
    if (arg1 === 'light') return { id: 'os_theme_light', kind: 'os', action: 'theme_light', label: 'Light Theme' };
  }
  if (cmd === '/snap') {
    if (arg1 === 'left') return { id: 'os_snap_left', kind: 'os', action: 'snap_left', label: 'Snap Left' };
    if (arg1 === 'right') return { id: 'os_snap_right', kind: 'os', action: 'snap_right', label: 'Snap Right' };
    if (arg1 === 'max' || arg1 === 'maximize') {
      return { id: 'os_snap_max', kind: 'os', action: 'snap_maximize', label: 'Maximize Window' };
    }
  }
  if (cmd === '/mute') {
    return { id: 'os_mute', kind: 'os', action: 'volume_mute', label: 'Toggle Mute' };
  }
  if (cmd === '/volume') {
    const val = Number(arg1);
    if (!Number.isNaN(val) && val >= 0 && val <= 150) {
      return { id: 'os_vol', kind: 'os', action: 'volume_set', value: Math.round(val), label: `Volume ${Math.round(val)}%` };
    }
  }
  if (cmd === '/dnd') {
    const off = arg1 === 'off';
    return { id: 'os_dnd', kind: 'os', action: off ? 'dnd_off' : 'dnd_on', label: off ? 'DND Off' : 'DND On' };
  }
  if (cmd === '/open' && rest) {
    if (/^https?:\/\//i.test(rest)) {
      return { id: 'os_url', kind: 'os', action: 'open_url', target: rest, label: `Open ${rest}` };
    }
    return { id: 'os_app', kind: 'os', action: 'open_app', target: rest, label: `Launch ${rest}` };
  }
  return null;
}

export function expandSlashTemplate(input: string): string {
  const trimmed = input.trimStart();
  if (!trimmed.startsWith('/')) return input;
  if (parseLocalCommand(trimmed)) return input;
  for (const tpl of PROMPT_TEMPLATES) {
    if (trimmed.startsWith(tpl.name)) {
      const rest = trimmed.slice(tpl.name.length);
      if (rest.length === 0 || /^\s/.test(rest)) {
        return `${tpl.template}${rest.trimStart()}`;
      }
    }
  }
  return input;
}
