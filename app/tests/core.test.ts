import { describe, expect, it } from 'vitest';
import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import {
  applyConfigOptionsToCatalog,
  classifyAcpLine,
  normalizeEffortLabel,
  parseSessionNewCatalog,
  pickMarkupAnnotationsFromText,
} from '../src/main/acp-parser';
import {
  captureWorkspaceCheckpoint,
  getGitGlance,
  restoreWorkspaceCheckpoint,
} from '../src/main/checkpoint';
import { expandSlashTemplate, parseLocalCommand } from '../src/main/commands';
import { extractActiveTrigger, matchMentionItems } from '../src/main/mentions';
import { presentToolCall } from '../src/renderer/tool-presenter';
import { HARNESS_LIST } from '../src/shared/types';

describe('HARNESS_LIST', () => {
  it('contains only pure ACP harnesses without pi_json', () => {
    const ids = HARNESS_LIST.map((h) => h.id);
    expect(ids).toEqual(['agy_acp', 'opencode_acp', 'pi_acp']);
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
