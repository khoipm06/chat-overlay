import { HARNESS_LIST } from '../shared/types';
import type {
  AppSettings,
  ChatAttachment,
  DesktopAction,
  GitGlance,
  HarnessKind,
  SavedSession,
  SessionCatalog,
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
import { renderGitDiffDrawer } from './diff-drawer';
import { hydrateRenderedMarkdown, renderMarkdownHtml } from './markdown';
import { PopoverController, type PopoverEntry } from './popovers';
import { renderTabbedSettingsDrawer } from './settings-drawer';
import { playAttentionChime, playDoneChime } from './sound';
import { presentToolCall, renderToolDetailHtml } from './tool-presenter';

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

const btnCaptureMenu = document.getElementById('btn-capture-menu')!;
const btnPinScreen = document.getElementById('btn-pin-screen')!;
const btnClearMarkup = document.getElementById('btn-clear-markup')!;
const btnSend = document.getElementById('btn-send') as HTMLButtonElement;
const sendIcon = document.getElementById('send-icon')!;

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

let currentSessionId = `ses_${Date.now()}`;
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

interface ActiveTurnRefs {
  turn: StoredTurn;
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
  metricsEl: HTMLElement;
  renderScheduled: number | null;
}

let activeTurn: ActiveTurnRefs | null = null;
let autocompleteKind: 'mention' | 'slash' | null = null;
let autocompleteStart = 0;

function basename(p: string): string {
  const parts = p.replace(/\\/g, '/').split('/').filter(Boolean);
  return parts[parts.length - 1] || p;
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
  if (!m?.totalTokens) {
    contextBudgetEl.classList.add('hidden');
    return;
  }
  contextBudgetEl.classList.remove('hidden');
  const limit = m.contextSize || 200_000;
  const pct = Math.min(100, Math.round((m.totalTokens / limit) * 100));
  contextBudgetText.textContent = `${formatTokenCount(m.totalTokens)} / ${formatTokenCount(limit)} (${pct}%)`;
  contextBudgetFill.style.width = `${Math.max(4, pct)}%`;
  contextBudgetFill.style.background =
    pct >= 80 ? 'var(--danger)' : pct >= 60 ? 'var(--warning)' : 'var(--success)';
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

function renderAttachments(): void {
  attachmentsRow.innerHTML = '';
  for (const att of pendingAttachments) {
    const chip = document.createElement('div');
    chip.className = 'att-chip';
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
  await window.agentOverlay.saveSession({
    id: currentSessionId,
    title: currentSessionTitle,
    harness: catalog.harness,
    workspaceDir: settings.workspaceDir,
    updatedAt: Date.now(),
    turns,
  });
}

function buildTurnDom(turn: StoredTurn, streamingNow: boolean): ActiveTurnRefs {
  welcomeView.classList.add('hidden');
  messagesView.classList.remove('hidden');

  const group = document.createElement('div');
  group.className = 'turn-group';

  // User Bubble with Revert action
  const userBubble = document.createElement('div');
  userBubble.className = 'user-bubble';

  const userTop = document.createElement('div');
  userTop.className = 'user-bubble-top';
  const attsWrap = document.createElement('div');
  attsWrap.className = 'user-attachments';
  for (const att of turn.attachments) {
    const c = document.createElement('span');
    c.className = 'user-att-chip';
    const ic = document.createElement('i');
    ic.className = `codicon ${att.kind === 'image' ? 'codicon-device-camera' : 'codicon-file-code'}`;
    const lbl = document.createElement('span');
    lbl.textContent = att.name;
    c.append(ic, lbl);
    attsWrap.appendChild(c);
  }
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
  userTop.append(attsWrap, revertBtn);
  userBubble.appendChild(userTop);

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

  // Collapsible Thinking Section
  const thinkingDisclosure = document.createElement('div');
  thinkingDisclosure.className = `disclosure${turn.thinkingText ? '' : ' hidden'}`;
  const thinkingTrigger = document.createElement('button');
  thinkingTrigger.type = 'button';
  thinkingTrigger.className = 'disclosure-trigger';
  const thinkingChevron = document.createElement('i');
  thinkingChevron.className = 'codicon codicon-chevron-right';
  const thinkingTriggerText = document.createElement('span');
  thinkingTriggerText.textContent = turn.thinkingDurationMs
    ? `Thought for ${(turn.thinkingDurationMs / 1000).toFixed(1)}s`
    : 'Thinking';
  thinkingTrigger.append(thinkingChevron, thinkingTriggerText);
  const thinkingBody = document.createElement('div');
  thinkingBody.className = 'disclosure-body hidden';
  thinkingBody.textContent = turn.thinkingText;
  thinkingTrigger.addEventListener('click', () => {
    const open = thinkingBody.classList.toggle('hidden');
    thinkingChevron.className = open
      ? 'codicon codicon-chevron-right'
      : 'codicon codicon-chevron-down';
  });
  thinkingDisclosure.append(thinkingTrigger, thinkingBody);
  assistantTurn.appendChild(thinkingDisclosure);

  // Tools container
  const toolsContainer = document.createElement('div');
  toolsContainer.style.display = 'flex';
  toolsContainer.style.flexDirection = 'column';
  toolsContainer.style.gap = '6px';
  assistantTurn.appendChild(toolsContainer);

  // Shimmer skeleton shown until first text arrives
  const skeletonEl = document.createElement('div');
  skeletonEl.className = `thinking-skeleton${streamingNow && !turn.assistantMarkdown ? '' : ' hidden'}`;
  skeletonEl.innerHTML = `
    <div class="skeleton-line" style="width: 78%"></div>
    <div class="skeleton-line" style="width: 92%"></div>
    <div class="skeleton-line" style="width: 54%"></div>
  `;
  assistantTurn.appendChild(skeletonEl);

  // Assistant Markdown Body
  const markdownEl = document.createElement('div');
  markdownEl.className = 'assistant-markdown';
  if (turn.assistantMarkdown) {
    markdownEl.innerHTML = renderMarkdownHtml(turn.assistantMarkdown);
    void hydrateRenderedMarkdown(markdownEl, false);
  }
  assistantTurn.appendChild(markdownEl);

  // Render stored tools if loading history
  const toolCardMap = new Map<string, ToolCardDom>();
  for (const t of turn.tools) {
    appendToolCard(toolsContainer, toolCardMap, t.id, t.name, t.title, t.args, t.status, t.output, t.durationMs);
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

  actionsEl.append(copyBtn, retryBtn);
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
    thinkingStartedAt: 0,
    thinkingDisclosure,
    thinkingTriggerText,
    thinkingBody,
    toolsContainer,
    toolCardMap,
    skeletonEl,
    markdownEl,
    workingBadge,
    footerEl,
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
  currentSessionId = `ses_${Date.now()}`;
  currentSessionTitle = 'New chat';
  sessionTitleChip.textContent = 'New chat';
  messagesView.innerHTML = '';
  messagesView.classList.add('hidden');
  welcomeView.classList.remove('hidden');
  renderAttachments();
  updateUndoRedoButtons();
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
  const previewBtn = document.createElement('button');
  previewBtn.type = 'button';
  previewBtn.className = 'mini-action-btn';
  previewBtn.textContent = 'Preview';
  previewBtn.addEventListener('click', () => {
    void window.agentOverlay.previewDesktopActions(actions);
  });

  const runAllBtn = document.createElement('button');
  runAllBtn.type = 'button';
  runAllBtn.className = 'mini-action-btn primary';
  runAllBtn.textContent = 'Run All';

  const panicBtn = document.createElement('button');
  panicBtn.type = 'button';
  panicBtn.className = 'mini-action-btn danger';
  panicBtn.textContent = 'Panic Stop';
  panicBtn.addEventListener('click', () => {
    desktopPanicToken++;
    void window.agentOverlay.panicStopDesktop();
    title.textContent = 'Desktop Actions · Stopped';
  });

  ctrls.append(previewBtn, runAllBtn, panicBtn);
  head.append(title, ctrls);
  card.appendChild(head);

  const rowRunners: Array<() => Promise<void>> = [];

  for (const act of actions) {
    const row = document.createElement('div');
    row.className = 'desktop-action-row';
    const desc = document.createElement('span');
    desc.className = 'desktop-action-desc';
    desc.textContent = describeDesktopAction(act);
    desc.title = desc.textContent;

    const btns = document.createElement('div');
    btns.className = 'desktop-action-btns';
    const runBtn = document.createElement('button');
    runBtn.type = 'button';
    runBtn.className = 'mini-action-btn';
    runBtn.textContent = 'Run';

    const executeOne = async () => {
      if (desktopPanicToken !== epoch || runBtn.disabled) return;
      runBtn.disabled = true;
      runBtn.textContent = '...';
      const res = await window.agentOverlay.executeDesktopAction(act, false);
      runBtn.textContent = res.ok ? 'Done ✓' : 'Failed';
      if (!res.ok) showNotice(res.message, true);
    };

    runBtn.addEventListener('click', () => void executeOne());
    rowRunners.push(executeOne);

    btns.appendChild(runBtn);
    row.append(desc, btns);
    card.appendChild(row);
  }

  const runAllSequence = async () => {
    runAllBtn.disabled = true;
    for (const fn of rowRunners) {
      if (desktopPanicToken !== epoch) break;
      await fn();
      await new Promise((r) => setTimeout(r, 150));
    }
    runAllBtn.textContent = desktopPanicToken === epoch ? 'Finished' : 'Stopped';
  };

  runAllBtn.addEventListener('click', () => void runAllSequence());
  container.appendChild(card);

  if (autoPreview && !window.location.search.includes('audit=1')) {
    void window.agentOverlay.previewDesktopActions(actions);
    title.textContent = 'Auto-running in 1.2s (Ctrl+Alt+X stops)...';
    window.setTimeout(() => {
      if (desktopPanicToken === epoch) {
        void runAllSequence();
      }
    }, 1200);
  }
}

function handleStreamEvent(ev: StreamEvent): void {
  switch (ev.type) {
    case 'catalog':
      catalog = ev.catalog;
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
      activeTurn.thinkingDisclosure.classList.remove('hidden');
      activeTurn.thinkingBody.textContent = activeTurn.turn.thinkingText;
      activeTurn.thinkingTriggerText.textContent = 'Thinking...';
      scrollToBottomIfPinned();
      break;
    }
    case 'text_delta': {
      if (!activeTurn) break;
      if (activeTurn.thinkingStartedAt > 0 && activeTurn.turn.thinkingDurationMs === 0) {
        activeTurn.turn.thinkingDurationMs = Date.now() - activeTurn.thinkingStartedAt;
        activeTurn.thinkingTriggerText.textContent = `Thought for ${(activeTurn.turn.thinkingDurationMs / 1000).toFixed(1)}s`;
      }
      activeTurn.turn.assistantMarkdown += ev.text;
      scheduleMarkdownUpdate(activeTurn);
      break;
    }
    case 'tool_start': {
      if (!activeTurn) break;
      activeTurn.turn.tools.push({
        id: ev.id,
        name: ev.name,
        title: ev.title,
        args: ev.args,
        status: 'running',
        output: '',
      });
      appendToolCard(
        activeTurn.toolsContainer,
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
      activeTurn.toolsContainer.appendChild(card);
      if (settings.soundCues) playAttentionChime();
      void window.agentOverlay.flashAttention();
      void window.agentOverlay.notifyBackground(
        'Approval Needed',
        req.toolTitle || 'Agent requires tool permission',
      );
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
        activeTurn.toolsContainer,
        ev.actions,
        settings.desktopControlMode === 'preview_auto',
      );
      scrollToBottomIfPinned();
      break;
    }
    case 'error': {
      if (!activeTurn) break;
      activeTurn.turn.assistantMarkdown += `\n\n> [!CAUTION]\n> ${ev.message}\n`;
      scheduleMarkdownUpdate(activeTurn);
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
        activeTurn.toolsContainer
          .querySelectorAll<HTMLButtonElement>('.permission-actions button')
          .forEach((btn) => {
            btn.disabled = true;
          });
        activeTurn.workingBadge.classList.add('hidden');
        activeTurn.skeletonEl.classList.add('hidden');
        activeTurn.footerEl.classList.remove('hidden');
        if (activeTurn.turn.thinkingText) {
          if (activeTurn.thinkingStartedAt > 0 && activeTurn.turn.thinkingDurationMs === 0) {
            activeTurn.turn.thinkingDurationMs = Date.now() - activeTurn.thinkingStartedAt;
          }
          const ms = activeTurn.turn.thinkingDurationMs || activeTurn.turn.metrics?.durationMs || 0;
          activeTurn.thinkingTriggerText.textContent = ms
            ? `Thought for ${(ms / 1000).toFixed(1)}s`
            : 'Thought';
        }
        activeTurn.markdownEl.innerHTML = renderMarkdownHtml(
          activeTurn.turn.assistantMarkdown,
        );
        void hydrateRenderedMarkdown(activeTurn.markdownEl, false).then(() => {
          scrollToBottomIfPinned();
        });
        btnClearMarkup.classList.toggle(
          'hidden',
          !/\[\[markup:/i.test(activeTurn.turn.assistantMarkdown),
        );
        void window.agentOverlay.saveSession({
          id: currentSessionId,
          title: currentSessionTitle,
          harness: catalog.harness,
          workspaceDir: currentSessionWorkspaceDir || settings.workspaceDir,
          updatedAt: Date.now(),
          turns,
        });
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

function openCaptureMenu(): void {
  popovers.openList({
    anchor: btnCaptureMenu,
    placement: 'above',
    headerText: 'Screen & Context',
    items: [
      {
        id: 'full',
        title: 'Capture Full Screen',
        icon: 'codicon-device-camera',
        onSelect: () => void executeLocalCommand('screen'),
      },
      {
        id: 'markup',
        title: 'Annotate Screen (Markup)',
        icon: 'codicon-edit',
        onSelect: () => void executeLocalCommand('markup'),
      },
      {
        id: 'region',
        title: 'Capture Region',
        icon: 'codicon-symbol-structure',
        onSelect: () => void executeLocalCommand('region'),
      },
      {
        id: 'window',
        title: 'Capture Active Window',
        icon: 'codicon-window',
        onSelect: () => void executeLocalCommand('window'),
      },
      {
        id: 'clipboard',
        title: 'Attach Clipboard',
        icon: 'codicon-clippy',
        onSelect: async () => {
          const clip = await window.agentOverlay.readClipboardContext();
          if (clip) addAttachment(clip);
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

async function openHistoryModal(): Promise<void> {
  modalTitle.textContent = 'Conversation History';
  const sessions = await window.agentOverlay.listSessions();
  modalBody.innerHTML = '';

  if (sessions.length === 0) {
    modalBody.innerHTML =
      '<div style="color:var(--text-muted);text-align:center;padding:18px;">No saved conversations yet.</div>';
  } else {
    for (const s of sessions) {
      const row = document.createElement('div');
      row.className = 'history-item';
      const info = document.createElement('div');
      const hTitle = document.createElement('div');
      hTitle.style.cssText = 'font-weight:500;color:var(--text-strong);';
      hTitle.textContent = s.title;
      const hMeta = document.createElement('div');
      hMeta.style.cssText = 'font-size:11px;color:var(--text-muted);';
      hMeta.textContent = `${s.harness} · ${s.turns.length} turns · ${new Date(s.updatedAt).toLocaleString()}`;
      info.append(hTitle, hMeta);
      const delBtn = document.createElement('button');
      delBtn.type = 'button';
      delBtn.className = 'icon-btn';
      delBtn.innerHTML = '<i class="codicon codicon-trash"></i>';
      delBtn.addEventListener('click', async (e) => {
        e.stopPropagation();
        await window.agentOverlay.deleteSession(s.id);
        void openHistoryModal();
      });

      row.addEventListener('click', () => {
        if (isStreaming) return;
        currentSessionId = s.id;
        currentSessionTitle = s.title;
        currentSessionWorkspaceDir = s.workspaceDir || settings.workspaceDir;
        sessionTitleChip.textContent = s.title;
        turns = structuredClone(s.turns);
        needsContextReplay = turns.length > 0;
        undoStack = [];
        redoStack = [];
        rebuildMessagesList();
        renderHeaderAndPickers();
        void refreshGitGlance();
        closeModal();
      });

      row.append(info, delBtn);
      modalBody.appendChild(row);
    }
  }
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

btnWorkspace.addEventListener('click', () => {
  void window.agentOverlay.pickWorkspaceFolder();
});
btnHarness.addEventListener('click', openHarnessMenu);
btnRetryWarmup.addEventListener('click', () => {
  void window.agentOverlay.resetSession();
});

btnAddContext.addEventListener('click', openCaptureMenu);
btnModePicker.addEventListener('click', openModePicker);
btnModelPicker.addEventListener('click', openModelPicker);
btnEffortPicker.addEventListener('click', openEffortPicker);
btnCaptureMenu.addEventListener('click', openCaptureMenu);

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
        args: JSON.stringify({ command: 'git status -sb', cwd: '/var/home/khoipm/playground/pi-overlay' }),
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

async function bootstrap(): Promise<void> {
  settings = await window.agentOverlay.getSettings();
  currentSessionWorkspaceDir = settings.workspaceDir;
  catalog = await window.agentOverlay.getCatalog();
  renderHeaderAndPickers();
  updateSendButton();
  void refreshGitGlance();

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
  composerInput.focus();

  if (window.location.search.includes('audit=1')) {
    void runAuditFlow();
  }
}

void bootstrap();
