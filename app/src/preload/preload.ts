import { contextBridge, ipcRenderer } from 'electron';
import type {
  AppSettings,
  ChatAttachment,
  GitGlance,
  HarnessDiagnostic,
  HarnessKind,
  MarkupAnnotation,
  MentionItem,
  SavedSession,
  SessionCatalog,
  StreamEvent,
  WidthPreset,
} from '../shared/types';
import type { CaptureKind } from '../main/capture';

export interface AgentOverlayApi {
  getSettings: () => Promise<AppSettings>;
  updateSettings: (patch: Partial<AppSettings>) => Promise<AppSettings>;
  inspectHarnesses: () => Promise<HarnessDiagnostic[]>;
  testHarness: (
    harness: HarnessKind,
  ) => Promise<{ ok: boolean; latencyMs: number; modelCount: number; message: string }>;
  clearSessionAllowedTools: () => Promise<void>;
  pickWorkspaceFolder: () => Promise<string | null>;
  getCatalog: () => Promise<SessionCatalog>;
  switchHarness: (harness: HarnessKind) => Promise<SessionCatalog>;
  setModel: (modelId: string) => Promise<SessionCatalog>;
  setThinkingEffort: (value: string) => Promise<SessionCatalog>;
  setMode: (modeId: string) => Promise<SessionCatalog>;
  resetSession: () => Promise<SessionCatalog>;
  sendPrompt: (prompt: string, attachments: ChatAttachment[]) => Promise<void>;
  cancelTurn: () => Promise<void>;
  respondPermission: (requestId: string, optionId: string | null) => Promise<void>;
  getMentions: (query: string) => Promise<MentionItem[]>;
  readClipboardContext: () => Promise<ChatAttachment | null>;
  captureScreen: (kind: CaptureKind) => Promise<ChatAttachment>;
  openMarkupCapture: () => Promise<ChatAttachment | null>;
  clearAgentMarkup: () => Promise<void>;
  listSessions: () => Promise<SavedSession[]>;
  saveSession: (session: SavedSession) => Promise<SavedSession[]>;
  deleteSession: (id: string) => Promise<SavedSession[]>;
  getGitGlance: (workspaceDir?: string) => Promise<GitGlance>;
  captureCheckpoint: (workspaceDir?: string) => Promise<{
    id: string;
    isGit: boolean;
    workspaceDir: string;
    treeSha: string | null;
    headSha: string | null;
    error?: string;
  }>;
  restoreCheckpoint: (
    treeSha: string | null,
    workspaceDir?: string,
  ) => Promise<{ restored: boolean; message: string }>;
  hideWindow: () => Promise<void>;
  applyWindowLayout: (preset?: WidthPreset) => Promise<void>;
  toggleGhostMode: () => Promise<boolean>;
  flashAttention: () => Promise<void>;
  openExternal: (url: string) => Promise<void>;
  auditCapture: (filename: string) => Promise<void>;
  auditDone: () => Promise<void>;
  onStreamEvent: (cb: (event: StreamEvent) => void) => () => void;
  onFocusComposer: (cb: () => void) => () => void;
  onGhostModeChanged: (cb: (enabled: boolean) => void) => () => void;
}

const api: AgentOverlayApi = {
  getSettings: () => ipcRenderer.invoke('app:get-settings'),
  updateSettings: (patch) => ipcRenderer.invoke('app:update-settings', patch),
  inspectHarnesses: () => ipcRenderer.invoke('settings:inspect-harnesses'),
  testHarness: (harness) => ipcRenderer.invoke('acp:test-harness', harness),
  clearSessionAllowedTools: () => ipcRenderer.invoke('acp:clear-session-allowed'),
  pickWorkspaceFolder: () => ipcRenderer.invoke('app:pick-workspace'),
  getCatalog: () => ipcRenderer.invoke('acp:get-catalog'),
  switchHarness: (harness) => ipcRenderer.invoke('acp:switch-harness', harness),
  setModel: (modelId) => ipcRenderer.invoke('acp:set-model', modelId),
  setThinkingEffort: (value) => ipcRenderer.invoke('acp:set-effort', value),
  setMode: (modeId) => ipcRenderer.invoke('acp:set-mode', modeId),
  resetSession: () => ipcRenderer.invoke('acp:reset-session'),
  sendPrompt: (prompt, attachments) =>
    ipcRenderer.invoke('acp:send-prompt', prompt, attachments),
  cancelTurn: () => ipcRenderer.invoke('acp:cancel-turn'),
  respondPermission: (requestId, optionId) =>
    ipcRenderer.invoke('acp:respond-permission', requestId, optionId),
  getMentions: (query) => ipcRenderer.invoke('mentions:query', query),
  readClipboardContext: () => ipcRenderer.invoke('context:clipboard'),
  captureScreen: (kind) => ipcRenderer.invoke('capture:screen', kind),
  openMarkupCapture: () => ipcRenderer.invoke('capture:markup'),
  clearAgentMarkup: () => ipcRenderer.invoke('markup:clear-agent'),
  listSessions: () => ipcRenderer.invoke('sessions:list'),
  saveSession: (session) => ipcRenderer.invoke('sessions:save', session),
  deleteSession: (id) => ipcRenderer.invoke('sessions:delete', id),
  getGitGlance: (workspaceDir) => ipcRenderer.invoke('git:glance', workspaceDir),
  captureCheckpoint: (workspaceDir) => ipcRenderer.invoke('checkpoint:capture', workspaceDir),
  restoreCheckpoint: (treeSha, workspaceDir) =>
    ipcRenderer.invoke('checkpoint:restore', treeSha, workspaceDir),
  hideWindow: () => ipcRenderer.invoke('window:hide'),
  applyWindowLayout: (preset) => ipcRenderer.invoke('window:apply-layout', preset),
  toggleGhostMode: () => ipcRenderer.invoke('window:toggle-ghost'),
  flashAttention: () => ipcRenderer.invoke('window:flash-attention'),
  openExternal: (url) => ipcRenderer.invoke('app:open-external', url),
  auditCapture: (filename) => ipcRenderer.invoke('audit:capture', filename),
  auditDone: () => ipcRenderer.invoke('audit:done'),
  onStreamEvent: (cb) => {
    const handler = (_ev: Electron.IpcRendererEvent, event: StreamEvent) => cb(event);
    ipcRenderer.on('acp:stream-event', handler);
    return () => ipcRenderer.off('acp:stream-event', handler);
  },
  onFocusComposer: (cb) => {
    const handler = () => cb();
    ipcRenderer.on('window:focus-composer', handler);
    return () => ipcRenderer.off('window:focus-composer', handler);
  },
  onGhostModeChanged: (cb) => {
    const handler = (_ev: Electron.IpcRendererEvent, enabled: boolean) => cb(enabled);
    ipcRenderer.on('window:ghost-mode', handler);
    return () => ipcRenderer.off('window:ghost-mode', handler);
  },
};

const markupBridge = {
  onInit: (
    cb: (payload: {
      mode: 'annotate' | 'agent';
      screenshotDataUrl?: string;
      annotations?: MarkupAnnotation[];
    }) => void,
  ) => {
    const handler = (
      _ev: Electron.IpcRendererEvent,
      payload: {
        mode: 'annotate' | 'agent';
        screenshotDataUrl?: string;
        annotations?: MarkupAnnotation[];
      },
    ) => cb(payload);
    ipcRenderer.on('markup:init', handler);
    return () => ipcRenderer.off('markup:init', handler);
  },
  submitImage: (dataUrl: string) => ipcRenderer.send('markup:submit-image', dataUrl),
  cancel: () => ipcRenderer.send('markup:cancel'),
  setClickThrough: (ignore: boolean) => ipcRenderer.send('markup:set-click-through', ignore),
};

contextBridge.exposeInMainWorld('agentOverlay', api);
contextBridge.exposeInMainWorld('markupBridge', markupBridge);
