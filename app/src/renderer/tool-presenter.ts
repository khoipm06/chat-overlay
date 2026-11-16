export interface PresentedTool {
  icon: string;
  verb: string;
  summary: string;
  badge?: string;
  isError?: boolean;
  sections: Array<
    | { kind: 'command'; command: string; exitCode?: number }
    | { kind: 'code'; label: string; text: string; isDiff?: boolean }
    | { kind: 'files'; label: string; items: string[] }
    | { kind: 'kv'; entries: Array<{ k: string; v: string }> }
    | { kind: 'text'; label: string; text: string }
  >;
}

function tryParseJson(raw: string): Record<string, unknown> | null {
  const trimmed = raw.trim();
  if (!trimmed.startsWith('{')) return null;
  try {
    const val = JSON.parse(trimmed);
    return val && typeof val === 'object' && !Array.isArray(val)
      ? (val as Record<string, unknown>)
      : null;
  } catch {
    return null;
  }
}

function truncateMiddle(str: string, max = 56): string {
  if (str.length <= max) return str;
  return `${str.slice(0, max - 1)}…`;
}

function shortenPath(p: string): string {
  const clean = p.replace(/\\/g, '/');
  const parts = clean.split('/').filter(Boolean);
  if (parts.length <= 3) return clean;
  return `…/${parts.slice(-3).join('/')}`;
}

function pickToolKind(name: string, title?: string): { icon: string; verb: string; category: string } {
  const nameL = (name || '').toLowerCase();
  const hay = `${nameL} ${(title || '').toLowerCase()}`;
  const hasWord = (text: string, words: string[]) =>
    words.some((w) => new RegExp(`(^|[^a-z])${w}([^a-z]|$)`, 'i').test(text));

  if (hasWord(nameL, ['shell', 'bash', 'command', 'exec', 'terminal', 'run'])) {
    return { icon: 'codicon-terminal', verb: 'Shell', category: 'shell' };
  }
  if (hasWord(nameL, ['read', 'view', 'cat'])) {
    return { icon: 'codicon-file-code', verb: 'Read', category: 'read' };
  }
  if (hasWord(nameL, ['edit', 'patch', 'replace'])) {
    return { icon: 'codicon-edit', verb: 'Edit', category: 'edit' };
  }
  if (hasWord(nameL, ['write', 'create'])) {
    return { icon: 'codicon-new-file', verb: 'Write', category: 'edit' };
  }
  if (hasWord(nameL, ['glob', 'find', 'list'])) {
    return { icon: 'codicon-folder-opened', verb: 'Glob', category: 'search' };
  }
  if (hasWord(nameL, ['grep', 'search', 'rg'])) {
    return { icon: 'codicon-search', verb: 'Search', category: 'search' };
  }
  if (hasWord(nameL, ['subagent', 'task', 'agent'])) {
    return { icon: 'codicon-hub', verb: 'Agent', category: 'agent' };
  }
  if (hasWord(nameL, ['web', 'fetch', 'webfetch', 'websearch'])) {
    return { icon: 'codicon-globe', verb: 'Web', category: 'web' };
  }
  // Secondary match by title words when tool name is generic
  if (hasWord(hay, ['shell', 'bash', 'exec'])) {
    return { icon: 'codicon-terminal', verb: 'Shell', category: 'shell' };
  }
  return { icon: 'codicon-tools', verb: name || title || 'Tool', category: 'other' };
}

/**
 * Converts raw ACP tool payloads into scannable header summaries and user-friendly
 * visual detail sections (instead of dumping raw JSON envelopes).
 */
export function presentToolCall(tool: {
  name: string;
  title?: string;
  args: string;
  output: string;
  status: 'running' | 'completed' | 'failed';
}): PresentedTool {
  const { icon, verb, category } = pickToolKind(tool.name, tool.title);
  const inputObj = tryParseJson(tool.args);
  const outputObj = tryParseJson(tool.output);
  const sections: PresentedTool['sections'] = [];

  let summary = tool.title && tool.title !== tool.name ? tool.title : '';
  const badges: string[] = [];
  let isError = tool.status === 'failed';

  // 1. Inspect structured input
  if (inputObj) {
    const rawCmd = inputObj.command ?? inputObj.cmd;
    const cmd =
      typeof rawCmd === 'string'
        ? rawCmd
        : Array.isArray(rawCmd)
          ? rawCmd.map((x) => String(x)).join(' ')
          : '';
    const filePath =
      typeof inputObj.path === 'string'
        ? inputObj.path
        : typeof inputObj.filePath === 'string'
          ? inputObj.filePath
          : typeof inputObj.file_path === 'string'
            ? inputObj.file_path
            : '';
    const pattern =
      typeof inputObj.pattern === 'string'
        ? inputObj.pattern
        : typeof inputObj.query === 'string'
          ? inputObj.query
          : '';
    const cwd = typeof inputObj.cwd === 'string' ? inputObj.cwd : '';

    if (category === 'shell' && cmd) {
      summary = truncateMiddle(cmd);
      const exitCode = extractExitCode(outputObj);
      sections.push({ kind: 'command', command: cmd, exitCode });
      if (exitCode !== undefined) {
        badges.push(`exit ${exitCode}`);
        if (exitCode !== 0) isError = true;
      }
      if (cwd && !cmd.includes(cwd)) {
        sections.push({ kind: 'kv', entries: [{ k: 'Directory', v: shortenPath(cwd) }] });
      }
    } else if (filePath) {
      summary = shortenPath(filePath);
      const kv: Array<{ k: string; v: string }> = [{ k: 'File', v: filePath }];
      if (typeof inputObj.offset === 'number' || typeof inputObj.line === 'number') {
        kv.push({ k: 'Start line', v: String(inputObj.offset ?? inputObj.line) });
      }
      if (typeof inputObj.limit === 'number') {
        kv.push({ k: 'Limit', v: `${inputObj.limit} lines` });
      }
      sections.push({ kind: 'kv', entries: kv });
      const oldTxt =
        typeof inputObj.oldString === 'string'
          ? inputObj.oldString
          : typeof inputObj.old_string === 'string'
            ? inputObj.old_string
            : typeof inputObj.oldText === 'string'
              ? inputObj.oldText
              : null;
      const newTxt =
        typeof inputObj.newString === 'string'
          ? inputObj.newString
          : typeof inputObj.new_string === 'string'
            ? inputObj.new_string
            : typeof inputObj.newText === 'string'
              ? inputObj.newText
              : null;
      if (oldTxt !== null && newTxt !== null) {
        const diffLines = [
          ...oldTxt.split('\n').map((l) => `- ${l}`),
          ...newTxt.split('\n').map((l) => `+ ${l}`),
        ].join('\n');
        sections.push({ kind: 'code', label: 'Changes', text: diffLines, isDiff: true });
      } else if (typeof inputObj.content === 'string') {
        sections.push({ kind: 'code', label: 'Written content', text: inputObj.content });
      }
    } else if (pattern) {
      summary = truncateMiddle(pattern);
      const kv: Array<{ k: string; v: string }> = [{ k: 'Query', v: pattern }];
      if (cwd || typeof inputObj.path === 'string') {
        kv.push({ k: 'Scope', v: shortenPath(String(cwd || inputObj.path)) });
      }
      sections.push({ kind: 'kv', entries: kv });
    } else if (cwd && category === 'shell') {
      summary = shortenPath(cwd);
      const exitCode = extractExitCode(outputObj);
      if (exitCode !== undefined) {
        badges.push(`exit ${exitCode}`);
        if (exitCode !== 0) isError = true;
      }
      sections.push({
        kind: 'kv',
        entries: [
          { k: 'Working dir', v: cwd },
          ...(exitCode !== undefined ? [{ k: 'Exit code', v: String(exitCode) }] : []),
        ],
      });
    } else {
      const cleanEntries = flattenReadableKv(inputObj);
      if (cleanEntries.length > 0) {
        if (!summary) summary = truncateMiddle(cleanEntries[0].v);
        sections.push({ kind: 'kv', entries: cleanEntries });
      }
    }
  } else if (tool.args.trim()) {
    if (!summary) summary = truncateMiddle(tool.args.trim());
    sections.push({ kind: 'text', label: 'Arguments', text: tool.args.trim() });
  }

  // 2. Inspect output cleanly (extract actual stdout/files/diff rather than metadata JSON!)
  if (tool.output.trim()) {
    if (outputObj) {
      const innerOutput =
        typeof outputObj.output === 'string'
          ? outputObj.output
          : typeof outputObj.stdout === 'string'
            ? outputObj.stdout
            : typeof outputObj.content === 'string'
              ? outputObj.content
              : '';
      const meta =
        outputObj.metadata && typeof outputObj.metadata === 'object'
          ? (outputObj.metadata as Record<string, unknown>)
          : outputObj;

      if (!badges.some((b) => b.startsWith('exit ')) && typeof meta.exit === 'number') {
        badges.push(`exit ${meta.exit}`);
        if (meta.exit !== 0) isError = true;
      }
      if (meta.truncated === true) {
        badges.push('truncated');
      }

      if (innerOutput.trim()) {
        if (category === 'search') {
          const lines = innerOutput.trim().split('\n').filter(Boolean);
          badges.push(`${lines.length} matches`);
          sections.push({ kind: 'files', label: 'Results', items: lines });
        } else {
          sections.push({ kind: 'code', label: 'Output', text: innerOutput.trim() });
        }
      } else {
        const readableMeta = flattenReadableKv(outputObj).filter(
          (e) => !['truncated', 'status'].includes(e.k.toLowerCase()),
        );
        if (readableMeta.length > 0) {
          sections.push({ kind: 'kv', entries: readableMeta });
        }
      }
    } else {
      const trimmed = tool.output.trim();
      if (trimmed.startsWith('Diff: ')) {
        sections.push({ kind: 'code', label: 'File diff', text: trimmed, isDiff: true });
      } else if (category === 'search' && trimmed.includes('\n')) {
        const lines = trimmed.split('\n').filter(Boolean);
        badges.push(`${lines.length} items`);
        sections.push({ kind: 'files', label: 'Matches', items: lines });
      } else {
        sections.push({ kind: 'code', label: 'Output', text: trimmed });
      }
    }
  }

  if (!summary && tool.title) {
    summary = truncateMiddle(tool.title);
  }
  if (summary.toLowerCase() === verb.toLowerCase()) {
    summary = '';
  }

  return {
    icon,
    verb,
    summary,
    badge: badges.length > 0 ? badges.join(' · ') : undefined,
    isError,
    sections,
  };
}

function extractExitCode(outputObj: Record<string, unknown> | null): number | undefined {
  if (!outputObj) return undefined;
  if (typeof outputObj.exit === 'number') return outputObj.exit;
  if (typeof outputObj.exitCode === 'number') return outputObj.exitCode;
  const meta =
    outputObj.metadata && typeof outputObj.metadata === 'object'
      ? (outputObj.metadata as Record<string, unknown>)
      : null;
  if (meta && typeof meta.exit === 'number') return meta.exit;
  if (meta && typeof meta.exitCode === 'number') return meta.exitCode;
  return undefined;
}

function flattenReadableKv(obj: Record<string, unknown>): Array<{ k: string; v: string }> {
  const entries: Array<{ k: string; v: string }> = [];
  for (const [key, val] of Object.entries(obj)) {
    if (val === undefined || val === null) continue;
    if (key === 'metadata' && typeof val === 'object' && !Array.isArray(val)) {
      entries.push(...flattenReadableKv(val as Record<string, unknown>));
      continue;
    }
    const str =
      typeof val === 'string'
        ? val
        : typeof val === 'number' || typeof val === 'boolean'
          ? String(val)
          : JSON.stringify(val);
    if (str.trim()) {
      entries.push({ k: key, v: truncateMiddle(str, 160) });
    }
  }
  return entries.slice(0, 8);
}

export function escapeHtmlText(str: string): string {
  return str
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

export function renderToolDetailHtml(presented: PresentedTool): string {
  if (presented.sections.length === 0) {
    return '<div class="tool-vis-empty">No extra details recorded.</div>';
  }
  return presented.sections
    .map((sec) => {
      if (sec.kind === 'command') {
        const badgeHtml =
          sec.exitCode !== undefined
            ? `<span class="tool-exit-badge ${sec.exitCode === 0 ? 'ok' : 'err'}">exit ${sec.exitCode}</span>`
            : '';
        return `<div class="tool-vis-cmd">
          <span class="tool-vis-prompt">$</span>
          <code>${escapeHtmlText(sec.command)}</code>
          ${badgeHtml}
        </div>`;
      }
      if (sec.kind === 'kv') {
        const rows = sec.entries
          .map(
            (e) =>
              `<div class="tool-vis-kv-row"><span class="tool-vis-k">${escapeHtmlText(e.k)}</span><span class="tool-vis-v">${escapeHtmlText(e.v)}</span></div>`,
          )
          .join('');
        return `<div class="tool-vis-kv">${rows}</div>`;
      }
      if (sec.kind === 'files') {
        const cap = 40;
        const shown = sec.items.slice(0, cap);
        const list = shown
          .map((item) => `<div class="tool-vis-file">${escapeHtmlText(item)}</div>`)
          .join('');
        const more =
          sec.items.length > cap
            ? `<div class="tool-vis-more">+${sec.items.length - cap} more items</div>`
            : '';
        return `<div class="tool-vis-section">
          <div class="tool-vis-sec-label">${escapeHtmlText(sec.label)} (${sec.items.length})</div>
          <div class="tool-vis-file-list">${list}${more}</div>
        </div>`;
      }
      if (sec.kind === 'code') {
        const maxChars = 16_000;
        const charTrimmed = sec.text.length > maxChars;
        const bounded = charTrimmed ? sec.text.slice(0, maxChars) : sec.text;
        const lines = bounded.split('\n');
        const lineTrimmed = lines.length > 60;
        const previewLines = lineTrimmed ? lines.slice(0, 60) : lines;
        const content = sec.isDiff
          ? previewLines
              .map((l) => {
                const cls = l.startsWith('+')
                  ? 'diff-add'
                  : l.startsWith('-')
                    ? 'diff-del'
                    : '';
                return `<div class="${cls}">${escapeHtmlText(l)}</div>`;
              })
              .join('')
          : escapeHtmlText(previewLines.join('\n'));
        const extraNote =
          lineTrimmed || charTrimmed
            ? `<div class="tool-vis-more">Truncated preview (first 60 lines / 16KB)</div>`
            : '';
        return `<div class="tool-vis-section">
          <div class="tool-vis-sec-label">${escapeHtmlText(sec.label)}</div>
          <pre class="tool-vis-code">${content}</pre>
          ${extraNote}
        </div>`;
      }
      return `<div class="tool-vis-section">
        <div class="tool-vis-sec-label">${escapeHtmlText(sec.label)}</div>
        <div class="tool-vis-text">${escapeHtmlText(sec.text)}</div>
      </div>`;
    })
    .join('');
}
