import { describe, expect, it } from 'vitest';
import {
  applyConfigOptionsToCatalog,
  classifyAcpLine,
  normalizeEffortLabel,
  parseSessionNewCatalog,
  pickMarkupAnnotationsFromText,
} from '../src/main/acp-parser';
import { expandSlashTemplate, parseLocalCommand } from '../src/main/commands';
import { extractActiveTrigger, matchMentionItems } from '../src/main/mentions';
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
