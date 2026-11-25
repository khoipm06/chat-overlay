import { HARNESS_LIST } from '../shared/types';
import type {
  AppSettings,
  ChatAttachment,
  DesktopAction,
  ElicitationProperty,
  GitGlance,
  HarnessKind,
  SavedSession,
  SessionCatalog,
  SpeechModelEntry,
  SpeechStatus,
  StoredTurn,
  StreamEvent,
  TurnMetrics,
} from '../shared/types';
import {
  expandSlashTemplate,
  LOCAL_COMMANDS,
  parseLocalCommand,
  parseOsQuickCommand,
  PROMPT_TEMPLATES,
} from '../main/commands';
import { extractActiveTrigger } from '../shared/triggers';
import { PcmWavRecorder } from './audio-recorder';
import { renderGitDiffDrawer } from './diff-drawer';
import { hydrateRenderedMarkdown, markdownToSpokenText, renderMarkdownHtml } from './markdown';
import { PopoverController, type PopoverEntry } from './popovers';
import { renderTabbedSettingsDrawer } from './settings-drawer';
import { playAttentionChime, playDoneChime } from './sound';
import { escapeHtmlText, presentToolCall, renderToolDetailHtml } from './tool-presenter';

// DOM refs
const appShell = document.getElementById('app-shell')!;
const dropOverlay = document.getElementById('drop-overlay')!;
const clipboardActionsBar = document.getElementById('clipboard-actions-bar')!;
const btnClipClose = document.getElementById('btn-clip-close')!;
const btnClipboardCompanion = document.getElementById('btn-clipboard-companion')!;

const liveStatusPill = document.getElementById('live-status-pill')!;
const liveStatusIcon = document.getElementById('live-status-icon')!;
const liveStatusText = document.getElementById('live-status-text')!;
const liveStatusTimer = document.getElementById('live-status-timer')!;
const progressRibbon = document.getElementById('progress-ribbon')!;

const btnUndoChat = document.getElementById('btn-undo-chat') as HTMLButtonElement;
const btnRedoChat = document.getElementById('btn-redo-chat') as HTMLButtonElement;
const btnRevertMode = document.getElementById('btn-revert-mode') as HTMLButtonElement;
const revertModeLabel = document.getElementById('revert-mode-label')!;

const btnWorkspace = document.getElementById('btn-workspace')!;
const workspaceNameEl = document.getElementById('workspace-name')!;
const gitBranchChip = document.getElementById('git-branch-chip')!;
const gitBranchText = document.getElementById('git-branch-text')!;
const btnHarness = document.getElementById('btn-harness')!;
const harnessDot = document.getElementById('harness-dot')!;
const harnessLabel = document.getElementById('harness-label')!;
const sessionTitleChip = document.getElementById('session-title-chip')!;
const ghostModeBadge = document.getElementById('ghost-mode-badge')!;
const contextBudgetEl = document.getElementById('context-budget')!;
const contextBudgetText = document.getElementById('context-budget-text')!;
const contextBudgetFill = document.getElementById('context-budget-fill')!;
const noticeToastEl = document.getElementById('notice-toast')!;

const queuedBannerEl = document.getElementById('queued-banner')!;
const queuedPreviewEl = document.getElementById('queued-preview')!;
const btnQueuedSteer = document.getElementById('btn-queued-steer')!;
const btnQueuedEdit = document.getElementById('btn-queued-edit')!;
const btnQueuedCancel = document.getElementById('btn-queued-cancel')!;

const harnessBanner = document.getElementById('harness-banner')!;
const harnessBannerTitle = document.getElementById('harness-banner-title')!;
const harnessBannerDetail = document.getElementById('harness-banner-detail')!;
const btnRetryWarmup = document.getElementById('btn-retry-warmup')!;

const chatViewport = document.getElementById('chat-viewport')!;
const welcomeView = document.getElementById('welcome-view')!;
const messagesView = document.getElementById('messages-view')!;
const btnScrollBottom = document.getElementById('btn-scroll-bottom')!;

const popoverLayer = document.getElementById('popover-layer')!;
const popovers = new PopoverController(popoverLayer);

const attachmentsRow = document.getElementById('attachments-row')!;
const composerInput = document.getElementById('composer-input') as HTMLTextAreaElement;
const composerCard = document.getElementById('composer-card')!;

const btnAddContext = document.getElementById('btn-add-context')!;
const btnModePicker = document.getElementById('btn-mode-picker')!;
const modeLabel = document.getElementById('mode-label')!;
const btnModelPicker = document.getElementById('btn-model-picker')!;
const modelLoadingSpinner = document.getElementById('model-loading-spinner')!;
const modelLabel = document.getElementById('model-label')!;
const btnEffortPicker = document.getElementById('btn-effort-picker')!;
const effortLabel = document.getElementById('effort-label')!;

const btnPinScreen = document.getElementById('btn-pin-screen')!;
const btnClearMarkup = document.getElementById('btn-clear-markup')!;
const btnSend = document.getElementById('btn-send') as HTMLButtonElement;
const sendIcon = document.getElementById('send-icon')!;

const contextPreviewDialog = document.getElementById('context-preview-dialog') as HTMLDialogElement;
const previewDialogTitle = document.getElementById('preview-dialog-title')!;
const previewDialogIcon = document.getElementById('preview-dialog-icon')!;
const previewDialogBadge = document.getElementById('preview-dialog-badge')!;
const previewDialogBody = document.getElementById('preview-dialog-body')!;
const btnPreviewZoom = document.getElementById('btn-preview-zoom') as HTMLButtonElement;
const previewZoomLabel = document.getElementById('preview-zoom-label')!;
const btnPreviewCopy = document.getElementById('btn-preview-copy') as HTMLButtonElement;
const previewCopyLabel = document.getElementById('preview-copy-label')!;
const btnPreviewClose = document.getElementById('btn-preview-close') as HTMLButtonElement;

const modalBackdrop = document.getElementById('modal-backdrop')!;
const modalTitle = document.getElementById('modal-title')!;
const modalBody = document.getElementById('modal-body')!;

// App State
let settings: AppSettings;
let catalog: SessionCatalog = {
  harness: 'agy_acp',
  status: 'connecting',
  statusMessage: 'Connecting...',
  sessionId: null,
  models: [],
  currentModel: '',
  thinking: { configId: null, supported: false, currentValue: '', options: [] },
  modes: [],
  currentMode: '',
  commands: [],
};

interface ChatSnapshot {
  turns: StoredTurn[];
  title: string;
  draftText: string;
  draftAttachments: ChatAttachment[];
  workspaceDir: string;
  workspaceTree: string | null;
}

interface QueuedPrompt {
  text: string;
  attachments: ChatAttachment[];
  steer: boolean;
}

let currentSessionId: string | null = null;
let currentSessionIsRemote = false;
let currentSessionUpdatedAt = 0;
let currentSessionTitle = 'New chat';
let currentSessionWorkspaceDir = '';
let gitGlance: GitGlance = {
  isRepo: true,
  branch: '',
  changedFiles: [],
  stagedCount: 0,
  modifiedCount: 0,
  insertions: 0,
  deletions: 0,
};
let turns: StoredTurn[] = [];
let undoStack: ChatSnapshot[] = [];
let redoStack: ChatSnapshot[] = [];
let queuedFollowUp: QueuedPrompt | null = null;
let needsContextReplay = false;
let pendingAttachments: ChatAttachment[] = [];
let noticeTimer: number | null = null;
let isStreaming = false;
let followBottom = true;
let userScrollIntentUntil = 0;
let statusStartedAt = 0;
let statusTimerInterval: number | null = null;

interface ToolCardDom {
  root: HTMLElement;
  kindIconEl: HTMLElement;
  verbEl: HTMLElement;
  summaryEl: HTMLElement;
  metaPillEl: HTMLElement;
  timeEl: HTMLElement;
  statusEl: HTMLElement;
  detailEl: HTMLElement;
}

interface ActiveSegmentDom {
  kind: 'thought' | 'tool' | 'text';
  id?: string;
  element: HTMLElement;
  triggerText?: HTMLElement;
  body?: HTMLElement;
  scrollToBottomIfFollowing?: () => void;
  thoughtStartedAt?: number;
  durationMs?: number;
  markdownRaw?: string;
}

interface ActiveTurnRefs {
  turn: StoredTurn;
  flowContainer: HTMLElement;
  activeSegments: ActiveSegmentDom[];
  currentThinkingSegment: ActiveSegmentDom | null;
  currentTextSegment: ActiveSegmentDom | null;
  thinkingStartedAt: number;
  thinkingDisclosure: HTMLElement;
  thinkingTriggerText: HTMLElement;
  thinkingBody: HTMLElement;
  toolsContainer: HTMLElement;
  toolCardMap: Map<string, ToolCardDom>;
  skeletonEl: HTMLElement;
  markdownEl: HTMLElement;
  workingBadge: HTMLElement;
  footerEl: HTMLElement;
  speakBtn: HTMLButtonElement;
  metricsEl: HTMLElement;
  renderScheduled: number | null;
}

let activeSpeakingTurnId: string | null = null;
let activeSpeakingBtn: HTMLButtonElement | null = null;
let usedVoiceForDraft = false;
let lastPromptUsedVoice = false;

function stopSpeechPlayback(): void {
  if (typeof window.speechSynthesis !== 'undefined') {
    window.speechSynthesis.cancel();
  }
  if (activeSpeakingBtn) {
    activeSpeakingBtn.classList.remove('tts-speaking');
    activeSpeakingBtn.innerHTML = '<i class="codicon codicon-unmute"></i>';
    activeSpeakingBtn.title = 'Read reply aloud';
  }
  activeSpeakingTurnId = null;
  activeSpeakingBtn = null;
}

function toggleSpeakTurn(turnId: string, markdown: string, btn: HTMLButtonElement): void {
  if (typeof window.speechSynthesis === 'undefined') {
    showNotice('Speech synthesis is unavailable in this environment.', true);
    return;
  }
  if (activeSpeakingTurnId === turnId) {
    stopSpeechPlayback();
    return;
  }
  stopSpeechPlayback();
  const spoken = markdownToSpokenText(markdown);
  if (!spoken) {
    showNotice('Nothing readable to speak.');
    return;
  }
  const utter = new SpeechSynthesisUtterance(spoken);
  if (settings?.speechLanguage && settings.speechLanguage !== 'auto') {
    utter.lang = settings.speechLanguage;
  }
  activeSpeakingTurnId = turnId;
  activeSpeakingBtn = btn;
  btn.classList.add('tts-speaking');
  btn.innerHTML = '<i class="codicon codicon-mute"></i>';
  btn.title = 'Stop reading aloud';
  const reset = () => {
    if (activeSpeakingTurnId === turnId) {
      stopSpeechPlayback();
    }
  };
  utter.onend = reset;
  utter.onerror = reset;
  window.speechSynthesis.speak(utter);
}

let activeTurn: ActiveTurnRefs | null = null;
let autocompleteKind: 'mention' | 'slash' | null = null;
let autocompleteStart = 0;

function basename(p: string): string {
  if (!p) return 'workspace';
  const parts = p.replace(/\\/g, '/').split('/').filter(Boolean);
  return parts[parts.length - 1] || 'workspace';
}

function setLiveStatus(
  active: boolean,
  phase: 'connecting' | 'warming' | 'thinking' | 'tool' | 'streaming' | 'retrying' | 'idle',
  label: string,
): void {
  if (!active || phase === 'idle') {
    liveStatusPill.classList.add('hidden');
    progressRibbon.classList.add('hidden');
    if (statusTimerInterval !== null) {
      window.clearInterval(statusTimerInterval);
      statusTimerInterval = null;
    }
    return;
  }

  liveStatusPill.classList.remove('hidden');
  progressRibbon.classList.remove('hidden');
  liveStatusText.textContent = label;
  liveStatusIcon.className =
    phase === 'retrying'
      ? 'codicon codicon-warning'
      : 'codicon codicon-loading codicon-modifier-spin';

  if (statusTimerInterval === null) {
    statusStartedAt = Date.now();
    liveStatusTimer.textContent = '0.0s';
    statusTimerInterval = window.setInterval(() => {
      const elapsed = (Date.now() - statusStartedAt) / 1000;
      liveStatusTimer.textContent = `${elapsed.toFixed(1)}s`;
    }, 100);
  }
}

function formatTokenCount(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}k`;
  return String(n);
}

function formatTurnMetrics(m: TurnMetrics): { text: string; tooltip: string } {
  const parts: string[] = [];
  if (m.totalTokens) {
    if (m.inputTokens !== undefined && m.outputTokens !== undefined) {
      parts.push(
        `${formatTokenCount(m.totalTokens)} tok (↑${formatTokenCount(m.inputTokens)} · ↓${formatTokenCount(m.outputTokens)})`,
      );
    } else if (m.contextSize) {
      parts.push(`${formatTokenCount(m.totalTokens)}/${formatTokenCount(m.contextSize)} ctx`);
    } else {
      parts.push(`${formatTokenCount(m.totalTokens)} ctx`);
    }
  }
  parts.push(`${(m.durationMs / 1000).toFixed(1)}s`);

  const tipLines: string[] = [];
  if (m.totalTokens !== undefined) {
    tipLines.push(`Context / Total: ${m.totalTokens.toLocaleString()} tokens`);
  }
  if (m.inputTokens !== undefined) {
    tipLines.push(`Input (prompt + history + tools): ${m.inputTokens.toLocaleString()}`);
  }
  if (m.cachedReadTokens !== undefined) {
    tipLines.push(`Cached read: ${m.cachedReadTokens.toLocaleString()}`);
  }
  if (m.thoughtTokens !== undefined) {
    tipLines.push(`Thinking / reasoning: ${m.thoughtTokens.toLocaleString()}`);
  }
  if (m.outputTokens !== undefined) {
    tipLines.push(`Output generated: ${m.outputTokens.toLocaleString()}`);
  }
  if (m.contextSize !== undefined) {
    tipLines.push(`Model context limit: ${m.contextSize.toLocaleString()}`);
  }
  tipLines.push(`Duration: ${(m.durationMs / 1000).toFixed(2)}s`);

  return { text: parts.join(' · '), tooltip: tipLines.join('\n') };
}

function updateUndoRedoButtons(): void {
  btnUndoChat.disabled = isStreaming || undoStack.length === 0;
  btnRedoChat.disabled = isStreaming || redoStack.length === 0;
  btnUndoChat.title =
    undoStack.length > 0
      ? `Undo Revert (${undoStack.length} saved)`
      : 'Nothing to undo';
  btnRedoChat.title =
    redoStack.length > 0
      ? `Redo Revert (${redoStack.length} saved)`
      : 'Nothing to redo';
}

function showNotice(message: string, warn = false): void {
  noticeToastEl.textContent = message;
  noticeToastEl.classList.toggle('warn', warn);
  noticeToastEl.classList.remove('hidden');
  if (noticeTimer !== null) window.clearTimeout(noticeTimer);
  noticeTimer = window.setTimeout(() => {
    noticeToastEl.classList.add('hidden');
    noticeTimer = null;
  }, 4500);
}

async function refreshGitGlance(): Promise<void> {
  const targetDir = currentSessionWorkspaceDir || settings?.workspaceDir;
  if (!targetDir) return;
  gitGlance = await window.agentOverlay.getGitGlance(targetDir);
  if (settings?.showGitGlance && gitGlance.isRepo && gitGlance.branch) {
    const stat =
      gitGlance.insertions || gitGlance.deletions
        ? ` +${gitGlance.insertions}/-${gitGlance.deletions}`
        : gitGlance.changedFiles.length > 0
          ? ` *${gitGlance.changedFiles.length}`
          : '';
    gitBranchText.textContent = `${gitGlance.branch}${stat}`;
    gitBranchChip.title = `Branch: ${gitGlance.branch}${stat} (click to review changes)`;
    gitBranchChip.classList.remove('hidden');
  } else {
    gitBranchChip.classList.add('hidden');
  }
  renderRevertModeControl();
}

function renderRevertModeControl(): void {
  if (!settings) return;
  if (!gitGlance.isRepo) {
    revertModeLabel.textContent = 'Chat (no git)';
    btnRevertMode.title =
      'Current workspace is not a Git repository — Revert applies to chat turns only';
  } else if (settings.revertMode === 'chat_only') {
    revertModeLabel.textContent = 'Chat Only';
    btnRevertMode.title =
      'Revert Mode: Chat Only (files untouched) — click to enable Workspace Files + Chat';
  } else {
    revertModeLabel.textContent = 'Files+Chat';
    btnRevertMode.title =
      'Revert Mode: Workspace Files + Chat (Git Checkpoint) — click to switch to Chat Only';
  }
}

function updateContextBudget(m?: TurnMetrics): void {
  if (!m || (!m.totalTokens && !m.inputTokens)) {
    contextBudgetEl.classList.add('hidden');
    return;
  }
  contextBudgetEl.classList.remove('hidden');

  const limit = m.contextSize || 200_000;
  // If inputTokens is provided, active context window is prompt context + generated output
  const hasTurnTokens = m.inputTokens !== undefined;
  const turnContext = hasTurnTokens
    ? (m.inputTokens || 0) + (m.outputTokens || 0)
    : (m.totalTokens || 0);

  if (turnContext <= limit) {
    const pct = Math.min(100, Math.max(1, Math.round((turnContext / limit) * 100)));
    contextBudgetText.textContent = `${formatTokenCount(turnContext)} / ${formatTokenCount(limit)} (${pct}%)`;
    contextBudgetFill.style.width = `${pct}%`;
    contextBudgetFill.style.background =
      pct >= 85 ? 'var(--danger)' : pct >= 65 ? 'rgba(0, 120, 212, 0.9)' : 'var(--success)';
  } else {
    // When cumulative session tokens exceed context window limit, show total tokens clearly
    const pct = Math.min(100, Math.round(((m.inputTokens || limit) / limit) * 100));
    contextBudgetText.textContent = `${formatTokenCount(m.totalTokens || turnContext)} tok · limit ${formatTokenCount(limit)}`;
    contextBudgetFill.style.width = `${pct}%`;
    contextBudgetFill.style.background =
      pct >= 85 ? 'var(--danger)' : pct >= 65 ? 'rgba(0, 120, 212, 0.9)' : 'var(--success)';
  }
  contextBudgetEl.title = formatTurnMetrics(m).tooltip;
}

function renderQueuedBanner(): void {
  if (!queuedFollowUp) {
    queuedBannerEl.classList.add('hidden');
    return;
  }
  queuedBannerEl.classList.remove('hidden');
  queuedPreviewEl.textContent = queuedFollowUp.text || '(attached context)';
}

function renderHeaderAndPickers(): void {
  if (settings) {
    const activeDir = currentSessionWorkspaceDir || settings.workspaceDir;
    workspaceNameEl.textContent = basename(activeDir);
    btnPinScreen.classList.toggle('active', settings.pinLiveScreen);
    document.documentElement.style.setProperty('--font-size', `${settings.fontSize}px`);
    renderRevertModeControl();
  }
  updateUndoRedoButtons();
  renderQueuedBanner();

  const meta = HARNESS_LIST.find((h) => h.id === catalog.harness) ?? HARNESS_LIST[0];
  harnessLabel.textContent = meta.label.replace(' (ACP)', '');
  harnessDot.className = `harness-dot ${catalog.status}`;

  // Banner on error
  if (catalog.status === 'error') {
    harnessBanner.classList.remove('hidden');
    harnessBannerTitle.textContent = `${meta.label} not connected`;
    harnessBannerDetail.textContent = catalog.errorDetail || catalog.statusMessage;
  } else {
    harnessBanner.classList.add('hidden');
  }

  // Model picker text — NEVER show stale/wrong-harness fallback!
  const isLoading = catalog.status === 'connecting' || catalog.status === 'warming';
  modelLoadingSpinner.classList.toggle('hidden', !isLoading);
  if (isLoading && catalog.models.length === 0) {
    modelLabel.textContent = `Loading...`;
  } else if (catalog.status === 'error' && catalog.models.length === 0) {
    modelLabel.textContent = 'No models';
  } else {
    const activeModel = catalog.models.find((m) => m.id === catalog.currentModel);
    modelLabel.textContent =
      activeModel?.name || catalog.currentModel || (catalog.models[0]?.name ?? 'Select model');
  }

  // Standardized Thinking Effort pill — strictly synced with live ACP configOptions!
  if (catalog.thinking.supported && catalog.thinking.options.length > 0) {
    btnEffortPicker.classList.remove('hidden');
    const curOpt = catalog.thinking.options.find(
      (o) => o.value === catalog.thinking.currentValue,
    );
    effortLabel.textContent = curOpt?.label || catalog.thinking.currentValue || 'Effort';
  } else {
    btnEffortPicker.classList.add('hidden');
  }

  // Mode picker
  if (catalog.modes.length > 0) {
    btnModePicker.classList.remove('hidden');
    const curMode = catalog.modes.find((m) => m.id === catalog.currentMode);
    modeLabel.textContent = curMode?.name || catalog.currentMode || catalog.modes[0].name;
  } else {
    btnModePicker.classList.add('hidden');
  }

  if (catalog.status === 'connecting' || catalog.status === 'warming') {
    setLiveStatus(true, catalog.status, catalog.statusMessage);
  } else if (!isStreaming) {
    setLiveStatus(false, 'idle', 'Ready');
  }
}

function updateSendButton(): void {
  const hasDraft = Boolean(composerInput.value.trim() || pendingAttachments.length > 0);
  if (isStreaming) {
    if (hasDraft) {
      btnSend.classList.remove('stop');
      btnSend.classList.add('queue');
      btnSend.title =
        settings?.followUpBehavior === 'steer'
          ? 'Steer / Interrupt immediately (Enter)'
          : 'Queue follow-up message (Enter)';
      sendIcon.className = 'codicon codicon-plus';
    } else {
      btnSend.classList.remove('queue');
      btnSend.classList.add('stop');
      btnSend.title = 'Stop generation (Esc)';
      sendIcon.className = 'codicon codicon-debug-stop';
    }
  } else {
    btnSend.classList.remove('stop', 'queue');
    btnSend.title = 'Send prompt (Enter)';
    sendIcon.className = 'codicon codicon-arrow-up';
  }
}

interface PreviewTarget {
  name: string;
  kind?: string;
  content?: string;
  dataUrl?: string;
  filePath?: string;
  mimeType?: string;
}

let activePreviewCopyHandler: (() => Promise<void>) | null = null;
let activePreviewZoomToggle: (() => void) | null = null;

async function openContextPreview(target: PreviewTarget): Promise<void> {
  if (!contextPreviewDialog) return;
  previewDialogTitle.textContent = target.name || 'Context Preview';
  previewDialogTitle.title = target.filePath || target.name || '';
  previewDialogBody.innerHTML = '';
  btnPreviewZoom.classList.add('hidden');
  previewCopyLabel.textContent = 'Copy';
  btnPreviewCopy.disabled = false;

  const isImage =
    target.kind === 'image' ||
    Boolean(target.dataUrl) ||
    /\.(png|jpe?g|webp|gif|svg)$/i.test(target.name);

  if (isImage && target.dataUrl) {
    previewDialogIcon.className = 'codicon codicon-device-camera';
    previewDialogBadge.textContent = 'IMAGE';
    btnPreviewZoom.classList.remove('hidden');
    previewZoomLabel.textContent = 'Zoom 2.5x';
    const zoomIcon = btnPreviewZoom.querySelector('i');
    if (zoomIcon) zoomIcon.className = 'codicon codicon-zoom-in';

    const viewport = document.createElement('div');
    viewport.className = 'preview-image-viewport';
    const img = document.createElement('img');
    img.src = target.dataUrl;
    img.className = 'preview-image';
    img.alt = target.name;
    img.draggable = false;
    img.addEventListener('dragstart', (e) => e.preventDefault());
    viewport.appendChild(img);
    previewDialogBody.appendChild(viewport);

    let zoom = 1;
    let panX = 0;
    let panY = 0;
    let isDragging = false;
    let didDrag = false;
    let pointerDownX = 0;
    let pointerDownY = 0;
    let startPanX = 0;
    let startPanY = 0;

    const clampPan = () => {
      if (zoom <= 1) {
        panX = 0;
        panY = 0;
        return;
      }
      const vw = viewport.clientWidth;
      const vh = viewport.clientHeight;
      const bw = img.offsetWidth;
      const bh = img.offsetHeight;
      const maxPanX = Math.max(0, (bw * zoom - vw) / 2 + 40);
      const maxPanY = Math.max(0, (bh * zoom - vh) / 2 + 40);
      panX = Math.min(maxPanX, Math.max(-maxPanX, panX));
      panY = Math.min(maxPanY, Math.max(-maxPanY, panY));
    };

    const updateZoomUi = () => {
      const zoomed = zoom > 1.01;
      img.classList.toggle('is-zoomed', zoomed);
      if (zoomed) {
        clampPan();
        img.style.transform = `translate(${panX}px, ${panY}px) scale(${zoom})`;
        previewZoomLabel.textContent = 'Fit (1x)';
        if (zoomIcon) zoomIcon.className = 'codicon codicon-zoom-out';
      } else {
        zoom = 1;
        panX = 0;
        panY = 0;
        img.classList.remove('is-zoomed', 'is-dragging');
        img.style.transform = 'none';
        previewZoomLabel.textContent = 'Zoom 2.5x';
        if (zoomIcon) zoomIcon.className = 'codicon codicon-zoom-in';
      }
    };

    const toggleZoom = () => {
      zoom = zoom > 1.01 ? 1 : 2.5;
      if (zoom === 1) {
        panX = 0;
        panY = 0;
      }
      updateZoomUi();
    };

    activePreviewZoomToggle = toggleZoom;

    viewport.addEventListener('pointerdown', (e: PointerEvent) => {
      if (e.button !== 0) return;
      e.preventDefault();
      didDrag = false;
      pointerDownX = e.clientX;
      pointerDownY = e.clientY;
      startPanX = panX;
      startPanY = panY;
      if (zoom > 1.01) {
        isDragging = true;
        img.classList.add('is-dragging');
        try {
          viewport.setPointerCapture(e.pointerId);
        } catch {
          // ignore
        }
      }
    });

    viewport.addEventListener('pointermove', (e: PointerEvent) => {
      if (!isDragging || zoom <= 1.01) return;
      const dx = e.clientX - pointerDownX;
      const dy = e.clientY - pointerDownY;
      if (Math.hypot(dx, dy) > 4) {
        didDrag = true;
      }
      panX = startPanX + dx;
      panY = startPanY + dy;
      updateZoomUi();
    });

    const stopDrag = (e: PointerEvent) => {
      const wasDragging = isDragging;
      if (isDragging) {
        isDragging = false;
        img.classList.remove('is-dragging');
        try {
          viewport.releasePointerCapture(e.pointerId);
        } catch {
          // ignore
        }
      }
      if (e.type === 'pointerup' && !didDrag) {
        if (!wasDragging || zoom > 1.01) {
          toggleZoom();
        }
      }
    };

    viewport.addEventListener('pointerup', stopDrag);
    viewport.addEventListener('pointercancel', stopDrag);
    viewport.addEventListener(
      'wheel',
      (e: WheelEvent) => {
        e.preventDefault();
        const factor = e.deltaY < 0 ? 1.2 : 1 / 1.2;
        zoom = Math.min(5, Math.max(1, Number((zoom * factor).toFixed(2))));
        updateZoomUi();
      },
      { passive: false },
    );

    activePreviewCopyHandler = async () => {
      await window.agentOverlay.copyImageToClipboard(target.dataUrl!);
      previewCopyLabel.textContent = 'Copied Image ✓';
      setTimeout(() => {
        previewCopyLabel.textContent = 'Copy';
      }, 1500);
    };
  } else {
    // Text / Code / File Preview
    previewDialogIcon.className = 'codicon codicon-file-code';
    previewDialogBadge.textContent = 'FILE';

    let content = target.content || '';
    let isTruncated = false;

    if (!content && (target.filePath || target.name)) {
      previewDialogBody.innerHTML =
        '<div style="padding:16px;color:var(--text-muted);"><i class="codicon codicon-loading codicon-modifier-spin"></i> Loading file preview...</div>';
      const fileRes = await window.agentOverlay.readFilePreview(target.filePath || target.name);
      previewDialogBody.innerHTML = '';
      if (fileRes.ok) {
        content = fileRes.content;
        isTruncated = fileRes.truncated;
        const lineCount = content.split('\n').length;
        const kb = Math.round(fileRes.totalBytes / 1024);
        previewDialogBadge.textContent = `${lineCount} lines · ${kb} KB`;
      } else {
        content = fileRes.error || 'Failed to read file';
        previewDialogBadge.textContent = 'ERROR';
      }
    } else {
      const lineCount = content.split('\n').length;
      previewDialogBadge.textContent = `${lineCount} lines`;
    }

    const pre = document.createElement('pre');
    pre.className = 'preview-code-block';
    const code = document.createElement('code');
    code.textContent = content;
    pre.appendChild(code);
    previewDialogBody.appendChild(pre);

    if (isTruncated) {
      const notice = document.createElement('div');
      notice.className = 'preview-truncation-notice';
      notice.textContent = 'Preview capped at 400KB for responsiveness.';
      previewDialogBody.appendChild(notice);
    }

    activePreviewCopyHandler = async () => {
      await window.agentOverlay.copyTextToClipboard(content);
      previewCopyLabel.textContent = 'Copied ✓';
      setTimeout(() => {
        previewCopyLabel.textContent = 'Copy';
      }, 1500);
    };
  }

  contextPreviewDialog.showModal();
}

function renderAttachments(): void {
  attachmentsRow.innerHTML = '';
  for (const att of pendingAttachments) {
    const chip = document.createElement('div');
    chip.className = 'att-chip';
    chip.title = 'Click to preview context';
    if (att.kind === 'image' && att.dataUrl) {
      const img = document.createElement('img');
      img.src = att.dataUrl;
      img.className = 'att-thumb';
      chip.appendChild(img);
    } else {
      const ic = document.createElement('i');
      ic.className = `codicon ${att.kind === 'file' ? 'codicon-file-code' : 'codicon-note'}`;
      chip.appendChild(ic);
    }
    const name = document.createElement('span');
    name.textContent = att.name;
    chip.appendChild(name);

    chip.addEventListener('click', (e) => {
      if ((e.target as HTMLElement).closest('.att-remove')) return;
      void openContextPreview(att);
    });

    const rm = document.createElement('button');
    rm.type = 'button';
    rm.className = 'att-remove';
    rm.title = 'Remove attachment';
    rm.innerHTML = '<i class="codicon codicon-close"></i>';
    rm.addEventListener('click', () => {
      pendingAttachments = pendingAttachments.filter((a) => a.id !== att.id);
      renderAttachments();
    });
    chip.appendChild(rm);
    attachmentsRow.appendChild(chip);
  }
}

function addAttachment(att: ChatAttachment): void {
  // Deduplicate files by path
  if (att.kind === 'file' && att.path) {
    if (pendingAttachments.some((a) => a.path === att.path)) return;
  }
  pendingAttachments.push(att);
  renderAttachments();
}

function autoResizeComposer(): void {
  composerInput.style.height = 'auto';
  composerInput.style.height = `${Math.min(160, Math.max(24, composerInput.scrollHeight))}px`;
}

function scrollToBottomIfPinned(): void {
  if (followBottom) {
    chatViewport.scrollTop = chatViewport.scrollHeight;
  }
}

function markUserScrollIntent(): void {
  userScrollIntentUntil = Date.now() + 350;
}

chatViewport.addEventListener(
  'wheel',
  (e) => {
    if (e.deltaY < 0) {
      followBottom = false;
      btnScrollBottom.classList.remove('hidden');
    } else {
      markUserScrollIntent();
    }
  },
  { passive: true },
);

chatViewport.addEventListener('pointerdown', markUserScrollIntent, { passive: true });
chatViewport.addEventListener('touchstart', markUserScrollIntent, { passive: true });
chatViewport.addEventListener('keydown', (e) => {
  if (['PageUp', 'ArrowUp', 'Home'].includes(e.key)) {
    followBottom = false;
    btnScrollBottom.classList.remove('hidden');
  } else if (['PageDown', 'ArrowDown', 'End'].includes(e.key)) {
    markUserScrollIntent();
  }
});

chatViewport.addEventListener('scroll', () => {
  const dist =
    chatViewport.scrollHeight - chatViewport.scrollTop - chatViewport.clientHeight;
  if (dist <= 24) {
    followBottom = true;
    btnScrollBottom.classList.add('hidden');
  } else if (Date.now() <= userScrollIntentUntil) {
    followBottom = false;
    btnScrollBottom.classList.remove('hidden');
  }
});

// Keep pinned to bottom whenever content or viewport/composer size changes
const messagesResizeObserver = new ResizeObserver(() => {
  if (followBottom) {
    chatViewport.scrollTop = chatViewport.scrollHeight;
  }
});
messagesResizeObserver.observe(messagesView);
messagesResizeObserver.observe(chatViewport);

btnScrollBottom.addEventListener('click', () => {
  followBottom = true;
  btnScrollBottom.classList.add('hidden');
  chatViewport.scrollTop = chatViewport.scrollHeight;
});

async function revertToTurn(turnId: string): Promise<void> {
  if (isStreaming) return;
  const idx = turns.findIndex((t) => t.id === turnId);
  if (idx < 0) return;

  const targetTurn = turns[idx];
  const targetDir = currentSessionWorkspaceDir || settings.workspaceDir;
  let capturedTree: string | null = null;
  if (settings.revertMode === 'workspace_and_chat' && gitGlance.isRepo) {
    const currentCkpt = await window.agentOverlay.captureCheckpoint(targetDir);
    if (!currentCkpt.treeSha) {
      showNotice(
        `Could not capture current files cleanly (${currentCkpt.error || 'git snapshot failed'}). Switched to safe Chat Only for this revert.`,
        true,
      );
    } else {
      capturedTree = currentCkpt.treeSha;
    }
  }

  undoStack.push({
    turns: structuredClone(turns),
    title: currentSessionTitle,
    draftText: composerInput.value,
    draftAttachments: structuredClone(pendingAttachments),
    workspaceDir: targetDir,
    workspaceTree: capturedTree,
  });
  if (undoStack.length > 30) undoStack.shift();
  redoStack = [];

  if (settings.revertMode === 'workspace_and_chat') {
    if (targetTurn.checkpointBefore) {
      const res = await window.agentOverlay.restoreCheckpoint(
        targetTurn.checkpointBefore,
        targetDir,
      );
      showNotice(res.message, !res.restored);
      void refreshGitGlance();
    } else {
      showNotice(
        'Chat reverted. Files untouched (this turn was created without a file checkpoint).',
        true,
      );
    }
  } else {
    showNotice('Chat reverted (Files left untouched in Chat Only mode)');
  }

  turns = turns.slice(0, idx);
  needsContextReplay = turns.length > 0;
  composerInput.value = targetTurn.userText;
  pendingAttachments = structuredClone(targetTurn.attachments);
  renderAttachments();
  autoResizeComposer();

  rebuildMessagesList();
  await persistCurrentSession();
  await window.agentOverlay.resetSession();
  updateUndoRedoButtons();
  updateSendButton();
  composerInput.focus();
}

async function applySnapshotRestore(
  snap: ChatSnapshot,
  pushInverseTo: ChatSnapshot[],
): Promise<void> {
  const targetDir = snap.workspaceDir || currentSessionWorkspaceDir || settings.workspaceDir;
  const currentCkpt =
    settings.revertMode === 'workspace_and_chat' && gitGlance.isRepo
      ? await window.agentOverlay.captureCheckpoint(targetDir)
      : { treeSha: null };

  pushInverseTo.push({
    turns: structuredClone(turns),
    title: currentSessionTitle,
    draftText: composerInput.value,
    draftAttachments: structuredClone(pendingAttachments),
    workspaceDir: targetDir,
    workspaceTree: currentCkpt.treeSha,
  });
  if (pushInverseTo.length > 30) pushInverseTo.shift();

  if (settings.revertMode === 'workspace_and_chat') {
    if (snap.workspaceTree) {
      const res = await window.agentOverlay.restoreCheckpoint(snap.workspaceTree, targetDir);
      showNotice(res.message, !res.restored);
      void refreshGitGlance();
    } else {
      showNotice('Conversation state restored (no file snapshot associated).', true);
    }
  } else {
    showNotice('Conversation state restored (Chat Only mode)');
  }

  currentSessionWorkspaceDir = targetDir;
  turns = structuredClone(snap.turns);
  needsContextReplay = turns.length > 0;
  currentSessionTitle = snap.title;
  sessionTitleChip.textContent = currentSessionTitle;
  composerInput.value = snap.draftText;
  pendingAttachments = structuredClone(snap.draftAttachments);
  renderAttachments();
  autoResizeComposer();
  rebuildMessagesList();
  await persistCurrentSession();
  await window.agentOverlay.resetSession();
  updateUndoRedoButtons();
  updateSendButton();
}

async function undoChatRevert(): Promise<void> {
  if (isStreaming || undoStack.length === 0) return;
  const snap = undoStack.pop()!;
  await applySnapshotRestore(snap, redoStack);
}

async function redoChatRevert(): Promise<void> {
  if (isStreaming || redoStack.length === 0) return;
  const snap = redoStack.pop()!;
  await applySnapshotRestore(snap, undoStack);
}

function rebuildMessagesList(): void {
  messagesView.innerHTML = '';
  if (turns.length === 0) {
    messagesView.classList.add('hidden');
    welcomeView.classList.remove('hidden');
  } else {
    welcomeView.classList.add('hidden');
    messagesView.classList.remove('hidden');
    for (const t of turns) buildTurnDom(t, false);
  }
}

async function persistCurrentSession(): Promise<void> {
  if (!currentSessionId || turns.length === 0) return;
  const now = Date.now();
  currentSessionUpdatedAt = now;
  await window.agentOverlay.saveSession({
    id: currentSessionId,
    title: currentSessionTitle,
    harness: catalog.harness,
    workspaceDir: currentSessionWorkspaceDir || settings.workspaceDir,
    updatedAt: now,
    turns,
  });
}

function createThinkingDisclosureDom(initialTriggerText: string, bodyText = ''): {
  root: HTMLElement;
  trigger: HTMLButtonElement;
  chevron: HTMLElement;
  triggerText: HTMLElement;
  body: HTMLElement;
  scrollToBottomIfFollowing: () => void;
} {
  const disclosure = document.createElement('div');
  disclosure.className = 'disclosure';
  const trigger = document.createElement('button');
  trigger.type = 'button';
  trigger.className = 'disclosure-trigger';
  const chevron = document.createElement('i');
  chevron.className = 'codicon codicon-chevron-right';
  const triggerText = document.createElement('span');
  triggerText.textContent = initialTriggerText;
  trigger.append(chevron, triggerText);
  const body = document.createElement('div');
  body.className = 'disclosure-body hidden';
  body.textContent = bodyText;

  let autoFollow = true;
  body.addEventListener('scroll', () => {
    const nearBottom = body.scrollHeight - body.scrollTop - body.clientHeight <= 24;
    autoFollow = nearBottom;
  });

  const scrollToBottomIfFollowing = () => {
    if (!body.classList.contains('hidden') && autoFollow) {
      body.scrollTop = body.scrollHeight;
    }
  };

  trigger.addEventListener('click', () => {
    const isHidden = body.classList.toggle('hidden');
    chevron.className = isHidden
      ? 'codicon codicon-chevron-right'
      : 'codicon codicon-chevron-down';
    if (!isHidden) {
      autoFollow = true;
      body.scrollTop = body.scrollHeight;
    }
  });
  disclosure.append(trigger, body);
  return { root: disclosure, trigger, chevron, triggerText, body, scrollToBottomIfFollowing };
}

function buildTurnDom(turn: StoredTurn, streamingNow: boolean): ActiveTurnRefs {
  welcomeView.classList.add('hidden');
  messagesView.classList.remove('hidden');

  const group = document.createElement('div');
  group.className = 'turn-group';

  // User Bubble with Revert action
  const userBubble = document.createElement('div');
  userBubble.className = 'user-bubble';

  const revertBtn = document.createElement('button');
  revertBtn.type = 'button';
  revertBtn.className = 'turn-revert-btn';
  revertBtn.title =
    settings?.revertMode === 'workspace_and_chat' && turn.checkpointBefore
      ? 'Revert conversation & workspace files to before this prompt'
      : 'Revert conversation to before this prompt (chat only)';
  revertBtn.innerHTML = '<i class="codicon codicon-discard"></i><span>Revert</span>';
  revertBtn.addEventListener('click', () => {
    void revertToTurn(turn.id);
  });
  userBubble.appendChild(revertBtn);

  if (turn.attachments && turn.attachments.length > 0) {
    const userTop = document.createElement('div');
    userTop.className = 'user-bubble-top';
    const attsWrap = document.createElement('div');
    attsWrap.className = 'user-attachments';
    for (const att of turn.attachments) {
      const c = document.createElement('span');
      c.className = 'user-att-chip';
      c.title = 'Click to preview context';
      const ic = document.createElement('i');
      ic.className = `codicon ${att.kind === 'image' ? 'codicon-device-camera' : 'codicon-file-code'}`;
      const lbl = document.createElement('span');
      lbl.textContent = att.name;
      c.append(ic, lbl);
      c.addEventListener('click', () => {
        void openContextPreview(att);
      });
      attsWrap.appendChild(c);
    }
    userTop.appendChild(attsWrap);
    userBubble.appendChild(userTop);
  }

  const userText = document.createElement('div');
  userText.className = 'user-text';
  userText.textContent = turn.userText;
  userBubble.appendChild(userText);
  group.appendChild(userBubble);

  // Assistant Turn
  const assistantTurn = document.createElement('div');
  assistantTurn.className = 'assistant-turn';

  const header = document.createElement('div');
  header.className = 'assistant-header';
  const curModelObj = catalog.models.find((m) => m.id === catalog.currentModel);
  const avatarEl = document.createElement('span');
  avatarEl.className = 'assistant-avatar';
  avatarEl.innerHTML = '<i class="codicon codicon-terminal"></i>';
  const nameEl = document.createElement('span');
  nameEl.className = 'assistant-name';
  nameEl.textContent = 'Assistant';
  const modelBadgeEl = document.createElement('span');
  modelBadgeEl.className = 'assistant-model-badge';
  modelBadgeEl.textContent = curModelObj?.name || catalog.currentModel || '';
  header.append(avatarEl, nameEl, modelBadgeEl);
  const workingBadge = document.createElement('span');
  workingBadge.className = `turn-working-badge${streamingNow ? '' : ' hidden'}`;
  workingBadge.innerHTML =
    '<i class="codicon codicon-loading codicon-modifier-spin"></i><span>Working...</span>';
  header.appendChild(workingBadge);
  assistantTurn.appendChild(header);

  // Sequential Flow Container
  const flowContainer = document.createElement('div');
  flowContainer.className = 'turn-flow-container';
  flowContainer.style.display = 'flex';
  flowContainer.style.flexDirection = 'column';
  flowContainer.style.gap = '6px';
  assistantTurn.appendChild(flowContainer);

  const toolCardMap = new Map<string, ToolCardDom>();
  const activeSegments: ActiveSegmentDom[] = [];

  // Default elements for ActiveTurnRefs compatibility
  const defaultDisc = createThinkingDisclosureDom(
    turn.thinkingDurationMs
      ? `Thought for ${(turn.thinkingDurationMs / 1000).toFixed(1)}s`
      : 'Thinking',
    turn.thinkingText,
  );
  if (!turn.thinkingText) {
    defaultDisc.root.classList.add('hidden');
  }

  const defaultToolsContainer = document.createElement('div');
  defaultToolsContainer.style.display = 'flex';
  defaultToolsContainer.style.flexDirection = 'column';
  defaultToolsContainer.style.gap = '6px';

  const defaultMarkdownEl = document.createElement('div');
  defaultMarkdownEl.className = 'assistant-markdown';

  // Shimmer skeleton shown until first text arrives
  const skeletonEl = document.createElement('div');
  skeletonEl.className = `thinking-skeleton${streamingNow && !turn.assistantMarkdown ? '' : ' hidden'}`;
  skeletonEl.innerHTML = `
    <div class="skeleton-line" style="width: 78%"></div>
    <div class="skeleton-line" style="width: 92%"></div>
    <div class="skeleton-line" style="width: 54%"></div>
  `;
  flowContainer.appendChild(skeletonEl);

  // If loading history with segments:
  if (turn.segments && turn.segments.length > 0) {
    for (const seg of turn.segments) {
      if (seg.kind === 'thought') {
        const disc = createThinkingDisclosureDom(
          seg.durationMs ? `Thought for ${(seg.durationMs / 1000).toFixed(1)}s` : 'Thought',
          seg.text,
        );
        flowContainer.appendChild(disc.root);
        activeSegments.push({
          kind: 'thought',
          element: disc.root,
          triggerText: disc.triggerText,
          body: disc.body,
          durationMs: seg.durationMs,
          markdownRaw: seg.text,
        });
      } else if (seg.kind === 'tool') {
        const toolId = seg.toolId || seg.id;
        const t = turn.tools.find((tool) => tool.id === toolId);
        if (t) {
          appendToolCard(
            flowContainer,
            toolCardMap,
            t.id,
            t.name,
            t.title,
            t.args,
            t.status,
            t.output,
            t.durationMs,
          );
        }
      } else if (seg.kind === 'text') {
        const mdEl = document.createElement('div');
        mdEl.className = 'assistant-markdown';
        const txt = seg.markdown || seg.text || '';
        mdEl.innerHTML = renderMarkdownHtml(txt);
        void hydrateRenderedMarkdown(mdEl, false);
        flowContainer.appendChild(mdEl);
        activeSegments.push({
          kind: 'text',
          element: mdEl,
          markdownRaw: txt,
        });
      }
    }
  } else if (!streamingNow) {
    // Legacy fallback rendering
    if (turn.thinkingText) {
      flowContainer.appendChild(defaultDisc.root);
    }
    for (const t of turn.tools) {
      appendToolCard(
        flowContainer,
        toolCardMap,
        t.id,
        t.name,
        t.title,
        t.args,
        t.status,
        t.output,
        t.durationMs,
      );
    }
    if (turn.assistantMarkdown) {
      defaultMarkdownEl.innerHTML = renderMarkdownHtml(turn.assistantMarkdown);
      void hydrateRenderedMarkdown(defaultMarkdownEl, false);
      flowContainer.appendChild(defaultMarkdownEl);
    }
  }

  // Footer
  const footerEl = document.createElement('div');
  footerEl.className = `turn-footer${streamingNow ? ' hidden' : ''}`;
  const actionsEl = document.createElement('div');
  actionsEl.className = 'turn-footer-actions';

  const copyBtn = document.createElement('button');
  copyBtn.type = 'button';
  copyBtn.className = 'icon-btn';
  copyBtn.title = 'Copy response markdown';
  copyBtn.innerHTML = '<i class="codicon codicon-copy"></i>';
  copyBtn.addEventListener('click', () => {
    void navigator.clipboard.writeText(turn.assistantMarkdown);
  });

  const retryBtn = document.createElement('button');
  retryBtn.type = 'button';
  retryBtn.className = 'icon-btn';
  retryBtn.title = 'Retry prompt';
  retryBtn.innerHTML = '<i class="codicon codicon-refresh"></i>';
  retryBtn.addEventListener('click', () => {
    if (isStreaming) return;
    composerInput.value = turn.userText;
    pendingAttachments = [...turn.attachments];
    renderAttachments();
    autoResizeComposer();
    void submitPrompt();
  });

  const speakBtn = document.createElement('button');
  speakBtn.type = 'button';
  speakBtn.className = 'icon-btn';
  speakBtn.title = 'Read reply aloud';
  speakBtn.innerHTML = '<i class="codicon codicon-unmute"></i>';
  speakBtn.addEventListener('click', () => {
    toggleSpeakTurn(turn.id, turn.assistantMarkdown, speakBtn);
  });

  actionsEl.append(copyBtn, speakBtn, retryBtn);
  const metricsEl = document.createElement('span');
  if (turn.metrics) {
    const formatted = formatTurnMetrics(turn.metrics);
    metricsEl.textContent = formatted.text;
    metricsEl.title = formatted.tooltip;
  }
  footerEl.append(actionsEl, metricsEl);
  assistantTurn.appendChild(footerEl);

  group.appendChild(assistantTurn);
  messagesView.appendChild(group);

  return {
    turn,
    flowContainer,
    activeSegments,
    currentThinkingSegment: null,
    currentTextSegment: null,
    thinkingStartedAt: 0,
    thinkingDisclosure: defaultDisc.root,
    thinkingTriggerText: defaultDisc.triggerText,
    thinkingBody: defaultDisc.body,
    toolsContainer: defaultToolsContainer,
    toolCardMap,
    skeletonEl,
    markdownEl: defaultMarkdownEl,
    workingBadge,
    footerEl,
    speakBtn,
    metricsEl,
    renderScheduled: null,
  };
}

function appendToolCard(
  container: HTMLElement,
  map: Map<string, ToolCardDom>,
  id: string,
  name: string,
  title: string | undefined,
  args: string,
  status: 'running' | 'completed' | 'failed',
  output = '',
  durationMs?: number,
): void {
  const presented = presentToolCall({ name, title, args, output, status });
  const card = document.createElement('div');
  card.className = 'tool-card';

  const hdr = document.createElement('button');
  hdr.type = 'button';
  hdr.className = 'tool-card-header';

  const left = document.createElement('div');
  left.className = 'tool-card-left';
  const statusEl = document.createElement('i');
  statusEl.className =
    status === 'running'
      ? 'codicon codicon-loading codicon-modifier-spin tool-status-running'
      : status === 'completed'
        ? 'codicon codicon-check tool-status-ok'
        : 'codicon codicon-error tool-status-err';

  const kindIconEl = document.createElement('i');
  kindIconEl.className = `codicon ${presented.icon} tool-card-kind-icon`;

  const verbEl = document.createElement('span');
  verbEl.className = 'tool-card-verb';
  verbEl.textContent = presented.verb;

  const summaryEl = document.createElement('span');
  summaryEl.className = 'tool-card-summary';
  summaryEl.textContent = presented.summary;

  left.append(statusEl, kindIconEl, verbEl, summaryEl);

  const right = document.createElement('div');
  right.className = 'tool-card-right';
  const metaPillEl = document.createElement('span');
  metaPillEl.className = `tool-card-meta-pill${presented.badge ? '' : ' hidden'}`;
  metaPillEl.textContent = presented.badge || '';

  const timeEl = document.createElement('span');
  timeEl.className = 'tool-card-time';
  timeEl.textContent = durationMs ? `${(durationMs / 1000).toFixed(1)}s` : '';

  right.append(metaPillEl, timeEl);
  hdr.append(left, right);

  const detailEl = document.createElement('div');
  detailEl.className = 'tool-card-detail hidden';
  detailEl.innerHTML = renderToolDetailHtml(presented);

  hdr.addEventListener('click', () => {
    detailEl.classList.toggle('hidden');
  });

  card.append(hdr, detailEl);
  container.appendChild(card);
  map.set(id, {
    root: card,
    kindIconEl,
    verbEl,
    summaryEl,
    metaPillEl,
    timeEl,
    statusEl,
    detailEl,
  });
}

function scheduleMarkdownUpdate(ref: ActiveTurnRefs): void {
  if (ref.renderScheduled !== null) return;
  ref.renderScheduled = window.setTimeout(() => {
    ref.renderScheduled = null;
    ref.skeletonEl.classList.add('hidden');
    ref.markdownEl.innerHTML = renderMarkdownHtml(ref.turn.assistantMarkdown);
    void hydrateRenderedMarkdown(ref.markdownEl, true).then(() => scrollToBottomIfPinned());
    scrollToBottomIfPinned();
  }, 60);
}

function scheduleMarkdownUpdateForSegment(
  ref: ActiveTurnRefs,
  seg: ActiveSegmentDom,
): void {
  if (ref.renderScheduled !== null) return;
  ref.renderScheduled = window.setTimeout(() => {
    ref.renderScheduled = null;
    ref.skeletonEl.classList.add('hidden');
    seg.element.innerHTML = renderMarkdownHtml(seg.markdownRaw || '');
    void hydrateRenderedMarkdown(seg.element, true).then(() => scrollToBottomIfPinned());
    scrollToBottomIfPinned();
  }, 60);
}

async function resolveInlineMentionsToAttachments(text: string): Promise<void> {
  if (settings.pinLiveScreen || /@screen\b/i.test(text)) {
    const alreadyHasScreen = pendingAttachments.some(
      (a) => a.kind === 'image' && a.name.startsWith('screen'),
    );
    if (!alreadyHasScreen) {
      try {
        const shot = await window.agentOverlay.captureScreen('fullscreen');
        addAttachment(shot);
      } catch {
        // Ignore capture failure if blocked
      }
    }
  }
  if (/@clipboard\b/i.test(text)) {
    const clip = await window.agentOverlay.readClipboardContext();
    if (clip) addAttachment(clip);
  }
  const fileMatches = [...text.matchAll(/(?:^|\s)@([A-Za-z0-9_./-]+\.[A-Za-z0-9_-]+)/g)];
  for (const m of fileMatches) {
    const rel = m[1];
    addAttachment({
      id: `att_file_${rel}`,
      kind: 'file',
      name: basename(rel),
      path: rel,
    });
  }
}

async function submitPrompt(overridePrompt?: QueuedPrompt): Promise<void> {
  clipboardActionsBar.classList.add('hidden');
  const raw = overridePrompt ? overridePrompt.text.trim() : composerInput.value.trim();
  const hasDraft = Boolean(raw || (overridePrompt?.attachments.length ?? pendingAttachments.length) > 0);

  if (isStreaming && !overridePrompt) {
    if (!hasDraft) {
      await window.agentOverlay.cancelTurn();
      return;
    }
    const text = composerInput.value.trim();
    const atts = [...pendingAttachments];
    composerInput.value = '';
    pendingAttachments = [];
    renderAttachments();
    autoResizeComposer();
    popovers.close();

    if (settings.followUpBehavior === 'steer') {
      queuedFollowUp = { text: `[Steering]: ${text}`, attachments: atts, steer: true };
      renderQueuedBanner();
      await window.agentOverlay.cancelTurn();
    } else {
      queuedFollowUp = { text, attachments: atts, steer: false };
      renderQueuedBanner();
      updateSendButton();
    }
    return;
  }

  if (!hasDraft) return;

  if (!overridePrompt) {
    const localCmd = parseLocalCommand(raw);
    if (localCmd) {
      composerInput.value = '';
      autoResizeComposer();
      await executeLocalCommand(localCmd);
      return;
    }
    const osCmd = parseOsQuickCommand(raw);
    if (osCmd) {
      composerInput.value = '';
      autoResizeComposer();
      updateSendButton();
      const res = await window.agentOverlay.executeDesktopAction(osCmd, true);
      showNotice(res.message, !res.ok);
      return;
    }
  }

  isStreaming = true;
  followBottom = true;
  redoStack = [];
  btnScrollBottom.classList.add('hidden');
  updateSendButton();
  updateUndoRedoButtons();
  lastPromptUsedVoice = usedVoiceForDraft;
  usedVoiceForDraft = false;

  const expanded = overridePrompt ? raw : expandSlashTemplate(raw);
  if (!overridePrompt) {
    composerInput.value = '';
    autoResizeComposer();
  }
  popovers.close();

  try {
    setLiveStatus(true, 'warming', 'Preparing context...');
    if (!overridePrompt) {
      await resolveInlineMentionsToAttachments(expanded);
    }

    const atts = overridePrompt ? [...overridePrompt.attachments] : [...pendingAttachments];
    if (!overridePrompt) {
      pendingAttachments = [];
      renderAttachments();
    }

    if (turns.length === 0 && expanded) {
      currentSessionTitle = expanded.length > 28 ? `${expanded.slice(0, 28)}…` : expanded;
      sessionTitleChip.textContent = currentSessionTitle;
    }

    const targetDir = currentSessionWorkspaceDir || settings.workspaceDir;
    currentSessionWorkspaceDir = targetDir;

    let checkpointBefore: string | null = null;
    if (settings.revertMode === 'workspace_and_chat' && gitGlance.isRepo) {
      const ckpt = await window.agentOverlay.captureCheckpoint(targetDir);
      checkpointBefore = ckpt.treeSha;
    }

    let promptForAgent = expanded;
    if (needsContextReplay && turns.length > 0) {
      const summary = turns
        .slice(-6)
        .map(
          (t) =>
            `User: ${t.userText.slice(0, 300)}\nAssistant: ${t.assistantMarkdown.slice(0, 500)}`,
        )
        .join('\n---\n');
      promptForAgent = `[Prior conversation context before revert/resume]\n${summary}\n\n[Current user request]\n${expanded}`;
      needsContextReplay = false;
    }

    const turn: StoredTurn = {
      id: `turn_${Date.now()}`,
      userText: expanded,
      attachments: atts,
      checkpointBefore,
      thinkingText: '',
      thinkingDurationMs: 0,
      tools: [],
      assistantMarkdown: '',
    };
    turns.push(turn);
    setLiveStatus(true, 'thinking', 'Thinking...');

    activeTurn = buildTurnDom(turn, true);
    scrollToBottomIfPinned();

    await window.agentOverlay.sendPrompt(promptForAgent, atts);
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err);
    isStreaming = false;
    updateSendButton();
    updateUndoRedoButtons();
    setLiveStatus(false, 'idle', 'Ready');
    if (activeTurn) {
      activeTurn.turn.assistantMarkdown += `\n\n> [!CAUTION]\n> ${msg}\n`;
      activeTurn.markdownEl.innerHTML = renderMarkdownHtml(activeTurn.turn.assistantMarkdown);
      activeTurn.workingBadge.classList.add('hidden');
      activeTurn.skeletonEl.classList.add('hidden');
      activeTurn.footerEl.classList.remove('hidden');
      activeTurn = null;
    }
  }
}

async function executeLocalCommand(cmd: string): Promise<void> {
  if (cmd === 'new' || cmd === 'clear') {
    await startNewConversation();
  } else if (cmd === 'screen') {
    addAttachment(await window.agentOverlay.captureScreen('fullscreen'));
  } else if (cmd === 'region') {
    addAttachment(await window.agentOverlay.captureScreen('region'));
  } else if (cmd === 'window') {
    addAttachment(await window.agentOverlay.captureScreen('window'));
  } else if (cmd === 'markup') {
    const att = await window.agentOverlay.openMarkupCapture();
    if (att) addAttachment(att);
  } else if (cmd === 'help') {
    openHelpModal();
  }
}

async function startNewConversation(): Promise<void> {
  turns = [];
  undoStack = [];
  redoStack = [];
  pendingAttachments = [];
  currentSessionId = null;
  currentSessionIsRemote = false;
  currentSessionUpdatedAt = 0;
  currentSessionTitle = 'New chat';
  sessionTitleChip.textContent = 'New chat';
  currentSessionWorkspaceDir = settings?.workspaceDir || currentSessionWorkspaceDir;
  messagesView.innerHTML = '';
  messagesView.classList.add('hidden');
  welcomeView.classList.remove('hidden');
  renderAttachments();
  updateUndoRedoButtons();
  renderHeaderAndPickers();
  void refreshGitGlance();
  await window.agentOverlay.resetSession();
}

let desktopPanicToken = 0;

function describeDesktopAction(a: DesktopAction): string {
  if (a.kind === 'os') {
    const detail = a.target ? ` ${a.target}` : a.value !== undefined ? ` ${a.value}%` : '';
    return `OS: ${a.action || 'action'}${detail}`;
  }
  if (a.kind === 'type') return `Type "${(a.text || '').slice(0, 32)}"`;
  if (a.kind === 'key') return `Key ${a.combo || ''}`;
  if (a.kind === 'scroll') return `Scroll (${a.delta ?? -3})`;
  const coords = `(${Math.round(a.x ?? 50)}%, ${Math.round(a.y ?? 50)}%)`;
  return `${a.kind} ${coords}${a.label ? ` · ${a.label}` : ''}`;
}

function appendDesktopActionsCard(
  container: HTMLElement,
  actions: DesktopAction[],
  autoPreview: boolean,
): void {
  const epoch = desktopPanicToken;
  const card = document.createElement('div');
  card.className = 'desktop-actions-card';

  const head = document.createElement('div');
  head.className = 'desktop-actions-head';
  const title = document.createElement('div');
  title.className = 'desktop-actions-title';
  title.innerHTML = `<i class="codicon codicon-vm"></i><span>Desktop Actions (${actions.length})</span>`;

  const ctrls = document.createElement('div');
  ctrls.className = 'desktop-actions-controls';

  // Only 1 Run button needed; hidden by default when autoApproveDesktopActions is enabled
  const runBtn = document.createElement('button');
  runBtn.type = 'button';
  runBtn.className = 'mini-action-btn primary';
  runBtn.textContent = 'Run';

  const isAutoApproved = settings?.autoApproveDesktopActions !== false;
  if (isAutoApproved) {
    runBtn.classList.add('hidden');
  }

  ctrls.appendChild(runBtn);
  head.append(title, ctrls);
  card.appendChild(head);

  // List actions cleanly without individual buttons
  for (const act of actions) {
    const row = document.createElement('div');
    row.className = 'desktop-action-row';
    const desc = document.createElement('span');
    desc.className = 'desktop-action-desc';
    desc.textContent = describeDesktopAction(act);
    desc.title = desc.textContent;
    row.appendChild(desc);
    card.appendChild(row);
  }

  const runAllSequence = async () => {
    runBtn.disabled = true;
    runBtn.textContent = 'Running...';
    for (const act of actions) {
      if (desktopPanicToken !== epoch) break;
      await window.agentOverlay.executeDesktopAction(act, false);
      await new Promise((r) => setTimeout(r, 120));
    }
    runBtn.textContent = desktopPanicToken === epoch ? 'Finished ✓' : 'Stopped';
    title.innerHTML = `<i class="codicon codicon-vm"></i><span>Desktop Actions (${actions.length}) · ${desktopPanicToken === epoch ? 'Done ✓' : 'Stopped'}</span>`;
  };

  runBtn.addEventListener('click', () => void runAllSequence());
  container.appendChild(card);

  // If auto-approved, execute immediately
  if (isAutoApproved && !window.location.search.includes('audit=1')) {
    title.innerHTML = `<i class="codicon codicon-vm"></i><span>Desktop Actions (${actions.length}) · Executing...</span>`;
    window.setTimeout(() => {
      if (desktopPanicToken === epoch) {
        void runAllSequence();
      }
    }, 80);
  }
}

function handleStreamEvent(ev: StreamEvent): void {
  switch (ev.type) {
    case 'catalog':
      catalog = ev.catalog;
      if (ev.catalog.sessionId && (!currentSessionId || isStreaming)) {
        currentSessionId = ev.catalog.sessionId;
        currentSessionIsRemote = true;
      }
      renderHeaderAndPickers();
      break;
    case 'status':
      if (!isStreaming && ev.phase !== 'idle' && ev.phase !== 'connecting' && ev.phase !== 'warming') {
        break;
      }
      setLiveStatus(ev.phase !== 'idle', ev.phase, ev.label);
      break;
    case 'session_title':
      currentSessionTitle = ev.title;
      sessionTitleChip.textContent = ev.title;
      break;
    case 'thinking_delta': {
      if (!activeTurn) break;
      if (activeTurn.thinkingStartedAt === 0) {
        activeTurn.thinkingStartedAt = Date.now();
      }
      activeTurn.turn.thinkingText += ev.text;

      const msgId = ev.messageId;
      // If we have an active thinking segment with a different messageId, finalize it and start a new one!
      if (
        activeTurn.currentThinkingSegment &&
        msgId &&
        activeTurn.currentThinkingSegment.id &&
        activeTurn.currentThinkingSegment.id !== msgId
      ) {
        const dur = activeTurn.currentThinkingSegment.thoughtStartedAt
          ? Date.now() - activeTurn.currentThinkingSegment.thoughtStartedAt
          : 0;
        activeTurn.currentThinkingSegment.durationMs = dur;
        if (activeTurn.currentThinkingSegment.triggerText) {
          activeTurn.currentThinkingSegment.triggerText.textContent =
            dur > 0 ? `Thought for ${(dur / 1000).toFixed(1)}s` : 'Thought';
        }
        activeTurn.currentThinkingSegment = null;
      }

      if (!activeTurn.currentThinkingSegment) {
        // Start a fresh thinking disclosure in the flow container
        const disc = createThinkingDisclosureDom('Thinking...');
        activeTurn.flowContainer.appendChild(disc.root);
        const segDom: ActiveSegmentDom = {
          kind: 'thought',
          id: msgId,
          element: disc.root,
          triggerText: disc.triggerText,
          body: disc.body,
          scrollToBottomIfFollowing: disc.scrollToBottomIfFollowing,
          thoughtStartedAt: Date.now(),
          markdownRaw: ev.text,
        };
        disc.body.textContent = ev.text;
        disc.scrollToBottomIfFollowing();
        activeTurn.activeSegments.push(segDom);
        activeTurn.currentThinkingSegment = segDom;
        activeTurn.currentTextSegment = null;
        if (!activeTurn.turn.segments) activeTurn.turn.segments = [];
        activeTurn.turn.segments.push({ kind: 'thought', id: msgId, text: ev.text });
      } else {
        activeTurn.currentThinkingSegment.markdownRaw =
          (activeTurn.currentThinkingSegment.markdownRaw || '') + ev.text;
        activeTurn.currentThinkingSegment.body!.textContent =
          activeTurn.currentThinkingSegment.markdownRaw;
        activeTurn.currentThinkingSegment.scrollToBottomIfFollowing?.();
        const lastSeg = activeTurn.turn.segments?.[activeTurn.turn.segments.length - 1];
        if (lastSeg && lastSeg.kind === 'thought') {
          lastSeg.text = activeTurn.currentThinkingSegment.markdownRaw;
        }
      }
      scrollToBottomIfPinned();
      break;
    }
    case 'text_delta': {
      if (!activeTurn) break;
      if (activeTurn.currentThinkingSegment) {
        const dur = activeTurn.currentThinkingSegment.thoughtStartedAt
          ? Date.now() - activeTurn.currentThinkingSegment.thoughtStartedAt
          : 0;
        activeTurn.currentThinkingSegment.durationMs = dur;
        if (activeTurn.currentThinkingSegment.triggerText) {
          activeTurn.currentThinkingSegment.triggerText.textContent =
            dur > 0 ? `Thought for ${(dur / 1000).toFixed(1)}s` : 'Thought';
        }
        const lastSeg = activeTurn.turn.segments?.[activeTurn.turn.segments.length - 1];
        if (lastSeg && lastSeg.kind === 'thought') {
          lastSeg.durationMs = dur;
        }
        activeTurn.currentThinkingSegment = null;
      }
      activeTurn.turn.assistantMarkdown += ev.text;
      activeTurn.skeletonEl.classList.add('hidden');

      if (!activeTurn.currentTextSegment) {
        const mdEl = document.createElement('div');
        mdEl.className = 'assistant-markdown';
        activeTurn.flowContainer.appendChild(mdEl);
        const segDom: ActiveSegmentDom = {
          kind: 'text',
          element: mdEl,
          markdownRaw: ev.text,
        };
        activeTurn.activeSegments.push(segDom);
        activeTurn.currentTextSegment = segDom;
        activeTurn.markdownEl = mdEl;
        if (!activeTurn.turn.segments) activeTurn.turn.segments = [];
        activeTurn.turn.segments.push({ kind: 'text', text: ev.text, markdown: ev.text });
      } else {
        activeTurn.currentTextSegment.markdownRaw =
          (activeTurn.currentTextSegment.markdownRaw || '') + ev.text;
        const lastSeg = activeTurn.turn.segments?.[activeTurn.turn.segments.length - 1];
        if (lastSeg && lastSeg.kind === 'text') {
          lastSeg.text = activeTurn.currentTextSegment.markdownRaw;
          lastSeg.markdown = activeTurn.currentTextSegment.markdownRaw;
        }
      }
      scheduleMarkdownUpdateForSegment(activeTurn, activeTurn.currentTextSegment);
      break;
    }
    case 'tool_start': {
      if (!activeTurn) break;
      if (activeTurn.currentThinkingSegment) {
        const dur = activeTurn.currentThinkingSegment.thoughtStartedAt
          ? Date.now() - activeTurn.currentThinkingSegment.thoughtStartedAt
          : 0;
        activeTurn.currentThinkingSegment.durationMs = dur;
        if (activeTurn.currentThinkingSegment.triggerText) {
          activeTurn.currentThinkingSegment.triggerText.textContent =
            dur > 0 ? `Thought for ${(dur / 1000).toFixed(1)}s` : 'Thought';
        }
        const lastSeg = activeTurn.turn.segments?.[activeTurn.turn.segments.length - 1];
        if (lastSeg && lastSeg.kind === 'thought') {
          lastSeg.durationMs = dur;
        }
        activeTurn.currentThinkingSegment = null;
      }
      activeTurn.currentTextSegment = null;
      activeTurn.skeletonEl.classList.add('hidden');

      activeTurn.turn.tools.push({
        id: ev.id,
        name: ev.name,
        title: ev.title,
        args: ev.args,
        status: 'running',
        output: '',
      });
      if (!activeTurn.turn.segments) activeTurn.turn.segments = [];
      activeTurn.turn.segments.push({ kind: 'tool', id: ev.id, toolId: ev.id });

      appendToolCard(
        activeTurn.flowContainer,
        activeTurn.toolCardMap,
        ev.id,
        ev.name,
        ev.title,
        ev.args,
        'running',
      );
      scrollToBottomIfPinned();
      break;
    }
    case 'tool_update': {
      if (!activeTurn) break;
      const tModel = activeTurn.turn.tools.find((t) => t.id === ev.id);
      if (tModel) {
        tModel.status = ev.status;
        if (ev.title) tModel.title = ev.title;
        if (ev.args) tModel.args = ev.args;
        if (ev.output) tModel.output = ev.output;
        if (ev.durationMs) tModel.durationMs = ev.durationMs;
      }
      const dom = activeTurn.toolCardMap.get(ev.id);
      if (dom && tModel) {
        const presented = presentToolCall({
          name: tModel.name,
          title: tModel.title,
          args: tModel.args,
          output: tModel.output,
          status: tModel.status,
        });
        dom.kindIconEl.className = `codicon ${presented.icon} tool-card-kind-icon`;
        dom.verbEl.textContent = presented.verb;
        dom.summaryEl.textContent = presented.summary;
        dom.metaPillEl.className = `tool-card-meta-pill${presented.badge ? '' : ' hidden'}`;
        dom.metaPillEl.textContent = presented.badge || '';
        dom.statusEl.className =
          ev.status === 'running'
            ? 'codicon codicon-loading codicon-modifier-spin tool-status-running'
            : ev.status === 'completed'
              ? 'codicon codicon-check tool-status-ok'
              : 'codicon codicon-error tool-status-err';
        dom.detailEl.innerHTML = renderToolDetailHtml(presented);
        if (ev.durationMs) {
          dom.timeEl.textContent = `${(ev.durationMs / 1000).toFixed(1)}s`;
        }
      }
      break;
    }
    case 'permission_request': {
      if (!activeTurn) break;
      const req = ev.request;
      const card = document.createElement('div');
      card.className = 'permission-card';
      const pTitle = document.createElement('div');
      pTitle.className = 'permission-title';
      pTitle.textContent = req.toolTitle;
      card.appendChild(pTitle);
      if (req.detail) {
        const pDet = document.createElement('div');
        pDet.className = 'permission-detail';
        pDet.textContent = req.detail;
        card.appendChild(pDet);
      }
      const actions = document.createElement('div');
      actions.className = 'permission-actions';
      for (const opt of req.options) {
        const b = document.createElement('button');
        b.type = 'button';
        b.className = `perm-btn${opt.kind?.startsWith('allow') ? ' primary' : ''}`;
        b.textContent = opt.name;
        b.addEventListener('click', () => {
          actions.querySelectorAll('button').forEach((btn) => {
            btn.disabled = true;
          });
          b.textContent = `${opt.name} ✓`;
          void window.agentOverlay.respondPermission(req.requestId, opt.optionId);
        });
        actions.appendChild(b);
      }
      card.appendChild(actions);
      activeTurn.flowContainer.appendChild(card);
      if (settings.soundCues) playAttentionChime();
      void window.agentOverlay.flashAttention();
      void window.agentOverlay.notifyBackground(
        'Approval Needed',
        req.toolTitle || 'Agent requires tool permission',
      );
      scrollToBottomIfPinned();
      break;
    }
    case 'elicitation_request': {
      if (!activeTurn) break;
      const req = ev.request;
      const card = document.createElement('div');
      card.className = 'elicitation-card';

      const head = document.createElement('div');
      head.className = 'elicitation-header';
      head.innerHTML = '<i class="codicon codicon-question"></i><span>Agent Question</span>';

      const msg = document.createElement('div');
      msg.className = 'elicitation-message';
      msg.textContent = req.message;

      card.append(head, msg);

      const fieldsContainer = document.createElement('div');
      fieldsContainer.className = 'elicitation-fields';
      const fieldDataGetters: Record<string, () => unknown> = {};
      const disableInputs: Array<() => void> = [];

      const schemaProps = (req.schema || req.requestedSchema)?.properties;
      if (schemaProps && Object.keys(schemaProps).length > 0) {
        for (const [key, rawProp] of Object.entries(schemaProps)) {
          const prop = rawProp as ElicitationProperty;
          const field = document.createElement('div');
          field.className = 'elicitation-field';
          const label = document.createElement('label');
          label.className = 'elicitation-label';
          label.textContent = prop.title || prop.description || key;
          field.appendChild(label);

          const rawOptions =
            prop.oneOf ||
            prop.anyOf ||
            prop.items?.anyOf ||
            prop.items?.oneOf ||
            (prop.enum ? prop.enum.map((e) => ({ const: e, title: e })) : []);

          if (rawOptions && rawOptions.length > 0) {
            if (prop.type === 'array') {
              const checkGroup = document.createElement('div');
              checkGroup.className = 'elicitation-checkbox-group';
              for (const opt of rawOptions) {
                const optAny = opt as any;
                const val = String(optAny.const ?? optAny.value ?? '');
                const labelText = String(optAny.title ?? optAny.label ?? val);
                const row = document.createElement('label');
                row.className = 'toggle-row';
                row.style.cssText = 'font-size:12px;margin:2px 0;cursor:pointer;';
                const chk = document.createElement('input');
                chk.type = 'checkbox';
                chk.value = val;
                if (Array.isArray(prop.default) && prop.default.includes(val)) {
                  chk.checked = true;
                }
                const span = document.createElement('span');
                span.textContent = labelText;
                row.append(span, chk);
                checkGroup.appendChild(row);
                disableInputs.push(() => { chk.disabled = true; });
              }
              fieldDataGetters[key] = () => {
                const selected: string[] = [];
                checkGroup.querySelectorAll('input[type="checkbox"]').forEach((c: any) => {
                  if (c.checked) selected.push(c.value);
                });
                return selected;
              };
              field.appendChild(checkGroup);
            } else {
              const select = document.createElement('select');
              select.className = 'field-select';
              for (const opt of rawOptions) {
                const optAny = opt as any;
                const val = String(optAny.const ?? optAny.value ?? '');
                const labelText = String(optAny.title ?? optAny.label ?? val);
                const optEl = document.createElement('option');
                optEl.value = val;
                optEl.textContent = labelText;
                if (prop.default !== undefined && String(prop.default) === val) {
                  optEl.selected = true;
                }
                select.appendChild(optEl);
              }
              disableInputs.push(() => { select.disabled = true; });
              fieldDataGetters[key] = () => select.value;
              field.appendChild(select);
            }
          } else if (prop.type === 'boolean') {
            const row = document.createElement('label');
            row.className = 'toggle-row';
            const chk = document.createElement('input');
            chk.type = 'checkbox';
            if (prop.default === true) chk.checked = true;
            row.appendChild(chk);
            disableInputs.push(() => { chk.disabled = true; });
            fieldDataGetters[key] = () => chk.checked;
            field.appendChild(row);
          } else if (prop.type === 'number' || prop.type === 'integer') {
            const input = document.createElement('input');
            input.type = 'number';
            input.className = 'field-input';
            if (prop.minimum !== undefined) input.min = String(prop.minimum);
            if (prop.maximum !== undefined) input.max = String(prop.maximum);
            if (prop.default !== undefined) input.value = String(prop.default);
            disableInputs.push(() => { input.disabled = true; });
            fieldDataGetters[key] = () => (input.value ? Number(input.value) : undefined);
            field.appendChild(input);
          } else {
            const input = document.createElement('input');
            input.type = 'text';
            input.className = 'field-input';
            input.placeholder = prop.description || `Enter ${key}...`;
            if (prop.default !== undefined) input.value = String(prop.default);
            disableInputs.push(() => { input.disabled = true; });
            fieldDataGetters[key] = () => input.value.trim();
            field.appendChild(input);
          }
          fieldsContainer.appendChild(field);
        }
      } else {
        const field = document.createElement('div');
        field.className = 'elicitation-field';
        const input = document.createElement('input');
        input.type = 'text';
        input.className = 'field-input';
        input.placeholder = 'Type your response here...';
        disableInputs.push(() => { input.disabled = true; });
        fieldDataGetters['response'] = () => input.value.trim();
        field.appendChild(input);
        fieldsContainer.appendChild(field);
      }

      const actions = document.createElement('div');
      actions.className = 'elicitation-actions';

      const submitBtn = document.createElement('button');
      submitBtn.type = 'button';
      submitBtn.className = 'mini-action-btn primary';
      submitBtn.textContent = 'Submit Answer';

      const cancelBtn = document.createElement('button');
      cancelBtn.type = 'button';
      cancelBtn.className = 'mini-action-btn';
      cancelBtn.textContent = 'Decline';

      actions.append(submitBtn, cancelBtn);
      card.append(fieldsContainer, actions);

      const disableForm = (statusText: string) => {
        disableInputs.forEach((fn) => fn());
        submitBtn.disabled = true;
        cancelBtn.disabled = true;
        submitBtn.textContent = statusText;
      };

      submitBtn.addEventListener('click', () => {
        const data: Record<string, any> = {};
        for (const [k, getter] of Object.entries(fieldDataGetters)) {
          const val = getter();
          if (val !== undefined && val !== '') {
            data[k] = val;
          }
        }
        disableForm('Answered ✓');
        void window.agentOverlay.respondElicitation(req.requestId, 'accept', data);
      });

      cancelBtn.addEventListener('click', () => {
        disableForm('Declined');
        void window.agentOverlay.respondElicitation(req.requestId, 'cancel');
      });

      activeTurn.flowContainer.appendChild(card);
      if (settings.soundCues) playAttentionChime();
      void window.agentOverlay.flashAttention();
      void window.agentOverlay.notifyBackground('Question from Agent', req.message);
      scrollToBottomIfPinned();
      break;
    }
    case 'metrics': {
      if (!activeTurn) break;
      activeTurn.turn.metrics = {
        inputTokens: ev.inputTokens ?? activeTurn.turn.metrics?.inputTokens,
        outputTokens: ev.outputTokens ?? activeTurn.turn.metrics?.outputTokens,
        thoughtTokens: ev.thoughtTokens ?? activeTurn.turn.metrics?.thoughtTokens,
        cachedReadTokens: ev.cachedReadTokens ?? activeTurn.turn.metrics?.cachedReadTokens,
        totalTokens: ev.totalTokens ?? activeTurn.turn.metrics?.totalTokens,
        contextSize: ev.contextSize ?? activeTurn.turn.metrics?.contextSize,
        durationMs: ev.durationMs,
      };
      const formatted = formatTurnMetrics(activeTurn.turn.metrics);
      activeTurn.metricsEl.textContent = formatted.text;
      activeTurn.metricsEl.title = formatted.tooltip;
      updateContextBudget(activeTurn.turn.metrics);
      break;
    }
    case 'desktop_actions': {
      if (!activeTurn || ev.actions.length === 0) break;
      appendDesktopActionsCard(
        activeTurn.flowContainer,
        ev.actions,
        settings.desktopControlMode === 'preview_auto',
      );
      scrollToBottomIfPinned();
      break;
    }
    case 'error': {
      if (!activeTurn) break;
      activeTurn.turn.assistantMarkdown += `\n\n> [!CAUTION]\n> ${ev.message}\n`;
      if (!activeTurn.markdownEl.parentElement) {
        activeTurn.flowContainer.appendChild(activeTurn.markdownEl);
      }
      activeTurn.markdownEl.innerHTML = renderMarkdownHtml(activeTurn.turn.assistantMarkdown);
      break;
    }
    case 'done': {
      isStreaming = false;
      updateSendButton();
      updateUndoRedoButtons();
      setLiveStatus(false, 'idle', 'Ready');
      if (activeTurn) {
        if (activeTurn.renderScheduled !== null) {
          window.clearTimeout(activeTurn.renderScheduled);
          activeTurn.renderScheduled = null;
        }
        // Finalize any running tools or open permission cards on cancel/exit
        for (const tModel of activeTurn.turn.tools) {
          if (tModel.status === 'running') {
            tModel.status = 'failed';
            const dom = activeTurn.toolCardMap.get(tModel.id);
            if (dom) {
              dom.statusEl.className = 'codicon codicon-error tool-status-err';
            }
          }
        }
        activeTurn.flowContainer
          .querySelectorAll<HTMLButtonElement>('.permission-actions button, .elicitation-actions button')
          .forEach((btn) => {
            btn.disabled = true;
          });
        activeTurn.workingBadge.classList.add('hidden');
        activeTurn.skeletonEl.classList.add('hidden');
        activeTurn.footerEl.classList.remove('hidden');

        // Finalize all segments
        for (const seg of activeTurn.activeSegments) {
          if (seg.kind === 'text') {
            seg.element.innerHTML = renderMarkdownHtml(seg.markdownRaw || '');
            void hydrateRenderedMarkdown(seg.element, false);
          } else if (seg.kind === 'thought') {
            if (seg.thoughtStartedAt && !seg.durationMs) {
              const ms = Date.now() - seg.thoughtStartedAt;
              seg.durationMs = ms;
              if (seg.triggerText) {
                seg.triggerText.textContent = `Thought for ${(ms / 1000).toFixed(1)}s`;
              }
            }
          }
        }

        if (activeTurn.activeSegments.length === 0 && activeTurn.turn.assistantMarkdown) {
          if (!activeTurn.markdownEl.parentElement) {
            activeTurn.flowContainer.appendChild(activeTurn.markdownEl);
          }
          activeTurn.markdownEl.innerHTML = renderMarkdownHtml(
            activeTurn.turn.assistantMarkdown,
          );
          void hydrateRenderedMarkdown(activeTurn.markdownEl, false).then(() => {
            scrollToBottomIfPinned();
          });
        }

        btnClearMarkup.classList.toggle(
          'hidden',
          !/\[\[markup:/i.test(activeTurn.turn.assistantMarkdown),
        );
        if (!currentSessionId && catalog.sessionId) {
          currentSessionId = catalog.sessionId;
          currentSessionIsRemote = true;
        }
        if (currentSessionId && turns.length > 0) {
          const now = Date.now();
          currentSessionUpdatedAt = now;
          void window.agentOverlay.saveSession({
            id: currentSessionId,
            title: currentSessionTitle,
            harness: catalog.harness,
            workspaceDir: currentSessionWorkspaceDir || settings.workspaceDir,
            updatedAt: now,
            turns,
          });
        }
        activeTurn = null;
      }
      if (settings.soundCues && !window.location.search.includes('audit=1')) {
        playDoneChime();
      }
      if (!window.location.search.includes('audit=1')) {
        void window.agentOverlay.notifyBackground(
          currentSessionTitle || 'Response Ready',
          turns[turns.length - 1]?.assistantMarkdown?.slice(0, 120) || 'Turn completed.',
        );
      }
      const spokenTurn = turns[turns.length - 1];
      if (settings.autoSpeakReplies && lastPromptUsedVoice && spokenTurn?.assistantMarkdown) {
        lastPromptUsedVoice = false;
        const btns = messagesView.querySelectorAll<HTMLButtonElement>('.turn-footer-actions .icon-btn[title="Read reply aloud"]');
        const lastSpeakBtn = btns[btns.length - 1];
        if (lastSpeakBtn) {
          toggleSpeakTurn(spokenTurn.id, spokenTurn.assistantMarkdown, lastSpeakBtn);
        }
      } else {
        lastPromptUsedVoice = false;
      }
      void refreshGitGlance();
      if (queuedFollowUp) {
        const nextQueued = queuedFollowUp;
        queuedFollowUp = null;
        renderQueuedBanner();
        void submitPrompt(nextQueued);
      }
      break;
    }
  }
}

// Pickers
function openModelPicker(): void {
  const meta = HARNESS_LIST.find((h) => h.id === catalog.harness) ?? HARNESS_LIST[0];
  if (catalog.status === 'connecting' || catalog.status === 'warming') {
    popovers.openList({
      anchor: btnModelPicker,
      placement: 'above',
      headerText: `Models · ${meta.label}`,
      items: [
        {
          id: 'loading',
          title: `Fetching models from ${meta.shortLabel}...`,
          icon: 'codicon-loading',
          disabled: true,
          onSelect: () => {},
        },
      ],
    });
    return;
  }

  const items: PopoverEntry[] = catalog.models.map((m) => ({
    id: m.id,
    title: m.name,
    detail: m.name !== m.id ? m.id : undefined,
    selected: m.id === catalog.currentModel,
    onSelect: () => {
      void window.agentOverlay.setModel(m.id);
    },
  }));

  popovers.openList({
    anchor: btnModelPicker,
    placement: 'above',
    searchable: true,
    searchPlaceholder: `Search ${meta.shortLabel} models...`,
    headerText: `${meta.label} (${catalog.models.length})`,
    emptyText: catalog.errorDetail || 'No live models returned by this harness',
    items,
    minWidth: 290,
  });
}

function openEffortPicker(): void {
  if (!catalog.thinking.supported) return;
  const items: PopoverEntry[] = catalog.thinking.options.map((opt) => ({
    id: opt.value,
    title: opt.label,
    detail: opt.label.toLowerCase() !== opt.value.toLowerCase() ? opt.value : undefined,
    icon: 'codicon-sparkle',
    selected: opt.value === catalog.thinking.currentValue,
    onSelect: () => {
      void window.agentOverlay.setThinkingEffort(opt.value);
    },
  }));

  popovers.openList({
    anchor: btnEffortPicker,
    placement: 'above',
    headerText: `Thinking Effort (${catalog.thinking.configId || 'effort'})`,
    items,
  });
}

function openModePicker(): void {
  const items: PopoverEntry[] = catalog.modes.map((m) => ({
    id: m.id,
    title: m.name,
    detail: m.description,
    selected: m.id === catalog.currentMode,
    onSelect: () => {
      void window.agentOverlay.setMode(m.id);
    },
  }));
  popovers.openList({
    anchor: btnModePicker,
    placement: 'above',
    headerText: 'Agent Mode',
    items,
  });
}

function openHarnessMenu(): void {
  const items: PopoverEntry[] = HARNESS_LIST.map((h) => ({
    id: h.id,
    title: h.label,
    detail: `${h.defaultCommand} ${h.defaultArgs.join(' ')}`,
    icon: 'codicon-terminal',
    selected: h.id === catalog.harness,
    onSelect: () => {
      void window.agentOverlay.switchHarness(h.id as HarnessKind);
    },
  }));
  popovers.openList({
    anchor: btnHarness,
    placement: 'below',
    headerText: 'ACP Harness',
    items,
  });
}

function openAddContextMenu(): void {
  popovers.openList({
    anchor: btnAddContext,
    placement: 'above',
    headerText: 'Context & Capture',
    items: [
      {
        id: 'full',
        title: 'Capture Full Screen (@screen)',
        icon: 'codicon-device-camera',
        onSelect: () => void executeLocalCommand('screen'),
      },
      {
        id: 'markup',
        title: 'Annotate Screen (/markup)',
        icon: 'codicon-edit',
        onSelect: () => void executeLocalCommand('markup'),
      },
      {
        id: 'region',
        title: 'Capture Region (/region)',
        icon: 'codicon-symbol-structure',
        onSelect: () => void executeLocalCommand('region'),
      },
      {
        id: 'window',
        title: 'Capture Active Window (/window)',
        icon: 'codicon-window',
        onSelect: () => void executeLocalCommand('window'),
      },
      {
        id: 'file',
        title: 'Attach File or Image...',
        icon: 'codicon-file-add',
        onSelect: async () => {
          const att = await window.agentOverlay.pickFileToAttach();
          if (att) addAttachment(att);
        },
      },
      {
        id: 'clipboard',
        title: 'Attach Clipboard (Ctrl+Alt+Q)',
        icon: 'codicon-clippy',
        onSelect: async () => {
          const clip = await window.agentOverlay.readClipboardContext();
          if (clip) addAttachment(clip);
        },
      },
      {
        id: 'pin_toggle',
        title: settings.pinLiveScreen ? 'Unpin Live Screen' : 'Pin Live Screen on Send',
        icon: 'codicon-eye',
        onSelect: async () => {
          settings = await window.agentOverlay.updateSettings({
            pinLiveScreen: !settings.pinLiveScreen,
          });
          renderHeaderAndPickers();
        },
      },
    ],
  });
}

async function refreshAutocomplete(): Promise<void> {
  const caret = composerInput.selectionStart ?? composerInput.value.length;
  const before = composerInput.value.slice(0, caret);
  const trigger = extractActiveTrigger(before);

  if (!trigger) {
    if (autocompleteKind !== null) {
      autocompleteKind = null;
      popovers.close();
    }
    return;
  }

  autocompleteStart = trigger.start;

  if (trigger.kind === 'slash') {
    autocompleteKind = 'slash';
    const q = trigger.query.toLowerCase();
    const combined = [...LOCAL_COMMANDS, ...PROMPT_TEMPLATES, ...catalog.commands];
    const filtered = combined.filter(
      (c) =>
        !q ||
        c.name.toLowerCase().includes(q) ||
        c.description.toLowerCase().includes(q),
    );
    popovers.openAutocomplete({
      anchor: composerCard,
      items: filtered.slice(0, 14).map((cmd) => ({
        id: cmd.name,
        title: cmd.name,
        detail: cmd.description,
        icon: cmd.isLocal ? 'codicon-terminal' : 'codicon-symbol-event',
        onSelect: () => {
          const after = composerInput.value.slice(caret);
          const inserted = `${cmd.name} `;
          composerInput.value = `${composerInput.value.slice(0, autocompleteStart)}${inserted}${after}`;
          const pos = autocompleteStart + inserted.length;
          composerInput.setSelectionRange(pos, pos);
          composerInput.focus();
          autoResizeComposer();
          autocompleteKind = null;
        },
      })),
    });
    return;
  }

  autocompleteKind = 'mention';
  const matches = await window.agentOverlay.getMentions(trigger.query);
  if (matches.length === 0) {
    popovers.close();
    return;
  }
  popovers.openAutocomplete({
    anchor: composerCard,
    items: matches.slice(0, 14).map((m) => ({
      id: m.insert,
      title: m.label,
      detail: m.detail,
      icon: m.isContext ? 'codicon-sparkle' : 'codicon-file-code',
      onSelect: () => {
        const after = composerInput.value.slice(caret);
        const inserted = `${m.insert} `;
        composerInput.value = `${composerInput.value.slice(0, autocompleteStart)}${inserted}${after}`;
        const pos = autocompleteStart + inserted.length;
        composerInput.setSelectionRange(pos, pos);
        composerInput.focus();
        autoResizeComposer();
        autocompleteKind = null;
      },
    })),
  });
}

// Modals (Settings / History / Help)
function closeModal(): void {
  modalBackdrop.classList.add('hidden');
  modalBody.innerHTML = '';
}

function openDiffModal(initialFile?: string): void {
  modalTitle.textContent = 'Workspace Diff Review';
  modalBackdrop.classList.remove('hidden');
  void renderGitDiffDrawer({
    container: modalBody,
    workspaceDir: currentSessionWorkspaceDir || settings.workspaceDir,
    initialFile,
    onReviewFiles: (files) => {
      closeModal();
      composerInput.value = `Review these uncommitted workspace changes:\n${files.map((f) => `- @${f}`).join('\n')}\n\n`;
      autoResizeComposer();
      composerInput.focus();
      updateSendButton();
    },
    onRestored: async () => {
      await refreshGitGlance();
    },
    onNotice: (msg) => showNotice(msg, false),
  });
}

function openSettingsModal(): void {
  modalTitle.textContent = 'Settings';
  void renderTabbedSettingsDrawer({
    container: modalBody,
    settings,
    onUpdateSettings: async (patch) => {
      settings = await window.agentOverlay.updateSettings(patch);
      renderHeaderAndPickers();
      void refreshGitGlance();
      return settings;
    },
    onClose: closeModal,
  });
  modalBackdrop.classList.remove('hidden');
}

function getConnectedServerLabel(harnessId?: string): string {
  const hId = harnessId || catalog?.harness || settings?.harness;
  const match = HARNESS_LIST.find((h) => h.id === hId);
  return match ? match.label.replace(/\s*\(ACP\)$/i, '') : 'ACP Server';
}

let isRenamingHistorySession = false;

async function renderHistoryList(container: HTMLElement): Promise<void> {
  if (isRenamingHistorySession) return;
  const sessions = await window.agentOverlay.listSessions();
  container.innerHTML = '';

  if (sessions.length === 0) {
    const emptyNotice = document.createElement('div');
    emptyNotice.style.cssText = 'color:var(--text-muted);text-align:center;padding:24px 18px;';
    emptyNotice.textContent = 'No saved conversations yet.';
    container.appendChild(emptyNotice);
    return;
  }

  for (const s of sessions) {
    const row = document.createElement('div');
    row.className = `history-item${s.id === currentSessionId ? ' active' : ''}`;
    const info = document.createElement('div');
    info.style.cssText = 'flex:1;min-width:0;';
    const hTitle = document.createElement('div');
    hTitle.style.cssText =
      'font-weight:500;color:var(--text-strong);display:flex;align-items:center;gap:6px;';
    const serverName = getConnectedServerLabel(s.harness);
    const badge = s.isRemote
      ? `<span style="font-size:10px;padding:1px 5px;border-radius:4px;background:rgba(0,120,212,0.2);color:#7cc4fa;">${escapeHtmlText(serverName)} ACP</span>`
      : '';
    const titleSpan = document.createElement('span');
    titleSpan.style.cssText = 'overflow:hidden;text-overflow:ellipsis;white-space:nowrap;';
    titleSpan.textContent = s.title || 'Untitled Session';
    hTitle.appendChild(titleSpan);
    if (badge) {
      const badgeWrap = document.createElement('span');
      badgeWrap.innerHTML = badge;
      hTitle.appendChild(badgeWrap.firstElementChild!);
    }

    const hMeta = document.createElement('div');
    hMeta.style.cssText = 'font-size:11px;color:var(--text-muted);';
    const turnCount = s.turns?.length ?? 0;
    const countLabel =
      turnCount > 0
        ? `${turnCount} turn${turnCount === 1 ? '' : 's'}`
        : s.isRemote
          ? 'ACP session'
          : '0 turns';
    hMeta.textContent = `${serverName} · ${countLabel} · ${new Date(s.updatedAt).toLocaleString()}`;
    info.append(hTitle, hMeta);

    const actions = document.createElement('div');
    actions.className = 'history-item-actions';

    const renameBtn = document.createElement('button');
    renameBtn.type = 'button';
    renameBtn.className = 'icon-btn';
    renameBtn.title = 'Rename conversation';
    renameBtn.innerHTML = '<i class="codicon codicon-edit"></i>';
    renameBtn.addEventListener('click', (e) => {
      e.stopPropagation();
      if (isRenamingHistorySession) return;
      isRenamingHistorySession = true;
      const input = document.createElement('input');
      input.type = 'text';
      input.className = 'history-rename-input';
      input.value = s.title || 'Untitled Session';
      hTitle.innerHTML = '';
      hTitle.appendChild(input);
      input.focus();
      input.select();

      let finished = false;
      const finish = async (save: boolean) => {
        if (finished) return;
        finished = true;
        isRenamingHistorySession = false;
        const nextTitle = input.value.trim();
        if (save && nextTitle && nextTitle !== s.title) {
          await window.agentOverlay.renameSession(s.id, nextTitle);
          if (s.id === currentSessionId) {
            currentSessionTitle = nextTitle;
            sessionTitleChip.textContent = nextTitle;
          }
          showNotice('Session renamed');
        }
        await refreshHistoryModalList();
      };

      input.addEventListener('click', (ev) => ev.stopPropagation());
      input.addEventListener('keydown', (ev) => {
        ev.stopPropagation();
        if (ev.key === 'Enter') {
          ev.preventDefault();
          void finish(true);
        } else if (ev.key === 'Escape') {
          ev.preventDefault();
          void finish(false);
        }
      });
      input.addEventListener('blur', () => void finish(true));
    });

    const delBtn = document.createElement('button');
    delBtn.type = 'button';
    delBtn.className = 'icon-btn';
    delBtn.title = 'Delete conversation';
    delBtn.innerHTML = '<i class="codicon codicon-trash"></i>';
    delBtn.addEventListener('click', async (e) => {
      e.stopPropagation();
      const wasCurrent = s.id === currentSessionId;
      await window.agentOverlay.deleteSession(s.id);
      if (wasCurrent) {
        await startNewConversation();
      }
      await refreshHistoryModalList();
      showNotice('Session deleted');
    });

    actions.append(renameBtn, delBtn);

    row.addEventListener('click', async () => {
      if (isStreaming || isRenamingHistorySession) return;
      if (s.isRemote) {
        showNotice(`Syncing remote session from ${serverName}...`);
        const res = await window.agentOverlay.loadRemoteSession(s.id);
        if (!res.ok) {
          showNotice(`Failed to load remote ACP session: ${res.error || 'unknown'}`, true);
          return;
        }
        if (res.turns) {
          s.turns = res.turns;
        }
        needsContextReplay = false;
      } else {
        needsContextReplay = (s.turns?.length ?? 0) > 0;
      }
      currentSessionId = s.id;
      currentSessionIsRemote = Boolean(s.isRemote);
      currentSessionUpdatedAt = s.updatedAt;
      currentSessionTitle = s.title;
      currentSessionWorkspaceDir = s.workspaceDir || settings.workspaceDir;
      sessionTitleChip.textContent = s.title;
      turns = structuredClone(s.turns || []);
      undoStack = [];
      redoStack = [];
      rebuildMessagesList();
      renderHeaderAndPickers();
      void refreshGitGlance();
      closeModal();
    });

    row.append(info, actions);
    container.appendChild(row);
  }
}

async function refreshHistoryModalList(): Promise<void> {
  const isHistoryOpen =
    !modalBackdrop.classList.contains('hidden') &&
    modalTitle.textContent?.includes('Conversation History');
  if (!isHistoryOpen) return;
  const prevScroll = modalBody.scrollTop;
  await renderHistoryList(modalBody);
  modalBody.scrollTop = prevScroll;
}

async function openHistoryModal(): Promise<void> {
  modalTitle.textContent = 'Conversation History & ACP Sessions';
  modalBody.innerHTML = '';
  await renderHistoryList(modalBody);
  modalBackdrop.classList.remove('hidden');
}

function openHelpModal(): void {
  modalTitle.textContent = 'Shortcuts & Slash Commands';
  modalBody.innerHTML = `
    <div style="font-size:12.5px;display:flex;flex-direction:column;gap:8px;">
      <div><strong>Ctrl+.</strong> — Open searchable model picker</div>
      <div><strong>Ctrl+N</strong> — Start new conversation</div>
      <div><strong>Ctrl+Alt+L / Super+Shift+L</strong> — Toggle overlay visibility</div>
      <div><strong>@file / @screen / @markup / @clipboard</strong> — Inline context mentions</div>
      <div><strong>/screen, /markup, /region, /window, /new, /clear</strong> — Local actions</div>
      <div><strong>/explain, /refactor, /test, /fix, /review, /doc</strong> — Prompt templates</div>
    </div>
  `;
  modalBackdrop.classList.remove('hidden');
}

// Event Wiring
btnUndoChat.addEventListener('click', () => {
  void undoChatRevert();
});
btnRedoChat.addEventListener('click', () => {
  void redoChatRevert();
});
btnRevertMode.addEventListener('click', async () => {
  if (!gitGlance.isRepo) {
    showNotice('Current directory is not a Git repository — only Chat Only revert is available.', true);
    return;
  }
  const next = settings.revertMode === 'workspace_and_chat' ? 'chat_only' : 'workspace_and_chat';
  settings = await window.agentOverlay.updateSettings({ revertMode: next });
  renderHeaderAndPickers();
});
btnQueuedSteer.addEventListener('click', async () => {
  if (!queuedFollowUp) return;
  queuedFollowUp = { ...queuedFollowUp, text: `[Steering]: ${queuedFollowUp.text}`, steer: true };
  await window.agentOverlay.cancelTurn();
});
btnQueuedEdit.addEventListener('click', () => {
  if (!queuedFollowUp) return;
  composerInput.value = queuedFollowUp.text;
  pendingAttachments = [...queuedFollowUp.attachments];
  queuedFollowUp = null;
  renderQueuedBanner();
  renderAttachments();
  autoResizeComposer();
  updateSendButton();
  composerInput.focus();
});
btnQueuedCancel.addEventListener('click', () => {
  queuedFollowUp = null;
  renderQueuedBanner();
  updateSendButton();
});
document.getElementById('btn-new-chat')?.addEventListener('click', () => {
  void startNewConversation();
});
document.getElementById('btn-history')?.addEventListener('click', () => {
  void openHistoryModal();
});
document.getElementById('btn-settings')?.addEventListener('click', openSettingsModal);
document.getElementById('btn-app-menu')?.addEventListener('click', (e) => {
  const btn = e.currentTarget as HTMLElement;
  popovers.openList({
    anchor: btn,
    placement: 'below',
    headerText: 'Menu & Commands',
    minWidth: 230,
    items: [
      {
        id: 'new_chat',
        title: 'New Conversation',
        detail: 'Ctrl+N',
        icon: 'codicon-add',
        onSelect: () => {
          void startNewConversation();
        },
      },
      {
        id: 'sync_sessions',
        title: 'Sync ACP Sessions',
        detail: 'Fetch ACP server / external sessions',
        icon: 'codicon-sync',
        onSelect: () => {
          void openHistoryModal();
        },
      },
      {
        id: 'sync_current_session',
        title: `Sync Active Session with ${getConnectedServerLabel()}`,
        detail: 'Pull latest turns & reasoning blocks',
        icon: 'codicon-refresh',
        onSelect: async () => {
          if (isStreaming) {
            showNotice('Cannot sync while streaming', true);
            return;
          }
          if (!currentSessionId) {
            showNotice('No active session to sync', true);
            return;
          }
          if (!currentSessionIsRemote) {
            showNotice('Current session is local, not a remote ACP session', true);
            return;
          }
          const serverName = getConnectedServerLabel();
          showNotice(`Syncing session ${currentSessionId.slice(0, 8)}...`);
          const res = await window.agentOverlay.loadRemoteSession(currentSessionId);
          if (res.ok && res.turns) {
            turns = structuredClone(res.turns);
            rebuildMessagesList();
            showNotice(`Synced ${turns.length} turn(s) from ${serverName}`);
          } else {
            showNotice(res.error ? `Sync error: ${res.error}` : 'Session loaded');
          }
        },
      },
      {
        id: 'switch_harness',
        title: 'Switch ACP Harness',
        detail: catalog.harness,
        icon: 'codicon-plug',
        onSelect: () => {
          btnHarness.click();
        },
      },
      {
        id: 'list_windows',
        title: 'Inspect Open Windows',
        detail: 'List desktop GUI windows',
        icon: 'codicon-window',
        onSelect: async () => {
          const res = await window.agentOverlay.executeDesktopAction({ id: `act_win_${Date.now()}`, kind: 'list_windows' }, true);
          showNotice(res.message, !res.ok);
        },
      },
      {
        id: 'list_apps',
        title: 'Inspect Desktop Apps',
        detail: 'List installed system applications',
        icon: 'codicon-apps',
        onSelect: async () => {
          const res = await window.agentOverlay.executeDesktopAction({ id: `act_app_${Date.now()}`, kind: 'list_apps' }, true);
          showNotice(res.message, !res.ok);
        },
      },
      {
        id: 'capture_screen',
        title: 'Capture Screen Context',
        detail: '@screen',
        icon: 'codicon-screen-normal',
        onSelect: async () => {
          try {
            const shot = await window.agentOverlay.captureScreen('fullscreen');
            addAttachment(shot);
            showNotice('Screen context attached to prompt');
          } catch (e: any) {
            showNotice(`Screen capture failed: ${e?.message || 'unknown'}`, true);
          }
        },
      },
      {
        id: 'clear_markups',
        title: 'Clear Screen Markups',
        icon: 'codicon-paintcan',
        onSelect: () => {
          void window.agentOverlay.clearAgentMarkup();
          showNotice('Screen markups cleared');
        },
      },
      {
        id: 'settings',
        title: 'Preferences & Settings',
        detail: 'Ctrl+,',
        icon: 'codicon-settings-gear',
        onSelect: () => {
          openSettingsModal();
        },
      },
      {
        id: 'acp_docs',
        title: 'ACP Protocol Documentation',
        detail: 'https://agentclientprotocol.com',
        icon: 'codicon-book',
        onSelect: () => {
          void window.agentOverlay.openExternal('https://agentclientprotocol.com');
        },
      },
      {
        id: 'help',
        title: 'Shortcuts & Slash Commands',
        detail: 'Ctrl+/',
        icon: 'codicon-question',
        onSelect: () => {
          openHelpModal();
        },
      },
    ],
  });
});
document.getElementById('btn-close')?.addEventListener('click', () => {
  void window.agentOverlay.hideWindow();
});
document.getElementById('btn-modal-close')?.addEventListener('click', closeModal);
modalBackdrop.addEventListener('mousedown', (e) => {
  if (e.target === modalBackdrop) closeModal();
});

gitBranchChip.addEventListener('click', () => {
  if (!gitGlance.isRepo) return;
  popovers.openList({
    anchor: gitBranchChip,
    placement: 'below',
    headerText: `${gitGlance.branch} · ${gitGlance.changedFiles.length} changed`,
    items:
      gitGlance.changedFiles.length === 0
        ? [
            {
              id: 'clean',
              title: 'Working tree clean',
              icon: 'codicon-check',
              disabled: true,
              onSelect: () => {},
            },
          ]
        : [
            {
              id: 'open-diff-drawer',
              title: 'Inspect Unified Diffs',
              detail: `+${gitGlance.insertions} / -${gitGlance.deletions} across ${gitGlance.changedFiles.length} file(s)`,
              icon: 'codicon-diff',
              onSelect: () => openDiffModal(),
            },
            {
              id: 'review-diff',
              title: `Ask Agent to Review ${gitGlance.changedFiles.length} Files`,
              detail: 'Pre-fill composer with @file mentions',
              icon: 'codicon-git-compare',
              onSelect: () => {
                composerInput.value = `Review these uncommitted workspace changes:\n${gitGlance.changedFiles.map((f) => `- @${f}`).join('\n')}\n\n`;
                autoResizeComposer();
                composerInput.focus();
                updateSendButton();
              },
            },
            ...gitGlance.changedFiles.slice(0, 12).map((f) => ({
              id: `f_${f}`,
              title: basename(f),
              detail: `${f} · click to view diff`,
              icon: 'codicon-file-code',
              onSelect: () => openDiffModal(f),
            })),
          ],
  });
});

btnWorkspace.addEventListener('click', async () => {
  const chosen = await window.agentOverlay.pickWorkspaceFolder();
  if (chosen) {
    if (settings) settings.workspaceDir = chosen;
    currentSessionWorkspaceDir = chosen;
    renderHeaderAndPickers();
    void refreshGitGlance();
    showNotice(`Workspace folder: ${basename(chosen)}`);
  }
});
window.agentOverlay.onWorkspaceChanged?.((dir) => {
  if (dir) {
    if (settings) settings.workspaceDir = dir;
    currentSessionWorkspaceDir = dir;
    renderHeaderAndPickers();
    void refreshGitGlance();
  }
});
btnHarness.addEventListener('click', openHarnessMenu);
btnRetryWarmup.addEventListener('click', () => {
  void window.agentOverlay.resetSession();
});

btnAddContext.addEventListener('click', openAddContextMenu);
btnModePicker.addEventListener('click', openModePicker);
btnModelPicker.addEventListener('click', openModelPicker);
btnEffortPicker.addEventListener('click', openEffortPicker);

if (contextPreviewDialog) {
  // Light dismiss fallback for <dialog closedby="any">
  contextPreviewDialog.addEventListener('click', (e) => {
    if (e.target === contextPreviewDialog) {
      contextPreviewDialog.close();
    }
  });
  btnPreviewClose?.addEventListener('click', () => {
    contextPreviewDialog.close();
  });
  btnPreviewZoom?.addEventListener('click', () => {
    if (activePreviewZoomToggle) activePreviewZoomToggle();
  });
  btnPreviewCopy?.addEventListener('click', () => {
    if (activePreviewCopyHandler) void activePreviewCopyHandler();
  });
}

btnPinScreen.addEventListener('click', async () => {
  settings = await window.agentOverlay.updateSettings({
    pinLiveScreen: !settings.pinLiveScreen,
  });
  renderHeaderAndPickers();
});

btnClearMarkup.addEventListener('click', () => {
  void window.agentOverlay.clearAgentMarkup();
  btnClearMarkup.classList.add('hidden');
});

btnSend.addEventListener('click', () => {
  void submitPrompt();
});

document.querySelectorAll<HTMLButtonElement>('.starter-card').forEach((btn) => {
  btn.addEventListener('click', () => {
    const p = btn.dataset.prompt || '';
    composerInput.value = p;
    autoResizeComposer();
    composerInput.focus();
    if (!p.endsWith(' ')) {
      void submitPrompt();
    }
  });
});

// Markdown interactions (Copy code button, Mermaid toggle, external links)
messagesView.addEventListener('click', (e) => {
  const target = e.target as HTMLElement | null;
  if (!target) return;

  const copyBtn = target.closest<HTMLButtonElement>('.md-copy-code-btn');
  if (copyBtn) {
    const raw = copyBtn.dataset.code || '';
    void navigator.clipboard.writeText(raw);
    const label = copyBtn.querySelector('span');
    if (label) {
      label.textContent = 'Copied';
      setTimeout(() => {
        label.textContent = 'Copy';
      }, 1200);
    }
    return;
  }

  const wrapBtn = target.closest<HTMLButtonElement>('.md-wrap-code-btn');
  if (wrapBtn) {
    wrapBtn.closest('.md-code-card')?.classList.toggle('soft-wrap');
    return;
  }

  const expandBtn = target.closest<HTMLButtonElement>('.md-code-expand-btn');
  if (expandBtn) {
    const card = expandBtn.closest<HTMLElement>('.md-code-card');
    if (card) {
      const collapsed = card.classList.toggle('is-collapsed');
      expandBtn.textContent = collapsed
        ? `Show all ${(card.dataset.rawCode || '').split('\n').length} lines`
        : 'Collapse code';
    }
    return;
  }

  const toggleBtn = target.closest<HTMLButtonElement>('.md-mermaid-toggle');
  if (toggleBtn) {
    const card = toggleBtn.closest('.md-mermaid-card');
    if (card) {
      const preview = card.querySelector('.md-mermaid-preview');
      const raw = card.querySelector('.md-mermaid-raw');
      const showingSource = !raw?.classList.contains('hidden');
      preview?.classList.toggle('hidden', !showingSource);
      raw?.classList.toggle('hidden', showingSource);
      toggleBtn.textContent = showingSource ? 'Source' : 'Diagram';
    }
    return;
  }

  const link = target.closest<HTMLAnchorElement>('a[href]');
  if (link) {
    const href = link.getAttribute('href') || '';
    if (/^https?:\/\//i.test(href)) {
      e.preventDefault();
      void window.agentOverlay.openExternal(href);
    }
  }
});

composerInput.addEventListener('input', () => {
  autoResizeComposer();
  updateSendButton();
  void refreshAutocomplete();
});

composerInput.addEventListener('keydown', (e) => {
  if (autocompleteKind !== null && popovers.isOpen()) {
    if (popovers.handleAutocompleteKey(e)) {
      return;
    }
  }
  if (e.key === 'Enter' && !e.shiftKey) {
    e.preventDefault();
    void submitPrompt();
  }
});

async function triggerClipboardCompanionFlow(preFetched?: ChatAttachment | null): Promise<void> {
  const att = preFetched !== undefined ? preFetched : await window.agentOverlay.readClipboardContext(true);
  if (!att) {
    showNotice('Clipboard & selection are empty.', true);
    return;
  }
  addAttachment(att);
  clipboardActionsBar.classList.remove('hidden');
  composerInput.focus();
}

btnClipboardCompanion.addEventListener('click', () => {
  void triggerClipboardCompanionFlow();
});

btnClipClose.addEventListener('click', () => {
  clipboardActionsBar.classList.add('hidden');
});

clipboardActionsBar.querySelectorAll<HTMLButtonElement>('[data-clip-action]').forEach((btn) => {
  btn.addEventListener('click', (e) => {
    const action = btn.dataset.clipAction || 'Explain';
    const promptMap: Record<string, string> = {
      Explain: 'Explain the attached clipboard/screen content clearly and break down key details.',
      Summarize: 'Summarize the key points from the attached clipboard content concisely.',
      Translate: 'Translate the attached content cleanly into natural Vietnamese (or English if already Vietnamese).',
      Fix: 'Identify any errors, bugs, or issues in the attached content and provide a concrete fix.',
    };
    clipboardActionsBar.classList.add('hidden');
    const prompt = promptMap[action] || `${action} the attached content.`;
    if ((e as MouseEvent).shiftKey) {
      composerInput.value = prompt;
      autoResizeComposer();
      composerInput.focus();
      updateSendButton();
    } else {
      void submitPrompt({ text: prompt, attachments: [...pendingAttachments], steer: false });
    }
  });
});

// Drag & Drop (files, screenshots, URLs, or text snippets)
let dragCounter = 0;
appShell.addEventListener('dragenter', (e) => {
  e.preventDefault();
  dragCounter++;
  dropOverlay.classList.remove('hidden');
});
appShell.addEventListener('dragover', (e) => {
  e.preventDefault();
});
appShell.addEventListener('dragleave', () => {
  dragCounter = Math.max(0, dragCounter - 1);
  if (dragCounter === 0) dropOverlay.classList.add('hidden');
});
appShell.addEventListener('drop', (e) => {
  e.preventDefault();
  dragCounter = 0;
  dropOverlay.classList.add('hidden');
  const dt = e.dataTransfer;
  if (!dt) return;

  if (dt.files && dt.files.length > 0) {
    Array.from(dt.files).forEach((file) => {
      const filePath = window.agentOverlay.getFilePath(file);
      if (file.type.startsWith('image/')) {
        const reader = new FileReader();
        reader.onload = () => {
          if (typeof reader.result === 'string') {
            addAttachment({
              id: `att_drop_${Date.now()}_${Math.random().toString(36).slice(2, 6)}`,
              kind: 'image',
              name: file.name || 'dropped-image.png',
              mimeType: file.type,
              dataUrl: reader.result,
            });
          }
        };
        reader.readAsDataURL(file);
      } else if (filePath) {
        addAttachment({
          id: `att_drop_${Date.now()}_${Math.random().toString(36).slice(2, 6)}`,
          kind: 'file',
          name: file.name || basename(filePath),
          path: filePath,
        });
      }
    });
    return;
  }

  const droppedUri = dt.getData('text/uri-list')?.trim();
  const droppedText = dt.getData('text/plain')?.trim();
  const textPayload = droppedUri || droppedText;
  if (textPayload) {
    addAttachment({
      id: `att_drop_txt_${Date.now()}`,
      kind: 'text',
      name: /^https?:\/\//i.test(textPayload) ? 'dropped-url.txt' : 'dropped-text.txt',
      content: textPayload,
    });
    composerInput.focus();
  }
});

// Global overlay keyboard shortcuts
window.addEventListener('keydown', (e) => {
  if ((e.ctrlKey || e.metaKey) && e.shiftKey && e.key.toLowerCase() === 'z') {
    e.preventDefault();
    void undoChatRevert();
    return;
  }
  if ((e.ctrlKey || e.metaKey) && e.shiftKey && e.key.toLowerCase() === 'y') {
    e.preventDefault();
    void redoChatRevert();
    return;
  }
  if ((e.ctrlKey || e.metaKey) && e.key === '.') {
    e.preventDefault();
    openModelPicker();
    return;
  }
  if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'n') {
    e.preventDefault();
    void startNewConversation();
    return;
  }
  if ((e.ctrlKey || e.metaKey) && e.altKey && e.key.toLowerCase() === 'g') {
    e.preventDefault();
    void window.agentOverlay.toggleGhostMode();
    return;
  }
  if ((e.ctrlKey || e.metaKey) && e.altKey && e.key.toLowerCase() === 'q') {
    e.preventDefault();
    void triggerClipboardCompanionFlow();
    return;
  }
  if (
    ((e.ctrlKey || e.metaKey) && e.altKey && e.key.toLowerCase() === 'l') ||
    (e.metaKey && e.shiftKey && e.key.toLowerCase() === 'l')
  ) {
    e.preventDefault();
    void window.agentOverlay.hideWindow();
    return;
  }
  if (e.key === 'Escape') {
    if (isRecordingSpeech) {
      cancelSpeechRecording();
      return;
    }
    if (isTranscribingSpeech) {
      cancelSpeechTranscription();
      return;
    }
    if (!modalBackdrop.classList.contains('hidden')) {
      closeModal();
    } else if (popovers.isOpen()) {
      popovers.close();
    } else if (!isStreaming) {
      void window.agentOverlay.hideWindow();
    }
  }
});

async function runAuditFlow(): Promise<void> {
  // Wait briefly for ACP warmup to complete (real agy acp catalog!)
  for (let i = 0; i < 40; i++) {
    if (catalog.status === 'ready' || catalog.status === 'error') break;
    await new Promise((r) => setTimeout(r, 200));
  }
  await new Promise((r) => setTimeout(r, 250));
  await window.agentOverlay.auditCapture('01-welcome.png');

  openModelPicker();
  await new Promise((r) => setTimeout(r, 200));
  await window.agentOverlay.auditCapture('02-model-picker.png');
  const modelSearch = popoverLayer.querySelector<HTMLInputElement>('.popover-search');
  if (modelSearch) {
    modelSearch.value = 'sol';
    modelSearch.dispatchEvent(new Event('input', { bubbles: true }));
    await new Promise((r) => setTimeout(r, 120));
    await window.agentOverlay.auditCapture('02b-model-picker-filtered.png');
  }
  popovers.close();

  composerInput.value = 'Inspect @scr';
  composerInput.setSelectionRange(composerInput.value.length, composerInput.value.length);
  await refreshAutocomplete();
  await new Promise((r) => setTimeout(r, 200));
  await window.agentOverlay.auditCapture('03-mention-popup.png');
  popovers.close();
  composerInput.value = '';

  const stressMarkdown = [
    '# Advanced Markdown Rendering Test',
    '',
    '## 1. Nested Blockquotes & Callouts',
    '',
    '> Level 1 blockquote',
    '>> Level 2 nested blockquote',
    '>>> Level 3 deepest blockquote',
    '',
    '> [!NOTE]',
    '> Live ACP catalog synced with effort levels.',
    '',
    '> [!TIP]',
    '> Press **Ctrl+.** with `focus` in the composer to search models.',
    '',
    '> [!WARNING]',
    '> Verify permissions before running destructive commands.',
    '',
    '## 2. Nested Lists & Task Lists',
    '',
    '1. First ordered item',
    '   - Nested bullet inside ordered',
    '   - Another nested bullet',
    '2. Second ordered item',
    '',
    '- [x] Top-level completed',
    '- [x] Nested completed',
    '- [ ] Pending task item',
    '',
    '## 3. Tables & Code Blocks',
    '',
    '| Left | Center | Right |',
    '|:-----|:------:|------:|',
    '| `code` | **bold** | 123 |',
    '| [Electron](https://electronjs.org) | $x = 1$ | ok |',
    '',
    '```python',
    'def fibonacci(n: int) -> list[int]:',
    '    a, b = 0, 1',
    '    return [a := (b, b := a + b)[0] for _ in range(n)]',
    '```',
    '',
    '## 4. Complex Math (KaTeX)',
    '',
    'Inline: $a^2 + b^2 = c^2$ and $E = mc^2$.',
    '',
    '$$\\int_{a}^{b} f(x)\\,dx = F(b) - F(a)$$',
    '',
    '$$\\begin{pmatrix} a & b \\\\ c & d \\end{pmatrix}$$',
    '',
    '## 5. Mermaid Diagrams',
    '',
    '```mermaid',
    'graph TD',
    '    A[Overlay UI] -->|JSON-RPC| B[ACP Controller]',
    '    B --> C[agy acp]',
    '    B --> D[opencode acp]',
    '```',
    '',
    '```mermaid',
    'sequenceDiagram',
    '    User->>UI: Send @screen prompt',
    '    UI->>Agent: session/prompt + image',
    '    Agent-->>UI: agent_message_chunk',
    '```',
    '',
    '## 6. Collapsible Details',
    '',
    '<details><summary>Click to expand details</summary>',
    '',
    'Hidden **Markdown**, `code`, and metrics inside native details.',
    '',
    '</details>',
  ].join('\n');

  const sampleTurn: StoredTurn = {
    id: 'audit_turn_1',
    userText: 'Inspect repo and verify visual tool cards + expanded thinking without scrollbar',
    attachments: [{ id: 'a1', kind: 'image', name: 'screen-preview.png' }],
    checkpointBefore: 'deadbee',
    thinkingText:
      'Line 1 of thinking trace: checking repository structure.\n' +
      'Line 2: verifying nested blockquotes and callouts.\n' +
      'Line 3: checking tool output formatting so users see clean visual cards instead of raw JSON.\n' +
      'Line 4: verifying thinking disclosure expands naturally with zero inner scrollbar.\n' +
      'Line 5: testing auto-follow scroll on thinking expansion.',
    thinkingDurationMs: 2400,
    tools: [
      {
        id: 't1',
        name: 'shell',
        title: 'Explore repo status',
        args: JSON.stringify({ command: 'git status -sb', cwd: settings?.workspaceDir || '.' }),
        status: 'completed',
        output: JSON.stringify({
          metadata: { status: 'completed', truncated: false, exit: 0 },
          output: '## feat/electron-universal\n M app/src/renderer/app.ts\n?? app/src/main/checkpoint.ts',
        }),
        durationMs: 120,
      },
      {
        id: 't2',
        name: 'glob',
        title: 'Find source files',
        args: JSON.stringify({ pattern: 'app/src/**/*.ts' }),
        status: 'completed',
        output: 'app/src/main/acp-client.ts\napp/src/main/checkpoint.ts\napp/src/renderer/tool-presenter.ts',
        durationMs: 45,
      },
    ],
    assistantMarkdown: stressMarkdown,
    metrics: {
      inputTokens: 6678,
      outputTokens: 320,
      cachedReadTokens: 1280,
      thoughtTokens: 410,
      totalTokens: 8688,
      contextSize: 1000000,
      durationMs: 2150,
    },
  };

  turns = [sampleTurn];
  messagesView.innerHTML = '';
  const refs = buildTurnDom(sampleTurn, false);
  // Expand Thinking + Shell tool card to verify no scrollbar, no orange border, visual tool UI!
  const thinkTrigger = refs.thinkingDisclosure.querySelector<HTMLButtonElement>('.disclosure-trigger');
  thinkTrigger?.focus();
  thinkTrigger?.click();
  const firstToolBtn = refs.toolsContainer.querySelector<HTMLButtonElement>('.tool-card-header');
  firstToolBtn?.click();
  await hydrateRenderedMarkdown(refs.markdownEl, false);
  await new Promise((r) => setTimeout(r, 450));

  chatViewport.scrollTo({ top: 0, behavior: 'instant' });
  await window.agentOverlay.auditCapture('04-visual-tools-thinking.png');

  const headings = refs.markdownEl.querySelectorAll<HTMLElement>('h2');
  const codeHeading = headings[2];
  if (codeHeading) {
    chatViewport.scrollTo({ top: codeHeading.offsetTop - 8, behavior: 'instant' });
    await window.agentOverlay.auditCapture('05-code-math.png');
  }

  const mermaidHeading = headings[4];
  const detailsEl = refs.markdownEl.querySelector('details');
  if (detailsEl) detailsEl.open = true;
  if (mermaidHeading) {
    chatViewport.scrollTo({ top: mermaidHeading.offsetTop - 8, behavior: 'instant' });
    await window.agentOverlay.auditCapture('06-mermaid-details.png');
  }

  // Test switching live to OpenCode (ACP) + selecting a reasoning model to inspect dynamic effort
  catalog = await window.agentOverlay.switchHarness('opencode_acp');
  renderHeaderAndPickers();
  if (catalog.models.some((m) => m.id === 'agentrouter/gpt-6.1-sol')) {
    catalog = await window.agentOverlay.setModel('agentrouter/gpt-6.1-sol');
    renderHeaderAndPickers();
  }
  await new Promise((r) => setTimeout(r, 200));
  openEffortPicker();
  await window.agentOverlay.auditCapture('07-opencode-effort.png');
  popovers.close();

  // Capture live working indicator + shimmer skeleton state
  const workingTurn: StoredTurn = {
    id: 'audit_working_1',
    userText: 'Analyze screen layout and explain ACP effort sync',
    attachments: [],
    thinkingText: '',
    thinkingDurationMs: 0,
    tools: [
      {
        id: 'tw1',
        name: 'read_file',
        title: 'Reading src/main/acp-parser.ts',
        args: '{"path":"src/main/acp-parser.ts"}',
        status: 'running',
        output: '',
      },
    ],
    assistantMarkdown: '',
  };
  isStreaming = true;
  queuedFollowUp = { text: 'Also check edge cases in tests/core.test.ts', attachments: [], steer: false };
  renderQueuedBanner();
  composerInput.value = 'Draft message';
  updateSendButton();
  setLiveStatus(true, 'tool', 'Running read_file...');
  buildTurnDom(workingTurn, true);
  chatViewport.scrollTo({ top: chatViewport.scrollHeight, behavior: 'instant' });
  await window.agentOverlay.auditCapture('08-working-state.png');

  queuedFollowUp = null;
  composerInput.value = '';
  renderQueuedBanner();
  isStreaming = false;
  updateSendButton();
  openSettingsModal();
  await new Promise((r) => setTimeout(r, 250));
  await window.agentOverlay.auditCapture('09-settings-drawer.png');
  closeModal();

  await window.agentOverlay.auditDone();
}

const btnMic = document.getElementById('btn-mic') as HTMLButtonElement | null;
let mediaStream: MediaStream | null = null;
let mediaRecorder: MediaRecorder | null = null;
let pcmRecorder: PcmWavRecorder | null = null;
let audioChunks: Blob[] = [];
let isRecordingSpeech = false;
let isTranscribingSpeech = false;
let speechAutoStopTimer: ReturnType<typeof setTimeout> | null = null;
let speechInterimInterval: ReturnType<typeof setInterval> | null = null;
let isInterimTranscribing = false;
let browserSpeechRecognition: any = null;
let browserRecognizedTranscript = '';

function cleanupMediaStream(): void {
  if (pcmRecorder) {
    pcmRecorder.cancel();
    pcmRecorder = null;
  }
  if (mediaStream) {
    try {
      for (const track of mediaStream.getTracks()) {
        track.stop();
      }
    } catch {
      // ignore
    }
    mediaStream = null;
  }
  mediaRecorder = null;
}

function cancelSpeechRecording(): void {
  if (speechAutoStopTimer) {
    clearTimeout(speechAutoStopTimer);
    speechAutoStopTimer = null;
  }
  if (speechInterimInterval) {
    clearInterval(speechInterimInterval);
    speechInterimInterval = null;
  }
  isInterimTranscribing = false;
  if (browserSpeechRecognition) {
    try {
      browserSpeechRecognition.abort();
    } catch {}
    browserSpeechRecognition = null;
  }
  composerInput.value = speechBaseText;
  composerInput.setSelectionRange(speechCursorStart, speechCursorStart);
  autoResizeComposer();
  updateSendButton();
  browserRecognizedTranscript = '';
  speechLiveCommitted = '';
  speechLiveInterim = '';
  isRecordingSpeech = false;
  btnMic?.classList.remove('recording');
  btnMic?.setAttribute('title', 'Voice Input (Speech-to-Text)');
  if (mediaRecorder && mediaRecorder.state !== 'inactive') {
    try {
      mediaRecorder.stop();
    } catch {
      // ignore
    }
  }
  audioChunks = [];
  cleanupMediaStream();
  showNotice('Voice input cancelled.');
}

function cancelSpeechTranscription(): void {
  isTranscribingSpeech = false;
  btnMic?.classList.remove('transcribing');
  btnMic?.setAttribute('title', 'Voice Input (Speech-to-Text)');
  showNotice('Voice transcription cancelled.');
}

let speechBaseText = '';
let speechCursorStart = 0;
let speechLiveCommitted = '';
let speechLiveInterim = '';

function updateLiveSpeechDisplay(committed: string, interim: string): void {
  const before = speechBaseText.slice(0, speechCursorStart);
  const after = speechBaseText.slice(speechCursorStart);
  const needsLeadingSpace = before.length > 0 && !before.endsWith(' ') && !before.endsWith('\n');
  const leading = needsLeadingSpace ? ' ' : '';
  const spoken = (committed + (interim ? (committed ? ' ' : '') + interim : '')).trim();
  if (!spoken) return;
  const fullInsertion = leading + spoken;
  composerInput.value = before + fullInsertion + after;
  const newCursor = before.length + fullInsertion.length;
  composerInput.setSelectionRange(newCursor, newCursor);
  autoResizeComposer();
  updateSendButton();
  composerInput.scrollTop = composerInput.scrollHeight;
}

function insertTranscribedText(text: string): void {
  const current = composerInput.value;
  const start = composerInput.selectionStart ?? current.length;
  const end = composerInput.selectionEnd ?? current.length;

  const before = current.slice(0, start);
  const after = current.slice(end);

  const needsLeadingSpace = before.length > 0 && !before.endsWith(' ') && !before.endsWith('\n');
  const leading = needsLeadingSpace ? ' ' : '';
  const insertion = leading + text;

  composerInput.value = before + insertion + after;
  const newCursor = start + insertion.length;
  composerInput.setSelectionRange(newCursor, newCursor);
  autoResizeComposer();
  updateSendButton();
  composerInput.dispatchEvent(new Event('input', { bubbles: true }));
  composerInput.focus();
}

function blobToBase64(blob: Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onloadend = () => {
      const res = reader.result as string;
      const commaIdx = res.indexOf(',');
      resolve(commaIdx >= 0 ? res.slice(commaIdx + 1) : res);
    };
    reader.onerror = () => reject(reader.error || new Error('Failed to read audio blob'));
    reader.readAsDataURL(blob);
  });
}

function openVoiceMethodPickerModal(st: SpeechStatus, autoRecordAfterSetup = false): void {
  modalTitle.textContent = 'Choose Voice Input Method';
  modalBody.innerHTML = '';

  const desc = document.createElement('div');
  desc.className = 'voice-setup-desc';
  desc.textContent =
    'Select how speech-to-text is processed. Cloud APIs require zero local storage; the built-in offline Whisper runtime is completely optional.';

  const list = document.createElement('div');
  list.className = 'voice-model-list';

  // Option 1: Cloud API (primary lightweight choice)
  const cloudCard = document.createElement('div');
  cloudCard.className = 'voice-method-card';
  const cloudTitle = document.createElement('div');
  cloudTitle.className = 'voice-method-header';
  cloudTitle.innerHTML =
    '<strong>Cloud Speech API</strong><span class="voice-badge">Zero Local Setup</span>';
  const cloudSub = document.createElement('div');
  cloudSub.className = 'voice-method-sub';
  cloudSub.textContent = st.hasCloudKey
    ? `Configured: ${[st.cloudProviders?.gemini ? 'Gemini' : '', st.cloudProviders?.openai ? 'OpenAI' : ''].filter(Boolean).join(', ')}`
    : 'Fast multilingual transcription via Gemini or OpenAI Whisper API.';

  const cloudForm = document.createElement('div');
  cloudForm.className = 'voice-inline-form';
  const providerSelect = document.createElement('select');
  providerSelect.className = 'field-select';
  providerSelect.innerHTML =
    '<option value="gemini">Google Gemini API</option><option value="openai">OpenAI Whisper API</option>';
  const keyInput = document.createElement('input');
  keyInput.type = 'password';
  keyInput.className = 'field-input';
  keyInput.placeholder = 'Paste API key (stored in voice.json)';
  const cloudMsg = document.createElement('div');
  cloudMsg.className = 'field-hint';
  cloudMsg.style.cssText = 'font-size:11px;min-height:14px;';

  const cloudBtns = document.createElement('div');
  cloudBtns.className = 'voice-crud-actions';
  const saveCloudBtn = document.createElement('button');
  saveCloudBtn.type = 'button';
  saveCloudBtn.className = 'mini-action-btn primary';
  saveCloudBtn.textContent = 'Save & Use Cloud';
  cloudBtns.appendChild(saveCloudBtn);

  if (st.hasCloudKey) {
    const useCloudBtn = document.createElement('button');
    useCloudBtn.type = 'button';
    useCloudBtn.className = 'mini-action-btn';
    useCloudBtn.textContent = 'Use Existing Key';
    useCloudBtn.addEventListener('click', async () => {
      settings = await window.agentOverlay.updateSettings({ speechMode: 'cloud', speechConfigured: true });
      closeModal();
      showNotice('Voice input set to Cloud API.');
      if (autoRecordAfterSetup) await startSpeechRecording();
    });

    const clearCloudBtn = document.createElement('button');
    clearCloudBtn.type = 'button';
    clearCloudBtn.className = 'mini-action-btn danger';
    clearCloudBtn.textContent = 'Clear Key';
    clearCloudBtn.addEventListener('click', async () => {
      await window.agentOverlay.clearCloudSpeechKey(providerSelect.value as 'gemini' | 'openai');
      const nextSt = await window.agentOverlay.getSpeechStatus();
      openVoiceMethodPickerModal(nextSt, autoRecordAfterSetup);
    });
    cloudBtns.append(useCloudBtn, clearCloudBtn);
  }

  saveCloudBtn.addEventListener('click', async () => {
    const res = await window.agentOverlay.saveCloudSpeechKey(
      providerSelect.value as 'gemini' | 'openai',
      keyInput.value,
    );
    if (!res.ok) {
      cloudMsg.textContent = res.error || 'Failed to save API key.';
      cloudMsg.style.color = '#f87171';
      return;
    }
    settings = await window.agentOverlay.updateSettings({ speechMode: 'cloud', speechConfigured: true });
    closeModal();
    showNotice('Cloud speech API key saved.');
    if (autoRecordAfterSetup) await startSpeechRecording();
  });

  cloudForm.append(providerSelect, keyInput, cloudBtns, cloudMsg);
  cloudCard.append(cloudTitle, cloudSub, cloudForm);

  // Option 2: External Local Server or CLI
  const extCard = document.createElement('div');
  extCard.className = 'voice-method-card';
  const extTitle = document.createElement('div');
  extTitle.className = 'voice-method-header';
  extTitle.innerHTML =
    '<strong>External Local Server or CLI</strong><span class="voice-badge">Bring Your Own</span>';
  const extSub = document.createElement('div');
  extSub.className = 'voice-method-sub';
  extSub.textContent = 'Connect your own local HTTP transcription server or CLI tool.';

  const extForm = document.createElement('div');
  extForm.className = 'voice-inline-form';
  const urlInput = document.createElement('input');
  urlInput.type = 'text';
  urlInput.className = 'field-input';
  urlInput.placeholder = 'HTTP URL, e.g. http://127.0.0.1:8080/inference';
  urlInput.value = settings?.localSpeechUrl || '';
  const cmdInput = document.createElement('input');
  cmdInput.type = 'text';
  cmdInput.className = 'field-input';
  cmdInput.placeholder = 'Or CLI command, e.g. whisper-cli {file}';
  cmdInput.value = settings?.localSpeechCommand || '';
  const extMsg = document.createElement('div');
  extMsg.className = 'field-hint';
  extMsg.style.cssText = 'font-size:11px;min-height:14px;';
  const extBtns = document.createElement('div');
  extBtns.className = 'voice-crud-actions';
  const saveExtBtn = document.createElement('button');
  saveExtBtn.type = 'button';
  saveExtBtn.className = 'mini-action-btn primary';
  saveExtBtn.textContent = 'Save & Use External';
  saveExtBtn.addEventListener('click', async () => {
    const localUrl = urlInput.value.trim();
    const localCmd = cmdInput.value.trim();
    if (!localUrl && !localCmd) {
      extMsg.textContent = 'Enter a server URL or a CLI command.';
      extMsg.style.color = '#f87171';
      return;
    }
    settings = await window.agentOverlay.updateSettings({
      speechMode: 'local',
      localSpeechUrl: localUrl,
      localSpeechCommand: localCmd,
      speechConfigured: true,
    });
    closeModal();
    showNotice('External local STT backend configured.');
    if (autoRecordAfterSetup) await startSpeechRecording();
  });
  extBtns.appendChild(saveExtBtn);
  extForm.append(urlInput, cmdInput, extBtns, extMsg);
  extCard.append(extTitle, extSub, extForm);

  // Option 3: Optional Built-in Offline Runtime (last card)
  const offlineCard = document.createElement('div');
  offlineCard.className = 'voice-method-card';
  const offlineTitle = document.createElement('div');
  offlineTitle.className = 'voice-method-header';
  offlineTitle.innerHTML =
    '<strong>Built-in Offline Whisper Runtime</strong><span class="voice-badge">Optional</span>';
  const offlineSub = document.createElement('div');
  offlineSub.className = 'voice-method-sub';
  offlineSub.textContent = st.hasLocalModel
    ? `Active model ready (${st.activeModelId || 'installed'}). Manage models or runtime.`
    : 'Optionally download whisper-server runtime & Whisper models to user data (~190–574 MB).';
  const offlineBtnRow = document.createElement('div');
  offlineBtnRow.className = 'voice-crud-actions';
  const manageOfflineBtn = document.createElement('button');
  manageOfflineBtn.type = 'button';
  manageOfflineBtn.className = 'mini-action-btn';
  manageOfflineBtn.textContent = 'Manage Models & Optional Runtime...';
  manageOfflineBtn.addEventListener('click', async () => {
    const latest = await window.agentOverlay.getSpeechStatus();
    openOfflineModelManagerModal(latest, autoRecordAfterSetup);
  });
  offlineBtnRow.appendChild(manageOfflineBtn);
  offlineCard.append(offlineTitle, offlineSub, offlineBtnRow);

  list.append(cloudCard, extCard, offlineCard);

  const footer = document.createElement('div');
  footer.style.cssText = 'display:flex;justify-content:flex-end;gap:8px;margin-top:8px;';
  const closeBtn = document.createElement('button');
  closeBtn.type = 'button';
  closeBtn.className = 'mini-action-btn';
  closeBtn.textContent = 'Cancel';
  closeBtn.addEventListener('click', closeModal);
  footer.appendChild(closeBtn);

  modalBody.append(desc, list, footer);
  modalBackdrop.classList.remove('hidden');
}

function openOfflineModelManagerModal(initialStatus: SpeechStatus, autoRecordAfterSetup = false): void {
  modalTitle.textContent = 'Offline Speech Models & Runtime (Optional)';
  modalBody.innerHTML = '';

  let st = initialStatus;
  let installingModelId: string | null = null;
  let editingModelId: string | null = null;
  let showAddForm = false;
  let unsub: (() => void) | null = null;

  const renderManager = () => {
    modalBody.innerHTML = '';

    // 1. Optional runtime bar
    const runtimeCard = document.createElement('div');
    runtimeCard.className = 'voice-method-card';
    const rtHead = document.createElement('div');
    rtHead.className = 'voice-method-header';
    rtHead.innerHTML = '<strong>Optional Offline Runtime (whisper.cpp)</strong>';
    const rtSub = document.createElement('div');
    rtSub.className = 'voice-method-sub';
    rtSub.textContent = st.managedRuntimeInstalled
      ? `Installed at ${st.managedRuntimePath}`
      : 'Not installed yet — downloaded automatically on demand when you download a model.';
    runtimeCard.append(rtHead, rtSub);
    if (st.managedRuntimeInstalled) {
      const rtBtns = document.createElement('div');
      rtBtns.className = 'voice-crud-actions';
      const rmRtBtn = document.createElement('button');
      rmRtBtn.type = 'button';
      rmRtBtn.className = 'mini-action-btn danger';
      rmRtBtn.textContent = 'Remove Optional Runtime';
      rmRtBtn.disabled = Boolean(installingModelId);
      rmRtBtn.addEventListener('click', async () => {
        await window.agentOverlay.uninstallSpeechRuntime();
        st = await window.agentOverlay.getSpeechStatus();
        renderManager();
      });
      rtBtns.appendChild(rmRtBtn);
      runtimeCard.appendChild(rtBtns);
    }

    // 2. Progress bar (only active while downloading)
    const progWrap = document.createElement('div');
    progWrap.className = `voice-progress-wrap${installingModelId ? '' : ' hidden'}`;
    const progLabel = document.createElement('div');
    progLabel.style.cssText = 'font-size:11.5px;color:var(--text);';
    const progBar = document.createElement('div');
    progBar.className = 'voice-progress-bar';
    const progFill = document.createElement('div');
    progFill.className = 'voice-progress-fill';
    progBar.appendChild(progFill);
    const cancelDlBtn = document.createElement('button');
    cancelDlBtn.type = 'button';
    cancelDlBtn.className = 'mini-action-btn';
    cancelDlBtn.style.alignSelf = 'flex-start';
    cancelDlBtn.textContent = 'Cancel Download';
    cancelDlBtn.addEventListener('click', async () => {
      await window.agentOverlay.cancelSpeechInstall();
      cancelDlBtn.disabled = true;
      progLabel.textContent = 'Cancelling download...';
    });
    progWrap.append(progLabel, progBar, cancelDlBtn);

    // 3. Model CRUD List
    const list = document.createElement('div');
    list.className = 'voice-model-list';
    for (const m of st.models) {
      const card = document.createElement('div');
      card.className = `voice-model-card${m.active ? ' selected' : ''}`;

      const topRow = document.createElement('div');
      topRow.className = 'voice-method-header';
      const titleSpan = document.createElement('strong');
      titleSpan.textContent = m.label;
      topRow.appendChild(titleSpan);

      const badges = document.createElement('div');
      badges.style.cssText = 'display:inline-flex;gap:4px;flex-wrap:wrap;';
      if (m.active) {
        const b = document.createElement('span');
        b.className = 'voice-badge active';
        b.textContent = 'Active';
        badges.appendChild(b);
      }
      if (m.installed) {
        const b = document.createElement('span');
        b.className = 'voice-badge installed';
        b.textContent = 'Ready';
        badges.appendChild(b);
      }
      const srcBadge = document.createElement('span');
      srcBadge.className = 'voice-badge';
      srcBadge.textContent =
        m.source === 'builtin' ? 'Built-in' : m.source === 'custom_url' ? 'Custom URL' : 'Local File';
      badges.appendChild(srcBadge);
      topRow.appendChild(badges);

      const sub = document.createElement('div');
      sub.className = 'voice-method-sub';
      const sizeStr = m.sizeBytes ? ` · ${(m.sizeBytes / (1024 * 1024)).toFixed(0)} MB` : '';
      sub.textContent = `${m.description}${sizeStr}`;

      card.append(topRow, sub);

      if (editingModelId === m.id) {
        const editForm = document.createElement('div');
        editForm.className = 'voice-inline-form';
        const eLabel = document.createElement('input');
        eLabel.type = 'text';
        eLabel.className = 'field-input';
        eLabel.value = m.label;
        const eDesc = document.createElement('input');
        eDesc.type = 'text';
        eDesc.className = 'field-input';
        eDesc.value = m.description;
        const eLoc = document.createElement('input');
        eLoc.type = 'text';
        eLoc.className = 'field-input';
        eLoc.value = m.source === 'custom_url' ? m.url || '' : m.localPath || '';
        const eErr = document.createElement('div');
        eErr.className = 'field-hint';
        eErr.style.cssText = 'font-size:11px;color:#f87171;';

        const eBtns = document.createElement('div');
        eBtns.className = 'voice-crud-actions';
        if (m.source === 'custom_local') {
          const brBtn = document.createElement('button');
          brBtn.type = 'button';
          brBtn.className = 'mini-action-btn';
          brBtn.textContent = 'Browse...';
          brBtn.addEventListener('click', async () => {
            const p = await window.agentOverlay.pickSpeechModelFile();
            if (p) eLoc.value = p;
          });
          eBtns.appendChild(brBtn);
        }
        const saveEditBtn = document.createElement('button');
        saveEditBtn.type = 'button';
        saveEditBtn.className = 'mini-action-btn primary';
        saveEditBtn.textContent = 'Save';
        saveEditBtn.addEventListener('click', async () => {
          const res = await window.agentOverlay.updateSpeechModel(m.id, {
            label: eLabel.value,
            description: eDesc.value,
            ...(m.source === 'custom_url' ? { url: eLoc.value } : { localPath: eLoc.value }),
          });
          if (!res.ok) {
            eErr.textContent = res.error || 'Update failed.';
            return;
          }
          editingModelId = null;
          st = await window.agentOverlay.getSpeechStatus();
          renderManager();
        });
        const cancelEditBtn = document.createElement('button');
        cancelEditBtn.type = 'button';
        cancelEditBtn.className = 'mini-action-btn';
        cancelEditBtn.textContent = 'Cancel';
        cancelEditBtn.addEventListener('click', () => {
          editingModelId = null;
          renderManager();
        });
        eBtns.append(saveEditBtn, cancelEditBtn);
        editForm.append(eLabel, eDesc, eLoc, eBtns, eErr);
        card.appendChild(editForm);
      } else {
        const actions = document.createElement('div');
        actions.className = 'voice-crud-actions';

        if (m.installed && !m.active) {
          const useBtn = document.createElement('button');
          useBtn.type = 'button';
          useBtn.className = 'mini-action-btn primary';
          useBtn.textContent = 'Use';
          useBtn.disabled = Boolean(installingModelId);
          useBtn.addEventListener('click', async () => {
            st = await window.agentOverlay.selectSpeechModel(m.id);
            settings = await window.agentOverlay.getSettings();
            renderManager();
          });
          actions.appendChild(useBtn);
        }

        if (!m.installed && m.url) {
          const dlBtn = document.createElement('button');
          dlBtn.type = 'button';
          dlBtn.className = 'mini-action-btn primary';
          dlBtn.textContent = 'Download';
          dlBtn.disabled = Boolean(installingModelId);
          dlBtn.addEventListener('click', async () => {
            installingModelId = m.id;
            renderManager();
            progLabel.textContent = `Preparing ${m.label}...`;
            progFill.style.width = '5%';
            unsub = window.agentOverlay.onSpeechInstallProgress((p) => {
              if (p.modelId !== m.id) return;
              progLabel.textContent = p.detail;
              progFill.style.width = `${Math.max(4, p.percent)}%`;
            });
            try {
              const res = await window.agentOverlay.installSpeechModel(m.id);
              unsub?.();
              unsub = null;
              installingModelId = null;
              if (res.ok) {
                settings = await window.agentOverlay.updateSettings({
                  speechMode: 'local',
                  speechModel: m.id,
                  speechConfigured: true,
                });
                st = await window.agentOverlay.getSpeechStatus();
                showNotice(`Offline speech model ready (${m.label})`);
                if (autoRecordAfterSetup) {
                  closeModal();
                  await startSpeechRecording();
                  return;
                }
              } else {
                showNotice(`Model download failed: ${res.error || 'Unknown error'}`, true);
              }
              renderManager();
            } catch (err: any) {
              unsub?.();
              unsub = null;
              installingModelId = null;
              showNotice(`Model download failed: ${err?.message || 'Error'}`, true);
              renderManager();
            }
          });
          actions.appendChild(dlBtn);
        }

        const isEditableCustom = m.source !== 'builtin' && !m.id.startsWith('local:');
        if (isEditableCustom) {
          const editBtn = document.createElement('button');
          editBtn.type = 'button';
          editBtn.className = 'mini-action-btn';
          editBtn.textContent = 'Edit';
          editBtn.disabled = Boolean(installingModelId);
          editBtn.addEventListener('click', () => {
            editingModelId = m.id;
            renderManager();
          });
          actions.appendChild(editBtn);
        }

        if (m.installed || isEditableCustom) {
          const delBtn = document.createElement('button');
          delBtn.type = 'button';
          delBtn.className = 'mini-action-btn danger';
          delBtn.textContent = m.source === 'custom_local' && m.installed ? 'Remove' : 'Delete';
          delBtn.disabled = Boolean(installingModelId);
          delBtn.addEventListener('click', async () => {
            await window.agentOverlay.deleteSpeechModel(m.id, m.source !== 'builtin');
            settings = await window.agentOverlay.getSettings();
            st = await window.agentOverlay.getSpeechStatus();
            renderManager();
          });
          actions.appendChild(delBtn);
        }

        card.appendChild(actions);
      }

      list.appendChild(card);
    }

    // 4. "+ Add Custom Model" collapsible form
    const addSection = document.createElement('div');
    if (showAddForm) {
      const form = document.createElement('div');
      form.className = 'voice-method-card';
      const fTitle = document.createElement('div');
      fTitle.className = 'voice-method-header';
      fTitle.innerHTML = '<strong>Add Custom Whisper Model</strong>';

      const srcSelect = document.createElement('select');
      srcSelect.className = 'field-select';
      srcSelect.innerHTML =
        '<option value="custom_url">Download from URL (.bin)</option><option value="custom_local">Existing Local File (.bin / .ggml)</option>';

      const fLabel = document.createElement('input');
      fLabel.type = 'text';
      fLabel.className = 'field-input';
      fLabel.placeholder = 'Model Name (e.g. Whisper Medium q5_0)';

      const fLocRow = document.createElement('div');
      fLocRow.style.cssText = 'display:flex;gap:6px;align-items:center;';
      const fLoc = document.createElement('input');
      fLoc.type = 'text';
      fLoc.className = 'field-input';
      fLoc.style.flex = '1';
      fLoc.placeholder = 'https://huggingface.co/.../ggml-model.bin';
      const fBrowse = document.createElement('button');
      fBrowse.type = 'button';
      fBrowse.className = 'mini-action-btn hidden';
      fBrowse.textContent = 'Browse...';
      fBrowse.addEventListener('click', async () => {
        const picked = await window.agentOverlay.pickSpeechModelFile();
        if (picked) {
          fLoc.value = picked;
          if (!fLabel.value.trim()) {
            const parts = picked.split(/[\\/]/);
            fLabel.value = parts[parts.length - 1] || 'Custom Local Model';
          }
        }
      });
      fLocRow.append(fLoc, fBrowse);

      srcSelect.addEventListener('change', () => {
        const isLocal = srcSelect.value === 'custom_local';
        fBrowse.classList.toggle('hidden', !isLocal);
        fLoc.placeholder = isLocal
          ? '/path/to/ggml-model.bin'
          : 'https://huggingface.co/.../ggml-model.bin';
      });

      const fDesc = document.createElement('input');
      fDesc.type = 'text';
      fDesc.className = 'field-input';
      fDesc.placeholder = 'Description (optional)';
      const fErr = document.createElement('div');
      fErr.className = 'field-hint';
      fErr.style.cssText = 'font-size:11px;color:#f87171;';

      const fBtns = document.createElement('div');
      fBtns.className = 'voice-crud-actions';
      const submitBtn = document.createElement('button');
      submitBtn.type = 'button';
      submitBtn.className = 'mini-action-btn primary';
      submitBtn.textContent = 'Add Model';
      submitBtn.addEventListener('click', async () => {
        const isLocal = srcSelect.value === 'custom_local';
        const res = await window.agentOverlay.addSpeechModel({
          label: fLabel.value,
          description: fDesc.value,
          source: isLocal ? 'custom_local' : 'custom_url',
          ...(isLocal ? { localPath: fLoc.value } : { url: fLoc.value }),
        });
        if (!res.ok) {
          fErr.textContent = res.error || 'Could not add model.';
          return;
        }
        showAddForm = false;
        st = await window.agentOverlay.getSpeechStatus();
        renderManager();
      });
      const cancelAddBtn = document.createElement('button');
      cancelAddBtn.type = 'button';
      cancelAddBtn.className = 'mini-action-btn';
      cancelAddBtn.textContent = 'Cancel';
      cancelAddBtn.addEventListener('click', () => {
        showAddForm = false;
        renderManager();
      });
      fBtns.append(submitBtn, cancelAddBtn);

      const inner = document.createElement('div');
      inner.className = 'voice-inline-form';
      inner.append(srcSelect, fLabel, fLocRow, fDesc, fBtns, fErr);
      form.append(fTitle, inner);
      addSection.appendChild(form);
    } else {
      const addToggleBtn = document.createElement('button');
      addToggleBtn.type = 'button';
      addToggleBtn.className = 'mini-action-btn';
      addToggleBtn.textContent = '+ Add Custom Model (URL or Local File)';
      addToggleBtn.disabled = Boolean(installingModelId);
      addToggleBtn.addEventListener('click', () => {
        showAddForm = true;
        renderManager();
      });
      addSection.appendChild(addToggleBtn);
    }

    // 5. Footer navigation
    const footer = document.createElement('div');
    footer.style.cssText = 'display:flex;align-items:center;justify-content:space-between;gap:8px;margin-top:10px;flex-wrap:wrap;';
    const backBtn = document.createElement('button');
    backBtn.type = 'button';
    backBtn.className = 'mini-action-btn';
    backBtn.textContent = '← Back to Voice Methods';
    backBtn.disabled = Boolean(installingModelId);
    backBtn.addEventListener('click', async () => {
      const latest = await window.agentOverlay.getSpeechStatus();
      openVoiceMethodPickerModal(latest, autoRecordAfterSetup);
    });

    const rightBtns = document.createElement('div');
    rightBtns.style.cssText = 'display:flex;gap:6px;';
    if (st.hasLocalEngine && st.hasLocalModel) {
      const recBtn = document.createElement('button');
      recBtn.type = 'button';
      recBtn.className = 'perm-btn primary';
      recBtn.textContent = autoRecordAfterSetup ? 'Use Offline STT & Record' : 'Done';
      recBtn.addEventListener('click', async () => {
        settings = await window.agentOverlay.updateSettings({
          speechMode: 'local',
          speechConfigured: true,
        });
        closeModal();
        if (autoRecordAfterSetup) {
          await startSpeechRecording();
        }
      });
      rightBtns.appendChild(recBtn);
    } else {
      const closeBtn = document.createElement('button');
      closeBtn.type = 'button';
      closeBtn.className = 'mini-action-btn';
      closeBtn.textContent = 'Close';
      closeBtn.disabled = Boolean(installingModelId);
      closeBtn.addEventListener('click', closeModal);
      rightBtns.appendChild(closeBtn);
    }
    footer.append(backBtn, rightBtns);

    modalBody.append(runtimeCard, progWrap, list, addSection, footer);
  };

  renderManager();
  modalBackdrop.classList.remove('hidden');
}

function setupSpeechToText(): void {
  if (!btnMic) return;

  window.addEventListener('chat-overlay:open-voice-picker', async () => {
    const st = await window.agentOverlay.getSpeechStatus();
    openVoiceMethodPickerModal(st, false);
  });

  window.addEventListener('chat-overlay:open-voice-setup', async () => {
    const st = await window.agentOverlay.getSpeechStatus();
    openOfflineModelManagerModal(st, false);
  });

  btnMic.addEventListener('click', async () => {
    if (isTranscribingSpeech) return;
    if (isRecordingSpeech) {
      await stopAndTranscribeSpeech();
    } else {
      const mode = settings?.speechMode || 'auto';
      try {
        const st = await window.agentOverlay.getSpeechStatus();
        const isReady =
          (mode === 'cloud' && st.hasCloudKey) ||
          (mode === 'local' && (st.hasCustomLocalBackend || (st.hasLocalEngine && st.hasLocalModel))) ||
          (mode === 'auto' &&
            (st.hasCloudKey || st.hasCustomLocalBackend || (st.hasLocalEngine && st.hasLocalModel)));
        if (!isReady) {
          openVoiceMethodPickerModal(st, true);
          return;
        }
      } catch {
        // proceed directly if status probe unavailable
      }
      await startSpeechRecording();
    }
  });

  window.agentOverlay.onSpeechProgress?.((progress) => {
    if (progress && progress.text && !progress.text.includes('[NO_SPEECH]')) {
      usedVoiceForDraft = true;
      updateLiveSpeechDisplay(progress.text, '');
    }
  });

  window.addEventListener('beforeunload', () => cleanupMediaStream());
}

async function startSpeechRecording(): Promise<void> {
  if (!navigator.mediaDevices?.getUserMedia) {
    showNotice('Microphone access is not supported in this environment.', true);
    return;
  }

  try {
    const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
    mediaStream = stream;
    audioChunks = [];

    speechBaseText = composerInput.value;
    speechCursorStart = composerInput.selectionStart ?? speechBaseText.length;
    speechLiveCommitted = '';
    speechLiveInterim = '';

    let recordedMime = 'audio/wav';
    pcmRecorder = new PcmWavRecorder();
    try {
      await pcmRecorder.start(stream);
    } catch {
      pcmRecorder = null;
      let mimeType = 'audio/webm;codecs=opus';
      if (typeof MediaRecorder !== 'undefined') {
        if (!MediaRecorder.isTypeSupported(mimeType)) {
          if (MediaRecorder.isTypeSupported('audio/webm')) {
            mimeType = 'audio/webm';
          } else if (MediaRecorder.isTypeSupported('audio/ogg')) {
            mimeType = 'audio/ogg';
          } else {
            mimeType = '';
          }
        }
      }
      recordedMime = mimeType || 'audio/webm';
      const recorder = mimeType ? new MediaRecorder(stream, { mimeType }) : new MediaRecorder(stream);
      mediaRecorder = recorder;
      recorder.ondataavailable = (event: BlobEvent) => {
        if (event.data && event.data.size > 0) {
          audioChunks.push(event.data);
        }
      };
      recorder.onerror = (event: any) => {
        console.warn('MediaRecorder error:', event);
        cleanupMediaStream();
        btnMic?.classList.remove('recording');
        isRecordingSpeech = false;
        showNotice('Audio recording error occurred.', true);
      };
      recorder.start(250);
    }

    isRecordingSpeech = true;
    btnMic?.classList.add('recording');
    btnMic?.setAttribute('title', 'Listening... Click to stop and transcribe (Esc to cancel)');
    showNotice('Listening... Speak now (real-time voice transcription active)');

    // 1. Optional in-process / browser-based speech recognition fallback with real-time progressive interim results
    try {
      const SpeechRec = (window as any).SpeechRecognition || (window as any).webkitSpeechRecognition;
      if (SpeechRec) {
        browserRecognizedTranscript = '';
        browserSpeechRecognition = new SpeechRec();
        browserSpeechRecognition.continuous = true;
        browserSpeechRecognition.interimResults = true;
        const currentLang = settings?.speechLanguage || 'auto';
        if (currentLang && currentLang !== 'auto') {
          browserSpeechRecognition.lang = currentLang;
        }
        browserSpeechRecognition.onresult = (ev: any) => {
          let finalPart = '';
          let interimPart = '';
          for (let i = 0; i < ev.results.length; ++i) {
            const item = ev.results[i];
            const text = item[0]?.transcript || '';
            if (item.isFinal) {
              finalPart += (finalPart ? ' ' : '') + text;
            } else {
              interimPart += (interimPart ? ' ' : '') + text;
            }
          }
          speechLiveCommitted = finalPart;
          speechLiveInterim = interimPart;
          browserRecognizedTranscript = (finalPart ? finalPart + ' ' : '') + interimPart;
          updateLiveSpeechDisplay(speechLiveCommitted, speechLiveInterim);
        };
        browserSpeechRecognition.onerror = () => {
          // ignore browser speech recognition errors silently
        };
        browserSpeechRecognition.start();
      }
    } catch {
      browserSpeechRecognition = null;
    }

    // 2. Progressive background transcription worker for local model or cloud AI ("nói đến đâu chữ nó hiện đến đấy")
    if (speechInterimInterval) clearInterval(speechInterimInterval);
    speechInterimInterval = setInterval(async () => {
      if (!isRecordingSpeech || isInterimTranscribing) return;
      if (browserRecognizedTranscript?.trim()) return; // browser speech recognition is already providing real-time words
      let base64: string | null = null;
      if (pcmRecorder) {
        base64 = pcmRecorder.getSnapshotBase64();
      } else if (audioChunks.length > 0) {
        const interimBlob = new Blob(audioChunks, { type: recordedMime });
        if (interimBlob.size >= 2000) {
          base64 = await blobToBase64(interimBlob);
        }
      }
      if (!base64 || base64.length < 2400) return;
      isInterimTranscribing = true;
      try {
        if (!isRecordingSpeech) return;
        const lang = settings?.speechLanguage || 'auto';
        const mode = settings?.speechMode || 'auto';
        const localUrl = settings?.localSpeechUrl || '';
        const localCommand = settings?.localSpeechCommand || '';
        const res = await window.agentOverlay.transcribeAudio({
          data: base64,
          mimeType: recordedMime,
          language: lang,
          mode,
          localUrl,
          localCommand,
        });
        if (isRecordingSpeech && res.ok && res.text) {
          updateLiveSpeechDisplay(res.text, '');
        }
      } catch {
        // ignore background interim errors
      } finally {
        isInterimTranscribing = false;
      }
    }, 1200);

    if (speechAutoStopTimer) clearTimeout(speechAutoStopTimer);
    speechAutoStopTimer = setTimeout(() => {
      if (isRecordingSpeech) {
        void stopAndTranscribeSpeech();
      }
    }, 60_000);
  } catch (err: any) {
    console.error('Failed to start microphone recording:', err);
    cleanupMediaStream();
    btnMic?.classList.remove('recording');
    isRecordingSpeech = false;
    showNotice(`Microphone error: ${err?.message || 'Access denied'}`, true);
  }
}

async function stopAndTranscribeSpeech(): Promise<void> {
  if (speechAutoStopTimer) {
    clearTimeout(speechAutoStopTimer);
    speechAutoStopTimer = null;
  }
  if (speechInterimInterval) {
    clearInterval(speechInterimInterval);
    speechInterimInterval = null;
  }
  isInterimTranscribing = false;

  if (browserSpeechRecognition) {
    try {
      browserSpeechRecognition.stop();
    } catch {}
  }

  if (!pcmRecorder && (!mediaRecorder || mediaRecorder.state === 'inactive')) {
    cleanupMediaStream();
    btnMic?.classList.remove('recording');
    isRecordingSpeech = false;
    return;
  }

  isRecordingSpeech = false;
  btnMic?.classList.remove('recording');
  btnMic?.classList.add('transcribing');
  btnMic?.setAttribute('title', 'Transcribing voice input... (Esc to cancel)');
  isTranscribingSpeech = true;
  showNotice('Transcribing audio...');

  let recordedMime = 'audio/wav';
  let base64: string | null = null;

  if (pcmRecorder) {
    base64 = await pcmRecorder.stopAndGetBase64();
    pcmRecorder = null;
    cleanupMediaStream();
  } else {
    const stoppedPromise = new Promise<void>((resolve) => {
      if (!mediaRecorder || mediaRecorder.state === 'inactive') {
        resolve();
        return;
      }
      const timeout = setTimeout(resolve, 350);
      mediaRecorder.onstop = () => {
        clearTimeout(timeout);
        resolve();
      };
      try {
        if (mediaRecorder.state === 'recording') {
          mediaRecorder.requestData();
        }
        mediaRecorder.stop();
      } catch {
        clearTimeout(timeout);
        resolve();
      }
    });

    await stoppedPromise;
    recordedMime = mediaRecorder?.mimeType || 'audio/webm';
    cleanupMediaStream();
    const blob = new Blob(audioChunks, { type: recordedMime });
    audioChunks = [];
    if (blob.size >= 200) {
      base64 = await blobToBase64(blob);
    }
  }

  try {
    if (!base64 || base64.length < 200) {
      if (browserRecognizedTranscript?.trim()) {
        const cleaned = browserRecognizedTranscript.trim();
        updateLiveSpeechDisplay(cleaned, '');
        composerInput.dispatchEvent(new Event('input', { bubbles: true }));
        composerInput.focus();
        showNotice(`Transcribed (browser): "${cleaned.slice(0, 40)}${cleaned.length > 40 ? '...' : ''}"`);
      } else {
        composerInput.value = speechBaseText;
        composerInput.setSelectionRange(speechCursorStart, speechCursorStart);
        autoResizeComposer();
        updateSendButton();
        showNotice('Voice input too short.');
      }
      return;
    }

    const lang = settings?.speechLanguage || 'auto';
    const mode = settings?.speechMode || 'auto';
    const localUrl = settings?.localSpeechUrl || '';
    const localCommand = settings?.localSpeechCommand || '';

    const result = await window.agentOverlay.transcribeAudio({
      data: base64,
      mimeType: recordedMime,
      language: lang,
      mode,
      localUrl,
      localCommand,
    });

    if (!isTranscribingSpeech) {
      return;
    }

    if (result.ok && result.text) {
      usedVoiceForDraft = true;
      updateLiveSpeechDisplay(result.text, '');
      composerInput.dispatchEvent(new Event('input', { bubbles: true }));
      composerInput.focus();
      const noticeLabel = lang === 'auto' ? '' : ` (${lang})`;
      const engineBadge = result.engine === 'local' ? ' [local]' : '';
      showNotice(`Transcribed${noticeLabel}${engineBadge}: "${result.text.slice(0, 40)}${result.text.length > 40 ? '...' : ''}"`);
    } else if (result.ok && !result.text) {
      if (browserRecognizedTranscript?.trim()) {
        const cleaned = browserRecognizedTranscript.trim();
        usedVoiceForDraft = true;
        updateLiveSpeechDisplay(cleaned, '');
        composerInput.dispatchEvent(new Event('input', { bubbles: true }));
        composerInput.focus();
        showNotice(`Transcribed (browser): "${cleaned.slice(0, 40)}${cleaned.length > 40 ? '...' : ''}"`);
      } else {
        composerInput.value = speechBaseText;
        composerInput.setSelectionRange(speechCursorStart, speechCursorStart);
        autoResizeComposer();
        updateSendButton();
        showNotice('No speech detected.');
      }
    } else {
      if (browserRecognizedTranscript?.trim()) {
        const cleaned = browserRecognizedTranscript.trim();
        usedVoiceForDraft = true;
        updateLiveSpeechDisplay(cleaned, '');
        composerInput.dispatchEvent(new Event('input', { bubbles: true }));
        composerInput.focus();
        showNotice(`Transcribed (browser): "${cleaned.slice(0, 40)}${cleaned.length > 40 ? '...' : ''}"`);
      } else {
        composerInput.value = speechBaseText;
        composerInput.setSelectionRange(speechCursorStart, speechCursorStart);
        autoResizeComposer();
        updateSendButton();
        if (result.error?.includes('No local speech-to-text engine responded')) {
          void window.agentOverlay
            .getSpeechStatus()
            .then((st) => openVoiceMethodPickerModal(st, true));
        } else {
          showNotice(`Voice transcription failed: ${result.error || 'Unknown error'}`, true);
        }
      }
    }
  } catch (err: any) {
    console.error('Speech transcription error:', err);
    if (isTranscribingSpeech) {
      showNotice(`Voice transcription error: ${err?.message || 'Network error'}`, true);
    }
  } finally {
    isTranscribingSpeech = false;
    browserSpeechRecognition = null;
    browserRecognizedTranscript = '';
    btnMic?.classList.remove('transcribing');
    btnMic?.setAttribute('title', 'Voice Input (Speech-to-Text)');
  }
}

function openFirstRunOnboardingModal(): void {
  modalTitle.textContent = 'Welcome to Chat Overlay';
  modalBody.innerHTML = '';

  const desc = document.createElement('div');
  desc.className = 'voice-setup-desc';
  desc.textContent =
    'No ACP coding agent was detected on this machine yet. Install OpenCode standalone right now (zero Node.js, Bun, or CLI prerequisites required), or configure another agent in Settings.';

  const statusLine = document.createElement('div');
  statusLine.className = 'field-hint';
  statusLine.style.cssText = 'font-size:12px;margin:8px 0;';

  const actions = document.createElement('div');
  actions.style.cssText = 'display:flex;align-items:center;justify-content:space-between;gap:8px;flex-wrap:wrap;margin-top:10px;';

  const left = document.createElement('div');
  left.style.cssText = 'display:flex;align-items:center;gap:6px;';

  const installBtn = document.createElement('button');
  installBtn.type = 'button';
  installBtn.className = 'perm-btn primary';
  installBtn.textContent = 'Install OpenCode (Recommended)';

  const settingsBtn = document.createElement('button');
  settingsBtn.type = 'button';
  settingsBtn.className = 'mini-action-btn';
  settingsBtn.textContent = 'Open Harness Settings';

  const laterBtn = document.createElement('button');
  laterBtn.type = 'button';
  laterBtn.className = 'mini-action-btn';
  laterBtn.textContent = 'Later';

  left.append(installBtn, settingsBtn);
  actions.append(left, laterBtn);
  modalBody.append(desc, statusLine, actions);
  modalBackdrop.classList.remove('hidden');

  laterBtn.addEventListener('click', closeModal);
  settingsBtn.addEventListener('click', () => {
    closeModal();
    openSettingsModal();
  });

  installBtn.addEventListener('click', async () => {
    installBtn.disabled = true;
    settingsBtn.disabled = true;
    laterBtn.disabled = true;
    installBtn.textContent = 'Downloading & Installing...';
    statusLine.textContent = 'Fetching standalone OpenCode binary from GitHub Releases...';
    try {
      const res = await window.agentOverlay.installHarness('opencode_acp');
      if (res.ok) {
        statusLine.textContent = res.message;
        catalog = await window.agentOverlay.switchHarness('opencode_acp');
        settings = await window.agentOverlay.getSettings();
        renderHeaderAndPickers();
        closeModal();
        showNotice('OpenCode installed & connected.');
      } else {
        statusLine.textContent = res.message;
        installBtn.disabled = false;
        settingsBtn.disabled = false;
        laterBtn.disabled = false;
        installBtn.textContent = 'Retry Install';
      }
    } catch (err: any) {
      statusLine.textContent = err?.message || 'Installation failed';
      installBtn.disabled = false;
      settingsBtn.disabled = false;
      laterBtn.disabled = false;
      installBtn.textContent = 'Retry Install';
    }
  });
}

async function ensureReadyHarnessOrOnboard(): Promise<void> {
  if (window.location.search.includes('audit=1')) return;
  try {
    const diagnostics = await window.agentOverlay.inspectHarnesses();
    const currentDiag = diagnostics.find((d) => d.id === settings.harness);
    if (currentDiag && (currentDiag.installed || currentDiag.viaRunner)) {
      return;
    }
    const available = diagnostics.find((d) => d.installed || d.viaRunner);
    if (available) {
      catalog = await window.agentOverlay.switchHarness(available.id);
      settings = await window.agentOverlay.getSettings();
      renderHeaderAndPickers();
      showNotice(`Switched to available agent: ${available.label}`);
      return;
    }
    openFirstRunOnboardingModal();
  } catch {
    // ignore onboarding probe failures
  }
}

async function bootstrap(): Promise<void> {
  settings = await window.agentOverlay.getSettings();
  currentSessionWorkspaceDir = settings.workspaceDir;
  catalog = await window.agentOverlay.getCatalog();
  renderHeaderAndPickers();
  updateSendButton();
  void refreshGitGlance();
  setupSpeechToText();

  window.agentOverlay.onUpdateStatus?.((st) => {
    if (st.state === 'available' && st.version) {
      showNotice(`Update v${st.version} available (Settings -> Check for Updates).`);
    } else if (st.state === 'downloaded' && st.version) {
      showNotice(`Update v${st.version} downloaded — ready to install in Settings.`);
    }
  });

  window.agentOverlay.onStreamEvent(handleStreamEvent);
  window.agentOverlay.onFocusComposer(() => composerInput.focus());
  window.agentOverlay.onGhostModeChanged((enabled) => {
    ghostModeBadge.classList.toggle('hidden', !enabled);
  });
  window.agentOverlay.onClipboardCompanion((att) => {
    void triggerClipboardCompanionFlow(att);
  });
  window.agentOverlay.onDesktopPanic(() => {
    desktopPanicToken++;
    showNotice('Desktop automation stopped (Panic Stop).', true);
  });

let acpSyncTimer: ReturnType<typeof setInterval> | null = null;
let isSyncingAcp = false;

function startPeriodicAcpSync(): void {
  if (acpSyncTimer) clearInterval(acpSyncTimer);
  acpSyncTimer = setInterval(async () => {
    if (isStreaming || isSyncingAcp || isRenamingHistorySession) return;
    const isHistoryOpen =
      !modalBackdrop.classList.contains('hidden') &&
      modalTitle.textContent?.includes('Conversation History');

    // Skip background polling when idle to save resources unless history is actively being viewed
    if (!isHistoryOpen && (document.visibilityState === 'hidden' || !document.hasFocus())) {
      return;
    }

    isSyncingAcp = true;
    try {
      if (isHistoryOpen) {
        await refreshHistoryModalList();
      } else if (currentSessionId && currentSessionIsRemote) {
        // Lightweight session/list check first; only run heavy session/load replay when timestamp advances
        const list = await window.agentOverlay.listSessions();
        const remoteMeta = list.find((item) => item.id === currentSessionId);
        if (remoteMeta && remoteMeta.updatedAt > currentSessionUpdatedAt) {
          const res = await window.agentOverlay.loadRemoteSession(currentSessionId);
          currentSessionUpdatedAt = remoteMeta.updatedAt;
          if (res.ok && res.turns && res.turns.length >= turns.length) {
            turns = structuredClone(res.turns);
            if (remoteMeta.title && remoteMeta.title !== currentSessionTitle) {
              currentSessionTitle = remoteMeta.title;
              sessionTitleChip.textContent = remoteMeta.title;
            }
            rebuildMessagesList();
            const serverName = getConnectedServerLabel();
            showNotice(`Synced ${turns.length} turn(s) from ${serverName}`);
          }
        }
      }
    } catch {
      // ignore periodic sync errors
    } finally {
      isSyncingAcp = false;
    }
  }, 7500); // Periodic sync interval between 5-10s (7.5s)
}

let lastFocusSync = 0;
  window.addEventListener('focus', async () => {
    if (isStreaming || !currentSessionId || !currentSessionIsRemote) return;
    const now = Date.now();
    if (now - lastFocusSync < 10_000) return;
    lastFocusSync = now;
    try {
      const list = await window.agentOverlay.listSessions();
      const remoteMeta = list.find((item) => item.id === currentSessionId);
      if (remoteMeta && remoteMeta.updatedAt > currentSessionUpdatedAt) {
        const res = await window.agentOverlay.loadRemoteSession(currentSessionId);
        currentSessionUpdatedAt = remoteMeta.updatedAt;
        if (res.ok && res.turns && res.turns.length >= turns.length) {
          turns = structuredClone(res.turns);
          rebuildMessagesList();
          const serverName = getConnectedServerLabel();
          showNotice(`Synced ${turns.length} turn(s) from ${serverName}`);
        }
      }
    } catch {
      // ignore
    }
  });

  startPeriodicAcpSync();
  void ensureReadyHarnessOrOnboard();
  composerInput.focus();

  if (window.location.search.includes('audit=1')) {
    void runAuditFlow();
  }
}

void bootstrap();
