import fs from 'node:fs/promises';
import path from 'node:path';
import type { MentionItem } from '../shared/types';
export { extractActiveTrigger, type ActiveTrigger, type TriggerKind } from '../shared/triggers';

export const MAX_INDEXED_FILES = 2500;

const SKIP_DIRS = new Set([
  '.git',
  'target',
  'node_modules',
  '.venv',
  'dist',
  'build',
  '.cache',
  '__pycache__',
]);

export function contextMentions(): MentionItem[] {
  return [
    {
      insert: '@screen',
      label: 'Screen',
      detail: 'Capture the live screen with this prompt',
      isContext: true,
    },
    {
      insert: '@markup',
      label: 'Markup',
      detail: 'Capture the screen and annotate it before sending',
      isContext: true,
    },
    {
      insert: '@region',
      label: 'Region',
      detail: 'Capture a region you select',
      isContext: true,
    },
    {
      insert: '@window',
      label: 'Active window',
      detail: 'Capture the focused window',
      isContext: true,
    },
    {
      insert: '@clipboard',
      label: 'Clipboard',
      detail: 'Attach clipboard contents',
      isContext: true,
    },
  ];
}

export async function indexWorkspaceFiles(workspaceDir: string): Promise<string[]> {
  const results: string[] = [];
  const root = path.resolve(workspaceDir);

  async function walk(dir: string): Promise<void> {
    if (results.length >= MAX_INDEXED_FILES) return;
    let entries;
    try {
      entries = await fs.readdir(dir, { withFileTypes: true });
    } catch {
      return;
    }
    entries.sort((a, b) => a.name.localeCompare(b.name));
    for (const entry of entries) {
      if (results.length >= MAX_INDEXED_FILES) return;
      if (entry.isDirectory()) {
        if (SKIP_DIRS.has(entry.name) || entry.name.startsWith('.')) {
          if (entry.name !== '.github') continue;
        }
        await walk(path.join(dir, entry.name));
      } else if (entry.isFile()) {
        const rel = path.relative(root, path.join(dir, entry.name)).replace(/\\/g, '/');
        results.push(rel);
      }
    }
  }

  await walk(root);
  return results;
}

export function matchMentionItems(query: string, files: string[], limit = 24): MentionItem[] {
  const q = query.trim().toLowerCase();
  const ctx = contextMentions();

  const fileItems: MentionItem[] = files.map((rel) => {
    const base = path.basename(rel);
    return {
      insert: `@${rel}`,
      label: base,
      detail: rel,
      isContext: false,
      filePath: rel,
    };
  });

  const combined = [...ctx, ...fileItems];
  if (!q) {
    return combined.slice(0, limit);
  }

  const scored: Array<{ score: number; item: MentionItem }> = [];
  for (const item of combined) {
    const labelL = item.label.toLowerCase();
    const detailL = item.detail.toLowerCase();
    const insL = item.insert.toLowerCase();
    let score = -1;
    if (labelL === q || insL === `@${q}`) score = 100;
    else if (labelL.startsWith(q)) score = 80;
    else if (detailL.startsWith(q)) score = 60;
    else if (labelL.includes(q)) score = 40;
    else if (detailL.includes(q)) score = 20;
    if (score > 0) {
      if (item.isContext) score += 5;
      scored.push({ score, item });
    }
  }

  scored.sort((a, b) => b.score - a.score || a.item.detail.localeCompare(b.item.detail));
  return scored.slice(0, limit).map((s) => s.item);
}
