import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import type { AppSettings, HarnessKind, SavedSession } from '../shared/types';
import { HARNESS_LIST } from '../shared/types';

export function resolveExecutable(configured: string): string {
  const trimmed = configured.trim();
  const isWin = process.platform === 'win32';
  const exts = isWin ? ['', '.exe', '.cmd', '.bat'] : [''];

  const isFile = (candidate: string): boolean => {
    try {
      return fs.statSync(candidate).isFile();
    } catch {
      return false;
    }
  };

  if (path.isAbsolute(trimmed) || trimmed.includes(path.sep)) {
    for (const ext of exts) {
      if (isFile(trimmed + ext)) return trimmed + ext;
    }
    return trimmed;
  }

  const pathDirs = (process.env.PATH ?? '').split(path.delimiter).filter(Boolean);
  const home = os.homedir();
  const extraDirs = [
    path.join(home, '.local', 'bin'),
    path.join(home, '.opencode', 'bin'),
    path.join(home, '.bun', 'bin'),
    path.join(home, '.cargo', 'bin'),
    '/usr/local/bin',
  ];

  for (const dir of [...pathDirs, ...extraDirs]) {
    for (const ext of exts) {
      const candidate = path.join(dir, trimmed + ext);
      if (isFile(candidate)) return candidate;
    }
  }

  return trimmed;
}

export function defaultWorkspaceDir(): string {
  const cwd = process.cwd();
  // When launched from /app, default to parent playground if Cargo.toml / workspace is there, else cwd
  if (path.basename(cwd) === 'app') {
    const parent = path.dirname(cwd);
    if (fs.existsSync(parent)) return parent;
  }
  return cwd || os.homedir();
}

export function defaultSettings(): AppSettings {
  return {
    harness: 'agy_acp',
    workspaceDir: defaultWorkspaceDir(),
    commands: {
      agy_acp: { command: 'agy', args: ['acp'] },
      opencode_acp: { command: 'opencode', args: ['acp'] },
      pi_acp: { command: 'pi', args: ['acp'] },
    },
    harnessModels: {},
    harnessEfforts: {},
    harnessModes: {},
    revertMode: 'workspace_and_chat',
    autoApprovePermissions: false,
    pinLiveScreen: false,
    autoHideOnBlur: false,
    fontSize: 13,
    edge: 'right',
    shortcut: 'CommandOrControl+Alt+L',
    backupShortcut: 'Super+Shift+L',
  };
}

export function configDir(): string {
  const base =
    process.env.XDG_CONFIG_HOME ||
    (process.platform === 'win32'
      ? process.env.APPDATA || path.join(os.homedir(), 'AppData', 'Roaming')
      : process.platform === 'darwin'
        ? path.join(os.homedir(), 'Library', 'Application Support')
        : path.join(os.homedir(), '.config'));
  return path.join(base, 'chat-overlay');
}

function legacyConfigDir(): string {
  return path.join(path.dirname(configDir()), 'agent-overlay');
}

function readWithMigration(filename: string): string | null {
  const primary = path.join(configDir(), filename);
  if (fs.existsSync(primary)) {
    return fs.readFileSync(primary, 'utf8');
  }
  const legacy = path.join(legacyConfigDir(), filename);
  if (fs.existsSync(legacy)) {
    return fs.readFileSync(legacy, 'utf8');
  }
  return null;
}

export function settingsFilePath(): string {
  return path.join(configDir(), 'settings.json');
}

export function sessionsFilePath(): string {
  return path.join(configDir(), 'sessions.json');
}

export function loadSettings(): AppSettings {
  const defaults = defaultSettings();
  try {
    const raw = readWithMigration('settings.json');
    if (!raw) return defaults;
    const parsed = JSON.parse(raw) as Partial<AppSettings>;
    const validHarness = HARNESS_LIST.some((h) => h.id === parsed.harness)
      ? (parsed.harness as HarnessKind)
      : defaults.harness;

    return {
      ...defaults,
      ...parsed,
      harness: validHarness,
      commands: {
        ...defaults.commands,
        ...(parsed.commands ?? {}),
      },
      harnessModels: { ...(parsed.harnessModels ?? {}) },
      harnessEfforts: { ...(parsed.harnessEfforts ?? {}) },
      harnessModes: { ...(parsed.harnessModes ?? {}) },
      workspaceDir:
        parsed.workspaceDir && fs.existsSync(parsed.workspaceDir)
          ? parsed.workspaceDir
          : defaults.workspaceDir,
    };
  } catch {
    return defaults;
  }
}

export function saveSettings(settings: AppSettings): void {
  fs.mkdirSync(configDir(), { recursive: true });
  fs.writeFileSync(settingsFilePath(), JSON.stringify(settings, null, 2), 'utf8');
}

export function loadSessions(): SavedSession[] {
  try {
    const raw = readWithMigration('sessions.json');
    if (!raw) return [];
    const parsed = JSON.parse(raw);
    return Array.isArray(parsed) ? (parsed as SavedSession[]) : [];
  } catch {
    return [];
  }
}

export function saveSessionRecord(session: SavedSession): SavedSession[] {
  const existing = loadSessions().filter((s) => s.id !== session.id);
  const next = [session, ...existing].slice(0, 40);
  fs.mkdirSync(configDir(), { recursive: true });
  fs.writeFileSync(sessionsFilePath(), JSON.stringify(next, null, 2), 'utf8');
  return next;
}

export function deleteSessionRecord(sessionId: string): SavedSession[] {
  const next = loadSessions().filter((s) => s.id !== sessionId);
  fs.mkdirSync(configDir(), { recursive: true });
  fs.writeFileSync(sessionsFilePath(), JSON.stringify(next, null, 2), 'utf8');
  return next;
}
