import { describe, expect, it, vi } from 'vitest';
import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

vi.mock('electron', () => ({
  desktopCapturer: {},
  nativeImage: {},
  screen: {},
  shell: {},
}));

import {
  applyConfigOptionsToCatalog,
  classifyAcpLine,
  normalizeEffortLabel,
  parseSessionNewCatalog,
  pickDesktopActionsFromText,
  pickMarkupAnnotationsFromText,
} from '../src/main/acp-parser';
import { kwinPixelsToBgra } from '../src/main/capture';
import {
  captureWorkspaceCheckpoint,
  getGitDiffReport,
  getGitGlance,
  parseUnifiedDiffOutput,
  restoreGitFile,
  restoreWorkspaceCheckpoint,
} from '../src/main/checkpoint';
import { expandSlashTemplate, parseLocalCommand, parseOsQuickCommand } from '../src/main/commands';
import { percentToPixelCoords } from '../src/main/desktop-actions';
import { extractActiveTrigger, matchMentionItems } from '../src/main/mentions';
import { defaultSettings, resolveHarnessLaunch, syncLinuxAutostartDesktopFile } from '../src/main/settings';
import { loadMcpServersForSession } from '../src/main/acp-client';
import { cleanAudioMimeType, resolveSpeechApiKey, sanitizeTranscription, transcribeAudio } from '../src/main/speech';
import { extractCleanOutput, pickToolKind, presentToolCall } from '../src/renderer/tool-presenter';
import { HARNESS_LIST } from '../src/shared/types';

describe('HARNESS_LIST', () => {
  it('contains pure ACP harnesses including Codex ACP and Claude Code ACP', () => {
    const ids = HARNESS_LIST.map((h) => h.id);
    expect(ids).toEqual(['agy_acp', 'opencode_acp', 'codex_acp', 'claude_acp', 'pi_acp']);
  });

  it('resolves npm runner fallback or direct binary for Codex and Claude ACP', () => {
    const settings = defaultSettings();
    const codex = resolveHarnessLaunch(settings, 'codex_acp');
    const claude = resolveHarnessLaunch(settings, 'claude_acp');
    expect(codex.meta.npmPackage).toBe('@agentclientprotocol/codex-acp');
    expect(claude.meta.npmPackage).toBe('@agentclientprotocol/claude-agent-acp');
    expect(codex.binary.length).toBeGreaterThan(0);
    expect(claude.binary.length).toBeGreaterThan(0);
  });
});

describe('ACP parser & thinking effort normalization', () => {
  it('prefers clean base models in configOptions over flattened legacy models', () => {
    const res = parseSessionNewCatalog('agy_acp', {
      sessionId: 'ses-agy-1',
      models: {
        currentModelId: 'gemini-4-argon-medium',
        availableModels: [
          { modelId: 'gemini-4-argon-low', name: 'Gemini 4 Argon (Low)' },
          { modelId: 'gemini-4-argon-medium', name: 'Gemini 4 Argon (Medium)' },
          { modelId: 'gemini-4-argon-high', name: 'Gemini 4 Argon (High)' },
        ],
      },
      configOptions: [
        {
          id: 'model',
          category: 'model',
          currentValue: 'gemini-4-argon',
          options: [
            { value: 'gemini-4-argon', name: 'Gemini 4 Argon' },
            { value: 'gemini-4-hydrogen', name: 'Gemini 4 Hydrogen' },
          ],
        },
        {
          id: 'thought_level',
          category: 'thought_level',
          currentValue: 'medium',
          options: [
            { value: 'low', name: 'Low' },
            { value: 'medium', name: 'Medium' },
            { value: 'high', name: 'High' },
          ],
        },
      ],
    });

    expect(res.models.map((m) => m.id)).toEqual(['gemini-4-argon', 'gemini-4-hydrogen']);
    expect(res.currentModel).toBe('gemini-4-argon');
    expect(res.thinking.supported).toBe(true);
    expect(res.thinking.configId).toBe('thought_level');
    expect(res.thinking.options.map((o) => o.value)).toEqual(['low', 'medium', 'high']);
  });

  it('updates and clears dynamic OpenCode effort options when model changes', () => {
    const initial = parseSessionNewCatalog('opencode_acp', {
      sessionId: 'ses-oc-1',
      configOptions: [
        {
          id: 'model',
          category: 'model',
          currentValue: '9router/lightweight',
          options: [
            { value: '9router/lightweight', name: '9router/lightweight' },
            { value: 'agentrouter/gpt-6.1-sol', name: 'agentrouter/GPT-6.1 Sol' },
          ],
        },
        {
          id: 'mode',
          category: 'mode',
          currentValue: 'build',
          options: [
            { value: 'build', name: 'Build' },
            { value: 'plan', name: 'Plan' },
          ],
        },
      ],
    });
    expect(initial.thinking.supported).toBe(false);

    // After selecting reasoning model
    const withEffort = applyConfigOptionsToCatalog(initial, [
      {
        id: 'model',
        category: 'model',
        currentValue: 'agentrouter/gpt-6.1-sol',
        options: initial.models.map((m) => ({ value: m.id, name: m.name })),
      },
      {
        id: 'effort',
        category: 'thought_level',
        currentValue: 'default',
        options: [
          { value: 'none', name: 'none' },
          { value: 'low', name: 'low' },
          { value: 'medium', name: 'medium' },
          { value: 'high', name: 'high' },
          { value: 'xhigh', name: 'xhigh' },
          { value: 'max', name: 'max' },
          { value: 'default', name: 'default' },
        ],
      },
    ]);
    expect(withEffort.thinking.supported).toBe(true);
    expect(withEffort.thinking.configId).toBe('effort');
    expect(withEffort.thinking.options.map((o) => o.label)).toEqual([
      'Off',
      'Low',
      'Medium',
      'High',
      'XHigh',
      'Max',
      'Default',
    ]);

    // Switching back to non-reasoning model clears effort cleanly
    const withoutEffort = applyConfigOptionsToCatalog(withEffort, [
      {
        id: 'model',
        category: 'model',
        currentValue: '9router/lightweight',
        options: initial.models.map((m) => ({ value: m.id, name: m.name })),
      },
    ]);
    expect(withoutEffort.thinking.supported).toBe(false);
    expect(withoutEffort.thinking.options).toEqual([]);
  });

  it('normalizes effort labels consistently', () => {
    expect(normalizeEffortLabel('none')).toBe('Off');
    expect(normalizeEffortLabel('xhigh')).toBe('XHigh');
    expect(normalizeEffortLabel('max')).toBe('Max');
  });

  it('classifies JSON-RPC lines accurately', () => {
    expect(
      classifyAcpLine('{"jsonrpc":"2.0","id":1,"method":"session/request_permission","params":{}}'),
    )?.toMatchObject({ kind: 'request', id: 1, method: 'session/request_permission' });
    expect(
      classifyAcpLine('{"jsonrpc":"2.0","method":"session/update","params":{"x":1}}'),
    )?.toMatchObject({ kind: 'notification', method: 'session/update' });
    expect(
      classifyAcpLine('{"jsonrpc":"2.0","id":2,"error":{"code":-32000,"message":"boom"}}'),
    )?.toMatchObject({ kind: 'error_response', id: 2, message: 'boom' });
  });

  it('parses inline markup tags', () => {
    const anns = pickMarkupAnnotationsFromText(
      'Look [[markup: box x=10 y=20 w=30 h=15 label="Button"]] and [[markup: arrow x1=5 x2=15 y1=10 y2=20]]',
    );
    expect(anns).toEqual([
      { kind: 'box', x: 10, y: 20, w: 30, h: 15, label: 'Button' },
      { kind: 'arrow', x1: 5, x2: 15, y1: 10, y2: 20 },
    ]);
  });
});

describe('Mentions & Commands', () => {
  it('extracts @, #, and / triggers without matching emails', () => {
    expect(extractActiveTrigger('foo@bar.com')).toBeNull();
    expect(extractActiveTrigger('check @src/ma')).toEqual({
      kind: 'at',
      start: 6,
      query: 'src/ma',
    });
    expect(extractActiveTrigger('/expl')).toEqual({
      kind: 'slash',
      start: 0,
      query: 'expl',
    });
  });

  it('ranks context items and file matches accurately', () => {
    const matches = matchMentionItems('scr', ['src/screen.ts', 'README.md']);
    expect(matches[0].insert).toBe('@screen');
    expect(matches.some((m) => m.insert === '@src/screen.ts')).toBe(true);
  });

  it('parses local commands and expands templates', () => {
    expect(parseLocalCommand('/screen')).toBe('screen');
    expect(parseLocalCommand('/explain foo')).toBeNull();
    expect(expandSlashTemplate('/fix null deref')).toContain('minimal fixes:');
  });
});

describe('Visual Tool Presenter', () => {
  it('formats shell tool calls visually without noisy JSON metadata', () => {
    const p = presentToolCall({
      name: 'shell',
      title: 'Run checks',
      args: JSON.stringify({ command: 'npm test', cwd: '/tmp/repo' }),
      output: JSON.stringify({
        metadata: { status: 'completed', truncated: false, exit: 0 },
        output: 'All 12 tests passed',
      }),
      status: 'completed',
    });
    expect(p.verb).toBe('Shell');
    expect(p.summary).toBe('npm test');
    expect(p.badge).toBe('exit 0');
    expect(p.sections.some((s) => s.kind === 'command' && s.exitCode === 0)).toBe(true);
    expect(p.sections.some((s) => s.kind === 'code' && s.text.includes('All 12 tests passed'))).toBe(true);
  });

  it('formats edit tool calls with +/- diff lines', () => {
    const p = presentToolCall({
      name: 'edit',
      title: 'Edit file',
      args: JSON.stringify({
        filePath: '/repo/src/index.ts',
        oldString: 'const a = 1;',
        newString: 'const a = 2;',
      }),
      output: '',
      status: 'completed',
    });
    expect(p.verb).toBe('Edit');
    expect(p.summary).toContain('index.ts');
    const diff = p.sections.find((s) => s.kind === 'code' && s.isDiff);
    expect(diff && diff.kind === 'code' ? diff.text : '').toContain('+ const a = 2;');
  });

  it('handles word boundaries, arrays, command arrays, oldText/newText, and non-zero exits', () => {
    const readCmdFile = presentToolCall({
      name: 'read',
      title: 'Read command.ts',
      args: JSON.stringify({ path: 'src/command.ts' }),
      output: '',
      status: 'completed',
    });
    expect(readCmdFile.verb).toBe('Read');

    const arrayCmd = presentToolCall({
      name: 'bash',
      title: 'bash',
      args: JSON.stringify({ command: ['bash', '-lc', 'echo hi'] }),
      output: JSON.stringify({ metadata: { exit: 2, truncated: true }, output: 'failed' }),
      status: 'completed',
    });
    expect(arrayCmd.verb).toBe('Shell');
    expect(arrayCmd.summary).toBe('bash -lc echo hi');
    expect(arrayCmd.badge).toBe('exit 2 · truncated');
    expect(arrayCmd.isError).toBe(true);

    const diffOut = presentToolCall({
      name: 'edit',
      title: 'edit',
      args: JSON.stringify({ path: 'a.ts', oldText: 'old', newText: 'new' }),
      output: 'Diff: a.ts\n- old\n+ new',
      status: 'completed',
    });
    expect(diffOut.sections.some((s) => s.kind === 'code' && s.isDiff)).toBe(true);
  });

  it('parses grouped ACP option lists cleanly', () => {
    const grouped = parseSessionNewCatalog('opencode_acp', {
      sessionId: 'ses-g',
      configOptions: [
        {
          id: 'model',
          category: 'model',
          currentValue: 'm1',
          options: [
            { group: 'Fast', options: [{ value: 'm1', name: 'Model One' }] },
            { group: 'Reasoning', options: [{ value: 'm2', name: 'Model Two' }] },
          ],
        },
      ],
    });
    expect(grouped.models.map((m) => m.id)).toEqual(['m1', 'm2']);
  });
});

describe('Git Workspace Checkpoint & Revert', () => {
  it('captures and cleanly restores file modifications and untracked additions without mutating HEAD', async () => {
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'chat-overlay-ckpt-'));
    try {
      execFileSync('git', ['init'], { cwd: dir });
      execFileSync('git', ['config', 'user.email', 'test@example.com'], { cwd: dir });
      execFileSync('git', ['config', 'user.name', 'Tester'], { cwd: dir });
      fs.writeFileSync(path.join(dir, 'a.txt'), 'original\n');
      execFileSync('git', ['add', '.'], { cwd: dir });
      execFileSync('git', ['commit', '-m', 'init'], { cwd: dir });

      const ckptBefore = await captureWorkspaceCheckpoint(dir);
      expect(ckptBefore.isGit).toBe(true);
      expect(ckptBefore.treeSha).toBeTruthy();

      // Simulate agent edits & newly created file
      fs.writeFileSync(path.join(dir, 'a.txt'), 'modified by agent\n');
      fs.writeFileSync(path.join(dir, 'new_file.txt'), 'temp');

      const ckptAfter = await captureWorkspaceCheckpoint(dir);
      expect(ckptAfter.treeSha).not.toBe(ckptBefore.treeSha);

      // Revert to before
      const res = await restoreWorkspaceCheckpoint(dir, ckptBefore.treeSha);
      expect(res.restored).toBe(true);
      expect(fs.readFileSync(path.join(dir, 'a.txt'), 'utf8')).toBe('original\n');
      expect(fs.existsSync(path.join(dir, 'new_file.txt'))).toBe(false);

      // Redo forward to ckptAfter
      const redoRes = await restoreWorkspaceCheckpoint(dir, ckptAfter.treeSha);
      expect(redoRes.restored).toBe(true);
      expect(fs.readFileSync(path.join(dir, 'a.txt'), 'utf8')).toBe('modified by agent\n');
      expect(fs.readFileSync(path.join(dir, 'new_file.txt'), 'utf8')).toBe('temp');
    } finally {
      fs.rmSync(dir, { recursive: true, force: true });
    }
  });

  it('never touches uncommitted files outside a subdir workspace', async () => {
    const repo = fs.mkdtempSync(path.join(os.tmpdir(), 'chat-overlay-sub-'));
    try {
      const sub = path.join(repo, 'sub');
      fs.mkdirSync(sub, { recursive: true });
      execFileSync('git', ['init'], { cwd: repo });
      execFileSync('git', ['config', 'user.email', 'test@example.com'], { cwd: repo });
      execFileSync('git', ['config', 'user.name', 'Tester'], { cwd: repo });
      fs.writeFileSync(path.join(repo, 'outside.txt'), 'o1\n');
      fs.writeFileSync(path.join(sub, 'inside.txt'), 'i1\n');
      execFileSync('git', ['add', '.'], { cwd: repo });
      execFileSync('git', ['commit', '-m', 'init'], { cwd: repo });

      // User edits outside file before starting chat inside sub/
      fs.writeFileSync(path.join(repo, 'outside.txt'), 'o2-uncommitted\n');

      const ckpt = await captureWorkspaceCheckpoint(sub);
      fs.writeFileSync(path.join(sub, 'inside.txt'), 'i2-by-agent\n');
      fs.writeFileSync(path.join(sub, 'extra.txt'), 'temp\n');

      const res = await restoreWorkspaceCheckpoint(sub, ckpt.treeSha);
      expect(res.restored).toBe(true);
      expect(fs.readFileSync(path.join(sub, 'inside.txt'), 'utf8')).toBe('i1\n');
      expect(fs.existsSync(path.join(sub, 'extra.txt'))).toBe(false);
      // Outside file MUST remain completely untouched
      expect(fs.readFileSync(path.join(repo, 'outside.txt'), 'utf8')).toBe('o2-uncommitted\n');
    } finally {
      fs.rmSync(repo, { recursive: true, force: true });
    }
  });

  it('pins checkpoint refs across git gc and cleans nested empty directories', async () => {
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'chat-overlay-gc-'));
    try {
      execFileSync('git', ['init'], { cwd: dir });
      execFileSync('git', ['config', 'user.email', 'test@example.com'], { cwd: dir });
      execFileSync('git', ['config', 'user.name', 'Tester'], { cwd: dir });
      fs.writeFileSync(path.join(dir, 'root.txt'), 'v1\n');
      execFileSync('git', ['add', '.'], { cwd: dir });
      execFileSync('git', ['commit', '-m', 'init'], { cwd: dir });

      const ckpt = await captureWorkspaceCheckpoint(dir);
      expect(ckpt.treeSha).toBeTruthy();

      // Run aggressive gc --prune=now to verify hidden ref keeps checkpoint alive
      execFileSync('git', ['gc', '--prune=now'], { cwd: dir });

      // Agent creates nested directory + modifies tracked file
      fs.mkdirSync(path.join(dir, 'src/utils/nested'), { recursive: true });
      fs.writeFileSync(path.join(dir, 'src/utils/nested/temp.ts'), 'export const x = 1;');
      fs.writeFileSync(path.join(dir, 'root.txt'), 'v2\n');

      const glance = await getGitGlance(dir);
      expect(glance.isRepo).toBe(true);
      expect(glance.changedFiles.length).toBeGreaterThanOrEqual(2);

      const restored = await restoreWorkspaceCheckpoint(dir, ckpt.treeSha);
      expect(restored.restored).toBe(true);
      expect(fs.readFileSync(path.join(dir, 'root.txt'), 'utf8')).toBe('v1\n');
      expect(fs.existsSync(path.join(dir, 'src'))).toBe(false);
    } finally {
      fs.rmSync(dir, { recursive: true, force: true });
    }
  });

  it('handles non-git directories safely in glance and checkpoint restore', async () => {
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'chat-overlay-nogit-'));
    try {
      const glance = await getGitGlance(dir);
      expect(glance.isRepo).toBe(false);
      const ckpt = await captureWorkspaceCheckpoint(dir);
      expect(ckpt.isGit).toBe(false);
      expect(ckpt.treeSha).toBeNull();
      const res = await restoreWorkspaceCheckpoint(dir, null);
      expect(res.restored).toBe(false);
    } finally {
      fs.rmSync(dir, { recursive: true, force: true });
    }
  });
});

describe('v3.1 layout defaults, autostart, and fast KWin pixel conversion', () => {
  it('defaults to 25% width and 78% compact height', () => {
    const d = defaultSettings();
    expect(d.widthPreset).toBe('wide_25');
    expect(d.heightPreset).toBe('balanced_78');
    expect(d.launchAtLogin).toBe(false);
    expect(d.idleSleepMinutes).toBe(15);
  });

  it('creates and removes XDG autostart desktop entries cleanly', () => {
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'chat-overlay-autostart-'));
    try {
      const desktopFile = path.join(dir, 'chat-overlay.desktop');
      syncLinuxAutostartDesktopFile(true, '"/usr/bin/chat-overlay" --daemon', dir);
      expect(fs.existsSync(desktopFile)).toBe(true);
      const content = fs.readFileSync(desktopFile, 'utf8');
      expect(content).toContain('Exec="/usr/bin/chat-overlay" --daemon');

      syncLinuxAutostartDesktopFile(false, '"/usr/bin/chat-overlay" --daemon', dir);
      expect(fs.existsSync(desktopFile)).toBe(false);
    } finally {
      fs.rmSync(dir, { recursive: true, force: true });
    }
  });

  it('converts RGBX8888 (fmt 16) with stride padding to tight BGRA', () => {
    // 2x2 image with stride=12 (4 bytes padding per row)
    const raw = Buffer.from([
      // Row 0: R=10, G=20, B=30, X=0 | R=40, G=50, B=60, X=99 | padding 4B
      10, 20, 30, 0, 40, 50, 60, 99, 255, 255, 255, 255,
      // Row 1: R=70, G=80, B=90, X=1 | R=1, G=2, B=3, X=2 | padding 4B
      70, 80, 90, 1, 1, 2, 3, 2, 128, 128, 128, 128,
    ]);
    const bgra = kwinPixelsToBgra(raw, 2, 2, 12, 16);
    expect([...bgra]).toEqual([
      30, 20, 10, 255, 60, 50, 40, 255,
      90, 80, 70, 255, 3, 2, 1, 255,
    ]);
  });
});

describe('v3.2 unified diff parser, file restore, and guided highlight markup', () => {
  it('parses unified diff chunks with additions, deletions, and statuses', () => {
    const sample = [
      'diff --git a/src/a.ts b/src/a.ts',
      'index 111..222 100644',
      '--- a/src/a.ts',
      '+++ b/src/a.ts',
      '@@ -1,3 +1,3 @@',
      ' const x = 1;',
      '-const old = true;',
      '+const updated = true;',
      'diff --git a/src/new.ts b/src/new.ts',
      'new file mode 100644',
      '--- /dev/null',
      '+++ b/src/new.ts',
      '@@ -0,0 +1,2 @@',
      '+export const a = 1;',
      '+export const b = 2;',
    ].join('\n');

    const files = parseUnifiedDiffOutput(sample);
    expect(files).toHaveLength(2);
    expect(files[0]).toMatchObject({
      filePath: 'src/a.ts',
      status: 'modified',
      additions: 1,
      deletions: 1,
    });
    expect(files[0].hunks[0].lines.map((l) => l.kind)).toEqual(['ctx', 'del', 'add']);
    expect(files[1]).toMatchObject({
      filePath: 'src/new.ts',
      status: 'added',
      additions: 2,
      deletions: 0,
    });
  });

  it('generates GitDiffReport including untracked files and restores individual files', async () => {
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'chat-overlay-diff-'));
    try {
      execFileSync('git', ['init'], { cwd: dir });
      execFileSync('git', ['config', 'user.email', 'test@example.com'], { cwd: dir });
      execFileSync('git', ['config', 'user.name', 'Tester'], { cwd: dir });
      fs.writeFileSync(path.join(dir, 'tracked.ts'), 'export const v = 1;\n');
      execFileSync('git', ['add', '.'], { cwd: dir });
      execFileSync('git', ['commit', '-m', 'init'], { cwd: dir });

      fs.writeFileSync(path.join(dir, 'tracked.ts'), 'export const v = 2;\n');
      fs.writeFileSync(path.join(dir, 'scratch.txt'), 'hello\nworld\n');

      const report = await getGitDiffReport(dir);
      expect(report.isRepo).toBe(true);
      expect(report.files.map((f) => f.filePath).sort()).toEqual(['scratch.txt', 'tracked.ts']);

      const r1 = await restoreGitFile(dir, 'tracked.ts');
      expect(r1.restored).toBe(true);
      expect(fs.readFileSync(path.join(dir, 'tracked.ts'), 'utf8')).toBe('export const v = 1;\n');

      const r2 = await restoreGitFile(dir, 'scratch.txt');
      expect(r2.restored).toBe(true);
      expect(fs.existsSync(path.join(dir, 'scratch.txt'))).toBe(false);
    } finally {
      fs.rmSync(dir, { recursive: true, force: true });
    }
  });

  it('parses step and pulse attributes in guided highlight markup annotations', () => {
    const text =
      'Click here [[markup: box x=10 y=20 w=30 h=12 step=1 pulse=true label="Open Settings"]] and then [[markup: arrow x1=50 y1=50 x2=80 y2=20 step=2 pulse=false label="Save"]]';
    const items = pickMarkupAnnotationsFromText(text);
    expect(items).toHaveLength(2);
    expect(items[0]).toMatchObject({ kind: 'box', step: 1, pulse: true, label: 'Open Settings' });
    expect(items[1]).toMatchObject({ kind: 'arrow', step: 2, pulse: false, label: 'Save' });
  });
});

describe('v3.2.1 desktop actions, OS slash commands, and coordinate mapping', () => {
  it('parses inline desktop actions and OS quick actions safely', () => {
    const src = [
      'Now saving:',
      '[[desktop: click x=45.5 y=22 label="Save Button"]]',
      '[[desktop: type text="hello world"]]',
      '[[desktop: key combo="ctrl+s"]]',
      '[[desktop: os action=theme_dark]]',
      '[[desktop: os action=volume_set value=65]]',
      '[[desktop: invalid_op foo=1]]',
    ].join('\n');

    const parsed = pickDesktopActionsFromText(src);
    expect(parsed).toHaveLength(5);
    expect(parsed[0]).toMatchObject({ kind: 'click', x: 45.5, y: 22, label: 'Save Button' });
    expect(parsed[1]).toMatchObject({ kind: 'type', text: 'hello world' });
    expect(parsed[2]).toMatchObject({ kind: 'key', combo: 'ctrl+s' });
    expect(parsed[3]).toMatchObject({ kind: 'os', action: 'theme_dark' });
    expect(parsed[4]).toMatchObject({ kind: 'os', action: 'volume_set', value: 65 });
  });

  it('parses direct OS slash commands', () => {
    expect(parseOsQuickCommand('/theme dark')).toMatchObject({ kind: 'os', action: 'theme_dark' });
    expect(parseOsQuickCommand('/snap left')).toMatchObject({ kind: 'os', action: 'snap_left' });
    expect(parseOsQuickCommand('/volume 40')).toMatchObject({ kind: 'os', action: 'volume_set', value: 40 });
    expect(parseOsQuickCommand('/mute')).toMatchObject({ kind: 'os', action: 'volume_mute' });
    expect(parseOsQuickCommand('/open https://example.com')).toMatchObject({
      kind: 'os',
      action: 'open_url',
      target: 'https://example.com',
    });
    expect(parseOsQuickCommand('/explain this')).toBeNull();
  });

  it('clamps and maps percentage screen coordinates to pixels', () => {
    const bounds = { x: 100, y: 50, width: 1920, height: 1080 };
    expect(percentToPixelCoords(50, 50, bounds)).toEqual({ px: 1060, py: 590 });
    expect(percentToPixelCoords(-10, 140, bounds)).toEqual({ px: 100, py: 1130 });
  });
});

describe('ACP question tool, shell output & settings updates', () => {
  it('correctly categorizes question and shell tool kinds', () => {
    expect(pickToolKind('question').category).toBe('question');
    expect(pickToolKind('ask_question').category).toBe('question');
    expect(pickToolKind('bash').category).toBe('shell');
    expect(pickToolKind('run_command').category).toBe('shell');
    expect(pickToolKind('terminal').category).toBe('shell');
  });

  it('extracts clean stdout from nested output objects without losing command results', () => {
    const outputWithStdout = { stdout: 'Line 1\nLine 2\n', exit: 0 };
    expect(extractCleanOutput(outputWithStdout, '')).toBe('Line 1\nLine 2');

    const outputWithNestedContent = {
      content: [{ type: 'text', text: 'khoipm\n' }],
    };
    expect(extractCleanOutput(outputWithNestedContent, '')).toBe('khoipm');

    const outputWithCombinedOut = { output: 'cargo build successful' };
    expect(extractCleanOutput(outputWithCombinedOut, '')).toBe('cargo build successful');
  });

  it('formats shell tool calls with both command and output sections', () => {
    const presented = presentToolCall({
      name: 'bash',
      args: JSON.stringify({ command: 'echo "hello ACP"' }),
      output: JSON.stringify({ stdout: 'hello ACP\n', exit: 0 }),
      status: 'completed',
    });

    expect(presented.verb).toBe('Shell');
    expect(presented.sections.some((s) => s.kind === 'command' && s.command.includes('hello ACP'))).toBe(true);
    expect(presented.sections.some((s) => s.kind === 'code' && s.text.includes('hello ACP'))).toBe(true);
  });

  it('formats question tool calls with questions and choices', () => {
    const presented = presentToolCall({
      name: 'question',
      args: JSON.stringify({
        question: 'Which framework do you prefer?',
        options: ['React', 'Vue', 'Svelte'],
      }),
      output: '',
      status: 'completed',
    });

    expect(presented.verb).toBe('Question');
    expect(presented.icon).toBe('codicon-question');
    expect(presented.sections.some((s) => s.kind === 'text' && s.text.includes('Which framework do you prefer?'))).toBe(true);
    expect(presented.sections.some((s) => s.kind === 'files' && s.items.includes('React'))).toBe(true);
  });

  it('provides autoApproveDesktopActions and speechLanguage defaults in app settings', () => {
    const settings = defaultSettings();
    expect(settings.autoApproveDesktopActions).toBe(true);
    expect(settings.speechLanguage).toBe('auto');
  });

  it('sanitizes audio mime types and speech transcription outputs', () => {
    expect(cleanAudioMimeType('audio/webm;codecs=opus')).toBe('audio/webm');
    expect(cleanAudioMimeType('audio/wav')).toBe('audio/wav');
    expect(cleanAudioMimeType('audio/ogg; codecs=opus')).toBe('audio/ogg');
    expect(cleanAudioMimeType('audio/mpeg')).toBe('audio/mp3');
    expect(cleanAudioMimeType('audio/x-wav')).toBe('audio/wav');
    expect(cleanAudioMimeType('audio/mp4')).toBe('audio/mp4');
    expect(cleanAudioMimeType('unsupported/xyz')).toBe('audio/webm');

    expect(sanitizeTranscription('"Hello world"')).toBe('Hello world');
    expect(sanitizeTranscription("`console.log('hi')`")).toBe("console.log('hi')");
    expect(sanitizeTranscription('```markdown\nHello world\n```')).toBe('Hello world');
    expect(sanitizeTranscription('```\nconst a = 10;\n```')).toBe('const a = 10;');
    expect(sanitizeTranscription('[NO_SPEECH]')).toBe('');
    expect(sanitizeTranscription('[NO_SPEECH].')).toBe('');
    expect(sanitizeTranscription('NO_SPEECH')).toBe('');
    expect(sanitizeTranscription('[Silence]')).toBe('');
    expect(sanitizeTranscription('(silence)')).toBe('');
    expect(sanitizeTranscription('const flag = true;')).toBe('const flag = true;');
  });

  it('rejects transcribeAudio when audio data is empty', async () => {
    const res = await transcribeAudio({ data: '', mimeType: 'audio/webm' });
    expect(res.ok).toBe(false);
    expect(res.error).toContain('No audio data');
  });

  it('transcribes speech audio buffer when API key is available', async () => {
    const speechKey = resolveSpeechApiKey();
    if (!speechKey) return;

    // Verify silence buffer handling (returns ok: true, text: '')
    const sampleRate = 16000;
    const numSamples = sampleRate * 1;
    const buffer = Buffer.alloc(44 + numSamples * 2);
    buffer.write('RIFF', 0);
    buffer.writeUInt32LE(36 + numSamples * 2, 4);
    buffer.write('WAVE', 8);
    buffer.write('fmt ', 12);
    buffer.writeUInt32LE(16, 16);
    buffer.writeUInt16LE(1, 20);
    buffer.writeUInt16LE(1, 22);
    buffer.writeUInt32LE(sampleRate, 24);
    buffer.writeUInt32LE(sampleRate * 2, 28);
    buffer.writeUInt16LE(2, 32);
    buffer.writeUInt16LE(16, 34);
    buffer.write('data', 36);
    buffer.writeUInt32LE(numSamples * 2, 40);

    const resSilence = await transcribeAudio({ data: buffer.toString('base64'), mimeType: 'audio/wav' });
    expect(resSilence.ok).toBe(true);
    expect(resSilence.text).toBe('');

    // Synthesize spoken phrase via espeak-ng if present and verify verbatim transcription
    const tmpSpeechWav = path.join(os.tmpdir(), `vitest-speech-${Date.now()}.wav`);
    try {
      execFileSync('espeak-ng', ['Hello world this is a test', '-w', tmpSpeechWav]);
      if (fs.existsSync(tmpSpeechWav)) {
        const wavB64 = fs.readFileSync(tmpSpeechWav).toString('base64');
        const resSpoken = await transcribeAudio({ data: wavB64, mimeType: 'audio/wav' });
        expect(resSpoken.ok).toBe(true);
        expect(resSpoken.text?.toLowerCase()).toContain('hello');
      }
    } catch {
      // espeak-ng not available in environment
    } finally {
      if (fs.existsSync(tmpSpeechWav)) {
        try {
          fs.unlinkSync(tmpSpeechWav);
        } catch {}
      }
    }
  }, 25000);

  it('formats stdio and http MCP servers conforming to OpenCode specification', async () => {
    const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'mcp-test-'));
    try {
      const mcpConfig = {
        mcpServers: {
          localServer: {
            command: 'node',
            args: ['./server.js'],
            env: { API_KEY: 'test_key', DEBUG: 'true' },
          },
          arrayCmdServer: {
            command: ['python3', '-m', 'server'],
          },
          remoteServer: {
            url: 'https://mcp.example.com/sse',
            headers: { Authorization: 'Bearer 123' },
          },
        },
      };
      fs.writeFileSync(path.join(tmpDir, '.mcp.json'), JSON.stringify(mcpConfig), 'utf8');

      const servers = await loadMcpServersForSession(tmpDir);
      expect(servers.length).toBe(3);

      const local = servers.find((s) => s.name === 'localServer');
      expect(local).toBeDefined();
      expect(local?.command).toBe('node');
      expect(local?.args).toEqual(['./server.js']);
      expect(local?.type).toBeUndefined(); // OpenCode stdio forbids type: 'local'
      expect(local?.env).toEqual([
        { name: 'API_KEY', value: 'test_key' },
        { name: 'DEBUG', value: 'true' },
      ]);

      const arrayCmd = servers.find((s) => s.name === 'arrayCmdServer');
      expect(arrayCmd).toBeDefined();
      expect(arrayCmd?.command).toBe('python3');
      expect(arrayCmd?.args).toEqual(['-m', 'server']);

      const remote = servers.find((s) => s.name === 'remoteServer');
      expect(remote).toBeDefined();
      expect(remote?.type).toBe('http'); // OpenCode requires type: 'http', not 'remote'
      expect(remote?.url).toBe('https://mcp.example.com/sse');
      expect(remote?.headers).toEqual([{ name: 'Authorization', value: 'Bearer 123' }]);
    } finally {
      fs.rmSync(tmpDir, { recursive: true, force: true });
    }
  });

  it('extracts output from response, value, and stderr fields', () => {
    expect(extractCleanOutput({ response: 'Assistant answer' }, '')).toBe('Assistant answer');
    expect(extractCleanOutput({ value: 'Evaluated result' }, '')).toBe('Evaluated result');
    expect(extractCleanOutput({ stdout: 'ok', stderr: 'warning: note' }, '')).toBe('ok\n\n[stderr]\nwarning: note');
  });
});
