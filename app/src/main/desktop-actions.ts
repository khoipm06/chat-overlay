import { execFile, spawn } from 'node:child_process';
import fsSync from 'node:fs';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { promisify } from 'node:util';
import { shell } from 'electron';
import type { DesktopAction, DesktopCapabilities } from '../shared/types';

const execFileAsync = promisify(execFile);

export interface ScreenBounds {
  x: number;
  y: number;
  width: number;
  height: number;
}

export function percentToPixelCoords(
  xPct = 50,
  yPct = 50,
  bounds: ScreenBounds,
): { px: number; py: number } {
  const clampedX = Math.min(100, Math.max(0, xPct));
  const clampedY = Math.min(100, Math.max(0, yPct));
  return {
    px: Math.round(bounds.x + (clampedX / 100) * bounds.width),
    py: Math.round(bounds.y + (clampedY / 100) * bounds.height),
  };
}

async function commandExists(cmd: string): Promise<boolean> {
  const pathDirs = (process.env.PATH ?? '').split(path.delimiter).filter(Boolean);
  for (const dir of pathDirs) {
    try {
      const stat = await fs.stat(path.join(dir, cmd));
      if (stat.isFile()) return true;
    } catch {
      // keep searching
    }
  }
  return false;
}

async function findOrStartYdotoolSocket(): Promise<string | null> {
  if (!(await commandExists('ydotool'))) return null;
  const uid = process.getuid ? process.getuid() : 1000;
  const candidates = [
    process.env.YDOTOOL_SOCKET,
    `/run/user/${uid}/.ydotool_socket`,
    '/tmp/.ydotool_socket',
  ].filter(Boolean) as string[];
  for (const sock of candidates) {
    if (fsSync.existsSync(sock)) return sock;
  }
  if (await commandExists('ydotoold')) {
    try {
      fsSync.accessSync('/dev/uinput', fsSync.constants.W_OK);
      const targetSock = process.env.YDOTOOL_SOCKET || `/run/user/${uid}/.ydotool_socket`;
      await fs.mkdir(path.dirname(targetSock), { recursive: true });
      const child = spawn('ydotoold', ['--socket-path', targetSock], {
        detached: true,
        stdio: 'ignore',
      });
      child.unref();
      for (let i = 0; i < 12; i++) {
        await new Promise((r) => setTimeout(r, 100));
        if (fsSync.existsSync(targetSock)) return targetSock;
      }
    } catch {
      // uinput not writable or spawn failed
    }
  }
  return null;
}

export async function detectDesktopCapabilities(): Promise<DesktopCapabilities> {
  if (process.platform === 'win32') {
    return {
      platform: 'win32',
      inputDriver: 'powershell-user32',
      themeControl: true,
      audioControl: true,
      windowSnap: true,
    };
  }
  if (process.platform === 'darwin') {
    const hasCliclick = await commandExists('cliclick');
    return {
      platform: 'darwin',
      inputDriver: hasCliclick ? 'cliclick' : 'osascript',
      themeControl: true,
      audioControl: true,
      windowSnap: true,
    };
  }

  const hasYdotool = await commandExists('ydotool');
  const ydotoolSock = hasYdotool ? await findOrStartYdotoolSocket() : null;
  const hasXdotool = !process.env.WAYLAND_DISPLAY && (await commandExists('xdotool'));
  const hasKwinDbus = Boolean(process.env.WAYLAND_DISPLAY) && fsSync.existsSync('/usr/bin/python3');
  const inputDriver = ydotoolSock
    ? 'ydotool'
    : hasXdotool
      ? 'xdotool'
      : hasYdotool
        ? 'ydotool (daemon unavailable)'
        : null;

  const themeControl =
    (await commandExists('plasma-apply-colorscheme')) || (await commandExists('gsettings'));
  const audioControl = (await commandExists('wpctl')) || (await commandExists('pactl'));
  const windowSnap = Boolean(ydotoolSock) || hasXdotool || hasKwinDbus || (await commandExists('kdotool'));

  let notes: string | undefined;
  if (hasYdotool && !ydotoolSock) {
    notes = 'Grant /dev/uinput access or start ydotoold to enable pointer & keyboard automation on Wayland.';
  } else if (!inputDriver) {
    notes = 'Install ydotool (Wayland) or xdotool (X11) to enable pointer & keyboard automation.';
  }

  return {
    platform: process.platform,
    inputDriver,
    themeControl,
    audioControl,
    windowSnap,
    notes,
  };
}

const LINUX_KEYCODES: Record<string, number> = {
  esc: 1,
  escape: 1,
  '1': 2, '2': 3, '3': 4, '4': 5, '5': 6, '6': 7, '7': 8, '8': 9, '9': 10, '0': 11,
  tab: 15,
  q: 16, w: 17, e: 18, r: 19, t: 20, y: 21, u: 22, i: 23, o: 24, p: 25,
  enter: 28,
  return: 28,
  ctrl: 29,
  control: 29,
  a: 30, s: 31, d: 32, f: 33, g: 34, h: 35, j: 36, k: 37, l: 38,
  shift: 42,
  z: 44, x: 45, c: 46, v: 47, b: 48, n: 49, m: 50,
  alt: 56,
  space: 57,
  up: 103,
  left: 105,
  right: 106,
  down: 108,
  super: 125,
  meta: 125,
  win: 125,
};

function toYdotoolKeySequence(combo: string): string[] | null {
  const parts = combo
    .toLowerCase()
    .split('+')
    .map((p) => p.trim())
    .filter(Boolean);
  const codes: number[] = [];
  for (const part of parts) {
    const code = LINUX_KEYCODES[part];
    if (code === undefined) return null;
    codes.push(code);
  }
  if (codes.length === 0) return null;
  const press = codes.map((c) => `${c}:1`);
  const release = [...codes].reverse().map((c) => `${c}:0`);
  return [...press, ...release];
}

async function runInputLinux(action: DesktopAction, bounds: ScreenBounds): Promise<{ ok: boolean; message: string }> {
  const { px, py } = percentToPixelCoords(action.x, action.y, bounds);
  const ySock = await findOrStartYdotoolSocket();
  if (ySock) {
    const yEnv = { ...process.env, YDOTOOL_SOCKET: ySock };
    if (action.kind === 'move' || action.kind === 'click' || action.kind === 'double_click' || action.kind === 'right_click') {
      await execFileAsync('ydotool', ['mousemove', '--absolute', '-x', String(px), '-y', String(py)], { timeout: 3000, env: yEnv });
      if (action.kind === 'click') {
        await execFileAsync('ydotool', ['click', '0xC0'], { timeout: 3000, env: yEnv });
      } else if (action.kind === 'double_click') {
        await execFileAsync('ydotool', ['click', '--repeat', '2', '--next-delay', '60', '0xC0'], { timeout: 3000, env: yEnv });
      } else if (action.kind === 'right_click') {
        await execFileAsync('ydotool', ['click', '0xC1'], { timeout: 3000, env: yEnv });
      }
      return { ok: true, message: `Executed ${action.kind} at (${px}, ${py})` };
    }
    if (action.kind === 'scroll') {
      const steps = Math.max(-20, Math.min(20, Math.round(action.delta ?? -3)));
      await execFileAsync('ydotool', ['mousemove', '-w', '--', '0', String(steps)], { timeout: 3000, env: yEnv });
      return { ok: true, message: `Scrolled ${steps}` };
    }
    if (action.kind === 'type' && action.text) {
      await execFileAsync('ydotool', ['type', '--', action.text], { timeout: 5000, env: yEnv });
      return { ok: true, message: `Typed ${action.text.length} chars` };
    }
    if (action.kind === 'key' && action.combo) {
      const seq = toYdotoolKeySequence(action.combo);
      if (!seq) return { ok: false, message: `Unsupported key combo: ${action.combo}` };
      await execFileAsync('ydotool', ['key', ...seq], { timeout: 3000, env: yEnv });
      return { ok: true, message: `Pressed ${action.combo}` };
    }
  }

  if (!process.env.WAYLAND_DISPLAY && (await commandExists('xdotool'))) {
    if (action.kind === 'move' || action.kind === 'click' || action.kind === 'double_click' || action.kind === 'right_click') {
      await execFileAsync('xdotool', ['mousemove', '--sync', String(px), String(py)], { timeout: 3000 });
      if (action.kind === 'click') await execFileAsync('xdotool', ['click', '1']);
      else if (action.kind === 'double_click') await execFileAsync('xdotool', ['click', '--repeat', '2', '1']);
      else if (action.kind === 'right_click') await execFileAsync('xdotool', ['click', '3']);
      return { ok: true, message: `Executed ${action.kind} at (${px}, ${py})` };
    }
    if (action.kind === 'type' && action.text) {
      await execFileAsync('xdotool', ['type', '--clearmodifiers', '--', action.text]);
      return { ok: true, message: `Typed ${action.text.length} chars` };
    }
    if (action.kind === 'key' && action.combo) {
      await execFileAsync('xdotool', ['key', '--clearmodifiers', action.combo]);
      return { ok: true, message: `Pressed ${action.combo}` };
    }
  }

  return {
    ok: false,
    message: 'Input driver unavailable. On Wayland start ydotoold (`systemctl --user start ydotool`), or use X11 xdotool.',
  };
}

async function runOsQuickAction(
  action: DesktopAction,
  bounds: ScreenBounds,
): Promise<{ ok: boolean; message: string }> {
  const act = (action.action || '').toLowerCase();

  if (act === 'open_url' && action.target) {
    if (!/^https?:\/\//i.test(action.target)) {
      return { ok: false, message: 'Only http:// and https:// URLs are allowed.' };
    }
    await shell.openExternal(action.target);
    return { ok: true, message: `Opened URL ${action.target}` };
  }

  if (act === 'open_app' && action.target) {
    const appName = action.target.trim();
    if (!/^[A-Za-z0-9._-]+$/.test(appName)) {
      return { ok: false, message: 'Invalid application identifier.' };
    }
    if (process.platform === 'darwin') {
      await execFileAsync('open', ['-a', appName], { timeout: 4000 });
      return { ok: true, message: `Launched ${appName}` };
    }
    if (process.platform === 'win32') {
      await execFileAsync('cmd', ['/c', 'start', '', appName], { timeout: 4000 });
      return { ok: true, message: `Launched ${appName}` };
    }
    if (await commandExists('gtk-launch')) {
      try {
        await execFileAsync('gtk-launch', [appName], { timeout: 4000 });
        return { ok: true, message: `Launched ${appName}` };
      } catch {
        // Fallback to direct binary if desktop id differs from binary name
      }
    }
    if (await commandExists(appName)) {
      const child = execFile(appName, [], { cwd: os.homedir() });
      child.unref();
      return { ok: true, message: `Launched ${appName}` };
    }
    return { ok: false, message: `Application "${appName}" not found.` };
  }

  if (act === 'theme_dark' || act === 'theme_light') {
    const isDark = act === 'theme_dark';
    if (process.platform === 'linux') {
      if (await commandExists('plasma-apply-colorscheme')) {
        await execFileAsync('plasma-apply-colorscheme', [isDark ? 'BreezeDark' : 'BreezeLight'], {
          timeout: 5000,
        });
        return { ok: true, message: `Switched KDE color scheme to ${isDark ? 'Dark' : 'Light'}` };
      }
      if (await commandExists('gsettings')) {
        await execFileAsync(
          'gsettings',
          ['set', 'org.gnome.desktop.interface', 'color-scheme', isDark ? 'prefer-dark' : 'prefer-light'],
          { timeout: 4000 },
        );
        return { ok: true, message: `Switched GNOME theme to ${isDark ? 'Dark' : 'Light'}` };
      }
      return { ok: false, message: 'No supported desktop theme switcher found.' };
    }
    if (process.platform === 'darwin') {
      await execFileAsync('osascript', [
        '-e',
        `tell application "System Events" to tell appearance preferences to set dark mode to ${isDark}`,
      ]);
      return { ok: true, message: `Switched macOS appearance to ${isDark ? 'Dark' : 'Light'}` };
    }
    return { ok: false, message: 'Theme toggle is not configured for this OS.' };
  }

  if (act === 'volume_set' || act === 'volume_mute') {
    if (process.platform === 'linux') {
      if (await commandExists('wpctl')) {
        if (act === 'volume_mute') {
          await execFileAsync('wpctl', ['set-mute', '@DEFAULT_AUDIO_SINK@', 'toggle'], { timeout: 3000 });
          return { ok: true, message: 'Toggled audio mute' };
        }
        const pct = Math.min(150, Math.max(0, Math.round(action.value ?? 50)));
        await execFileAsync('wpctl', ['set-volume', '@DEFAULT_AUDIO_SINK@', `${pct}%`], { timeout: 3000 });
        return { ok: true, message: `Set volume to ${pct}%` };
      }
      if (await commandExists('pactl')) {
        if (act === 'volume_mute') {
          await execFileAsync('pactl', ['set-sink-mute', '@DEFAULT_SINK@', 'toggle'], { timeout: 3000 });
          return { ok: true, message: 'Toggled audio mute' };
        }
        const pct = Math.min(150, Math.max(0, Math.round(action.value ?? 50)));
        await execFileAsync('pactl', ['set-sink-volume', '@DEFAULT_SINK@', `${pct}%`], { timeout: 3000 });
        return { ok: true, message: `Set volume to ${pct}%` };
      }
      return { ok: false, message: 'Neither wpctl nor pactl is available.' };
    }
    if (process.platform === 'darwin') {
      const script =
        act === 'volume_mute'
          ? 'set volume output muted not (output muted of (get volume settings))'
          : `set volume output volume ${Math.min(100, Math.max(0, Math.round(action.value ?? 50)))}`;
      await execFileAsync('osascript', ['-e', script]);
      return { ok: true, message: act === 'volume_mute' ? 'Toggled macOS mute' : 'Updated macOS volume' };
    }
    return { ok: false, message: 'Volume control requires Linux (wpctl/pactl) or macOS.' };
  }

  if (act === 'snap_left' || act === 'snap_right' || act === 'snap_maximize') {
    const combo =
      act === 'snap_left'
        ? 'super+left'
        : act === 'snap_right'
          ? 'super+right'
          : 'super+up';
    if (process.platform === 'linux') {
      return runInputLinux({ id: action.id, kind: 'key', combo }, bounds);
    }
    return { ok: false, message: 'Window snapping is currently supported via Linux input driver.' };
  }

  if (act === 'dnd_on' || act === 'dnd_off') {
    if (process.platform === 'linux' && (await commandExists('gdbus'))) {
      const inhibited = act === 'dnd_on' ? 'true' : 'false';
      await execFileAsync('gdbus', [
        'call',
        '--session',
        '--dest',
        'org.freedesktop.Notifications',
        '--object-path',
        '/org/freedesktop/Notifications',
        '--method',
        'org.freedesktop.DBus.Properties.Set',
        'org.freedesktop.Notifications',
        'Inhibited',
        `<boolean ${inhibited}>`,
      ]);
      return { ok: true, message: `Do Not Disturb ${act === 'dnd_on' ? 'enabled' : 'disabled'}` };
    }
    return { ok: false, message: 'DND toggle requires Freedesktop Notifications D-Bus support.' };
  }

  if (act === 'list_windows' || act === 'windows') {
    if (process.platform === 'linux') {
      if (await commandExists('wmctrl')) {
        try {
          const { stdout } = await execFileAsync('wmctrl', ['-l', '-p']);
          const lines = stdout.trim().split('\n').filter(Boolean);
          const formatted = lines.map((l) => {
            const parts = l.split(/\s+/);
            const title = parts.slice(4).join(' ');
            return `[PID ${parts[2] || '?'}] ${title || parts[0]}`;
          }).slice(0, 25);
          return { ok: true, message: `Open windows (${lines.length}):\n${formatted.join('\n')}` };
        } catch {
          // fallback
        }
      }
      if (await commandExists('kdotool')) {
        try {
          const { stdout } = await execFileAsync('kdotool', ['search', '--onlyvisible', '.*']);
          const winIds = stdout.trim().split('\n').filter(Boolean).slice(0, 20);
          const names: string[] = [];
          for (const wid of winIds) {
            try {
              const { stdout: name } = await execFileAsync('kdotool', ['getwindowname', wid]);
              if (name.trim()) names.push(`[${wid}] ${name.trim()}`);
            } catch {
              // ignore
            }
          }
          if (names.length > 0) {
            return { ok: true, message: `Open windows (${names.length}):\n${names.join('\n')}` };
          }
        } catch {
          // fallback
        }
      }
      if (await commandExists('xdotool')) {
        try {
          const { stdout } = await execFileAsync('xdotool', ['search', '--onlyvisible', '--name', '.*']);
          const winIds = stdout.trim().split('\n').filter(Boolean).slice(0, 20);
          const names: string[] = [];
          for (const wid of winIds) {
            try {
              const { stdout: name } = await execFileAsync('xdotool', ['getwindowname', wid]);
              if (name.trim()) names.push(`[${wid}] ${name.trim()}`);
            } catch {
              // ignore
            }
          }
          if (names.length > 0) {
            return { ok: true, message: `Open windows (${names.length}):\n${names.join('\n')}` };
          }
        } catch {
          // fallback
        }
      }
      try {
        const { stdout } = await execFileAsync('ps', ['-eo', 'pid,comm', '--sort=-%mem']);
        const guiProcs = stdout
          .split('\n')
          .filter((l) => /(code|chrome|firefox|electron|konsole|terminal|nautilus|dolphin|claude|slack|discord)/i.test(l))
          .slice(0, 15);
        return { ok: true, message: `Running GUI processes:\n${guiProcs.join('\n').trim()}` };
      } catch {
        return { ok: false, message: 'Unable to list windows without wmctrl, kdotool, or xdotool.' };
      }
    }
    return { ok: false, message: 'Window listing is supported on Linux.' };
  }

  if (act === 'list_apps' || act === 'apps') {
    const appDirs = [
      '/usr/share/applications',
      '/usr/local/share/applications',
      path.join(os.homedir(), '.local/share/applications'),
      '/var/lib/flatpak/exports/share/applications',
    ];
    const foundApps: Array<{ name: string; exec: string }> = [];
    for (const d of appDirs) {
      try {
        const entries = await fs.readdir(d);
        for (const f of entries) {
          if (!f.endsWith('.desktop')) continue;
          try {
            const content = await fs.readFile(path.join(d, f), 'utf8');
            if (content.includes('NoDisplay=true')) continue;
            const nameMatch = content.match(/^Name=([^\n\r]+)/m);
            const execMatch = content.match(/^Exec=([^\n\r]+)/m);
            if (nameMatch) {
              const execCmd = execMatch ? execMatch[1].split(/\s+/)[0] : f.replace('.desktop', '');
              foundApps.push({ name: nameMatch[1].trim(), exec: execCmd });
            }
          } catch {
            // skip
          }
        }
      } catch {
        // skip
      }
    }
    const dedup = Array.from(new Map(foundApps.map((a) => [a.exec, a])).values()).slice(0, 40);
    const text = dedup.map((a) => `${a.name} (launch command: ${a.exec})`).join('\n');
    return { ok: true, message: `Installed Applications (${dedup.length}):\n${text}` };
  }

  if (act === 'active_window') {
    if (process.platform === 'linux') {
      if (await commandExists('kdotool')) {
        try {
          const { stdout: wid } = await execFileAsync('kdotool', ['getactivewindow']);
          const { stdout: title } = await execFileAsync('kdotool', ['getwindowname', wid.trim()]);
          return { ok: true, message: `Active Window: [${wid.trim()}] "${title.trim()}"` };
        } catch {
          // fallback
        }
      }
      if (await commandExists('xdotool')) {
        try {
          const { stdout: wid } = await execFileAsync('xdotool', ['getactivewindow']);
          const { stdout: title } = await execFileAsync('xdotool', ['getwindowname', wid.trim()]);
          return { ok: true, message: `Active Window: [${wid.trim()}] "${title.trim()}"` };
        } catch {
          // fallback
        }
      }
    }
    return { ok: false, message: 'Could not determine active window.' };
  }

  if (act === 'screenshot') {
    const tmpPath = path.join(os.tmpdir(), `screen-inspect-${Date.now()}.png`);
    if (await commandExists('grim')) {
      try {
        await execFileAsync('grim', [tmpPath], { timeout: 4000 });
        return { ok: true, message: `Saved fullscreen capture to ${tmpPath}` };
      } catch {
        // fallback
      }
    }
    if (await commandExists('spectacle')) {
      try {
        await execFileAsync('spectacle', ['-b', '-n', '-o', tmpPath], { timeout: 4000 });
        return { ok: true, message: `Saved fullscreen capture to ${tmpPath}` };
      } catch {
        // fallback
      }
    }
    return { ok: true, message: 'Screen context is attached via @screen tag or window capture.' };
  }

  return { ok: false, message: `Unsupported OS action: ${action.action ?? 'unknown'}` };
}

export async function executeDesktopAction(
  action: DesktopAction,
  displayBounds: ScreenBounds,
): Promise<{ ok: boolean; message: string }> {
  try {
    if (action.kind === 'os') {
      return await runOsQuickAction(action, displayBounds);
    }
    if (process.platform === 'linux') {
      return await runInputLinux(action, displayBounds);
    }
    if (process.platform === 'darwin' && (await commandExists('cliclick'))) {
      const { px, py } = percentToPixelCoords(action.x, action.y, displayBounds);
      if (action.kind === 'click') {
        await execFileAsync('cliclick', [`c:${px},${py}`]);
        return { ok: true, message: `Clicked (${px}, ${py})` };
      }
      if (action.kind === 'double_click') {
        await execFileAsync('cliclick', [`dc:${px},${py}`]);
        return { ok: true, message: `Double-clicked (${px}, ${py})` };
      }
      if (action.kind === 'move') {
        await execFileAsync('cliclick', [`m:${px},${py}`]);
        return { ok: true, message: `Moved to (${px}, ${py})` };
      }
      if (action.kind === 'type' && action.text) {
        await execFileAsync('cliclick', [`t:${action.text}`]);
        return { ok: true, message: `Typed ${action.text.length} chars` };
      }
    }
    return { ok: false, message: `Desktop action "${action.kind}" not supported on ${process.platform}.` };
  } catch (err) {
    return {
      ok: false,
      message: err instanceof Error ? err.message : String(err),
    };
  }
}
