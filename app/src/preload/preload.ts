import { contextBridge, ipcRenderer, webUtils } from 'electron';
import type {
  AppSettings,
  ChatAttachment,
  DesktopAction,
  DesktopCapabilities,
  FilePreviewResult,
  GitDiffReport,
  GitGlance,
  HarnessDiagnostic,
  HarnessKind,
  HeightPreset,
  MarkupAnnotation,
  MentionItem,
  SavedSession,
  ScreenshotQuality,
  SessionCatalog,
  StoredTurn,
  StreamEvent,
  WidthPreset,
} from '../shared/types';
import type { CaptureKind } from '../main/capture';

export interface AgentOverlayApi {
  getSettings: () => Promise<AppSettings>;
  updateSettings: (patch: Partial<AppSettings>) => Promise<AppSettings>;
  inspectHarnesses: () => Promise<HarnessDiagnostic[]>;
  installHarness: (harness: HarnessKind) => Promise<{ ok: boolean; message: string }>;
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
  respondElicitation: (
    requestId: string,
    action: 'accept' | 'decline' | 'cancel',
    content?: Record<string, unknown>,
  ) => Promise<void>;
  loadRemoteSession: (sessionId: string) => Promise<{ ok: boolean; turns?: StoredTurn[]; error?: string }>;
  getMentions: (query: string) => Promise<MentionItem[]>;
  readClipboardContext: (preferSelection?: boolean) => Promise<ChatAttachment | null>;
  readFilePreview: (filePath: string) => Promise<FilePreviewResult>;
  pickFileToAttach: () => Promise<ChatAttachment | null>;
  copyImageToClipboard: (dataUrl: string) => Promise<boolean>;
  copyTextToClipboard: (text: string) => Promise<boolean>;
  getFilePath: (file: File) => string;
  captureScreen: (kind: CaptureKind) => Promise<ChatAttachment>;
  openMarkupCapture: () => Promise<ChatAttachment | null>;
  clearAgentMarkup: () => Promise<void>;
  listSessions: () => Promise<SavedSession[]>;
  saveSession: (session: SavedSession) => Promise<SavedSession[]>;
  deleteSession: (id: string) => Promise<SavedSession[]>;
  getGitGlance: (workspaceDir?: string) => Promise<GitGlance>;
  getGitDiffReport: (workspaceDir?: string, targetFile?: string) => Promise<GitDiffReport>;
  restoreGitFile: (
    filePath: string,
    workspaceDir?: string,
  ) => Promise<{ restored: boolean; message: string }>;
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
  applyWindowLayout: (wPreset?: WidthPreset, hPreset?: HeightPreset) => Promise<void>;
  toggleGhostMode: () => Promise<boolean>;
  flashAttention: () => Promise<void>;
  notifyBackground: (title: string, body: string) => Promise<void>;
  getDesktopCapabilities: () => Promise<DesktopCapabilities>;
  previewDesktopActions: (actions: DesktopAction[]) => Promise<void>;
  executeDesktopAction: (
    action: DesktopAction,
    isUserSlashCommand?: boolean,
  ) => Promise<{ ok: boolean; message: string }>;
  panicStopDesktop: () => Promise<void>;
  transcribeAudio: (audioData: {
    data: string;
    mimeType: string;
    language?: string;
  }) => Promise<{ ok: boolean; text?: string; error?: string }>;
  openExternal: (url: string) => Promise<void>;
  auditCapture: (filename: string) => Promise<void>;
  auditDone: () => Promise<void>;
  onStreamEvent: (cb: (event: StreamEvent) => void) => () => void;
  onFocusComposer: (cb: () => void) => () => void;
  onGhostModeChanged: (cb: (enabled: boolean) => void) => () => void;
  onClipboardCompanion: (cb: (att: ChatAttachment | null) => void) => () => void;
  onDesktopPanic: (cb: () => void) => () => void;
}

const api: AgentOverlayApi = {
  getSettings: () => ipcRenderer.invoke('app:get-settings'),
  updateSettings: (patch) => ipcRenderer.invoke('app:update-settings', patch),
  inspectHarnesses: () => ipcRenderer.invoke('settings:inspect-harnesses'),
  installHarness: (harness) => ipcRenderer.invoke('settings:install-harness', harness),
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
  respondElicitation: (requestId, action, content) =>
    ipcRenderer.invoke('acp:respond-elicitation', requestId, action, content),
  loadRemoteSession: (sessionId) => ipcRenderer.invoke('sessions:load-remote', sessionId),
  getMentions: (query) => ipcRenderer.invoke('mentions:query', query),
  readClipboardContext: (preferSelection) =>
    ipcRenderer.invoke('context:clipboard', preferSelection),
  readFilePreview: (filePath) => ipcRenderer.invoke('context:read-file-preview', filePath),
  pickFileToAttach: () => ipcRenderer.invoke('context:pick-file'),
  copyImageToClipboard: (dataUrl) => ipcRenderer.invoke('clipboard:write-image', dataUrl),
  copyTextToClipboard: (text) => ipcRenderer.invoke('clipboard:write-text', text),
  getFilePath: (file) => {
    try {
      return webUtils.getPathForFile(file);
    } catch {
      return '';
    }
  },
  captureScreen: (kind) => ipcRenderer.invoke('capture:screen', kind),
  openMarkupCapture: () => ipcRenderer.invoke('capture:markup'),
  clearAgentMarkup: () => ipcRenderer.invoke('markup:clear-agent'),
  listSessions: () => ipcRenderer.invoke('sessions:list'),
  saveSession: (session) => ipcRenderer.invoke('sessions:save', session),
  deleteSession: (id) => ipcRenderer.invoke('sessions:delete', id),
  getGitGlance: (workspaceDir) => ipcRenderer.invoke('git:glance', workspaceDir),
  getGitDiffReport: (workspaceDir, targetFile) =>
    ipcRenderer.invoke('git:diff-report', workspaceDir, targetFile),
  restoreGitFile: (filePath, workspaceDir) =>
    ipcRenderer.invoke('git:restore-file', filePath, workspaceDir),
  captureCheckpoint: (workspaceDir) => ipcRenderer.invoke('checkpoint:capture', workspaceDir),
  restoreCheckpoint: (treeSha, workspaceDir) =>
    ipcRenderer.invoke('checkpoint:restore', treeSha, workspaceDir),
  hideWindow: () => ipcRenderer.invoke('window:hide'),
  applyWindowLayout: (wPreset, hPreset) =>
    ipcRenderer.invoke('window:apply-layout', wPreset, hPreset),
  toggleGhostMode: () => ipcRenderer.invoke('window:toggle-ghost'),
  flashAttention: () => ipcRenderer.invoke('window:flash-attention'),
  notifyBackground: (title, body) => ipcRenderer.invoke('window:notify', title, body),
  getDesktopCapabilities: () => ipcRenderer.invoke('desktop:capabilities'),
  previewDesktopActions: (actions) => ipcRenderer.invoke('desktop:preview-actions', actions),
  executeDesktopAction: (action, isUserSlashCommand) =>
    ipcRenderer.invoke('desktop:execute-action', action, isUserSlashCommand),
  panicStopDesktop: () => ipcRenderer.invoke('desktop:panic-stop'),
  transcribeAudio: (audioData) => ipcRenderer.invoke('speech:transcribe', audioData),
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
  onClipboardCompanion: (cb) => {
    const handler = (_ev: Electron.IpcRendererEvent, att: ChatAttachment | null) => cb(att);
    ipcRenderer.on('context:clipboard-companion', handler);
    return () => ipcRenderer.off('context:clipboard-companion', handler);
  },
  onDesktopPanic: (cb) => {
    const handler = () => cb();
    ipcRenderer.on('desktop:panic', handler);
    return () => ipcRenderer.off('desktop:panic', handler);
  },
};

const markupBridge = {
  onInit: (
    cb: (payload: {
      mode: 'annotate' | 'region' | 'agent';
      screenshotDataUrl?: string;
      quality?: ScreenshotQuality;
      annotations?: MarkupAnnotation[];
    }) => void,
  ) => {
    const handler = (
      _ev: Electron.IpcRendererEvent,
      payload: {
        mode: 'annotate' | 'region' | 'agent';
        screenshotDataUrl?: string;
        quality?: ScreenshotQuality;
        annotations?: MarkupAnnotation[];
      },
    ) => cb(payload);
    ipcRenderer.on('markup:init', handler);
    return () => ipcRenderer.off('markup:init', handler);
  },
  submitImage: (dataUrl: string, mimeType?: string) =>
    ipcRenderer.send('markup:submit-image', { dataUrl, mimeType }),
  cancel: () => ipcRenderer.send('markup:cancel'),
  setClickThrough: (ignore: boolean) => ipcRenderer.send('markup:set-click-through', ignore),
};

export type MarkupBridgeApi = typeof markupBridge;

contextBridge.exposeInMainWorld('agentOverlay', api);
contextBridge.exposeInMainWorld('markupBridge', markupBridge);
