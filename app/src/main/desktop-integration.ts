import { execFile } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

export function resolveLinuxLaunchExecutable(): string {
  return process.env.APPIMAGE || process.execPath;
}

function quoteDesktopExec(execPath: string): string {
  if (/[\s"'$`\\]/.test(execPath)) {
    return `"${execPath.replace(/(["\\$`])/g, '\\$1')}"`;
  }
  return execPath;
}

export interface DesktopIntegrationOptions {
  execPath?: string;
  iconSourcePath?: string | null;
  applicationsDir?: string;
  iconsDir?: string;
  skipDesktopTools?: boolean;
}

export function syncLinuxAppImageDesktopEntry(opts: DesktopIntegrationOptions = {}): boolean {
  if (process.platform !== 'linux') return false;
  const execPath = opts.execPath || resolveLinuxLaunchExecutable();
  if (!execPath) return false;

  const dataHome = process.env.XDG_DATA_HOME || path.join(os.homedir(), '.local', 'share');
  const applicationsDir = opts.applicationsDir || path.join(dataHome, 'applications');
  const iconsDir = opts.iconsDir || path.join(dataHome, 'icons', 'hicolor', '256x256', 'apps');
  const desktopFilePath = path.join(applicationsDir, 'chat-overlay.desktop');
  const iconDestPath = path.join(iconsDir, 'chat-overlay.png');

  if (opts.iconSourcePath && fs.existsSync(opts.iconSourcePath)) {
    try {
      fs.mkdirSync(iconsDir, { recursive: true });
      const srcStat = fs.statSync(opts.iconSourcePath);
      const needsCopy =
        !fs.existsSync(iconDestPath) ||
        fs.statSync(iconDestPath).size !== srcStat.size ||
        fs.statSync(iconDestPath).mtimeMs < srcStat.mtimeMs;
      if (needsCopy) {
        fs.copyFileSync(opts.iconSourcePath, iconDestPath);
      }
    } catch {
      // ignore non-critical icon copy failure
    }
  }

  const desktopContent = [
    '[Desktop Entry]',
    'Name=Chat Overlay',
    'Comment=Universal cross-platform ACP coding and screen assistant overlay',
    `Exec=${quoteDesktopExec(execPath)}`,
    'Icon=chat-overlay',
    'Terminal=false',
    'Type=Application',
    'Categories=Development;Utility;',
    'StartupNotify=false',
    'DBusActivatable=false',
    'StartupWMClass=chat-overlay',
    'X-KDE-Shortcuts=Ctrl+Alt+L,Meta+Shift+L',
    'X-KDE-DBUS-Restricted-Interfaces=org.kde.KWin.ScreenShot2',
    '',
  ].join('\n');

  let changed = false;
  try {
    const prev = fs.existsSync(desktopFilePath) ? fs.readFileSync(desktopFilePath, 'utf8') : '';
    if (prev !== desktopContent) {
      fs.mkdirSync(applicationsDir, { recursive: true });
      fs.writeFileSync(desktopFilePath, desktopContent, { encoding: 'utf8', mode: 0o644 });
      changed = true;
    }
  } catch {
    return false;
  }

  if (changed && !opts.skipDesktopTools) {
    try {
      execFile('update-desktop-database', [applicationsDir], () => {});
    } catch {
      // ignore
    }
    try {
      execFile(
        'kwriteconfig6',
        [
          '--file',
          'kglobalshortcutsrc',
          '--group',
          'services',
          '--group',
          'chat-overlay.desktop',
          '--key',
          '_launch',
          'Ctrl+Alt+L\tMeta+Shift+L,Ctrl+Alt+L\tMeta+Shift+L,Chat Overlay',
        ],
        () => {},
      );
    } catch {
      // ignore on non-KDE desktops
    }
    try {
      execFile('kbuildsycoca6', ['--noincremental'], () => {});
    } catch {
      // ignore on non-KDE desktops
    }
  }

  return changed;
}
