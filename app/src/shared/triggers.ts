export type TriggerKind = 'at' | 'hash' | 'slash';

export interface ActiveTrigger {
  kind: TriggerKind;
  start: number;
  query: string;
}

export function extractActiveTrigger(textBeforeCursor: string): ActiveTrigger | null {
  if (textBeforeCursor.startsWith('/')) {
    const rest = textBeforeCursor.slice(1);
    if (!/\s/.test(rest) && !rest.includes('/')) {
      return { kind: 'slash', start: 0, query: rest };
    }
  }

  const lineStart = textBeforeCursor.lastIndexOf('\n') + 1;
  const line = textBeforeCursor.slice(lineStart);
  let found: { idx: number; ch: '@' | '#' } | null = null;

  for (let i = 0; i < line.length; i++) {
    const ch = line[i];
    if (ch === '@' || ch === '#') {
      const prev = i > 0 ? line[i - 1] : '';
      const precededByWord = /[a-zA-Z0-9_]/.test(prev);
      found = precededByWord ? null : { idx: i, ch };
    } else if (/\s/.test(ch)) {
      found = null;
    }
  }

  if (!found) return null;
  const query = line.slice(found.idx + 1);
  return {
    kind: found.ch === '@' ? 'at' : 'hash',
    start: lineStart + found.idx,
    query,
  };
}
