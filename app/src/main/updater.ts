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
let suppressAutoErrorDuringCheck = false;

export function compareSemver(a: string, b: string): number {
  const parse = (v: string) =>
    v
      .replace(/^v/i, '')
      .split('-')[0]
      .split('.')
      .map((n) => Number.parseInt(n, 10) || 0);
  const pa = parse(a);
  const pb = parse(b);
  const len = Math.max(pa.length, pb.length, 3);
  for (let i = 0; i < len; i++) {
    const da = pa[i] ?? 0;
    const db = pb[i] ?? 0;
    if (da > db) return 1;
    if (da < db) return -1;
  }
  return 0;
}

export function formatUpdaterError(err: unknown): string {
  const raw = err instanceof Error ? err.message : String(err || '');
  if (!raw.trim()) return 'Update check failed.';

  if (/404|cannot find latest/i.test(raw)) {
    return 'Release update metadata is not available yet.';
  }
  if (/enotfound|econnrefused|etimedout|err_internet_disconnected|err_name_not_resolved|fetch failed/i.test(raw)) {
    return 'Cannot reach update server. Check your internet connection.';
  }

  let cleaned = raw
    .replace(/Headers:\s*\{[\s\S]*$/m, '')
    .replace(/\n\s+at\s+[\s\S]*$/m, '')
    .replace(/\/tmp\/\.mount_[^\s)]+/g, '[app-bundle]')
    .replace(/\s+/g, ' ')
    .trim();

  if (!cleaned) return 'Update check failed.';
  if (cleaned.length > 140) {
    cleaned = `${cleaned.slice(0, 137)}...`;
  }
  return cleaned;
}

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
    if (suppressAutoErrorDuringCheck) return;
    updateAndEmit({
      state: 'error',
      message: formatUpdaterError(err),
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

  suppressAutoErrorDuringCheck = true;
  try {
    updateAndEmit({ state: 'checking', message: 'Checking for updates...' });
    await updater.checkForUpdates();
  } catch (err) {
    const raw = err instanceof Error ? err.message : String(err);
    if (/404|Cannot find latest/i.test(raw)) {
      try {
        const res = await fetch('https://api.github.com/repos/khoipm06/chat-overlay/releases/latest', {
          headers: { Accept: 'application/vnd.github+json', 'User-Agent': 'chat-overlay-updater' },
          signal: AbortSignal.timeout(5000),
        });
        if (res.ok) {
          const data = (await res.json()) as { tag_name?: string };
          const latestTag = String(data.tag_name || '').replace(/^v/i, '');
          const cur = app.getVersion();
          if (latestTag && compareSemver(latestTag, cur) <= 0) {
            updateAndEmit({
              state: 'not-available',
              version: cur,
              message: 'You are on the latest version',
            });
            return currentStatus;
          }
          if (latestTag) {
            updateAndEmit({
              state: 'available',
              version: latestTag,
              message: `Update v${latestTag} available on GitHub Releases`,
            });
            return currentStatus;
          }
        }
      } catch {
        // fall through to clean error formatting
      }
    }
    updateAndEmit({
      state: 'error',
      message: formatUpdaterError(err),
    });
  } finally {
    suppressAutoErrorDuringCheck = false;
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
      message: formatUpdaterError(err),
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
