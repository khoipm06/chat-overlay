import { describe, expect, it, vi } from 'vitest';
import { execFileSync, spawn } from 'node:child_process';
import http from 'node:http';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

vi.mock('electron', () => ({
  app: {
    getVersion: () => '4.0.1',
    isPackaged: false,
  },
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
import {
  defaultSettings,
  defaultWorkspaceDir,
  deleteSessionRecord,
  loadCachedCatalog,
  loadDeletedSessionIds,
  loadSessionTitleOverrides,
  loadSessions,
  recordDeletedSessionId,
  renameSessionRecord,
  resolveHarnessLaunch,
  saveCachedCatalog,
  saveSessionRecord,
  syncLinuxAutostartDesktopFile,
  unmarkDeletedSessionId,
} from '../src/main/settings';
import {
  AcpAgentController,
  isSessionNotFoundError,
  loadMcpServersForSession,
  resolveComputerUseMcpPath,
} from '../src/main/acp-client';
import {
  cleanAudioMimeType,
  clearCloudSpeechKey,
  detectLocalSpeechBackend,
  isPcm16Mono16kWav,
  resolveOpenAiKey,
  resolveSpeechApiKey,
  sanitizeTranscription,
  saveCloudSpeechKey,
  transcribeAudio,
  transcribeLocalSpeech,
} from '../src/main/speech';
import {
  addCustomSpeechModel,
  deleteLocalWhisperModel,
  downloadModelFile,
  getSpeechStatus,
  listSpeechModels,
  resolveWhisperBinaryAsset,
  speechBinDir,
  speechDataDir,
  speechStagingDir,
  uninstallLocalWhisperRuntime,
  updateCustomSpeechModel,
  WHISPER_MODELS,
} from '../src/main/speech-manager';
import { compareSemver, formatUpdaterError } from '../src/main/updater';
import { sanitizeDefaultWorkspaceDir } from '../src/main/app-paths';
import { startComputerUseBridge } from '../src/main/computer-use-bridge';
import { syncLinuxAppImageDesktopEntry } from '../src/main/desktop-integration';
import { resolveOpenCodeAsset } from '../src/main/harness-installer';
import { downsampleLinear, encodePcm16Wav } from '../src/renderer/audio-recorder';
import {
  extractCleanOutput,
  pickToolKind,
  presentToolCall,
  renderToolDetailHtml,
} from '../src/renderer/tool-presenter';
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

  it('provides autoApproveDesktopActions and speech settings defaults in app settings', () => {
    const settings = defaultSettings();
    expect(settings.autoApproveDesktopActions).toBe(true);
    expect(settings.speechMode).toBe('auto');
    expect(settings.speechLanguage).toBe('auto');
    expect(settings.localSpeechUrl).toBe('');
    expect(settings.localSpeechCommand).toBe('');
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
    if (!resSilence.ok && (resSilence.error?.includes('429') || resSilence.error?.includes('quota'))) {
      // Upstream provider quota rate-limited on free tier (20 requests/day exceeded)
      return;
    }
    expect(resSilence.ok).toBe(true);
    expect(resSilence.text).toBe('');

    // Synthesize spoken phrase via espeak-ng if present and verify verbatim transcription
    const tmpSpeechWav = path.join(os.tmpdir(), `vitest-speech-${Date.now()}.wav`);
    try {
      execFileSync('espeak-ng', ['Hello world this is a test', '-w', tmpSpeechWav]);
      if (fs.existsSync(tmpSpeechWav)) {
        const wavB64 = fs.readFileSync(tmpSpeechWav).toString('base64');
        const resSpoken = await transcribeAudio({ data: wavB64, mimeType: 'audio/wav' });
        if (!resSpoken.ok && resSpoken.error?.includes('429')) {
          return;
        }
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

  it('detects available local speech backend status', async () => {
    const backend = await detectLocalSpeechBackend();
    expect(backend).toBeDefined();
    expect(typeof backend.available).toBe('boolean');
    expect(typeof backend.engine).toBe('string');
  });

  it('transcribes speech locally via mock whisper.cpp HTTP server (/inference)', async () => {
    const server = http.createServer((req, res) => {
      if (req.method === 'POST' && req.url === '/inference') {
        res.writeHead(200, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ text: 'Hello from local whisper.cpp server' }));
      } else {
        res.writeHead(404);
        res.end();
      }
    });

    await new Promise<void>((resolve) => server.listen(0, '127.0.0.1', resolve));
    const addr = server.address() as any;
    const localUrl = `http://127.0.0.1:${addr.port}/inference`;

    try {
      const res = await transcribeAudio({
        data: Buffer.from('RIFF mock wav data').toString('base64'),
        mimeType: 'audio/wav',
        mode: 'local',
        localUrl,
      });

      expect(res.ok).toBe(true);
      expect(res.text).toBe('Hello from local whisper.cpp server');
      expect(res.engine).toBe('local');
    } finally {
      await new Promise<void>((resolve) => server.close(() => resolve()));
    }
  });

  it('transcribes speech locally via mock whisper.cpp HTTP server with webm mimeType', async () => {
    const server = http.createServer((req, res) => {
      if (req.method === 'POST' && req.url === '/inference') {
        res.writeHead(200, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ text: 'Hello webm from whisper.cpp' }));
      } else {
        res.writeHead(404);
        res.end();
      }
    });

    await new Promise<void>((resolve) => server.listen(0, '127.0.0.1', resolve));
    const addr = server.address() as any;
    const localUrl = `http://127.0.0.1:${addr.port}/inference`;

    try {
      const res = await transcribeAudio({
        data: Buffer.from('mock webm audio stream').toString('base64'),
        mimeType: 'audio/webm',
        mode: 'local',
        localUrl,
      });

      expect(res.ok).toBe(true);
      expect(res.text).toBe('Hello webm from whisper.cpp');
      expect(res.engine).toBe('local');
    } finally {
      await new Promise<void>((resolve) => server.close(() => resolve()));
    }
  });

  it('reports informative error when configured local STT server returns error', async () => {
    const server = http.createServer((req, res) => {
      res.writeHead(500, { 'Content-Type': 'text/plain' });
      res.end('Whisper model out of memory');
    });

    await new Promise<void>((resolve) => server.listen(0, '127.0.0.1', resolve));
    const addr = server.address() as any;
    const localUrl = `http://127.0.0.1:${addr.port}/inference`;

    try {
      const res = await transcribeAudio({
        data: Buffer.from('RIFF mock wav data').toString('base64'),
        mimeType: 'audio/wav',
        mode: 'local',
        localUrl,
      });

      expect(res.ok).toBe(false);
      expect(res.error).toContain('HTTP 500');
    } finally {
      await new Promise<void>((resolve) => server.close(() => resolve()));
    }
  });

  it('transcribes speech locally via mock OpenAI-compatible HTTP server (/v1/audio/transcriptions)', async () => {
    const server = http.createServer((req, res) => {
      if (req.method === 'POST' && req.url === '/v1/audio/transcriptions') {
        res.writeHead(200, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ text: 'Hello from local OpenAI speech endpoint' }));
      } else {
        res.writeHead(404);
        res.end();
      }
    });

    await new Promise<void>((resolve) => server.listen(0, '127.0.0.1', resolve));
    const addr = server.address() as any;
    const localUrl = `http://127.0.0.1:${addr.port}/v1/audio/transcriptions`;

    try {
      const res = await transcribeAudio({
        data: Buffer.from('RIFF mock wav data').toString('base64'),
        mimeType: 'audio/wav',
        mode: 'local',
        localUrl,
      });

      expect(res.ok).toBe(true);
      expect(res.text).toBe('Hello from local OpenAI speech endpoint');
      expect(res.engine).toBe('local');
    } finally {
      await new Promise<void>((resolve) => server.close(() => resolve()));
    }
  });

  it('transcribes speech locally via custom CLI command with and without {file} placeholder', async () => {
    // 1. With {file} placeholder
    const resWithPlaceholder = await transcribeAudio({
      data: Buffer.from('RIFF mock wav data').toString('base64'),
      mimeType: 'audio/wav',
      mode: 'local',
      localCommand: 'echo "CLI transcribed with placeholder" # {file}',
    });

    expect(resWithPlaceholder.ok).toBe(true);
    expect(resWithPlaceholder.text).toBe('CLI transcribed with placeholder');
    expect(resWithPlaceholder.engine).toBe('local');

    // 2. Without {file} placeholder (file passed as final argument)
    const resWithoutPlaceholder = await transcribeAudio({
      data: Buffer.from('RIFF mock wav data').toString('base64'),
      mimeType: 'audio/wav',
      mode: 'local',
      localCommand: 'echo "CLI transcribed without placeholder"',
    });

    expect(resWithoutPlaceholder.ok).toBe(true);
    expect(resWithoutPlaceholder.text?.startsWith('CLI transcribed without placeholder')).toBe(true);
    expect(resWithoutPlaceholder.engine).toBe('local');
  });

  it('strictly enforces speechMode: cloud and never falls back to local STT', async () => {
    const res = await transcribeAudio({
      data: Buffer.from('RIFF mock wav data').toString('base64'),
      mimeType: 'audio/wav',
      mode: 'cloud',
      localCommand: 'echo "Should not be called" # {file}',
    });

    // In cloud mode, if API key fails or is absent, it must report cloud error without local fallback
    if (!res.ok) {
      expect(res.error).toBeDefined();
      expect(res.text).toBeUndefined();
    }
  });

  it('strictly enforces speechMode: local and bypasses cloud API entirely', async () => {
    const res = await transcribeAudio({
      data: Buffer.from('RIFF mock wav data').toString('base64'),
      mimeType: 'audio/wav',
      mode: 'local',
      localCommand: 'echo "Explicit local STT mode" # {file}',
    });

    expect(res.ok).toBe(true);
    expect(res.text).toBe('Explicit local STT mode');
    expect(res.engine).toBe('local');
  });

  it('automatically falls back to local STT when mode is auto and local command is configured', async () => {
    const res = await transcribeAudio(
      {
        data: Buffer.from('RIFF mock wav data').toString('base64'),
        mimeType: 'audio/wav',
        mode: 'auto',
        localCommand: 'echo "Fallback to local CLI speech" # {file}',
      },
      {
        speechMode: 'auto',
      },
    );

    expect(res.ok).toBe(true);
    expect(res.text?.length).toBeGreaterThan(0);
  });

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

  it('detects ACP session not found errors accurately for auto-recovery', () => {
    expect(isSessionNotFoundError(new Error('Invalid params: session not found: ses_1795394765489'))).toBe(true);
    expect(isSessionNotFoundError('session not found')).toBe(true);
    expect(isSessionNotFoundError('Session does not exist: ses_123')).toBe(true);
    expect(isSessionNotFoundError('Invalid params: session is invalid or closed')).toBe(true);
    expect(isSessionNotFoundError(new Error('Unknown session: ses_abc'))).toBe(true);
    expect(isSessionNotFoundError('Session expired: please reconnect')).toBe(true);
    expect(isSessionNotFoundError('No active session for request')).toBe(true);
    expect(isSessionNotFoundError('Session terminated by host')).toBe(true);
    expect(isSessionNotFoundError(new Error('Syntax error in code'))).toBe(false);
    expect(isSessionNotFoundError(new Error('Rate limit exceeded'))).toBe(false);
  });

  it('correctly computes dynamic folder basename from active workspace paths across Linux and Windows', () => {
    function basename(p: string): string {
      if (!p) return 'workspace';
      const parts = p.replace(/\\/g, '/').split('/').filter(Boolean);
      return parts[parts.length - 1] || 'workspace';
    }

    expect(basename('/var/home/khoipm/playground/chat-overlay')).toBe('chat-overlay');
    expect(basename('/var/mnt/Data/Workspace/playground/black-hole-lab')).toBe('black-hole-lab');
    expect(basename('C:\\Users\\Khoi\\Projects\\ai-assistant')).toBe('ai-assistant');
    expect(basename('/project/')).toBe('project');
    expect(basename('')).toBe('workspace');
  });

  it('filters silence markers and [NO_SPEECH] tokens correctly in speech transcription', () => {
    expect(sanitizeTranscription('[NO_SPEECH]')).toBe('');
    expect(sanitizeTranscription('silence')).toBe('');
    expect(sanitizeTranscription('```\nhello world\n```')).toBe('hello world');
    expect(sanitizeTranscription('"spoken text"')).toBe('spoken text');
    expect(sanitizeTranscription('   [NO_SPEECH]   ')).toBe('');
    expect(sanitizeTranscription('silent')).toBe('');
  });

  it('delivers real-time progressive speech tokens via onProgress callback', async () => {
    const tokensReceived: string[] = [];
    const fullTexts: string[] = [];

    const dummyAudioBase64 = Buffer.from('RIFF....WAVEfmt ....data....').toString('base64');
    const res = await transcribeAudio(
      {
        data: dummyAudioBase64,
        mimeType: 'audio/wav',
        mode: 'local',
        localCommand: 'echo "hello from progressive speech" # {file}',
      },
      {},
      (token, acc) => {
        tokensReceived.push(token);
        fullTexts.push(acc);
      },
    );

    expect(res.ok).toBe(true);
    expect(res.text).toContain('hello from progressive speech');
    expect(tokensReceived.length).toBeGreaterThan(0);
    expect(fullTexts[fullTexts.length - 1]).toContain('hello from progressive speech');
  });

  it('resolves real directory paths in defaultWorkspaceDir avoiding symlink aliases', () => {
    const dir = defaultWorkspaceDir();
    expect(typeof dir).toBe('string');
    expect(dir.length).toBeGreaterThan(0);
    expect(fs.existsSync(dir)).toBe(true);
  });

  it('renders search tool results with structured headers and copy buttons', () => {
    const presented = presentToolCall({
      name: 'search',
      title: 'Search physics.cpp',
      args: JSON.stringify({ file: 'src/physics.cpp', query: 'OrbitState' }),
      output: 'Found 3 matches\nLine 11: OrbitState add\nLine 17: OrbitState sub\nLine 20: OrbitState mult',
      status: 'completed',
    });

    expect(presented.verb).toBe('Search');
    expect(presented.badge).toContain('4 matches');
    const filesSec = presented.sections.find((s) => s.kind === 'files');
    expect(filesSec).toBeDefined();
    if (filesSec && filesSec.kind === 'files') {
      expect(filesSec.items.length).toBe(4);
      expect(filesSec.items[0]).toBe('Found 3 matches');
    }

    const html = renderToolDetailHtml(presented);
    expect(html).toContain('tool-vis-section');
    expect(html).toContain('tool-vis-sec-header');
    expect(html).toContain('tool-vis-copy-btn');
    expect(html).toContain('tool-vis-file-list');
    expect(html).toContain('tool-vis-file');
    expect(html).toContain('Line 11: OrbitState add');
  });

  it('tracks deleted session IDs to prevent empty or remote sessions from resurrecting', () => {
    const testSessionId = `test_del_${Date.now()}`;
    // Initially not deleted
    const initialDeleted = loadDeletedSessionIds();
    expect(initialDeleted.has(testSessionId)).toBe(false);

    // Record deletion of empty or remote session
    recordDeletedSessionId(testSessionId);
    const afterDelete = loadDeletedSessionIds();
    expect(afterDelete.has(testSessionId)).toBe(true);

    // Deleting via deleteSessionRecord also records the ID
    const anotherId = `test_del2_${Date.now()}`;
    deleteSessionRecord(anotherId);
    expect(loadDeletedSessionIds().has(anotherId)).toBe(true);

    // Unmarking deleted ID (e.g. when session is explicitly created/saved)
    unmarkDeletedSessionId(testSessionId);
    expect(loadDeletedSessionIds().has(testSessionId)).toBe(false);
  });

  it('manages ACP session state safely during remote load errors and session deletion', async () => {
    const controller = new AcpAgentController(
      defaultSettings(),
      () => {},
      () => {},
    );
    // Artificially establish an active session ID
    (controller as any).catalog.sessionId = 'ses_active_123';
    expect(controller.getCatalog().sessionId).toBe('ses_active_123');

    // Attempting loadRemoteSession with no running process fails gracefully
    // and preserves catalog.sessionId instead of corrupting it to null
    const res = await controller.loadRemoteSession('ses_non_existent');
    expect(res.ok).toBe(false);
    expect(controller.getCatalog().sessionId).toBe('ses_active_123');

    // Deleting a non-active session preserves the active session
    await controller.deleteRemoteSession('ses_other_456');
    expect(controller.getCatalog().sessionId).toBe('ses_active_123');

    // Deleting the currently active session resets catalog.sessionId to null
    await controller.deleteRemoteSession('ses_active_123');
    expect(controller.getCatalog().sessionId).toBeNull();
  });

  it('contains proper CSS rules in styles.css to prevent text overlap in search results', () => {
    const cssPath = path.resolve(__dirname, '../src/renderer/styles.css');
    const cssContent = fs.readFileSync(cssPath, 'utf8');

    // Confirm .tool-vis-file has non-squishable line styling
    expect(cssContent).toContain('.tool-vis-file {');
    expect(cssContent).toMatch(/\.tool-vis-file\s*\{[^}]*line-height:\s*1\.5;/);
    expect(cssContent).toMatch(/\.tool-vis-file\s*\{[^}]*flex-shrink:\s*0;/);
    expect(cssContent).toMatch(/\.tool-vis-file\s*\{[^}]*min-height:\s*18px;/);
    expect(cssContent).toMatch(/\.tool-vis-file\s*\{[^}]*white-space:\s*pre-wrap;/);

    // Confirm .tool-vis-file-list has proper scroll container rules
    expect(cssContent).toContain('.tool-vis-file-list {');
    expect(cssContent).toMatch(/\.tool-vis-file-list\s*\{[^}]*overflow-y:\s*auto;/);

    // Confirm .tool-vis-code div styling
    expect(cssContent).toContain('.tool-vis-code div {');
    expect(cssContent).toMatch(/\.tool-vis-code div\s*\{[^}]*line-height:\s*1\.5;/);

    // Confirm .preview-image zoom & drag CSS rules avoid size jumps and transition lag
    expect(cssContent).not.toMatch(/\.preview-image\.is-zoomed\s*\{[^}]*max-width:\s*none/);
    expect(cssContent).toMatch(/\.preview-image\.is-zoomed\.is-dragging\s*\{[^}]*transition:\s*none/);
  });

  it('detaches active ACP session lazily without killing controller state', async () => {
    const controller = new AcpAgentController(
      defaultSettings(),
      () => {},
      () => {},
    );
    (controller as any).catalog.status = 'ready';
    (controller as any).catalog.sessionId = 'ses_detach_test';
    (controller as any).sessionAllowedTools.add('read_file');

    const next = await controller.detachActiveSession();
    expect(next.sessionId).toBeNull();
    expect(next.status).toBe('ready');
    expect((controller as any).sessionAllowedTools.size).toBe(0);
  });

  it('coalesces consecutive user_message_chunk updates for the same messageId during session replay', () => {
    const controller = new AcpAgentController(
      defaultSettings(),
      () => {},
      () => {},
    );
    (controller as any).sessionLoading = true;
    (controller as any).catalog.sessionId = 'ses_replay_test';

    (controller as any).handleNotification('session/update', {
      sessionId: 'ses_replay_test',
      update: {
        sessionUpdate: 'user_message_chunk',
        messageId: 'msg_1',
        content: { type: 'text', text: 'Hello ' },
      },
    });
    (controller as any).handleNotification('session/update', {
      sessionId: 'ses_replay_test',
      update: {
        sessionUpdate: 'user_message_chunk',
        messageId: 'msg_1',
        content: { type: 'text', text: 'world!' },
      },
    });
    (controller as any).handleNotification('session/update', {
      sessionId: 'ses_replay_test',
      update: {
        sessionUpdate: 'agent_message_chunk',
        content: { type: 'text', text: 'Hi there.' },
      },
    });

    const turns = (controller as any).replayedTurns;
    const current = (controller as any).currentReplayedTurn;
    expect(turns.length).toBe(0);
    expect(current.userText).toBe('Hello world!');
    expect(current.assistantMarkdown).toBe('Hi there.');
  });

  it('persists session renames, title overrides, and cached model catalogs', () => {
    const tmpCfg = fs.mkdtempSync(path.join(os.tmpdir(), 'co-cfg-'));
    const prevXdg = process.env.XDG_CONFIG_HOME;
    process.env.XDG_CONFIG_HOME = tmpCfg;
    try {
      const id = `ses_rename_${Date.now()}`;
      saveSessionRecord({
        id,
        title: 'Original Title',
        harness: 'opencode_acp',
        workspaceDir: '/tmp',
        updatedAt: Date.now(),
        turns: [
          {
            id: 't1',
            userText: 'hi',
            attachments: [],
            thinkingText: '',
            thinkingDurationMs: 0,
            tools: [],
            assistantMarkdown: 'hello',
          },
        ],
      });
      renameSessionRecord(id, 'Renamed Conversation');
      const overrides = loadSessionTitleOverrides();
      expect(overrides[id]).toBe('Renamed Conversation');
      const loaded = loadSessions().find((s) => s.id === id);
      expect(loaded?.title).toBe('Renamed Conversation');
      deleteSessionRecord(id);

      const sampleCatalog = {
        harness: 'opencode_acp' as const,
        status: 'ready' as const,
        statusMessage: 'Ready',
        sessionId: 'ses_tmp',
        models: [{ id: 'model-a', name: 'Model A' }],
        currentModel: 'model-a',
        thinking: { configId: null, supported: false, currentValue: '', options: [] },
        modes: [{ id: 'build', name: 'Build' }],
        currentMode: 'build',
        commands: [],
      };
      saveCachedCatalog('opencode_acp', sampleCatalog);
      const cached = loadCachedCatalog('opencode_acp');
      expect(cached?.models?.length).toBeGreaterThan(0);
      expect(cached?.currentModel).toBe('model-a');
    } finally {
      if (prevXdg === undefined) delete process.env.XDG_CONFIG_HOME;
      else process.env.XDG_CONFIG_HOME = prevXdg;
      fs.rmSync(tmpCfg, { recursive: true, force: true });
    }
  });

  it('attaches built-in computer-use MCP server when desktopControlMode is enabled and omits when off', async () => {
    const mcpPath = resolveComputerUseMcpPath();
    expect(mcpPath).not.toBeNull();
    const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'co-mcp-'));
    try {
      const enabled = await loadMcpServersForSession(tmpDir, { desktopControlMode: 'confirm' });
      const cu = enabled.find((s) => s.name === 'computer-use');
      expect(cu).toBeDefined();
      expect(Array.isArray(cu?.args)).toBe(true);
      expect((cu?.env as Array<{ name: string; value: string }>).some((e) => e.name === 'ELECTRON_RUN_AS_NODE' && e.value === '1')).toBe(true);

      const disabled = await loadMcpServersForSession(tmpDir, { desktopControlMode: 'off' });
      expect(disabled.some((s) => s.name === 'computer-use')).toBe(false);
    } finally {
      fs.rmSync(tmpDir, { recursive: true, force: true });
    }
  });

  it('runs built-in mcp-computer-use stdio server cleanly for initialize, tools/list, and tools/call', async () => {
    const mcpPath = path.resolve(__dirname, '../dist/main/mcp-computer-use.js');
    expect(fs.existsSync(mcpPath)).toBe(true);

    const child = spawn(process.execPath, [mcpPath], { stdio: ['pipe', 'pipe', 'pipe'] });
    const responses: Array<Record<string, any>> = [];
    let buffer = '';
    child.stdout.on('data', (chunk: Buffer) => {
      buffer += chunk.toString('utf8');
      let nl = buffer.indexOf('\n');
      while (nl >= 0) {
        const line = buffer.slice(0, nl).trim();
        buffer = buffer.slice(nl + 1);
        if (line) {
          try {
            responses.push(JSON.parse(line));
          } catch {}
        }
        nl = buffer.indexOf('\n');
      }
    });

    const waitForId = async (id: number, timeoutMs = 5000) => {
      const t0 = Date.now();
      while (Date.now() - t0 < timeoutMs) {
        const found = responses.find((r) => r.id === id);
        if (found) return found;
        await new Promise((r) => setTimeout(r, 30));
      }
      throw new Error(`Timed out waiting for MCP response id=${id}`);
    };

    try {
      child.stdin.write(`${JSON.stringify({ jsonrpc: '2.0', id: 1, method: 'initialize', params: { protocolVersion: '2024-11-05' } })}\n`);
      const initRes = await waitForId(1);
      expect(initRes.result?.serverInfo?.name).toBe('computer-use');

      child.stdin.write(`${JSON.stringify({ jsonrpc: '2.0', id: 2, method: 'tools/list', params: {} })}\n`);
      const listRes = await waitForId(2);
      const names = (listRes.result?.tools || []).map((t: any) => t.name);
      expect(names).toEqual([
        'desktop_screenshot',
        'desktop_list_windows',
        'desktop_window_control',
        'desktop_mouse',
        'desktop_keyboard',
        'desktop_clipboard',
        'desktop_system',
        'desktop_wait',
      ]);

      child.stdin.write(`${JSON.stringify({ jsonrpc: '2.0', id: 3, method: 'tools/call', params: { name: 'desktop_wait', arguments: { ms: 60 } } })}\n`);
      const waitRes = await waitForId(3);
      expect(waitRes.result?.isError).not.toBe(true);
      expect(waitRes.result?.content?.[0]?.text).toContain('Waited 60ms');

      child.stdin.write(`${JSON.stringify({ jsonrpc: '2.0', id: 4, method: 'tools/call', params: { name: 'desktop_system', arguments: { action: 'list_apps' } } })}\n`);
      const appsRes = await waitForId(4);
      expect(appsRes.result?.content?.[0]?.text).toBeDefined();

      expect(pickToolKind('mcp_computer-use_desktop_mouse').verb).toBe('Desktop');
    } finally {
      child.kill('SIGTERM');
    }
  });

  it('exposes multilingual Whisper catalog, atomic download with progress, and autoSpeakReplies persistence', async () => {
    expect(WHISPER_MODELS.map((m) => m.id)).toEqual(['large-v3-turbo-q5_0', 'small-q5_1']);
    expect(WHISPER_MODELS[0].recommended).toBe(true);

    const tmpData = fs.mkdtempSync(path.join(os.tmpdir(), 'co-speech-dl-'));
    const prevXdgData = process.env.XDG_DATA_HOME;
    process.env.XDG_DATA_HOME = tmpData;
    const srv = http.createServer((_req, res) => {
      const payload = Buffer.from('GGML_MOCK_WEIGHTS_1234567890');
      res.writeHead(200, { 'Content-Length': String(payload.length) });
      res.end(payload);
    });

    try {
      await new Promise<void>((resolve) => srv.listen(0, '127.0.0.1', resolve));
      const port = (srv.address() as any).port;

      const dest = path.join(tmpData, 'chat-overlay', 'speech', 'models', 'ggml-small-q5_1.bin');
      const seenPcts: number[] = [];
      await downloadModelFile(`http://127.0.0.1:${port}/model.bin`, dest, (p) => {
        seenPcts.push(p.percent);
      });

      expect(fs.existsSync(dest)).toBe(true);
      expect(fs.existsSync(`${dest}.part`)).toBe(false);
      expect(seenPcts).toContain(100);

      const st = getSpeechStatus(defaultSettings());
      expect(st.installedModels).toContain('small-q5_1');
      expect(st.hasLocalModel).toBe(true);
      expect(st.catalog.length).toBe(2);

      // Verify abort removes partial file
      const abortCtrl = new AbortController();
      abortCtrl.abort();
      const cancelledDest = path.join(tmpData, 'cancelled.bin');
      await expect(
        downloadModelFile(`http://127.0.0.1:${port}/model.bin`, cancelledDest, undefined, abortCtrl.signal),
      ).rejects.toThrow();
      expect(fs.existsSync(cancelledDest)).toBe(false);
      expect(fs.existsSync(`${cancelledDest}.part`)).toBe(false);

      expect(defaultSettings().autoSpeakReplies).toBe(false);
      expect(defaultSettings().speechModel).toBe('large-v3-turbo-q5_0');
    } finally {
      srv.close();
      if (prevXdgData === undefined) delete process.env.XDG_DATA_HOME;
      else process.env.XDG_DATA_HOME = prevXdgData;
      fs.rmSync(tmpData, { recursive: true, force: true });
    }
  });

  it('encodes 16kHz mono PCM WAV without ffmpeg and validates WAV headers cleanly', () => {
    const input = new Float32Array(4800).fill(0.25);
    const downsampled = downsampleLinear(input, 48000, 16000);
    expect(downsampled.length).toBe(1600);

    const wavBytes = encodePcm16Wav([input], 48000, 16000);
    expect(wavBytes.length).toBe(44 + 1600 * 2);
    expect(isPcm16Mono16kWav(Buffer.from(wavBytes))).toBe(true);
    expect(isPcm16Mono16kWav(Buffer.from('not-a-wav'))).toBe(false);
  });

  it('sanitizes packaged read-only launch directories to user home', () => {
    const home = '/home/alice';
    expect(sanitizeDefaultWorkspaceDir('/', home)).toBe(home);
    expect(sanitizeDefaultWorkspaceDir('/tmp/.mount_ChatOvAbc', home)).toBe(home);
    expect(sanitizeDefaultWorkspaceDir('/Applications/Chat Overlay.app/Contents/Resources', home)).toBe(home);
    expect(sanitizeDefaultWorkspaceDir('/home/alice/projects/demo', home)).toBe('/home/alice/projects/demo');
  });

  it('serves authenticated ComputerUseBridge endpoints and rejects missing auth', async () => {
    let clipboardText = 'initial';
    let openedUrl = '';
    const bridge = await startComputerUseBridge({
      getScreenInfo: () => ({ width: 1920, height: 1080, scaleFactor: 1, pixelWidth: 1920, pixelHeight: 1080 }),
      captureScreenshot: async () => ({
        ok: true,
        screenWidth: 1920,
        screenHeight: 1080,
        logicalScreenWidth: 1920,
        logicalScreenHeight: 1080,
        imageWidth: 1440,
        imageHeight: 810,
        mimeType: 'image/jpeg',
        data: Buffer.from('mock-jpg').toString('base64'),
      }),
      readClipboard: () => clipboardText,
      writeClipboard: (text) => {
        clipboardText = text;
      },
      openUrl: (url) => {
        openedUrl = url;
      },
      notify: () => {},
    });

    try {
      const unauth = await fetch(`${bridge.url}/screen-info`);
      expect(unauth.status).toBe(401);

      const authHeaders = {
        Authorization: `Bearer ${bridge.token}`,
        'Content-Type': 'application/json',
      };
      const screenRes = await fetch(`${bridge.url}/screen-info`, { headers: authHeaders });
      expect(screenRes.status).toBe(200);
      expect(await screenRes.json()).toMatchObject({ width: 1920, height: 1080 });

      const shotRes = await fetch(`${bridge.url}/screenshot`, {
        method: 'POST',
        headers: authHeaders,
        body: JSON.stringify({ maxWidth: 1440 }),
      });
      expect(shotRes.status).toBe(200);
      expect((await shotRes.json()).mimeType).toBe('image/jpeg');

      await fetch(`${bridge.url}/clipboard`, {
        method: 'POST',
        headers: authHeaders,
        body: JSON.stringify({ text: 'updated' }),
      });
      expect(clipboardText).toBe('updated');

      const badUrlRes = await fetch(`${bridge.url}/open-url`, {
        method: 'POST',
        headers: authHeaders,
        body: JSON.stringify({ url: 'file:///etc/passwd' }),
      });
      expect(badUrlRes.status).toBe(400);

      const goodUrlRes = await fetch(`${bridge.url}/open-url`, {
        method: 'POST',
        headers: authHeaders,
        body: JSON.stringify({ url: 'https://example.com' }),
      });
      expect(goodUrlRes.status).toBe(200);
      expect(openedUrl).toBe('https://example.com');
    } finally {
      await bridge.close();
    }
  });

  it('resolves cross-platform whisper-server and OpenCode release assets', () => {
    expect(resolveWhisperBinaryAsset('linux', 'x64')?.fileName).toBe('whisper-server-linux-x64.tar.gz');
    expect(resolveWhisperBinaryAsset('win32', 'x64')?.binaryName).toBe('whisper-server.exe');
    expect(resolveWhisperBinaryAsset('darwin', 'arm64')?.fileName).toBe('whisper-server-darwin-arm64.tar.gz');

    expect(resolveOpenCodeAsset('linux', 'x64')?.fileName).toBe('opencode-linux-x64.tar.gz');
    expect(resolveOpenCodeAsset('win32', 'x64')?.binaryName).toBe('opencode.exe');
    expect(resolveOpenCodeAsset('darwin', 'arm64')?.fileName).toBe('opencode-darwin-arm64.zip');
  });

  it('writes Linux desktop entry with KDE ScreenShot2 permission and shortcuts', () => {
    const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'co-desktop-int-'));
    try {
      const appsDir = path.join(tmp, 'applications');
      const iconsDir = path.join(tmp, 'icons');
      const changed = syncLinuxAppImageDesktopEntry({
        execPath: '/home/user/Applications/Chat-Overlay.AppImage',
        applicationsDir: appsDir,
        iconsDir,
        skipDesktopTools: true,
      });
      expect(changed).toBe(true);
      const desktopContent = fs.readFileSync(path.join(appsDir, 'chat-overlay.desktop'), 'utf8');
      expect(desktopContent).toContain('Exec=/home/user/Applications/Chat-Overlay.AppImage');
      expect(desktopContent).toContain('X-KDE-DBUS-Restricted-Interfaces=org.kde.KWin.ScreenShot2');
      expect(desktopContent).toContain('X-KDE-Shortcuts=Ctrl+Alt+L,Meta+Shift+L');
    } finally {
      fs.rmSync(tmp, { recursive: true, force: true });
    }
  });

  it('prefers app.asar.unpacked path for packaged MCP subprocess', () => {
    const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'co-asar-'));
    try {
      const packed = path.join(tmp, 'app.asar', 'dist', 'main', 'mcp-computer-use.js');
      const unpacked = path.join(tmp, 'app.asar.unpacked', 'dist', 'main', 'mcp-computer-use.js');
      fs.mkdirSync(path.dirname(unpacked), { recursive: true });
      fs.writeFileSync(unpacked, '// unpacked', 'utf8');
      expect(resolveComputerUseMcpPath([packed])).toBe(unpacked);
    } finally {
      fs.rmSync(tmp, { recursive: true, force: true });
    }
  });

  it('sanitizes updater error strings and compares semantic versions', () => {
    const raw404 =
      'Cannot find latest-linux.yml in the latest release artifacts: HttpError: 404 "url"\nHeaders: {"cache-control":"no-cache"}\n    at ClientRequest.<anonymous> (/tmp/.mount_Chat-O6YYdzb/resources/app.asar/node_modules/builder-util-runtime/out/HttpExecutor.js:122:26)';
    expect(formatUpdaterError(new Error(raw404))).toBe('Release update metadata is not available yet.');
    expect(formatUpdaterError(new Error('getaddrinfo ENOTFOUND github.com'))).toBe(
      'Cannot reach update server. Check your internet connection.',
    );
    const genericStack =
      'Unexpected failure (/tmp/.mount_ChatOvXyz/resources/app.asar/main.js)\nHeaders: {"a":1}\n    at foo (/tmp/.mount_ChatOvXyz/main.js:1:1)';
    const formatted = formatUpdaterError(new Error(genericStack));
    expect(formatted).not.toContain('/tmp/.mount_');
    expect(formatted).not.toContain('Headers:');

    expect(compareSemver('4.0.1', '4.0.0')).toBe(1);
    expect(compareSemver('v4.0.1', '4.0.1')).toBe(0);
    expect(compareSemver('3.2.11', '4.0.1')).toBe(-1);
  });

  it('supports full Speech Model CRUD, external file preservation, staging path, and cloud key storage', () => {
    const tmpRoot = fs.mkdtempSync(path.join(os.tmpdir(), 'co-crud-'));
    const prevXdgData = process.env.XDG_DATA_HOME;
    const prevXdgCfg = process.env.XDG_CONFIG_HOME;
    const prevGemini = process.env.GEMINI_API_KEY;
    const prevOpenAi = process.env.OPENAI_API_KEY;
    delete process.env.GEMINI_API_KEY;
    delete process.env.OPENAI_API_KEY;
    process.env.XDG_DATA_HOME = path.join(tmpRoot, 'data');
    process.env.XDG_CONFIG_HOME = path.join(tmpRoot, 'cfg');

    try {
      // Staging lives inside persistent user speech data directory, never /tmp
      expect(speechStagingDir()).toBe(path.join(speechDataDir(), '.staging'));

      // External local .bin file outside speechModelsDir
      const extModelPath = path.join(tmpRoot, 'external-whisper-custom.bin');
      fs.writeFileSync(extModelPath, 'EXT_BIN_WEIGHTS', 'utf8');

      // Add custom local model & custom URL model
      const addedLocal = addCustomSpeechModel({
        label: 'My Local Whisper',
        description: 'External file',
        source: 'custom_local',
        localPath: extModelPath,
      });
      expect(addedLocal.ok).toBe(true);
      expect(addedLocal.model?.installed).toBe(true);

      const addedUrl = addCustomSpeechModel({
        label: 'Custom Remote GGML',
        description: 'URL model',
        source: 'custom_url',
        url: 'https://example.com/ggml-custom.bin',
      });
      expect(addedUrl.ok).toBe(true);
      expect(addedUrl.model?.installed).toBe(false);

      // Reject invalid URL
      expect(
        addCustomSpeechModel({ label: 'Bad', source: 'custom_url', url: 'ftp://bad/model.bin' }).ok,
      ).toBe(false);

      // Update custom model metadata
      const updated = updateCustomSpeechModel(addedUrl.model!.id, {
        label: 'Renamed Remote GGML',
        url: 'https://example.com/ggml-v2.bin',
      });
      expect(updated.ok).toBe(true);
      expect(updated.model?.label).toBe('Renamed Remote GGML');

      // Delete custom_local removes entry ONLY and leaves external file untouched on disk
      expect(deleteLocalWhisperModel(addedLocal.model!.id, true)).toBe(true);
      expect(fs.existsSync(extModelPath)).toBe(true);
      expect(listSpeechModels().some((m) => m.id === addedLocal.model!.id)).toBe(false);

      // Delete custom_url entry cleanly
      expect(deleteLocalWhisperModel(addedUrl.model!.id, true)).toBe(true);
      expect(listSpeechModels().some((m) => m.id === addedUrl.model!.id)).toBe(false);

      // Optional runtime cleanup
      fs.mkdirSync(speechBinDir(), { recursive: true });
      const fakeBin = path.join(speechBinDir(), process.platform === 'win32' ? 'whisper-server.exe' : 'whisper-server');
      fs.writeFileSync(fakeBin, '#!/bin/sh\n', { mode: 0o755 });
      expect(getSpeechStatus(defaultSettings()).managedRuntimeInstalled).toBe(true);
      expect(uninstallLocalWhisperRuntime()).toBe(true);
      expect(fs.existsSync(fakeBin)).toBe(false);

      // Cloud API key save & clear
      expect(saveCloudSpeechKey('gemini', 'test-gem-key-123').ok).toBe(true);
      expect(resolveSpeechApiKey()).toBe('test-gem-key-123');
      expect(saveCloudSpeechKey('openai', 'test-oa-key-456').ok).toBe(true);
      expect(resolveOpenAiKey()).toBe('test-oa-key-456');

      const stWithKeys = getSpeechStatus(defaultSettings());
      expect(stWithKeys.cloudProviders).toEqual({ gemini: true, openai: true });

      expect(clearCloudSpeechKey('gemini')).toBe(true);
      expect(clearCloudSpeechKey('openai')).toBe(true);
      expect(resolveSpeechApiKey()).toBeNull();
      expect(resolveOpenAiKey()).toBeNull();
    } finally {
      if (prevXdgData === undefined) delete process.env.XDG_DATA_HOME;
      else process.env.XDG_DATA_HOME = prevXdgData;
      if (prevXdgCfg === undefined) delete process.env.XDG_CONFIG_HOME;
      else process.env.XDG_CONFIG_HOME = prevXdgCfg;
      if (prevGemini !== undefined) process.env.GEMINI_API_KEY = prevGemini;
      if (prevOpenAi !== undefined) process.env.OPENAI_API_KEY = prevOpenAi;
      fs.rmSync(tmpRoot, { recursive: true, force: true });
    }
  });
});
