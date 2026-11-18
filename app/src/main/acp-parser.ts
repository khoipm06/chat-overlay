import type {
  HarnessKind,
  ModeOption,
  ModelOption,
  SessionCatalog,
  SlashCommandEntry,
  ThinkingEffortState,
  ThinkingOption,
} from '../shared/types';

export type ClassifiedAcpMessage =
  | { kind: 'request'; id: string | number; method: string; params: Record<string, unknown> }
  | { kind: 'notification'; method: string; params: Record<string, unknown> }
  | { kind: 'response'; id: string | number; result: unknown }
  | { kind: 'error_response'; id: string | number; message: string; code?: number };

export function classifyAcpLine(line: string): ClassifiedAcpMessage | null {
  const trimmed = line.trim();
  if (!trimmed) return null;
  let parsed: Record<string, unknown>;
  try {
    parsed = JSON.parse(trimmed) as Record<string, unknown>;
  } catch {
    return null;
  }
  if (!parsed || typeof parsed !== 'object') return null;

  const hasId = parsed.id !== undefined && parsed.id !== null;
  const id = parsed.id as string | number;
  const method = typeof parsed.method === 'string' ? parsed.method : null;
  const params =
    parsed.params && typeof parsed.params === 'object'
      ? (parsed.params as Record<string, unknown>)
      : {};

  if (hasId && method) {
    return { kind: 'request', id, method, params };
  }
  if (!hasId && method) {
    return { kind: 'notification', method, params };
  }
  if (hasId && !method) {
    if (parsed.error && typeof parsed.error === 'object') {
      const err = parsed.error as Record<string, unknown>;
      return {
        kind: 'error_response',
        id,
        message: typeof err.message === 'string' ? err.message : 'ACP JSON-RPC error',
        code: typeof err.code === 'number' ? err.code : undefined,
      };
    }
    return { kind: 'response', id, result: parsed.result ?? null };
  }
  return null;
}

const THINKING_KEYS = new Set([
  'thought_level',
  'thinking_level',
  'reasoning_effort',
  'effort',
  'thinking',
  'variant',
]);

export function isThinkingConfigOption(opt: Record<string, unknown>): boolean {
  const id = typeof opt.id === 'string' ? opt.id.toLowerCase() : '';
  const category = typeof opt.category === 'string' ? opt.category.toLowerCase() : '';
  if (THINKING_KEYS.has(id) || THINKING_KEYS.has(category)) return true;
  return (
    id.includes('thought') ||
    id.includes('thinking') ||
    id.includes('effort') ||
    category.includes('thought') ||
    category.includes('effort')
  );
}

export function normalizeEffortLabel(rawValue: string, rawName?: string): string {
  const trimmedName = rawName?.trim();
  if (trimmedName && trimmedName.toLowerCase() !== rawValue.toLowerCase()) {
    return trimmedName;
  }
  switch (rawValue.trim().toLowerCase()) {
    case 'none':
    case 'off':
    case 'disabled':
      return 'Off';
    case 'minimal':
      return 'Minimal';
    case 'low':
      return 'Low';
    case 'medium':
    case 'med':
    case 'balance':
      return 'Medium';
    case 'high':
      return 'High';
    case 'xhigh':
    case 'very_high':
    case 'extra_high':
      return 'XHigh';
    case 'max':
      return 'Max';
    case 'default':
    case 'auto':
      return rawValue.charAt(0).toUpperCase() + rawValue.slice(1).toLowerCase();
    default:
      return trimmedName || rawValue;
  }
}

function parseOptionPairs(rawOptions: unknown): Array<{ value: string; name: string; description?: string }> {
  if (!Array.isArray(rawOptions)) return [];
  const out: Array<{ value: string; name: string; description?: string }> = [];
  for (const item of rawOptions) {
    if (!item || typeof item !== 'object') continue;
    const obj = item as Record<string, unknown>;
    if (Array.isArray(obj.options)) {
      out.push(...parseOptionPairs(obj.options));
      continue;
    }
    const value =
      typeof obj.value === 'string'
        ? obj.value
        : typeof obj.id === 'string'
          ? obj.id
          : typeof obj.modelId === 'string'
            ? obj.modelId
            : null;
    if (!value) continue;
    const name =
      typeof obj.name === 'string'
        ? obj.name
        : typeof obj.displayName === 'string'
          ? obj.displayName
          : value;
    const description = typeof obj.description === 'string' ? obj.description : undefined;
    out.push({ value, name, description });
  }
  return out;
}

export function applyConfigOptionsToCatalog(
  catalog: SessionCatalog,
  rawConfigOptions: unknown,
): SessionCatalog {
  if (!Array.isArray(rawConfigOptions) || rawConfigOptions.length === 0) return catalog;

  let nextModels = catalog.models;
  let nextCurrentModel = catalog.currentModel;
  let nextModes = catalog.modes;
  let nextCurrentMode = catalog.currentMode;

  const hasFullSnapshotOrThinking = rawConfigOptions.some((entry) => {
    if (!entry || typeof entry !== 'object') return false;
    const opt = entry as Record<string, unknown>;
    const id = typeof opt.id === 'string' ? opt.id : '';
    const cat = typeof opt.category === 'string' ? opt.category : '';
    return id === 'model' || cat === 'model' || isThinkingConfigOption(opt);
  });

  // Reset thinking only when the update carries a model catalog refresh or thinking entry
  // so partial config_option_update notifications (e.g. unrelated toggle) keep current thinking state.
  let nextThinking: ThinkingEffortState = hasFullSnapshotOrThinking
    ? { configId: null, supported: false, currentValue: '', options: [] }
    : catalog.thinking;

  for (const entry of rawConfigOptions) {
    if (!entry || typeof entry !== 'object') continue;
    const opt = entry as Record<string, unknown>;
    const id = typeof opt.id === 'string' ? opt.id : '';
    const category = typeof opt.category === 'string' ? opt.category : '';
    const currentValue = typeof opt.currentValue === 'string' ? opt.currentValue : '';
    const pairs = parseOptionPairs(opt.options);

    if (id === 'model' || category === 'model') {
      if (pairs.length > 0) {
        nextModels = pairs.map((p): ModelOption => ({
          id: p.value,
          name: p.name,
          description: p.description,
        }));
      }
      if (currentValue) {
        nextCurrentModel = currentValue;
      }
    } else if (id === 'mode' || category === 'mode') {
      if (pairs.length > 0) {
        nextModes = pairs.map((p): ModeOption => ({
          id: p.value,
          name: p.name,
          description: p.description,
        }));
      }
      if (currentValue) {
        nextCurrentMode = currentValue;
      }
    } else if (isThinkingConfigOption(opt)) {
      const thinkingOptions: ThinkingOption[] = pairs.map((p) => ({
        value: p.value,
        label: normalizeEffortLabel(p.value, p.name),
      }));
      nextThinking = {
        configId: id || category,
        supported: thinkingOptions.length > 0,
        currentValue: currentValue || thinkingOptions[0]?.value || '',
        options: thinkingOptions,
      };
    }
  }

  return {
    ...catalog,
    models: nextModels,
    currentModel: nextCurrentModel,
    modes: nextModes,
    currentMode: nextCurrentMode,
    thinking: nextThinking,
  };
}

export function parseSessionNewCatalog(
  harness: HarnessKind,
  result: Record<string, unknown>,
  statusMessage = 'Ready',
): SessionCatalog {
  const sessionId = typeof result.sessionId === 'string' ? result.sessionId : null;

  let catalog: SessionCatalog = {
    harness,
    status: 'ready',
    statusMessage,
    sessionId,
    models: [],
    currentModel: typeof result.currentModelId === 'string' ? result.currentModelId : '',
    thinking: {
      configId: null,
      supported: false,
      currentValue: '',
      options: [],
    },
    modes: [],
    currentMode: typeof result.currentModeId === 'string' ? result.currentModeId : '',
    commands: [],
  };

  // Fallback legacy `models` & `modes` blocks first, then prefer `configOptions`
  // so harnesses like `agy acp` yield clean base models instead of duplicated level combos.
  if (result.models && typeof result.models === 'object') {
    const m = result.models as Record<string, unknown>;
    const avail = parseOptionPairs(m.availableModels);
    if (avail.length > 0) {
      catalog.models = avail.map((p) => ({
        id: p.value,
        name: p.name,
        description: p.description,
      }));
    }
    if (!catalog.currentModel && typeof m.currentModelId === 'string') {
      catalog.currentModel = m.currentModelId;
    }
  }

  if (result.modes && typeof result.modes === 'object') {
    const m = result.modes as Record<string, unknown>;
    const avail = parseOptionPairs(m.availableModes);
    if (avail.length > 0) {
      catalog.modes = avail.map((p) => ({
        id: p.value,
        name: p.name,
        description: p.description,
      }));
    }
    if (!catalog.currentMode && typeof m.currentModeId === 'string') {
      catalog.currentMode = m.currentModeId;
    }
  }

  if (Array.isArray(result.configOptions)) {
    catalog = applyConfigOptionsToCatalog(catalog, result.configOptions);
  }

  if (Array.isArray(result.availableCommands)) {
    catalog.commands = parseAvailableCommands(result.availableCommands);
  }

  return catalog;
}

export function parseAvailableCommands(raw: unknown): SlashCommandEntry[] {
  if (!Array.isArray(raw)) return [];
  const out: SlashCommandEntry[] = [];
  for (const item of raw) {
    if (!item || typeof item !== 'object') continue;
    const obj = item as Record<string, unknown>;
    const rawName = typeof obj.name === 'string' ? obj.name.trim() : '';
    if (!rawName) continue;
    const name = rawName.startsWith('/') ? rawName : `/${rawName}`;
    const description = typeof obj.description === 'string' ? obj.description : 'Agent command';
    out.push({ name, description, isLocal: false });
  }
  return out;
}

export function pickMarkupAnnotationsFromText(text: string): Array<Record<string, unknown>> {
  const results: Array<Record<string, unknown>> = [];
  const regex = /\[\[markup:\s*([^\]]+)\]\]/g;
  let match: RegExpExecArray | null;
  while ((match = regex.exec(text)) !== null) {
    const body = match[1].trim();
    const tokens = body.match(/(\w+=(?:"[^"]*"|'[^']*'|\S+)|^\w+)/g) ?? [];
    const kind = (body.split(/\s+/)[0] || '').toLowerCase();
    if (kind !== 'box' && kind !== 'arrow') continue;
    const record: Record<string, unknown> = { kind };
    for (const tok of tokens) {
      const eq = tok.indexOf('=');
      if (eq === -1) continue;
      const k = tok.slice(0, eq);
      let v = tok.slice(eq + 1);
      if ((v.startsWith('"') && v.endsWith('"')) || (v.startsWith("'") && v.endsWith("'"))) {
        v = v.slice(1, -1);
      }
      const num = Number(v);
      record[k] = k === 'label' || Number.isNaN(num) ? v : num;
    }
    results.push(record);
  }
  return results;
}
