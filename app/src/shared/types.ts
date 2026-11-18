export type HarnessKind = 'agy_acp' | 'opencode_acp' | 'pi_acp';

export interface HarnessInfo {
  id: HarnessKind;
  label: string;
  shortLabel: string;
  defaultCommand: string;
  defaultArgs: string[];
}

export const HARNESS_LIST: readonly HarnessInfo[] = [
  {
    id: 'agy_acp',
    label: 'Antigravity (ACP)',
    shortLabel: 'agy acp',
    defaultCommand: 'agy',
    defaultArgs: ['acp'],
  },
  {
    id: 'opencode_acp',
    label: 'OpenCode (ACP)',
    shortLabel: 'opencode acp',
    defaultCommand: 'opencode',
    defaultArgs: ['acp'],
  },
  {
    id: 'pi_acp',
    label: 'Pi (ACP)',
    shortLabel: 'pi acp',
    defaultCommand: 'pi',
    defaultArgs: ['acp'],
  },
] as const;

export interface ModelOption {
  id: string;
  name: string;
  description?: string;
}

export interface ThinkingOption {
  value: string;
  label: string;
}

export interface ThinkingEffortState {
  configId: string | null;
  supported: boolean;
  currentValue: string;
  options: ThinkingOption[];
}

export interface ModeOption {
  id: string;
  name: string;
  description?: string;
}

export interface SlashCommandEntry {
  name: string;
  description: string;
  isLocal?: boolean;
  template?: string;
}

export type CatalogStatus = 'idle' | 'connecting' | 'warming' | 'ready' | 'error';

export interface SessionCatalog {
  harness: HarnessKind;
  status: CatalogStatus;
  statusMessage: string;
  sessionId: string | null;
  models: ModelOption[];
  currentModel: string;
  thinking: ThinkingEffortState;
  modes: ModeOption[];
  currentMode: string;
  commands: SlashCommandEntry[];
  errorDetail?: string;
}

export interface PermissionOption {
  optionId: string;
  name: string;
  kind?: string;
}

export interface PermissionRequest {
  requestId: string;
  toolTitle: string;
  toolKind?: string;
  detail: string;
  options: PermissionOption[];
}

export interface ChatAttachment {
  id: string;
  kind: 'file' | 'image' | 'text';
  name: string;
  path?: string;
  mimeType?: string;
  dataUrl?: string;
  content?: string;
}

export interface MarkupAnnotation {
  kind: 'box' | 'arrow';
  x?: number;
  y?: number;
  w?: number;
  h?: number;
  x1?: number;
  y1?: number;
  x2?: number;
  y2?: number;
  label?: string;
}

export interface MentionItem {
  insert: string;
  label: string;
  detail: string;
  isContext: boolean;
  filePath?: string;
}

export type RevertMode = 'workspace_and_chat' | 'chat_only';
export type PermissionPolicy = 'ask' | 'auto_read' | 'auto_workspace' | 'yolo';
export type FollowUpBehavior = 'queue' | 'steer';
export type WidthPreset = 'narrow_15' | 'balanced_20' | 'wide_25' | 'custom';
export type ScreenshotQuality = 'compressed' | 'original';

export interface HarnessDiagnostic {
  id: HarnessKind;
  label: string;
  installed: boolean;
  resolvedPath: string;
  version: string | null;
}

export interface GitGlance {
  isRepo: boolean;
  branch: string;
  changedFiles: string[];
  stagedCount: number;
  modifiedCount: number;
  insertions: number;
  deletions: number;
}

export interface AppSettings {
  layoutVersion?: number;
  harness: HarnessKind;
  workspaceDir: string;
  commands: Record<HarnessKind, { command: string; args: string[] }>;
  harnessEnv: Partial<Record<HarnessKind, Record<string, string>>>;
  harnessModels: Partial<Record<HarnessKind, string>>;
  harnessEfforts: Partial<Record<HarnessKind, string>>;
  harnessModes: Partial<Record<HarnessKind, string>>;
  revertMode: RevertMode;
  permissionPolicy: PermissionPolicy;
  autoApprovePermissions: boolean;
  followUpBehavior: FollowUpBehavior;
  soundCues: boolean;
  flashOnAttention: boolean;
  showGitGlance: boolean;
  screenshotQuality: ScreenshotQuality;
  markupAutoClearSeconds: number;
  pinLiveScreen: boolean;
  autoHideOnBlur: boolean;
  dimOnBlur: boolean;
  opacity: number;
  fontSize: number;
  edge: 'right' | 'left';
  widthPreset: WidthPreset;
  windowBounds?: {
    x: number;
    y: number;
    width: number;
    height: number;
  };
  shortcut: string;
  backupShortcut: string;
  ghostShortcut: string;
}

export interface TurnMetrics {
  inputTokens?: number;
  outputTokens?: number;
  thoughtTokens?: number;
  cachedReadTokens?: number;
  totalTokens?: number;
  contextSize?: number;
  durationMs: number;
}

export interface StoredTurn {
  id: string;
  userText: string;
  attachments: ChatAttachment[];
  checkpointBefore?: string | null;
  thinkingText: string;
  thinkingDurationMs: number;
  tools: Array<{
    id: string;
    name: string;
    title?: string;
    args: string;
    status: 'running' | 'completed' | 'failed';
    output: string;
    durationMs?: number;
  }>;
  assistantMarkdown: string;
  metrics?: TurnMetrics;
  errorMessage?: string;
}

export interface SavedSession {
  id: string;
  title: string;
  harness: HarnessKind;
  workspaceDir: string;
  updatedAt: number;
  turns: StoredTurn[];
}

export type StreamPhase =
  | 'connecting'
  | 'warming'
  | 'thinking'
  | 'tool'
  | 'streaming'
  | 'retrying'
  | 'idle';

export type StreamEvent =
  | { type: 'status'; phase: StreamPhase; label: string }
  | { type: 'catalog'; catalog: SessionCatalog }
  | { type: 'thinking_delta'; text: string }
  | { type: 'text_delta'; text: string }
  | {
      type: 'tool_start';
      id: string;
      name: string;
      title?: string;
      kind?: string;
      args: string;
    }
  | {
      type: 'tool_update';
      id: string;
      status: 'running' | 'completed' | 'failed';
      title?: string;
      args?: string;
      output?: string;
      durationMs?: number;
    }
  | { type: 'permission_request'; request: PermissionRequest }
  | { type: 'session_title'; title: string }
  | {
      type: 'metrics';
      inputTokens?: number;
      outputTokens?: number;
      thoughtTokens?: number;
      cachedReadTokens?: number;
      totalTokens?: number;
      contextSize?: number;
      durationMs: number;
    }
  | { type: 'error'; message: string }
  | { type: 'done'; stopReason?: string };
