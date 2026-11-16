import { spawn, type ChildProcessWithoutNullStreams } from 'node:child_process';
import fs from 'node:fs/promises';
import path from 'node:path';
import readline from 'node:readline';
import { HARNESS_LIST } from '../shared/types';
import type {
  AppSettings,
  ChatAttachment,
  HarnessKind,
  MarkupAnnotation,
  PermissionOption,
  SessionCatalog,
  StreamEvent,
} from '../shared/types';
import {
  applyConfigOptionsToCatalog,
  classifyAcpLine,
  parseAvailableCommands,
  parseSessionNewCatalog,
  pickMarkupAnnotationsFromText,
} from './acp-parser';
import { resolveExecutable } from './settings';

interface PendingRpc {
  resolve: (value: unknown) => void;
  reject: (err: Error) => void;
}

interface ToolTracker {
  name: string;
  startedAt: number;
}

const SCREEN_MARKUP_HINT =
  '\n\n[Screen context attached. To point at something on the user\'s screen, emit an inline tag like [[markup: box x=12 y=30 w=25 h=10 label="Retry button"]] or [[markup: arrow x1=50 y1=50 x2=25 y2=30 label="here"]]. Coordinates are percentages of the screen (0-100), origin top-left.]';

export class AcpAgentController {
  private proc: ChildProcessWithoutNullStreams | null = null;
  private rpcSeq = 1;
  private pendingRpc = new Map<string, PendingRpc>();
  private stderrTail: string[] = [];
  private activeTools = new Map<string, ToolTracker>();

  private currentHarness: HarnessKind;
  private currentWorkspace: string;
  private autoApprove: boolean;
  private catalog: SessionCatalog;
  private disposed = false;

  private turnActive = false;
  private turnStartedAt = 0;
  private assistantTextBuffer = '';

  private onEvent: (ev: StreamEvent) => void;
  private onMarkupAnnotations: (annotations: MarkupAnnotation[]) => void;

  constructor(
    settings: AppSettings,
    onEvent: (ev: StreamEvent) => void,
    onMarkupAnnotations: (annotations: MarkupAnnotation[]) => void,
  ) {
    this.currentHarness = settings.harness;
    this.currentWorkspace = settings.workspaceDir;
    this.autoApprove = settings.autoApprovePermissions;
    this.onEvent = onEvent;
    this.onMarkupAnnotations = onMarkupAnnotations;
    this.catalog = this.emptyCatalog(this.currentHarness, 'idle', 'Waiting to connect');
  }

  private emptyCatalog(
    harness: HarnessKind,
    status: SessionCatalog['status'],
    statusMessage: string,
    errorDetail?: string,
  ): SessionCatalog {
    return {
      harness,
      status,
      statusMessage,
      sessionId: null,
      models: [],
      currentModel: '',
      thinking: { configId: null, supported: false, currentValue: '', options: [] },
      modes: [],
      currentMode: '',
      commands: [],
      errorDetail,
    };
  }

  getCatalog(): SessionCatalog {
    return this.catalog;
  }

  async warmup(settings: AppSettings): Promise<SessionCatalog> {
    this.currentHarness = settings.harness;
    this.currentWorkspace = settings.workspaceDir;
    this.autoApprove = settings.autoApprovePermissions;
    await this.shutdownProcess();

    const meta = HARNESS_LIST.find((h) => h.id === settings.harness) ?? HARNESS_LIST[0];
    const cmdCfg = settings.commands[settings.harness];
    const binary = resolveExecutable(cmdCfg.command || meta.defaultCommand);
    const args = cmdCfg.args.length > 0 ? cmdCfg.args : meta.defaultArgs;

    this.catalog = this.emptyCatalog(
      settings.harness,
      'connecting',
      `Connecting to ${meta.label}...`,
    );
    this.emitCatalog();
    this.emitStatus('connecting', `Starting ${meta.shortLabel}...`);

    try {
      await this.spawnProcess(binary, args, settings.workspaceDir);

      this.catalog = {
        ...this.catalog,
        status: 'warming',
        statusMessage: `Initializing ${meta.label} ACP session...`,
      };
      this.emitCatalog();
      this.emitStatus('warming', `Fetching live models & effort options from ${meta.label}...`);

      await this.sendRpc('initialize', {
        protocolVersion: 1,
        clientCapabilities: {
          fs: { readTextFile: true, writeTextFile: true },
          terminal: false,
        },
        clientInfo: {
          name: 'agent-overlay',
          version: '0.3.0',
        },
      });

      const sessionRes = (await this.sendRpc('session/new', {
        cwd: settings.workspaceDir,
        mcpServers: [],
      })) as Record<string, unknown>;

      let parsed = parseSessionNewCatalog(
        settings.harness,
        sessionRes,
        `Connected (${meta.shortLabel})`,
      );

      // Apply remembered per-harness model if it exists in the live catalog
      const desiredModel = settings.harnessModels[settings.harness];
      if (
        desiredModel &&
        desiredModel !== parsed.currentModel &&
        parsed.models.some((m) => m.id === desiredModel)
      ) {
        try {
          const updated = (await this.sendRpc('session/set_config_option', {
            sessionId: parsed.sessionId,
            configId: 'model',
            value: desiredModel,
          })) as Record<string, unknown>;
          if (Array.isArray(updated?.configOptions)) {
            parsed = applyConfigOptionsToCatalog(parsed, updated.configOptions);
          } else {
            parsed.currentModel = desiredModel;
          }
        } catch {
          // Keep live default if model set fails
        }
      }

      // Apply remembered mode if supported by this harness
      const desiredMode = settings.harnessModes[settings.harness];
      if (
        desiredMode &&
        desiredMode !== parsed.currentMode &&
        parsed.modes.some((m) => m.id === desiredMode)
      ) {
        try {
          await this.sendRpc('session/set_mode', {
            sessionId: parsed.sessionId,
            modeId: desiredMode,
          });
          parsed.currentMode = desiredMode;
        } catch {
          // Ignore unsupported mode
        }
      }

      // Apply remembered thinking effort only if this model exposes a compatible option
      const desiredEffort = settings.harnessEfforts[settings.harness];
      if (
        desiredEffort &&
        parsed.thinking.supported &&
        parsed.thinking.configId &&
        desiredEffort !== parsed.thinking.currentValue &&
        parsed.thinking.options.some((o) => o.value === desiredEffort)
      ) {
        try {
          const updated = (await this.sendRpc('session/set_config_option', {
            sessionId: parsed.sessionId,
            configId: parsed.thinking.configId,
            value: desiredEffort,
          })) as Record<string, unknown>;
          if (Array.isArray(updated?.configOptions)) {
            parsed = applyConfigOptionsToCatalog(parsed, updated.configOptions);
          } else {
            parsed.thinking.currentValue = desiredEffort;
          }
        } catch {
          // Keep harness default
        }
      }

      this.catalog = parsed;
      this.emitCatalog();
      this.emitStatus('idle', `Ready · ${meta.shortLabel}`);
      return this.catalog;
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      const stderrNote =
        this.stderrTail.length > 0 ? `\n\nStderr:\n${this.stderrTail.slice(-6).join('\n')}` : '';
      const hint =
        settings.harness === 'pi_acp'
          ? 'Pi (ACP) requires `pi acp` ACP server support (not yet installed/supported by the current `pi` binary).'
          : `Failed to start ${meta.label}: ${msg}`;
      this.catalog = this.emptyCatalog(
        settings.harness,
        'error',
        `${meta.shortLabel} unavailable`,
        `${hint}${stderrNote}`,
      );
      this.emitCatalog();
      this.emitStatus('idle', `${meta.shortLabel} unavailable`);
      await this.shutdownProcess();
      return this.catalog;
    }
  }

  async setModel(modelId: string, settings: AppSettings): Promise<SessionCatalog> {
    if (!this.catalog.sessionId || this.catalog.status !== 'ready') {
      return this.catalog;
    }
    if (!this.catalog.models.some((m) => m.id === modelId)) {
      return this.catalog;
    }

    this.emitStatus('warming', `Switching model to ${modelId}...`);
    try {
      const res = (await this.sendRpc('session/set_config_option', {
        sessionId: this.catalog.sessionId,
        configId: 'model',
        value: modelId,
      })) as Record<string, unknown>;

      if (Array.isArray(res?.configOptions)) {
        this.catalog = applyConfigOptionsToCatalog(this.catalog, res.configOptions);
      } else {
        this.catalog = { ...this.catalog, currentModel: modelId };
      }

      // If remembered effort fits the newly selected model and differs from default, apply it
      const rememberedEffort = settings.harnessEfforts[this.currentHarness];
      if (
        rememberedEffort &&
        this.catalog.thinking.supported &&
        this.catalog.thinking.configId &&
        rememberedEffort !== this.catalog.thinking.currentValue &&
        this.catalog.thinking.options.some((o) => o.value === rememberedEffort)
      ) {
        const effRes = (await this.sendRpc('session/set_config_option', {
          sessionId: this.catalog.sessionId,
          configId: this.catalog.thinking.configId,
          value: rememberedEffort,
        })) as Record<string, unknown>;
        if (Array.isArray(effRes?.configOptions)) {
          this.catalog = applyConfigOptionsToCatalog(this.catalog, effRes.configOptions);
        } else {
          this.catalog.thinking.currentValue = rememberedEffort;
        }
      }

      this.emitCatalog();
      this.emitStatus('idle', 'Model updated');
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      this.onEvent({ type: 'error', message: `Failed to switch model: ${msg}` });
      this.emitStatus('idle', 'Ready');
    }
    return this.catalog;
  }

  async setThinkingEffort(value: string): Promise<SessionCatalog> {
    if (
      !this.catalog.sessionId ||
      !this.catalog.thinking.supported ||
      !this.catalog.thinking.configId
    ) {
      return this.catalog;
    }
    if (!this.catalog.thinking.options.some((o) => o.value === value)) {
      return this.catalog;
    }

    this.emitStatus('warming', `Setting thinking effort to ${value}...`);
    try {
      const res = (await this.sendRpc('session/set_config_option', {
        sessionId: this.catalog.sessionId,
        configId: this.catalog.thinking.configId,
        value,
      })) as Record<string, unknown>;

      if (Array.isArray(res?.configOptions)) {
        this.catalog = applyConfigOptionsToCatalog(this.catalog, res.configOptions);
      } else {
        this.catalog = {
          ...this.catalog,
          thinking: { ...this.catalog.thinking, currentValue: value },
        };
      }
      this.emitCatalog();
      this.emitStatus('idle', 'Effort updated');
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      this.onEvent({ type: 'error', message: `Failed to set thinking effort: ${msg}` });
      this.emitStatus('idle', 'Ready');
    }
    return this.catalog;
  }

  async setMode(modeId: string): Promise<SessionCatalog> {
    if (!this.catalog.sessionId) return this.catalog;
    if (!this.catalog.modes.some((m) => m.id === modeId)) return this.catalog;

    try {
      await this.sendRpc('session/set_mode', {
        sessionId: this.catalog.sessionId,
        modeId,
      });
      this.catalog = { ...this.catalog, currentMode: modeId };
      this.emitCatalog();
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      this.onEvent({ type: 'error', message: `Failed to set mode: ${msg}` });
    }
    return this.catalog;
  }

  setAutoApprove(enabled: boolean): void {
    this.autoApprove = enabled;
  }

  async sendPrompt(
    promptText: string,
    attachments: ChatAttachment[],
    settings: AppSettings,
  ): Promise<void> {
    if (!this.catalog.sessionId || this.catalog.status !== 'ready') {
      await this.warmup(settings);
    }
    if (!this.catalog.sessionId) {
      this.onEvent({
        type: 'error',
        message:
          this.catalog.errorDetail ||
          'Agent session is not ready. Check the harness binary and settings.',
      });
      this.onEvent({ type: 'done', stopReason: 'error' });
      return;
    }

    const blocks = await this.buildPromptBlocks(promptText, attachments);
    this.turnActive = true;
    this.turnStartedAt = Date.now();
    this.assistantTextBuffer = '';
    this.activeTools.clear();
    this.emitStatus('thinking', 'Thinking...');

    try {
      const res = (await this.sendRpc('session/prompt', {
        sessionId: this.catalog.sessionId,
        prompt: blocks,
      })) as Record<string, unknown> | null;

      const durationMs = Date.now() - this.turnStartedAt;
      const usage =
        res?.usage && typeof res.usage === 'object'
          ? (res.usage as Record<string, unknown>)
          : null;
      if (usage || durationMs > 0) {
        this.onEvent({
          type: 'metrics',
          inputTokens: typeof usage?.inputTokens === 'number' ? usage.inputTokens : undefined,
          outputTokens: typeof usage?.outputTokens === 'number' ? usage.outputTokens : undefined,
          totalTokens: typeof usage?.totalTokens === 'number' ? usage.totalTokens : undefined,
          durationMs,
        });
      }

      const markupMatches = pickMarkupAnnotationsFromText(this.assistantTextBuffer);
      if (markupMatches.length > 0) {
        this.onMarkupAnnotations(markupMatches as unknown as MarkupAnnotation[]);
      }

      const stopReason = typeof res?.stopReason === 'string' ? res.stopReason : 'end_turn';
      this.turnActive = false;
      this.emitStatus('idle', 'Ready');
      this.onEvent({ type: 'done', stopReason });
    } catch (err) {
      this.turnActive = false;
      const msg = err instanceof Error ? err.message : String(err);
      this.emitStatus('idle', 'Ready');
      this.onEvent({ type: 'error', message: msg });
      this.onEvent({ type: 'done', stopReason: 'error' });
    }
  }

  async cancelTurn(): Promise<void> {
    if (!this.catalog.sessionId) return;
    try {
      this.writeNotification('session/cancel', {
        sessionId: this.catalog.sessionId,
      });
      this.emitStatus('idle', 'Cancelled');
    } catch {
      // Ignore write failures during cancel
    }
  }

  respondPermission(requestId: string, optionId: string | null): void {
    const numId = Number(requestId);
    const id = Number.isNaN(numId) ? requestId : numId;
    if (optionId) {
      this.writeResponse(id, { outcome: { outcome: 'selected', optionId } });
    } else {
      this.writeResponse(id, { outcome: { outcome: 'cancelled' } });
    }
  }

  async shutdownProcess(): Promise<void> {
    const proc = this.proc;
    this.proc = null;
    for (const [, waiter] of this.pendingRpc) {
      waiter.reject(new Error('ACP session reset'));
    }
    this.pendingRpc.clear();
    if (!proc) return;
    try {
      proc.kill('SIGTERM');
    } catch {
      // Ignore already exited
    }
  }

  dispose(): void {
    this.disposed = true;
    void this.shutdownProcess();
  }

  private async buildPromptBlocks(
    promptText: string,
    attachments: ChatAttachment[],
  ): Promise<Array<Record<string, unknown>>> {
    const blocks: Array<Record<string, unknown>> = [];
    let hasImage = false;

    for (const att of attachments) {
      if (att.kind === 'image') {
        let base64 = '';
        if (att.dataUrl) {
          const comma = att.dataUrl.indexOf(',');
          base64 = comma >= 0 ? att.dataUrl.slice(comma + 1) : att.dataUrl;
        } else if (att.path) {
          try {
            const buf = await fs.readFile(att.path);
            base64 = buf.toString('base64');
          } catch {
            // Skip unreadable image
          }
        }
        if (base64) {
          hasImage = true;
          blocks.push({
            type: 'image',
            mimeType: att.mimeType || 'image/png',
            data: base64,
          });
        }
      } else if (att.kind === 'file' && att.path) {
        const abs = path.isAbsolute(att.path)
          ? att.path
          : path.resolve(this.currentWorkspace, att.path);
        blocks.push({
          type: 'resource_link',
          uri: `file://${abs}`,
          name: att.name || path.basename(abs),
        });
      } else if (att.content && att.content.trim()) {
        blocks.push({
          type: 'text',
          text: `[Attached Context: ${att.name}]\n\`\`\`\n${att.content}\n\`\`\`\n`,
        });
      }
    }

    let finalText = promptText;
    if (hasImage) {
      finalText = finalText.replace(/@screen\b/g, '[attached screenshot]');
      finalText += SCREEN_MARKUP_HINT;
    }
    blocks.push({ type: 'text', text: finalText });
    return blocks;
  }

  private async spawnProcess(binary: string, args: string[], cwd: string): Promise<void> {
    this.stderrTail = [];
    const child = spawn(binary, args, {
      cwd,
      stdio: ['pipe', 'pipe', 'pipe'],
      env: { ...process.env, NO_COLOR: '1' },
    });

    await new Promise<void>((resolve, reject) => {
      const onError = (err: Error) => {
        child.off('spawn', onSpawn);
        reject(err);
      };
      const onSpawn = () => {
        child.off('error', onError);
        resolve();
      };
      child.once('error', onError);
      child.once('spawn', onSpawn);
    });

    this.proc = child;

    const rl = readline.createInterface({ input: child.stdout, crlfDelay: Infinity });
    rl.on('line', (line) => {
      if (this.proc !== child) return;
      this.handleStdoutLine(line);
    });

    const errRl = readline.createInterface({ input: child.stderr, crlfDelay: Infinity });
    errRl.on('line', (line) => {
      if (this.proc !== child) return;
      const trimmed = line.trim();
      if (!trimmed) return;
      this.stderrTail.push(trimmed);
      if (this.stderrTail.length > 30) this.stderrTail.shift();
    });

    child.on('exit', (code, signal) => {
      if (this.proc !== child) return;
      this.proc = null;
      for (const [, waiter] of this.pendingRpc) {
        waiter.reject(
          new Error(`ACP process exited (${code !== null ? `code ${code}` : `signal ${signal}`})`),
        );
      }
      this.pendingRpc.clear();
      if (!this.disposed && this.turnActive) {
        this.turnActive = false;
        this.onEvent({
          type: 'error',
          message: `Agent process exited unexpectedly (${code ?? signal}).`,
        });
        this.onEvent({ type: 'done', stopReason: 'error' });
      }
    });
  }

  private sendRpc(method: string, params: Record<string, unknown>): Promise<unknown> {
    const proc = this.proc;
    if (!proc || !proc.stdin.writable) {
      return Promise.reject(new Error('ACP process is not running'));
    }
    const id = this.rpcSeq++;
    const key = String(id);
    const payload = JSON.stringify({ jsonrpc: '2.0', id, method, params });

    return new Promise((resolve, reject) => {
      this.pendingRpc.set(key, { resolve, reject });
      proc.stdin.write(`${payload}\n`, (err) => {
        if (err) {
          this.pendingRpc.delete(key);
          reject(err);
        }
      });
    });
  }

  private writeNotification(method: string, params: Record<string, unknown>): void {
    if (!this.proc?.stdin.writable) return;
    this.proc.stdin.write(`${JSON.stringify({ jsonrpc: '2.0', method, params })}\n`);
  }

  private writeResponse(id: string | number, result: unknown): void {
    if (!this.proc?.stdin.writable) return;
    this.proc.stdin.write(`${JSON.stringify({ jsonrpc: '2.0', id, result })}\n`);
  }

  private writeErrorResponse(id: string | number, code: number, message: string): void {
    if (!this.proc?.stdin.writable) return;
    this.proc.stdin.write(
      `${JSON.stringify({ jsonrpc: '2.0', id, error: { code, message } })}\n`,
    );
  }

  private handleStdoutLine(line: string): void {
    const msg = classifyAcpLine(line);
    if (!msg) return;

    if (msg.kind === 'response') {
      const key = String(msg.id);
      const waiter = this.pendingRpc.get(key);
      if (waiter) {
        this.pendingRpc.delete(key);
        waiter.resolve(msg.result);
      }
      return;
    }

    if (msg.kind === 'error_response') {
      const key = String(msg.id);
      const waiter = this.pendingRpc.get(key);
      if (waiter) {
        this.pendingRpc.delete(key);
        waiter.reject(new Error(msg.message));
      }
      return;
    }

    if (msg.kind === 'request') {
      void this.handleServerRequest(msg.id, msg.method, msg.params);
      return;
    }

    if (msg.kind === 'notification') {
      this.handleNotification(msg.method, msg.params);
    }
  }

  private async handleServerRequest(
    id: string | number,
    method: string,
    params: Record<string, unknown>,
  ): Promise<void> {
    if (method === 'session/request_permission') {
      const rawOptions = Array.isArray(params.options) ? params.options : [];
      const options: PermissionOption[] = rawOptions
        .filter((o): o is Record<string, unknown> => Boolean(o && typeof o === 'object'))
        .map((o) => ({
          optionId: String(o.optionId ?? o.id ?? ''),
          name: String(o.name ?? o.label ?? 'Allow'),
          kind: typeof o.kind === 'string' ? o.kind : undefined,
        }))
        .filter((o) => o.optionId);

      if (this.autoApprove && options.length > 0) {
        const allowOpt =
          options.find((o) => o.kind === 'allow_once' || o.kind === 'allow_always') ?? options[0];
        this.writeResponse(id, {
          outcome: { outcome: 'selected', optionId: allowOpt.optionId },
        });
        return;
      }

      const toolCall =
        params.toolCall && typeof params.toolCall === 'object'
          ? (params.toolCall as Record<string, unknown>)
          : {};
      const title =
        typeof toolCall.title === 'string'
          ? toolCall.title
          : typeof toolCall.name === 'string'
            ? toolCall.name
            : 'Tool execution approval';
      const detail =
        typeof toolCall.rawInput === 'string'
          ? toolCall.rawInput
          : toolCall.rawInput
            ? JSON.stringify(toolCall.rawInput, null, 2)
            : '';

      this.onEvent({
        type: 'permission_request',
        request: {
          requestId: String(id),
          toolTitle: title,
          toolKind: typeof toolCall.kind === 'string' ? toolCall.kind : undefined,
          detail,
          options,
        },
      });
      return;
    }

    if (method === 'fs/read_text_file') {
      const rawPath = typeof params.path === 'string' ? params.path : '';
      const resolved = path.isAbsolute(rawPath)
        ? rawPath
        : path.resolve(this.currentWorkspace, rawPath);
      try {
        const text = await fs.readFile(resolved, 'utf8');
        const lineOffset = typeof params.line === 'number' ? Math.max(0, params.line - 1) : 0;
        const limit = typeof params.limit === 'number' ? params.limit : 0;
        if (lineOffset > 0 || limit > 0) {
          const lines = text.split('\n');
          const sliced =
            limit > 0 ? lines.slice(lineOffset, lineOffset + limit) : lines.slice(lineOffset);
          this.writeResponse(id, { content: sliced.join('\n') });
        } else {
          this.writeResponse(id, { content: text });
        }
      } catch (err) {
        this.writeErrorResponse(
          id,
          -32002,
          err instanceof Error ? err.message : 'Unable to read file',
        );
      }
      return;
    }

    if (method === 'fs/write_text_file') {
      const rawPath = typeof params.path === 'string' ? params.path : '';
      const content = typeof params.content === 'string' ? params.content : '';
      const resolved = path.isAbsolute(rawPath)
        ? rawPath
        : path.resolve(this.currentWorkspace, rawPath);
      try {
        await fs.mkdir(path.dirname(resolved), { recursive: true });
        await fs.writeFile(resolved, content, 'utf8');
        this.writeResponse(id, {});
      } catch (err) {
        this.writeErrorResponse(
          id,
          -32002,
          err instanceof Error ? err.message : 'Unable to write file',
        );
      }
      return;
    }

    this.writeErrorResponse(id, -32601, `Unsupported client method: ${method}`);
  }

  private handleNotification(method: string, params: Record<string, unknown>): void {
    if (method !== 'session/update') return;
    const update =
      params.update && typeof params.update === 'object'
        ? (params.update as Record<string, unknown>)
        : {};
    const kind = typeof update.sessionUpdate === 'string' ? update.sessionUpdate : '';

    switch (kind) {
      case 'agent_thought_chunk': {
        const text = extractChunkText(update.content);
        if (text) {
          this.emitStatus('thinking', 'Thinking...');
          this.onEvent({ type: 'thinking_delta', text });
        }
        break;
      }
      case 'agent_message_chunk': {
        const text = extractChunkText(update.content);
        if (text) {
          this.assistantTextBuffer += text;
          this.emitStatus('streaming', 'Writing response...');
          this.onEvent({ type: 'text_delta', text });
        }
        break;
      }
      case 'tool_call': {
        const id = String(update.toolCallId ?? `tool_${Date.now()}`);
        const name = String(update.title ?? update.name ?? update.kind ?? 'tool');
        const args = formatToolInput(update.rawInput);
        this.activeTools.set(id, { name, startedAt: Date.now() });
        this.emitStatus('tool', `Running ${name}...`);
        this.onEvent({
          type: 'tool_start',
          id,
          name,
          title: typeof update.title === 'string' ? update.title : undefined,
          kind: typeof update.kind === 'string' ? update.kind : undefined,
          args,
        });
        break;
      }
      case 'tool_call_update': {
        const id = String(update.toolCallId ?? '');
        if (!id) break;
        const rawStatus = typeof update.status === 'string' ? update.status : 'in_progress';
        const status: 'running' | 'completed' | 'failed' =
          rawStatus === 'completed' || rawStatus === 'done'
            ? 'completed'
            : rawStatus === 'failed' || rawStatus === 'error'
              ? 'failed'
              : 'running';
        const tracker = this.activeTools.get(id);
        const durationMs = tracker ? Date.now() - tracker.startedAt : undefined;
        const output = formatToolOutput(update.rawOutput ?? update.content);
        this.onEvent({
          type: 'tool_update',
          id,
          status,
          title: typeof update.title === 'string' ? update.title : undefined,
          output: output || undefined,
          durationMs,
        });
        if (status !== 'running') {
          this.activeTools.delete(id);
          if (this.turnActive) {
            this.emitStatus('thinking', 'Processing tool result...');
          }
        }
        break;
      }
      case 'available_commands_update': {
        const cmds = parseAvailableCommands(update.availableCommands);
        this.catalog = { ...this.catalog, commands: cmds };
        this.emitCatalog();
        break;
      }
      case 'current_mode_update': {
        const modeId = typeof update.currentModeId === 'string' ? update.currentModeId : '';
        if (modeId) {
          this.catalog = { ...this.catalog, currentMode: modeId };
          this.emitCatalog();
        }
        break;
      }
      case 'config_option_update': {
        this.catalog = applyConfigOptionsToCatalog(this.catalog, update.configOptions);
        this.emitCatalog();
        break;
      }
      case 'session_info_update': {
        if (typeof update.title === 'string' && update.title.trim()) {
          this.onEvent({ type: 'session_title', title: update.title.trim() });
        }
        if (update.retry && typeof update.retry === 'object') {
          const retry = update.retry as Record<string, unknown>;
          const msg =
            typeof retry.message === 'string' ? retry.message : 'Retrying request...';
          this.emitStatus('retrying', msg);
        }
        break;
      }
      case 'usage_update': {
        const used = typeof update.used === 'number' ? update.used : undefined;
        if (used !== undefined && this.turnStartedAt > 0) {
          this.onEvent({
            type: 'metrics',
            totalTokens: used,
            durationMs: Date.now() - this.turnStartedAt,
          });
        }
        break;
      }
      default:
        break;
    }
  }

  private emitCatalog(): void {
    this.onEvent({ type: 'catalog', catalog: this.catalog });
  }

  private emitStatus(
    phase: 'connecting' | 'warming' | 'thinking' | 'tool' | 'streaming' | 'retrying' | 'idle',
    label: string,
  ): void {
    this.onEvent({ type: 'status', phase, label });
  }
}

function extractChunkText(content: unknown): string {
  if (!content || typeof content !== 'object') return '';
  const obj = content as Record<string, unknown>;
  if (typeof obj.text === 'string') return obj.text;
  return '';
}

function formatToolInput(raw: unknown): string {
  if (raw === undefined || raw === null) return '';
  if (typeof raw === 'string') return raw;
  try {
    return JSON.stringify(raw, null, 2);
  } catch {
    return String(raw);
  }
}

function formatToolOutput(raw: unknown): string {
  if (raw === undefined || raw === null) return '';
  if (typeof raw === 'string') return raw;
  if (Array.isArray(raw)) {
    return raw
      .map((item) => {
        if (item && typeof item === 'object') {
          const obj = item as Record<string, unknown>;
          if (obj.content && typeof obj.content === 'object') {
            const inner = obj.content as Record<string, unknown>;
            if (typeof inner.text === 'string') return inner.text;
          }
          if (typeof obj.text === 'string') return obj.text;
        }
        return JSON.stringify(item);
      })
      .join('\n');
  }
  try {
    return JSON.stringify(raw, null, 2);
  } catch {
    return String(raw);
  }
}
