import { app } from 'electron';

export type UpdateState =
  | 'idle'
  | 'checking'
  | 'available'
  | 'downloading'
  | 'downloaded'
  | 'not-available'
  | 'error';

export interface UpdateStatus {
  state: UpdateState;
  version?: string;
  percent?: number;
  message?: string;
}

type StatusEmitter = (status: UpdateStatus) => void;

let currentStatus: UpdateStatus = { state: 'idle' };
let statusEmitter: StatusEmitter | null = null;
let initialized = false;

function getAutoUpdater(): any | null {
  try {
    // eslint-disable-next-line @typescript-eslint/no-var-requires
    const mod = require('electron-updater');
    return mod?.autoUpdater || null;
  } catch {
    return null;
  }
}

function updateAndEmit(next: UpdateStatus): void {
  currentStatus = next;
  statusEmitter?.(next);
}

export function getUpdateStatus(): UpdateStatus {
  return currentStatus;
}

export function initAutoUpdater(emit: StatusEmitter): void {
  statusEmitter = emit;
  if (initialized) return;
  initialized = true;

  if (!app.isPackaged) {
    return;
  }

  const updater = getAutoUpdater();
  if (!updater) {
    return;
  }

  updater.autoDownload = false;
  updater.autoInstallOnAppQuit = true;

  updater.on('checking-for-update', () => {
    updateAndEmit({ state: 'checking', message: 'Checking for updates...' });
  });

  updater.on('update-available', (info: { version?: string }) => {
    updateAndEmit({
      state: 'available',
      version: info?.version,
      message: info?.version ? `Update v${info.version} available` : 'Update available',
    });
  });

  updater.on('update-not-available', (info: { version?: string }) => {
    updateAndEmit({
      state: 'not-available',
      version: info?.version,
      message: 'You are on the latest version',
    });
  });

  updater.on('download-progress', (prog: { percent?: number }) => {
    const pct = Math.max(0, Math.min(100, Math.round(prog?.percent ?? 0)));
    updateAndEmit({
      state: 'downloading',
      version: currentStatus.version,
      percent: pct,
      message: `Downloading update (${pct}%)...`,
    });
  });

  updater.on('update-downloaded', (info: { version?: string }) => {
    updateAndEmit({
      state: 'downloaded',
      version: info?.version || currentStatus.version,
      percent: 100,
      message: `Update v${info?.version || currentStatus.version || ''} ready to install`,
    });
  });

  updater.on('error', (err: Error) => {
    updateAndEmit({
      state: 'error',
      message: err?.message || 'Update check failed',
    });
  });
}

export async function checkForUpdates(): Promise<UpdateStatus> {
  if (!app.isPackaged) {
    const st: UpdateStatus = {
      state: 'not-available',
      version: app.getVersion(),
      message: 'Development build',
    };
    updateAndEmit(st);
    return st;
  }

  const updater = getAutoUpdater();
  if (!updater) {
    const st: UpdateStatus = {
      state: 'error',
      message: 'Auto-updater unavailable in this build',
    };
    updateAndEmit(st);
    return st;
  }

  try {
    updateAndEmit({ state: 'checking', message: 'Checking for updates...' });
    await updater.checkForUpdates();
  } catch (err) {
    updateAndEmit({
      state: 'error',
      message: err instanceof Error ? err.message : String(err),
    });
  }
  return currentStatus;
}

export async function downloadUpdate(): Promise<UpdateStatus> {
  const updater = getAutoUpdater();
  if (!app.isPackaged || !updater) {
    return currentStatus;
  }
  try {
    updateAndEmit({
      state: 'downloading',
      version: currentStatus.version,
      percent: 0,
      message: 'Downloading update...',
    });
    await updater.downloadUpdate();
  } catch (err) {
    updateAndEmit({
      state: 'error',
      message: err instanceof Error ? err.message : String(err),
    });
  }
  return currentStatus;
}

export function installUpdateNow(): void {
  const updater = getAutoUpdater();
  if (app.isPackaged && updater) {
    updater.quitAndInstall();
  }
}
