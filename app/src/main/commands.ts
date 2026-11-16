import type { SlashCommandEntry } from '../shared/types';

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
