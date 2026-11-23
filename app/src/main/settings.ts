import { execFile } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { promisify } from 'node:util';
import type { AppSettings, HarnessDiagnostic, HarnessKind, SavedSession } from '../shared/types';
import { HARNESS_LIST } from '../shared/types';

const execFileAsync = promisify(execFile);

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
    path.join(home, '.npm-global', 'bin'),
    path.join(home, '.local', 'share', 'mise', 'shims'),
    '/home/linuxbrew/.linuxbrew/bin',
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

export const CURRENT_LAYOUT_VERSION = 3;

export function defaultSettings(): AppSettings {
  return {
    layoutVersion: CURRENT_LAYOUT_VERSION,
    harness: 'agy_acp',
    workspaceDir: defaultWorkspaceDir(),
    commands: {
      agy_acp: { command: 'agy', args: ['acp'] },
      opencode_acp: { command: 'opencode', args: ['acp'] },
      codex_acp: { command: 'codex-acp', args: [] },
      claude_acp: { command: 'claude-agent-acp', args: [] },
      pi_acp: { command: 'pi-acp', args: [] },
    },
    harnessEnv: {},
    harnessModels: {},
    harnessEfforts: {},
    harnessModes: {},
    revertMode: 'workspace_and_chat',
    permissionPolicy: 'auto_read',
    desktopControlMode: 'confirm',
    autoApprovePermissions: false,
    autoApproveDesktopActions: true,
    speechLanguage: 'vi-VN',
    followUpBehavior: 'queue',
    soundCues: true,
    flashOnAttention: true,
    backgroundNotifications: true,
    showGitGlance: true,
    screenshotQuality: 'compressed',
    markupAutoClearSeconds: 12,
    pinLiveScreen: false,
    autoHideOnBlur: false,
    dimOnBlur: false,
    opacity: 1,
    fontSize: 13,
    edge: 'right',
    widthPreset: 'wide_25',
    heightPreset: 'balanced_78',
    launchAtLogin: false,
    idleSleepMinutes: 15,
    shortcut: 'CommandOrControl+Alt+L',
    backupShortcut: 'Super+Shift+L',
    ghostShortcut: 'CommandOrControl+Alt+G',
    quickClipboardShortcut: 'CommandOrControl+Alt+Q',
    panicShortcut: 'CommandOrControl+Alt+X',
  };
}

export function resolveHarnessLaunch(
  settings: AppSettings,
  harness: HarnessKind,
): { binary: string; args: string[]; viaRunner: boolean; meta: (typeof HARNESS_LIST)[number] } {
  const meta = HARNESS_LIST.find((h) => h.id === harness) ?? HARNESS_LIST[0];
  const cmdCfg = settings.commands?.[harness] ?? {
    command: meta.defaultCommand,
    args: [...meta.defaultArgs],
  };
  const directCmd = cmdCfg.command || meta.defaultCommand;
  const resolved = resolveExecutable(directCmd);
  const directArgs = cmdCfg.args.length > 0 ? cmdCfg.args : [...meta.defaultArgs];

  if (fs.existsSync(resolved)) {
    return { binary: resolved, args: directArgs, viaRunner: false, meta };
  }

  if (meta.npmPackage) {
    const npx = resolveExecutable('npx');
    if (fs.existsSync(npx)) {
      return {
        binary: npx,
        args: ['-y', meta.npmPackage, ...directArgs],
        viaRunner: true,
        meta,
      };
    }
    const bunx = resolveExecutable('bunx');
    if (fs.existsSync(bunx)) {
      return {
        binary: bunx,
        args: [meta.npmPackage, ...directArgs],
        viaRunner: true,
        meta,
      };
    }
  }

  return { binary: resolved, args: directArgs, viaRunner: false, meta };
}

export async function installHarnessPackage(
  harness: HarnessKind,
): Promise<{ ok: boolean; message: string }> {
  const meta = HARNESS_LIST.find((h) => h.id === harness);
  if (!meta?.npmPackage) {
    return { ok: false, message: 'No installable npm package configured for this harness.' };
  }
  const bunBin = resolveExecutable('bun');
  const npmBin = resolveExecutable('npm');
  const installer = fs.existsSync(bunBin) ? bunBin : npmBin;
  const installArgs = fs.existsSync(bunBin)
    ? ['add', '-g', meta.npmPackage]
    : ['install', '-g', meta.npmPackage];

  if (!fs.existsSync(installer)) {
    return { ok: false, message: 'Neither npm nor bun was found on PATH.' };
  }

  try {
    await execFileAsync(installer, installArgs, { timeout: 120_000 });
    return { ok: true, message: `Installed ${meta.npmPackage}` };
  } catch (err) {
    return {
      ok: false,
      message: `Install failed: ${err instanceof Error ? err.message : String(err)}`,
    };
  }
}

export async function inspectHarnesses(settings: AppSettings): Promise<HarnessDiagnostic[]> {
  return Promise.all(
    HARNESS_LIST.map(async (meta) => {
      const cmd = settings.commands?.[meta.id]?.command || meta.defaultCommand;
      const resolvedPath = resolveExecutable(cmd);
      const installed = fs.existsSync(resolvedPath);
      const launch = resolveHarnessLaunch(settings, meta.id);
      let version: string | null = null;
      if (installed) {
        try {
          const { stdout } = await execFileAsync(resolvedPath, ['--version'], { timeout: 2000 });
          version = stdout.trim().split('\n')[0]?.slice(0, 48) || null;
        } catch {
          version = null;
        }
      }
      return {
        id: meta.id,
        label: meta.label,
        installed,
        resolvedPath: installed ? resolvedPath : launch.viaRunner ? `${launch.binary} ${launch.args.join(' ')}` : resolvedPath,
        version,
        viaRunner: !installed && launch.viaRunner,
        npmPackage: meta.npmPackage,
      };
    }),
  );
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

    const prevLayout = parsed.layoutVersion ?? 1;
    const isLegacyLayout = prevLayout < CURRENT_LAYOUT_VERSION;
    const migratedBounds = isLegacyLayout ? undefined : parsed.windowBounds;
    const widthPreset = isLegacyLayout ? defaults.widthPreset : (parsed.widthPreset ?? defaults.widthPreset);
    const heightPreset = isLegacyLayout ? defaults.heightPreset : (parsed.heightPreset ?? defaults.heightPreset);
    const permissionPolicy =
      parsed.permissionPolicy ??
      (parsed.autoApprovePermissions ? 'yolo' : defaults.permissionPolicy);

    const mergedCommands = {
      ...defaults.commands,
      ...(parsed.commands ?? {}),
    };
    // Migrate legacy default `pi acp` (unsupported in modern `pi` CLI) to the `pi-acp` adapter
    if (
      mergedCommands.pi_acp?.command === 'pi' &&
      mergedCommands.pi_acp.args?.length === 1 &&
      mergedCommands.pi_acp.args[0] === 'acp'
    ) {
      mergedCommands.pi_acp = { command: 'pi-acp', args: [] };
    }

    return {
      ...defaults,
      ...parsed,
      layoutVersion: CURRENT_LAYOUT_VERSION,
      widthPreset,
      heightPreset,
      windowBounds: migratedBounds,
      permissionPolicy,
      autoApproveDesktopActions: parsed.autoApproveDesktopActions ?? defaults.autoApproveDesktopActions,
      speechLanguage: parsed.speechLanguage ?? defaults.speechLanguage,
      harness: validHarness,
      commands: mergedCommands,
      harnessEnv: { ...(parsed.harnessEnv ?? {}) },
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

export function linuxAutostartFilePath(autostartDir?: string): string {
  const base =
    autostartDir ||
    path.join(process.env.XDG_CONFIG_HOME || path.join(os.homedir(), '.config'), 'autostart');
  return path.join(base, 'chat-overlay.desktop');
}

export function syncLinuxAutostartDesktopFile(
  enabled: boolean,
  execLine: string,
  autostartDir?: string,
): void {
  const target = linuxAutostartFilePath(autostartDir);
  if (!enabled) {
    if (fs.existsSync(target)) {
      fs.rmSync(target, { force: true });
    }
    return;
  }
  fs.mkdirSync(path.dirname(target), { recursive: true });
  const content = [
    '[Desktop Entry]',
    'Type=Application',
    'Version=1.0',
    'Name=Chat Overlay',
    'Comment=Cross-platform ACP coding and screen assistant overlay',
    `Exec=${execLine}`,
    'Terminal=false',
    'X-GNOME-Autostart-enabled=true',
    'X-KDE-DBUS-Restricted-Interfaces=org.kde.KWin.ScreenShot2',
    'StartupNotify=false',
    '',
  ].join('\n');
  fs.writeFileSync(target, content, 'utf8');
}

export function deleteSessionRecord(sessionId: string): SavedSession[] {
  const next = loadSessions().filter((s) => s.id !== sessionId);
  fs.mkdirSync(configDir(), { recursive: true });
  fs.writeFileSync(sessionsFilePath(), JSON.stringify(next, null, 2), 'utf8');
  return next;
}
