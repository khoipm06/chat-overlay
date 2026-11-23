import { spawn, type ChildProcessWithoutNullStreams } from 'node:child_process';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import readline from 'node:readline';
import { HARNESS_LIST } from '../shared/types';
import type {
  AppSettings,
  ChatAttachment,
  HarnessKind,
  MarkupAnnotation,
  PermissionOption,
  PermissionPolicy,
  SessionCatalog,
  StoredTurn,
  StreamEvent,
} from '../shared/types';
import {
  applyConfigOptionsToCatalog,
  classifyAcpLine,
  parseAvailableCommands,
  parseSessionNewCatalog,
  pickDesktopActionsFromText,
  pickMarkupAnnotationsFromText,
} from './acp-parser';
import { resolveHarnessLaunch } from './settings';

interface PendingRpc {
  resolve: (value: unknown) => void;
  reject: (err: Error) => void;
}

interface ToolTracker {
  name: string;
  startedAt: number;
}

const SCREEN_MARKUP_HINT =
  '\n\n[Screen context attached. To highlight steps or targets on the user\'s screen, emit inline tags like [[markup: box x=12 y=30 w=25 h=10 step=1 pulse=true label="Retry button"]] or [[markup: arrow x1=50 y1=50 x2=25 y2=30 step=2 label="Click here"]]. Coordinates are percentages of the screen (0-100), origin top-left.]';

const DESKTOP_CONTROL_HINT =
  '\n\n[Desktop Quick Actions & UI Control are available (user-confirmed). Only when the user asks to perform an OS setting or UI interaction, emit inline tags: [[desktop: os action=theme_dark|theme_light|snap_left|snap_right|snap_maximize|volume_mute|dnd_on|dnd_off|list_windows|list_apps|active_window|screenshot]], [[desktop: os action=volume_set value=50]], [[desktop: os action=open_app target="konsole"]], [[desktop: click x=45 y=20 label="Save"]], [[desktop: double_click x=45 y=20]], [[desktop: right_click x=45 y=20]], [[desktop: scroll x=50 y=50 delta=-3]], [[desktop: type text="hello"]], or [[desktop: key combo="ctrl+s"]]. Pointer x/y are 0-100 screen percentages.]';

function toEnvArray(env: unknown): Array<{ name: string; value: string }> {
  if (!env || typeof env !== 'object') return [];
  if (Array.isArray(env)) {
    return env
      .map((item) => {
        if (item && typeof item === 'object') {
          const o = item as Record<string, unknown>;
          return { name: String(o.name ?? ''), value: String(o.value ?? '') };
        }
        return { name: String(item), value: '' };
      })
      .filter((e) => e.name);
  }
  return Object.entries(env as Record<string, unknown>).map(([name, val]) => ({
    name,
    value: String(val ?? ''),
  }));
}

export async function loadMcpServersForSession(
  workspaceDir: string,
): Promise<Array<Record<string, unknown>>> {
  const servers: Array<Record<string, unknown>> = [];
  try {
    const wsMcp = path.join(workspaceDir, '.mcp.json');
    const raw = await fs.readFile(wsMcp, 'utf8');
    const parsed = JSON.parse(raw);
    const mcpServers = parsed?.mcpServers || parsed;
    if (mcpServers && typeof mcpServers === 'object') {
      for (const [name, cfg] of Object.entries(mcpServers)) {
        if (!cfg || typeof cfg !== 'object') continue;
        const c = cfg as Record<string, unknown>;
        if (typeof c.url === 'string') {
          servers.push({
            name,
            type: 'http',
            url: c.url,
            headers: toEnvArray(c.headers),
          });
        } else if (Array.isArray(c.command) && c.command.length > 0) {
          servers.push({
            name,
            command: String(c.command[0]),
            args: c.command.slice(1).map(String),
            env: toEnvArray(c.env || c.environment),
          });
        } else if (typeof c.command === 'string') {
          const args = Array.isArray(c.args) ? c.args.map(String) : [];
          servers.push({
            name,
            command: c.command,
            args,
            env: toEnvArray(c.env || c.environment),
          });
        }
      }
    }
  } catch {
    // ignore
  }

  if (servers.length === 0) {
    try {
      const opencodeCfg = path.join(
        process.env.XDG_CONFIG_HOME || path.join(os.homedir(), '.config'),
        'opencode',
        'opencode.json',
      );
      const raw = await fs.readFile(opencodeCfg, 'utf8');
      const parsed = JSON.parse(raw);
      const mcpObj = parsed?.mcp?.servers || parsed?.mcpServers;
      if (mcpObj && typeof mcpObj === 'object') {
        for (const [name, cfg] of Object.entries(mcpObj)) {
          if (!cfg || typeof cfg !== 'object') continue;
          const c = cfg as Record<string, unknown>;
          if (typeof c.url === 'string') {
            servers.push({
              name,
              type: 'http',
              url: c.url,
              headers: toEnvArray(c.headers),
            });
          } else if (Array.isArray(c.command) && c.command.length > 0) {
            servers.push({
              name,
              command: String(c.command[0]),
              args: c.command.slice(1).map(String),
              env: toEnvArray(c.env || c.environment),
            });
          } else if (typeof c.command === 'string') {
            const args = Array.isArray(c.args) ? c.args.map(String) : [];
            servers.push({
              name,
              command: c.command,
              args,
              env: toEnvArray(c.env || c.environment),
            });
          }
        }
      }
    } catch {
      // ignore
    }
  }
  return servers;
}

export class AcpAgentController {
  private proc: ChildProcessWithoutNullStreams | null = null;
  private rpcSeq = 1;
  private warmupSeq = 0;
  private pendingRpc = new Map<string, PendingRpc>();
  private pendingPermissions = new Map<string, { toolKey: string; allowAlwaysIds: Set<string> }>();
  private stderrTail: string[] = [];
  private activeTools = new Map<string, ToolTracker>();
  private pendingElicitations = new Map<string, unknown>();
  private currentContextSize: number | undefined;

  private sessionLoading = false;
  private replayedTurns: StoredTurn[] = [];
  private currentReplayedTurn: StoredTurn | null = null;

  private currentHarness: HarnessKind;
  private currentWorkspace: string;
  private permissionPolicy: PermissionPolicy;
  private sessionAllowedTools = new Set<string>();
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
    this.permissionPolicy =
      settings.permissionPolicy ?? (settings.autoApprovePermissions ? 'yolo' : 'auto_read');
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
    const gen = ++this.warmupSeq;
    this.currentHarness = settings.harness;
    this.currentWorkspace = settings.workspaceDir;
    this.permissionPolicy =
      settings.permissionPolicy ?? (settings.autoApprovePermissions ? 'yolo' : 'auto_read');
    this.turnActive = false;
    await this.shutdownProcess();
    if (gen !== this.warmupSeq) {
      return this.catalog;
    }

    const { binary, args, meta } = resolveHarnessLaunch(settings, settings.harness);
    const extraEnv = settings.harnessEnv?.[settings.harness] ?? {};

    this.catalog = this.emptyCatalog(
      settings.harness,
      'connecting',
      `Connecting to ${meta.label}...`,
    );
    this.emitCatalog();
    this.emitStatus('connecting', `Starting ${meta.shortLabel}...`);

    try {
      await this.spawnProcess(binary, args, settings.workspaceDir, extraEnv);
      if (gen !== this.warmupSeq) return this.catalog;

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
          elicitation: {
            form: {},
            url: {},
          },
          _meta: {
            'opencode/child-session-updates': true,
          },
        },
        clientInfo: {
          name: 'chat-overlay',
          version: '3.2.4',
        },
      }, 20_000);
      if (gen !== this.warmupSeq) return this.catalog;

      const mcpServers = await loadMcpServersForSession(settings.workspaceDir);
      const sessionRes = (await this.sendRpc('session/new', {
        cwd: settings.workspaceDir,
        mcpServers,
      }, 25_000)) as Record<string, unknown>;
      if (gen !== this.warmupSeq) return this.catalog;

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
        parsed = await this.applyModelRpc(parsed, desiredModel);
      }
      if (gen !== this.warmupSeq) return this.catalog;

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
      if (gen !== this.warmupSeq) return this.catalog;

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
      if (gen !== this.warmupSeq) return this.catalog;

      this.catalog = parsed;
      this.emitCatalog();
      this.emitStatus('idle', `Ready · ${meta.shortLabel}`);
      return this.catalog;
    } catch (err) {
      if (gen !== this.warmupSeq) {
        return this.catalog;
      }
      const msg = err instanceof Error ? err.message : String(err);
      const stderrNote =
        this.stderrTail.length > 0 ? `\n\nStderr:\n${this.stderrTail.slice(-6).join('\n')}` : '';
      const hint =
        settings.harness === 'pi_acp'
          ? 'Pi (ACP) failed handshake. Ensure `pi-acp` is installed and `pi` is authenticated (`pi auth`).'
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

  private async applyModelRpc(catalog: SessionCatalog, modelId: string): Promise<SessionCatalog> {
    if (!catalog.sessionId) return catalog;
    try {
      const updated = (await this.sendRpc('session/set_config_option', {
        sessionId: catalog.sessionId,
        configId: 'model',
        value: modelId,
      })) as Record<string, unknown>;
      if (Array.isArray(updated?.configOptions)) {
        return applyConfigOptionsToCatalog(catalog, updated.configOptions);
      }
      return { ...catalog, currentModel: modelId };
    } catch {
      // Fallback for harnesses exposing legacy `session/set_model` only
      try {
        await this.sendRpc('session/set_model', {
          sessionId: catalog.sessionId,
          modelId,
        });
        return { ...catalog, currentModel: modelId };
      } catch {
        return catalog;
      }
    }
  }

  async setModel(modelId: string, settings: AppSettings): Promise<SessionCatalog> {
    if (!this.catalog.sessionId || this.catalog.status !== 'ready') {
      return this.catalog;
    }
    if (!this.catalog.models.some((m) => m.id === modelId)) {
      return this.catalog;
    }

    if (!this.turnActive) {
      this.emitStatus('warming', `Switching model to ${modelId}...`);
    }
    try {
      this.catalog = await this.applyModelRpc(this.catalog, modelId);

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
      if (!this.turnActive) this.emitStatus('idle', 'Model updated');
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      this.onEvent({ type: 'error', message: `Failed to switch model: ${msg}` });
      if (!this.turnActive) this.emitStatus('idle', 'Ready');
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

    if (!this.turnActive) {
      this.emitStatus('warming', `Setting thinking effort to ${value}...`);
    }
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
      if (!this.turnActive) this.emitStatus('idle', 'Effort updated');
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      this.onEvent({ type: 'error', message: `Failed to set thinking effort: ${msg}` });
      if (!this.turnActive) this.emitStatus('idle', 'Ready');
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

  setPermissionPolicy(policy: PermissionPolicy): void {
    this.permissionPolicy = policy;
  }

  clearSessionAllowedTools(): void {
    this.sessionAllowedTools.clear();
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

    const blocks = await this.buildPromptBlocks(promptText, attachments, settings);
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
          thoughtTokens: typeof usage?.thoughtTokens === 'number' ? usage.thoughtTokens : undefined,
          cachedReadTokens:
            typeof usage?.cachedReadTokens === 'number' ? usage.cachedReadTokens : undefined,
          totalTokens: typeof usage?.totalTokens === 'number' ? usage.totalTokens : undefined,
          contextSize: this.currentContextSize,
          durationMs,
        });
      }

      const markupMatches = pickMarkupAnnotationsFromText(this.assistantTextBuffer);
      if (markupMatches.length > 0) {
        this.onMarkupAnnotations(markupMatches as unknown as MarkupAnnotation[]);
      }

      const desktopMatches = pickDesktopActionsFromText(this.assistantTextBuffer);
      if (desktopMatches.length > 0 && settings.desktopControlMode !== 'off') {
        this.onEvent({ type: 'desktop_actions', actions: desktopMatches });
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
    for (const [reqId] of [...this.pendingPermissions]) {
      this.respondPermission(reqId, null);
    }
    this.pendingPermissions.clear();
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
    const meta = this.pendingPermissions.get(requestId);
    this.pendingPermissions.delete(requestId);
    if (meta && optionId && meta.allowAlwaysIds.has(optionId) && meta.toolKey) {
      this.sessionAllowedTools.add(meta.toolKey);
    }
    const numId = Number(requestId);
    const id = Number.isNaN(numId) ? requestId : numId;
    if (optionId) {
      this.writeResponse(id, { outcome: { outcome: 'selected', optionId } });
    } else {
      this.writeResponse(id, { outcome: { outcome: 'cancelled' } });
    }
  }

  respondElicitation(
    requestId: string,
    action: 'accept' | 'decline' | 'cancel',
    content?: Record<string, unknown>,
  ): void {
    const rawId = this.pendingElicitations.get(requestId);
    this.pendingElicitations.delete(requestId);
    const numId = Number(requestId);
    const id = (rawId !== undefined ? rawId : Number.isNaN(numId) ? requestId : numId) as
      | string
      | number;
    if (action === 'accept') {
      this.writeResponse(id, { action: 'accept', content: content ?? {} });
    } else if (action === 'decline') {
      this.writeResponse(id, { action: 'decline' });
    } else {
      this.writeResponse(id, { action: 'cancel' });
    }
  }

  async listRemoteSessions(): Promise<Array<{ id: string; title: string; updatedAt: number }>> {
    if (!this.proc) return [];
    try {
      const res = (await this.sendRpc('session/list', {
        cwd: this.currentWorkspace,
      }, 5000)) as Record<string, unknown>;
      if (Array.isArray(res?.sessions)) {
        return res.sessions
          .map((s: Record<string, unknown>) => ({
            id: String(s.sessionId ?? s.id ?? ''),
            title: String(s.title ?? 'Session'),
            updatedAt:
              typeof s.updatedAt === 'string'
                ? new Date(s.updatedAt).getTime()
                : typeof s.updatedAt === 'number'
                  ? s.updatedAt
                  : Date.now(),
          }))
          .filter((s) => s.id);
      }
    } catch {
      // Harness does not support session/list
    }
    return [];
  }

  async loadRemoteSession(
    sessionId: string,
  ): Promise<{ ok: boolean; turns?: StoredTurn[]; error?: string }> {
    if (!this.proc) return { ok: false, error: 'No ACP process running' };
    try {
      this.sessionLoading = true;
      this.replayedTurns = [];
      this.currentReplayedTurn = null;
      this.catalog.sessionId = sessionId;
      const mcpServers = await loadMcpServersForSession(this.currentWorkspace);
      await this.sendRpc(
        'session/load',
        {
          sessionId,
          cwd: this.currentWorkspace,
          mcpServers,
        },
        25_000,
      );
      if (this.currentReplayedTurn) {
        this.replayedTurns.push(this.currentReplayedTurn);
        this.currentReplayedTurn = null;
      }
      return { ok: true, turns: this.replayedTurns };
    } catch (err) {
      return { ok: false, error: err instanceof Error ? err.message : String(err) };
    } finally {
      this.sessionLoading = false;
      this.currentReplayedTurn = null;
    }
  }

  async shutdownProcess(): Promise<void> {
    const proc = this.proc;
    this.proc = null;
    this.pendingPermissions.clear();
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

  isBusy(): boolean {
    return this.turnActive;
  }

  /**
   * Stops the idle ACP subprocess cleanly to reclaim RAM when the overlay stays hidden.
   * Returns true if a live session was put to sleep.
   */
  sleepIfIdle(): boolean {
    if (this.turnActive || (!this.proc && !this.catalog.sessionId)) {
      return false;
    }
    ++this.warmupSeq;
    void this.shutdownProcess();
    this.catalog = this.emptyCatalog(
      this.currentHarness,
      'idle',
      'Sleeping to save memory — will reconnect on open',
    );
    this.emitCatalog();
    return true;
  }

  dispose(): void {
    this.disposed = true;
    void this.shutdownProcess();
  }

  private async buildPromptBlocks(
    promptText: string,
    attachments: ChatAttachment[],
    settings: AppSettings,
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
    if (settings.desktopControlMode !== 'off') {
      finalText += DESKTOP_CONTROL_HINT;
    }
    blocks.push({ type: 'text', text: finalText });
    return blocks;
  }

  private async spawnProcess(
    binary: string,
    args: string[],
    cwd: string,
    extraEnv: Record<string, string> = {},
  ): Promise<void> {
    this.stderrTail = [];
    const child = spawn(binary, args, {
      cwd,
      stdio: ['pipe', 'pipe', 'pipe'],
      env: { ...process.env, ...extraEnv, NO_COLOR: '1' },
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
    child.stdin.on('error', () => {
      // Prevent unhandled EPIPE crash when child exits mid-write
    });
    child.on('error', () => {
      // Handled via exit & pendingRpc rejections
    });

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

  private sendRpc(
    method: string,
    params: Record<string, unknown>,
    timeoutMs?: number,
  ): Promise<unknown> {
    const proc = this.proc;
    if (!proc || !proc.stdin.writable) {
      return Promise.reject(new Error('ACP process is not running'));
    }
    const id = this.rpcSeq++;
    const key = String(id);
    const payload = JSON.stringify({ jsonrpc: '2.0', id, method, params });

    return new Promise((resolve, reject) => {
      let timer: NodeJS.Timeout | undefined;
      if (timeoutMs) {
        timer = setTimeout(() => {
          const waiter = this.pendingRpc.get(key);
          if (waiter) {
            this.pendingRpc.delete(key);
            reject(new Error(`${method} timed out after ${timeoutMs}ms`));
          }
        }, timeoutMs);
      }
      this.pendingRpc.set(key, {
        resolve: (val) => {
          if (timer) clearTimeout(timer);
          resolve(val);
        },
        reject: (err) => {
          if (timer) clearTimeout(timer);
          reject(err);
        },
      });
      proc.stdin.write(`${payload}\n`, (err) => {
        if (err) {
          if (timer) clearTimeout(timer);
          this.pendingRpc.delete(key);
          reject(err);
        }
      });
    });
  }

  private safeWriteLine(line: string): void {
    const stdin = this.proc?.stdin;
    if (!stdin || !stdin.writable) return;
    try {
      stdin.write(`${line}\n`, () => {});
    } catch {
      // Ignore write failures on closing pipe
    }
  }

  private writeNotification(method: string, params: Record<string, unknown>): void {
    this.safeWriteLine(JSON.stringify({ jsonrpc: '2.0', method, params }));
  }

  private writeResponse(id: string | number, result: unknown): void {
    this.safeWriteLine(JSON.stringify({ jsonrpc: '2.0', id, result }));
  }

  private writeErrorResponse(id: string | number, code: number, message: string): void {
    this.safeWriteLine(JSON.stringify({ jsonrpc: '2.0', id, error: { code, message } }));
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
      const toolKind = typeof toolCall.kind === 'string' ? toolCall.kind : undefined;
      const rawInputObj =
        toolCall.rawInput && typeof toolCall.rawInput === 'object'
          ? (toolCall.rawInput as Record<string, unknown>)
          : null;
      const toolKey = (toolKind || title).toLowerCase();

      const pickAllowOption = () =>
        options.find(
          (o) =>
            o.kind === 'allow_once' ||
            o.kind === 'allow_always' ||
            Boolean(o.kind && o.kind.toLowerCase().startsWith('allow')),
        );

      const isReadOnlyTool = /\b(read|view|cat|glob|grep|find|list|search|ls)\b/i.test(toolKey);
      const isWorkspaceEditTool = /\b(edit|write|patch|create)\b/i.test(toolKey);
      const targetPath =
        rawInputObj &&
        (typeof rawInputObj.path === 'string'
          ? rawInputObj.path
          : typeof rawInputObj.filePath === 'string'
            ? rawInputObj.filePath
            : typeof rawInputObj.file_path === 'string'
              ? rawInputObj.file_path
              : '');
      const insideWorkspace = targetPath ? Boolean(this.resolveWorkspacePath(targetPath)) : false;

      const autoPermitted =
        this.permissionPolicy === 'yolo' ||
        this.sessionAllowedTools.has(toolKey) ||
        (this.permissionPolicy === 'auto_read' && isReadOnlyTool) ||
        (this.permissionPolicy === 'auto_workspace' &&
          (isReadOnlyTool || (isWorkspaceEditTool && insideWorkspace)));

      if (autoPermitted && options.length > 0) {
        const allowOpt = pickAllowOption();
        if (allowOpt) {
          this.writeResponse(id, {
            outcome: { outcome: 'selected', optionId: allowOpt.optionId },
          });
          return;
        }
      }

      const detail =
        typeof toolCall.rawInput === 'string'
          ? toolCall.rawInput
          : toolCall.rawInput
            ? JSON.stringify(toolCall.rawInput, null, 2)
            : '';

      const reqKey = String(id);
      const allowAlwaysIds = new Set<string>(
        options
          .filter((o) => o.kind === 'allow_always' || o.kind === 'allowAlways')
          .map((o) => o.optionId),
      );
      this.pendingPermissions.set(reqKey, { toolKey, allowAlwaysIds });
      this.onEvent({
        type: 'permission_request',
        request: {
          requestId: reqKey,
          toolTitle: title,
          toolKind,
          detail,
          options,
        },
      });
      return;
    }

    if (method === 'fs/read_text_file') {
      const rawPath = typeof params.path === 'string' ? params.path : '';
      const resolved = this.resolveWorkspacePath(rawPath);
      if (!resolved) {
        this.writeErrorResponse(id, -32002, 'Access denied outside active workspace');
        return;
      }
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
      const resolved = this.resolveWorkspacePath(rawPath);
      if (!resolved) {
        this.writeErrorResponse(id, -32002, 'Access denied outside active workspace');
        return;
      }
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

    if (method === 'elicitation/create') {
      const reqKey = String(id);
      this.pendingElicitations.set(reqKey, id);
      const mode = (typeof params.mode === 'string' ? params.mode : 'form') as 'form' | 'url';
      const message =
        typeof params.message === 'string'
          ? params.message
          : 'The agent has a question for you:';
      const requestedSchema =
        params.requestedSchema && typeof params.requestedSchema === 'object'
          ? (params.requestedSchema as Record<string, unknown>)
          : null;
      const toolCallId =
        typeof params.toolCallId === 'string' ? params.toolCallId : undefined;

      this.onEvent({
        type: 'elicitation_request',
        request: {
          requestId: reqKey,
          mode,
          message,
          requestedSchema: requestedSchema as any,
          toolCallId,
        },
      });
      return;
    }

    this.writeErrorResponse(id, -32601, `Unsupported client method: ${method}`);
  }

  private handleNotification(method: string, params: Record<string, unknown>): void {
    if (method !== 'session/update') return;
    if (
      typeof params.sessionId === 'string' &&
      this.catalog.sessionId &&
      params.sessionId !== this.catalog.sessionId
    ) {
      return;
    }
    const update =
      params.update && typeof params.update === 'object'
        ? (params.update as Record<string, unknown>)
        : {};
    const kind = typeof update.sessionUpdate === 'string' ? update.sessionUpdate : '';

    switch (kind) {
      case 'user_message_chunk': {
        const text = extractChunkText(update.content);
        if (this.sessionLoading) {
          if (this.currentReplayedTurn) {
            this.replayedTurns.push(this.currentReplayedTurn);
          }
          this.currentReplayedTurn = {
            id: String(update.messageId || `turn_${Date.now()}`),
            userText: text,
            attachments: [],
            thinkingText: '',
            thinkingDurationMs: 0,
            assistantMarkdown: '',
            tools: [],
            segments: [],
          };
        }
        break;
      }
      case 'agent_thought_chunk': {
        const text = extractChunkText(update.content);
        const messageId = typeof update.messageId === 'string' ? update.messageId : undefined;
        if (this.sessionLoading) {
          if (!this.currentReplayedTurn) {
            this.currentReplayedTurn = {
              id: `turn_${Date.now()}`,
              userText: '',
              attachments: [],
              thinkingText: '',
              thinkingDurationMs: 0,
              assistantMarkdown: '',
              tools: [],
              segments: [],
            };
          }
          if (text) {
            this.currentReplayedTurn.thinkingText += (this.currentReplayedTurn.thinkingText ? '\n' : '') + text;
            const lastSeg = this.currentReplayedTurn.segments?.[this.currentReplayedTurn.segments.length - 1];
            if (lastSeg && lastSeg.kind === 'thought' && (!messageId || lastSeg.id === messageId)) {
              lastSeg.text += text;
            } else {
              this.currentReplayedTurn.segments?.push({
                kind: 'thought',
                id: messageId,
                text,
              });
            }
          }
          break;
        }
        if (!this.turnActive) break;
        if (text) {
          this.emitStatus('thinking', 'Thinking...');
          this.onEvent({ type: 'thinking_delta', text, messageId });
        }
        break;
      }
      case 'agent_message_chunk': {
        const text = extractChunkText(update.content);
        if (this.sessionLoading) {
          if (!this.currentReplayedTurn) {
            this.currentReplayedTurn = {
              id: `turn_${Date.now()}`,
              userText: '',
              attachments: [],
              thinkingText: '',
              thinkingDurationMs: 0,
              assistantMarkdown: '',
              tools: [],
              segments: [],
            };
          }
          if (text) {
            this.currentReplayedTurn.assistantMarkdown += text;
            const lastSeg = this.currentReplayedTurn.segments?.[this.currentReplayedTurn.segments.length - 1];
            if (lastSeg && lastSeg.kind === 'text') {
              lastSeg.text += text;
              lastSeg.markdown = lastSeg.text;
            } else {
              this.currentReplayedTurn.segments?.push({
                kind: 'text',
                text,
                markdown: text,
              });
            }
          }
          break;
        }
        if (!this.turnActive) break;
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
        const contentText = extractContentText(update.content);
        let initialOut: string;
        if (update.rawOutput && typeof update.rawOutput === 'object' && !Array.isArray(update.rawOutput)) {
          const rawObj = { ...(update.rawOutput as Record<string, unknown>) };
          if (contentText && !rawObj.output && !rawObj.stdout) {
            rawObj.output = contentText;
          }
          initialOut = JSON.stringify(rawObj);
        } else {
          initialOut = contentText || formatToolOutput(update.rawOutput ?? update.content);
        }
        if (this.sessionLoading) {
          if (!this.currentReplayedTurn) {
            this.currentReplayedTurn = {
              id: `turn_${Date.now()}`,
              userText: '',
              attachments: [],
              thinkingText: '',
              thinkingDurationMs: 0,
              assistantMarkdown: '',
              tools: [],
              segments: [],
            };
          }
          const tModel = {
            id,
            name,
            title: typeof update.title === 'string' ? update.title : undefined,
            args,
            status: 'running' as const,
            output: initialOut,
          };
          this.currentReplayedTurn.tools.push(tModel);
          this.currentReplayedTurn.segments?.push({
            kind: 'tool',
            id,
            toolId: id,
            name,
            title: tModel.title,
            args,
            status: 'running',
            output: initialOut,
          });
          break;
        }
        if (!this.turnActive) break;
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
        if (initialOut) {
          this.onEvent({
            type: 'tool_update',
            id,
            status: 'running',
            output: initialOut,
          });
        }
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
        const args = update.rawInput !== undefined ? formatToolInput(update.rawInput) : undefined;
        const contentText = extractContentText(update.content);
        let output: string;
        if (update.rawOutput && typeof update.rawOutput === 'object' && !Array.isArray(update.rawOutput)) {
          const rawObj = { ...(update.rawOutput as Record<string, unknown>) };
          if (contentText && !rawObj.output && !rawObj.stdout) {
            rawObj.output = contentText;
          }
          output = JSON.stringify(rawObj);
        } else {
          output = contentText || formatToolOutput(update.rawOutput ?? update.content);
        }
        if (this.sessionLoading && this.currentReplayedTurn) {
          const t = this.currentReplayedTurn.tools.find((x) => x.id === id);
          if (t) {
            t.status = status;
            if (output) t.output = output;
            if (args) t.args = args;
          }
          const s = this.currentReplayedTurn.segments?.find((x) => x.kind === 'tool' && x.id === id);
          if (s && s.kind === 'tool') {
            s.status = status;
            if (output) s.output = output;
            if (args) s.args = args;
          }
          break;
        }
        if (!this.turnActive) break;
        this.onEvent({
          type: 'tool_update',
          id,
          status,
          title: typeof update.title === 'string' ? update.title : undefined,
          args: args || undefined,
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
        if (this.turnActive && update.retry && typeof update.retry === 'object') {
          const retry = update.retry as Record<string, unknown>;
          const msg =
            typeof retry.message === 'string' ? retry.message : 'Retrying request...';
          this.emitStatus('retrying', msg);
        }
        break;
      }
      case 'usage_update': {
        const used = typeof update.used === 'number' ? update.used : undefined;
        const size = typeof update.size === 'number' ? update.size : undefined;
        if (size !== undefined) {
          this.currentContextSize = size;
        }
        if (used !== undefined && this.turnStartedAt > 0) {
          this.onEvent({
            type: 'metrics',
            totalTokens: used,
            contextSize: this.currentContextSize,
            durationMs: Date.now() - this.turnStartedAt,
          });
        }
        break;
      }
      default:
        break;
    }
  }

  private resolveWorkspacePath(rawPath: string): string | null {
    if (!rawPath) return null;
    const root = path.resolve(this.currentWorkspace);
    const resolved = path.isAbsolute(rawPath) ? path.resolve(rawPath) : path.resolve(root, rawPath);
    if (resolved === root || resolved.startsWith(root + path.sep)) {
      return resolved;
    }
    return null;
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

export async function testHarnessConnection(
  settings: AppSettings,
  harness: HarnessKind,
): Promise<{ ok: boolean; latencyMs: number; modelCount: number; message: string }> {
  const startedAt = Date.now();
  const { binary, args, viaRunner } = resolveHarnessLaunch(settings, harness);
  const extraEnv = settings.harnessEnv?.[harness] ?? {};

  return new Promise((resolve) => {
    const child = spawn(binary, args, {
      cwd: settings.workspaceDir,
      stdio: ['pipe', 'pipe', 'pipe'],
      env: { ...process.env, ...extraEnv, NO_COLOR: '1' },
    });
    let finished = false;
    const finish = (res: { ok: boolean; modelCount: number; message: string }) => {
      if (finished) return;
      finished = true;
      clearTimeout(timer);
      try {
        child.kill('SIGTERM');
      } catch {
        // ignore
      }
      resolve({ ...res, latencyMs: Date.now() - startedAt });
    };

    const timer = setTimeout(() => {
      finish({ ok: false, modelCount: 0, message: 'Timed out waiting for ACP session/new' });
    }, viaRunner ? 15_000 : 8_000);

    child.on('error', (err) => {
      finish({ ok: false, modelCount: 0, message: err.message });
    });

    const rl = readline.createInterface({ input: child.stdout, crlfDelay: Infinity });
    rl.on('line', (line) => {
      const msg = classifyAcpLine(line);
      if (!msg) return;
      if (msg.kind === 'response' && msg.id === 1) {
        child.stdin.write(
          `${JSON.stringify({
            jsonrpc: '2.0',
            id: 2,
            method: 'session/new',
            params: { cwd: settings.workspaceDir, mcpServers: [] },
          })}\n`,
        );
      } else if (msg.kind === 'response' && msg.id === 2) {
        const parsed = parseSessionNewCatalog(
          harness,
          (msg.result ?? {}) as Record<string, unknown>,
        );
        finish({
          ok: true,
          modelCount: parsed.models.length,
          message: `Handshake OK · ${parsed.models.length} live models`,
        });
      } else if (msg.kind === 'error_response') {
        finish({ ok: false, modelCount: 0, message: msg.message });
      }
    });

    child.on('spawn', () => {
      child.stdin.write(
        `${JSON.stringify({
          jsonrpc: '2.0',
          id: 1,
          method: 'initialize',
          params: { protocolVersion: 1, clientCapabilities: {} },
        })}\n`,
      );
    });
  });
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

function extractContentText(content: unknown): string {
  if (!content) return '';
  if (typeof content === 'string') return content;
  if (Array.isArray(content)) {
    return content
      .map((item) => {
        if (!item) return '';
        if (typeof item === 'string') return item;
        if (typeof item === 'object') {
          const obj = item as Record<string, unknown>;
          if (obj.content && typeof obj.content === 'object') {
            const inner = obj.content as Record<string, unknown>;
            if (typeof inner.text === 'string') return inner.text;
          }
          if (typeof obj.text === 'string') return obj.text;
          if (obj.type === 'text' && typeof obj.text === 'string') return obj.text;
        }
        return '';
      })
      .filter(Boolean)
      .join('\n');
  }
  if (typeof content === 'object') {
    const obj = content as Record<string, unknown>;
    if (typeof obj.text === 'string') return obj.text;
    if (obj.content && typeof obj.content === 'object') {
      const inner = obj.content as Record<string, unknown>;
      if (typeof inner.text === 'string') return inner.text;
    }
  }
  return '';
}

function formatToolOutput(raw: unknown): string {
  if (raw === undefined || raw === null) return '';
  if (typeof raw === 'string') return raw;
  if (Array.isArray(raw)) {
    return raw
      .map((item) => {
        if (typeof item === 'string') return item;
        if (item && typeof item === 'object') {
          const obj = item as Record<string, unknown>;
          if (obj.type === 'diff') {
            const p = typeof obj.path === 'string' ? obj.path : 'file';
            const oldT = typeof obj.oldText === 'string' ? obj.oldText : '';
            const newT = typeof obj.newText === 'string' ? obj.newText : '';
            const lines = [
              `Diff: ${p}`,
              ...(oldT ? oldT.split('\n').map((l) => `- ${l}`) : []),
              ...(newT ? newT.split('\n').map((l) => `+ ${l}`) : []),
            ];
            return lines.join('\n');
          }
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
