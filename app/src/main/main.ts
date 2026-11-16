import {
  app,
  BrowserWindow,
  clipboard,
  dialog,
  globalShortcut,
  ipcMain,
  Menu,
  nativeImage,
  screen,
  shell,
  Tray,
} from 'electron';
import fs from 'node:fs';
import net from 'node:net';
import os from 'node:os';
import path from 'node:path';
import type {
  AppSettings,
  ChatAttachment,
  HarnessKind,
  MarkupAnnotation,
  SavedSession,
  StreamEvent,
} from '../shared/types';
import { AcpAgentController } from './acp-client';
import { captureScreenDataUrl, type CaptureKind } from './capture';
import { captureWorkspaceCheckpoint, restoreWorkspaceCheckpoint } from './checkpoint';
import { indexWorkspaceFiles, matchMentionItems } from './mentions';
import {
  deleteSessionRecord,
  loadSessions,
  loadSettings,
  saveSessionRecord,
  saveSettings,
} from './settings';

let mainWindow: BrowserWindow | null = null;
let markupWindow: BrowserWindow | null = null;
let tray: Tray | null = null;
let settings: AppSettings = loadSettings();
let workspaceFileCache: string[] = [];
let cacheWorkspaceDir = '';
let acpController: AcpAgentController;
let ipcSocketServer: net.Server | null = null;
let quitting = false;

const START_HIDDEN = process.argv.includes('--daemon');
const AUDIT_ARG = process.argv.find((a) => a.startsWith('--audit-dir='));
const AUDIT_DIR = AUDIT_ARG ? AUDIT_ARG.slice('--audit-dir='.length) : null;

function sendStreamEvent(ev: StreamEvent): void {
  if (mainWindow && !mainWindow.isDestroyed()) {
    mainWindow.webContents.send('acp:stream-event', ev);
  }
}

function createTrayIcon(): Electron.NativeImage {
  // 16x16 minimal crisp dark badge PNG (encoded dynamically via minimal SVG data URL)
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="22" height="22" viewBox="0 0 24 24">
    <rect x="2" y="2" width="20" height="20" rx="5" fill="#18181b" stroke="#0078d4" stroke-width="1.6"/>
    <path d="M8 9l4 3-4 3m5 0h3" stroke="#e4e4e7" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" fill="none"/>
  </svg>`;
  return nativeImage.createFromDataURL(
    `data:image/svg+xml;base64,${Buffer.from(svg).toString('base64')}`,
  );
}

function computeDefaultBounds(): { x: number; y: number; width: number; height: number } {
  const display = screen.getPrimaryDisplay();
  const wa = display.workArea;
  const width = settings.windowBounds?.width ?? Math.min(460, Math.round(wa.width * 0.36));
  const height = settings.windowBounds?.height ?? Math.min(820, wa.height - 32);
  const margin = 16;
  const x =
    settings.windowBounds?.x ??
    (settings.edge === 'left' ? wa.x + margin : wa.x + wa.width - width - margin);
  const y = settings.windowBounds?.y ?? wa.y + margin;
  return { x, y, width, height };
}

function createMainWindow(): void {
  const bounds = computeDefaultBounds();
  mainWindow = new BrowserWindow({
    x: bounds.x,
    y: bounds.y,
    width: bounds.width,
    height: bounds.height,
    minWidth: 340,
    minHeight: 420,
    frame: false,
    transparent: false,
    backgroundColor: '#181818',
    alwaysOnTop: true,
    resizable: true,
    show: !START_HIDDEN,
    title: 'Chat Overlay',
    webPreferences: {
      preload: path.join(__dirname, '../preload/preload.js'),
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: false,
    },
  });

  mainWindow.setVisibleOnAllWorkspaces(true, { visibleOnFullScreen: true });
  mainWindow.loadFile(path.join(__dirname, '../renderer/index.html'), {
    query: AUDIT_DIR ? { audit: '1' } : undefined,
  });

  if (AUDIT_DIR) {
    mainWindow.webContents.on('console-message', (_e, level, msg) => {
      if (level >= 2) {
        console.error(`[renderer-console level=${level}] ${msg}`);
      }
    });
  }

  const saveBounds = () => {
    if (!mainWindow || mainWindow.isDestroyed()) return;
    const b = mainWindow.getBounds();
    settings = { ...settings, windowBounds: b };
    saveSettings(settings);
  };
  mainWindow.on('moved', saveBounds);
  mainWindow.on('resized', saveBounds);

  mainWindow.on('blur', () => {
    if (settings.autoHideOnBlur && mainWindow && !mainWindow.isDestroyed() && !markupWindow) {
      mainWindow.hide();
    }
  });

  mainWindow.on('close', (e) => {
    if (!quitting) {
      e.preventDefault();
      mainWindow?.hide();
    }
  });
}

function toggleOverlay(): void {
  if (!mainWindow || mainWindow.isDestroyed()) {
    createMainWindow();
  }
  if (!mainWindow) return;
  if (mainWindow.isVisible() && mainWindow.isFocused()) {
    mainWindow.hide();
  } else {
    mainWindow.show();
    mainWindow.focus();
    mainWindow.webContents.send('window:focus-composer');
  }
}

function registerShortcuts(): void {
  globalShortcut.unregisterAll();
  for (const accel of [settings.shortcut, settings.backupShortcut]) {
    if (!accel) continue;
    try {
      globalShortcut.register(accel, () => toggleOverlay());
    } catch {
      // Wayland compositors may restrict global grabs; CLI / tray toggle still works
    }
  }
}

function setupUnixToggleSocket(): void {
  if (process.platform === 'win32') return;
  const runtimeDir = process.env.XDG_RUNTIME_DIR || os.tmpdir();
  const sockPath = path.join(runtimeDir, 'chat-overlay.sock');
  try {
    fs.unlinkSync(sockPath);
  } catch {
    // ignore missing socket
  }
  ipcSocketServer = net.createServer((conn) => {
    conn.on('data', (buf) => {
      const cmd = buf.toString('utf8').trim();
      if (cmd === 'show') {
        mainWindow?.show();
        mainWindow?.focus();
        mainWindow?.webContents.send('window:focus-composer');
      } else if (cmd === 'hide') {
        mainWindow?.hide();
      } else {
        toggleOverlay();
      }
      conn.end('ok\n');
    });
  });
  ipcSocketServer.listen(sockPath);
}

function closeMarkupWindow(): void {
  if (markupWindow && !markupWindow.isDestroyed()) {
    markupWindow.close();
  }
  markupWindow = null;
}

function openAgentMarkupOverlay(annotations: MarkupAnnotation[]): void {
  closeMarkupWindow();
  const display = screen.getPrimaryDisplay();
  const { x, y, width, height } = display.bounds;

  markupWindow = new BrowserWindow({
    x,
    y,
    width,
    height,
    frame: false,
    transparent: true,
    backgroundColor: '#00000000',
    alwaysOnTop: true,
    hasShadow: false,
    focusable: false,
    skipTaskbar: true,
    webPreferences: {
      preload: path.join(__dirname, '../preload/preload.js'),
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: false,
    },
  });

  markupWindow.setIgnoreMouseEvents(true, { forward: true });
  markupWindow.loadFile(path.join(__dirname, '../renderer/markup.html'));
  markupWindow.webContents.once('did-finish-load', () => {
    markupWindow?.webContents.send('markup:init', {
      mode: 'agent',
      annotations,
    });
  });
}

async function openInteractiveMarkupCapture(): Promise<ChatAttachment | null> {
  const shot = await captureScreenDataUrl('fullscreen', mainWindow);
  closeMarkupWindow();

  const display = screen.getPrimaryDisplay();
  const { x, y, width, height } = display.bounds;

  return new Promise((resolve) => {
    let settled = false;
    const finish = (att: ChatAttachment | null) => {
      if (settled) return;
      settled = true;
      ipcMain.removeListener('markup:submit-image', onSubmit);
      ipcMain.removeListener('markup:cancel', onCancel);
      closeMarkupWindow();
      if (mainWindow && !mainWindow.isDestroyed()) {
        mainWindow.show();
        mainWindow.focus();
      }
      resolve(att);
    };

    const onSubmit = (_ev: Electron.IpcMainEvent, dataUrl: string) => {
      finish({
        id: `att_${Date.now()}`,
        kind: 'image',
        name: `markup-${Date.now()}.png`,
        mimeType: 'image/png',
        dataUrl,
      });
    };

    const onCancel = () => finish(null);

    ipcMain.once('markup:submit-image', onSubmit);
    ipcMain.once('markup:cancel', onCancel);

    markupWindow = new BrowserWindow({
      x,
      y,
      width,
      height,
      frame: false,
      transparent: true,
      backgroundColor: '#00000000',
      alwaysOnTop: true,
      skipTaskbar: true,
      webPreferences: {
        preload: path.join(__dirname, '../preload/preload.js'),
        contextIsolation: true,
        nodeIntegration: false,
        sandbox: false,
      },
    });

    markupWindow.on('closed', () => finish(null));
    markupWindow.loadFile(path.join(__dirname, '../renderer/markup.html'));
    markupWindow.webContents.once('did-finish-load', () => {
      markupWindow?.webContents.send('markup:init', {
        mode: 'annotate',
        screenshotDataUrl: shot.dataUrl,
      });
    });
  });
}

async function refreshWorkspaceMentions(): Promise<void> {
  cacheWorkspaceDir = settings.workspaceDir;
  workspaceFileCache = await indexWorkspaceFiles(settings.workspaceDir);
}

function registerIpcHandlers(): void {
  ipcMain.handle('app:get-settings', () => settings);

  ipcMain.handle('app:update-settings', async (_ev, patch: Partial<AppSettings>) => {
    const prevWorkspace = settings.workspaceDir;
    const prevHarness = settings.harness;
    settings = {
      ...settings,
      ...patch,
      commands: { ...settings.commands, ...(patch.commands ?? {}) },
      harnessModels: { ...settings.harnessModels, ...(patch.harnessModels ?? {}) },
      harnessEfforts: { ...settings.harnessEfforts, ...(patch.harnessEfforts ?? {}) },
      harnessModes: { ...settings.harnessModes, ...(patch.harnessModes ?? {}) },
    };
    saveSettings(settings);
    acpController.setAutoApprove(settings.autoApprovePermissions);

    if (patch.shortcut !== undefined || patch.backupShortcut !== undefined) {
      registerShortcuts();
    }
    if (settings.workspaceDir !== prevWorkspace) {
      void refreshWorkspaceMentions();
      await acpController.warmup(settings);
    } else if (settings.harness !== prevHarness) {
      await acpController.warmup(settings);
    }
    return settings;
  });

  ipcMain.handle('app:pick-workspace', async () => {
    if (!mainWindow) return null;
    const res = await dialog.showOpenDialog(mainWindow, {
      title: 'Select Workspace Directory',
      defaultPath: settings.workspaceDir,
      properties: ['openDirectory'],
    });
    if (res.canceled || res.filePaths.length === 0) return null;
    const chosen = res.filePaths[0];
    settings = { ...settings, workspaceDir: chosen };
    saveSettings(settings);
    void refreshWorkspaceMentions();
    await acpController.warmup(settings);
    return chosen;
  });

  ipcMain.handle('acp:get-catalog', () => acpController.getCatalog());

  ipcMain.handle('acp:switch-harness', async (_ev, harness: HarnessKind) => {
    settings = { ...settings, harness };
    saveSettings(settings);
    return acpController.warmup(settings);
  });

  ipcMain.handle('acp:set-model', async (_ev, modelId: string) => {
    settings = {
      ...settings,
      harnessModels: { ...settings.harnessModels, [settings.harness]: modelId },
    };
    saveSettings(settings);
    return acpController.setModel(modelId, settings);
  });

  ipcMain.handle('acp:set-effort', async (_ev, value: string) => {
    settings = {
      ...settings,
      harnessEfforts: { ...settings.harnessEfforts, [settings.harness]: value },
    };
    saveSettings(settings);
    return acpController.setThinkingEffort(value);
  });

  ipcMain.handle('acp:set-mode', async (_ev, modeId: string) => {
    settings = {
      ...settings,
      harnessModes: { ...settings.harnessModes, [settings.harness]: modeId },
    };
    saveSettings(settings);
    return acpController.setMode(modeId);
  });

  ipcMain.handle('acp:reset-session', () => acpController.warmup(settings));

  ipcMain.handle('acp:send-prompt', async (_ev, prompt: string, attachments: ChatAttachment[]) => {
    await acpController.sendPrompt(prompt, attachments, settings);
  });

  ipcMain.handle('acp:cancel-turn', () => acpController.cancelTurn());

  ipcMain.handle('acp:respond-permission', (_ev, requestId: string, optionId: string | null) => {
    acpController.respondPermission(requestId, optionId);
  });

  ipcMain.handle('mentions:query', async (_ev, query: string) => {
    if (cacheWorkspaceDir !== settings.workspaceDir || workspaceFileCache.length === 0) {
      await refreshWorkspaceMentions();
    }
    return matchMentionItems(query, workspaceFileCache);
  });

  ipcMain.handle('context:clipboard', (): ChatAttachment | null => {
    const img = clipboard.readImage();
    if (!img.isEmpty()) {
      return {
        id: `att_${Date.now()}`,
        kind: 'image',
        name: `clipboard-${Date.now()}.png`,
        mimeType: 'image/png',
        dataUrl: img.toDataURL(),
      };
    }
    const text = clipboard.readText().trim();
    if (!text) return null;
    return {
      id: `att_${Date.now()}`,
      kind: 'text',
      name: 'clipboard.txt',
      content: text,
    };
  });

  ipcMain.handle('capture:screen', async (_ev, kind: CaptureKind): Promise<ChatAttachment> => {
    const shot = await captureScreenDataUrl(kind, mainWindow);
    return {
      id: `att_${Date.now()}`,
      kind: 'image',
      name: shot.name,
      mimeType: shot.mimeType,
      dataUrl: shot.dataUrl,
    };
  });

  ipcMain.handle('capture:markup', () => openInteractiveMarkupCapture());
  ipcMain.handle('markup:clear-agent', () => closeMarkupWindow());

  ipcMain.on('markup:set-click-through', (_ev, ignore: boolean) => {
    if (markupWindow && !markupWindow.isDestroyed()) {
      markupWindow.setIgnoreMouseEvents(ignore, { forward: true });
    }
  });

  ipcMain.handle('sessions:list', () => loadSessions());
  ipcMain.handle('sessions:save', (_ev, session: SavedSession) => saveSessionRecord(session));
  ipcMain.handle('sessions:delete', (_ev, id: string) => deleteSessionRecord(id));

  ipcMain.handle('checkpoint:capture', () =>
    captureWorkspaceCheckpoint(settings.workspaceDir),
  );
  ipcMain.handle('checkpoint:restore', (_ev, treeSha: string | null) =>
    restoreWorkspaceCheckpoint(settings.workspaceDir, treeSha),
  );

  ipcMain.handle('window:hide', () => {
    mainWindow?.hide();
  });

  ipcMain.handle('audit:capture', async (_ev, filename: string) => {
    if (!AUDIT_DIR || !mainWindow || mainWindow.isDestroyed()) return;
    fs.mkdirSync(AUDIT_DIR, { recursive: true });
    await new Promise((r) => setTimeout(r, 150));
    const img = await mainWindow.webContents.capturePage();
    fs.writeFileSync(path.join(AUDIT_DIR, filename), img.toPNG());
  });

  ipcMain.handle('audit:done', () => {
    if (!AUDIT_DIR) return;
    quitting = true;
    app.quit();
  });

  ipcMain.handle('app:open-external', (_ev, url: string) => {
    if (/^https?:\/\//i.test(url)) {
      return shell.openExternal(url);
    }
    return Promise.resolve();
  });
}

const gotLock = app.requestSingleInstanceLock();
if (!gotLock) {
  app.quit();
} else {
  app.on('second-instance', () => {
    toggleOverlay();
  });

  app.whenReady().then(() => {
    acpController = new AcpAgentController(settings, sendStreamEvent, (annotations) => {
      openAgentMarkupOverlay(annotations);
    });

    registerIpcHandlers();
    createMainWindow();
    registerShortcuts();
    setupUnixToggleSocket();

    tray = new Tray(createTrayIcon());
    tray.setToolTip('Chat Overlay (ACP)');
    tray.setContextMenu(
      Menu.buildFromTemplate([
        { label: 'Toggle Overlay', click: () => toggleOverlay() },
        {
          label: 'New Conversation',
          click: () => {
            mainWindow?.show();
            mainWindow?.focus();
            void acpController.warmup(settings);
          },
        },
        { type: 'separator' },
        {
          label: 'Quit',
          click: () => {
            quitting = true;
            app.quit();
          },
        },
      ]),
    );
    tray.on('click', () => toggleOverlay());

    void refreshWorkspaceMentions();
    void acpController.warmup(settings);
  });

  app.on('before-quit', () => {
    quitting = true;
    acpController?.dispose();
    globalShortcut.unregisterAll();
    ipcSocketServer?.close();
  });
}
