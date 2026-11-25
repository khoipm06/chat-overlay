import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

export function appDataDir(): string {
  const home = os.homedir();
  let base: string;
  if (process.platform === 'win32') {
    base = process.env.LOCALAPPDATA || path.join(home, 'AppData', 'Local');
  } else if (process.platform === 'darwin') {
    base = path.join(home, 'Library', 'Application Support');
  } else {
    base = process.env.XDG_DATA_HOME || path.join(home, '.local', 'share');
  }
  return path.join(base, 'chat-overlay');
}

export function managedBinDir(): string {
  return path.join(appDataDir(), 'bin');
}

export function bundledBinDir(): string | null {
  const resPath = (process as NodeJS.Process & { resourcesPath?: string }).resourcesPath;
  if (!resPath) return null;
  const candidate = path.join(resPath, 'bin');
  try {
    if (fs.existsSync(candidate)) return candidate;
  } catch {
    // ignore
  }
  return null;
}

export function sanitizeDefaultWorkspaceDir(
  cwd: string,
  homeDir = os.homedir(),
  resourcesPath = (process as NodeJS.Process & { resourcesPath?: string }).resourcesPath || '',
): string {
  const trimmed = (cwd || '').trim();
  const normalized = trimmed.replace(/\\/g, '/');
  if (!trimmed || trimmed === '/') {
    return homeDir;
  }
  if (
    normalized.startsWith('/tmp/.mount_') ||
    normalized === '/usr' ||
    normalized.startsWith('/usr/') ||
    normalized === '/opt' ||
    normalized.startsWith('/opt/') ||
    normalized.startsWith('/Applications/') ||
    normalized.includes('/AppData/Local/Programs/') ||
    (resourcesPath && trimmed.startsWith(resourcesPath))
  ) {
    return homeDir;
  }
  return trimmed;
}
