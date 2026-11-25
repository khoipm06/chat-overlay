import { execFile, spawn } from 'node:child_process';
import fsSync from 'node:fs';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import readline from 'node:readline';
import { promisify } from 'node:util';

const execFileAsync = promisify(execFile);

interface McpToolDef {
  name: string;
  description: string;
  inputSchema: Record<string, unknown>;
}

const LINUX_KEYCODES: Record<string, number> = {
  esc: 1,
  escape: 1,
  '1': 2,
  '2': 3,
  '3': 4,
  '4': 5,
  '5': 6,
  '6': 7,
  '7': 8,
  '8': 9,
  '9': 10,
  '0': 11,
  minus: 12,
  '-': 12,
  equal: 13,
  '=': 13,
  backspace: 14,
  tab: 15,
  q: 16,
  w: 17,
  e: 18,
  r: 19,
  t: 20,
  y: 21,
  u: 22,
  i: 23,
  o: 24,
  p: 25,
  lbracket: 26,
  '[': 26,
  rbracket: 27,
  ']': 27,
  enter: 28,
  return: 28,
  ctrl: 29,
  control: 29,
  a: 30,
  s: 31,
  d: 32,
  f: 33,
  g: 34,
  h: 35,
  j: 36,
  k: 37,
  l: 38,
  semicolon: 39,
  ';': 39,
  apostrophe: 40,
  "'": 40,
  grave: 41,
  '`': 41,
  shift: 42,
  backslash: 43,
  '\\': 43,
  z: 44,
  x: 45,
  c: 46,
  v: 47,
  b: 48,
  n: 49,
  m: 50,
  comma: 51,
  ',': 51,
  dot: 52,
  '.': 52,
  slash: 53,
  '/': 53,
  alt: 56,
  space: 57,
  capslock: 58,
  f1: 59,
  f2: 60,
  f3: 61,
  f4: 62,
  f5: 63,
  f6: 64,
  f7: 65,
  f8: 66,
  f9: 67,
  f10: 68,
  f11: 87,
  f12: 88,
  home: 102,
  up: 103,
  pageup: 104,
  left: 105,
  right: 106,
  end: 107,
  down: 108,
  pagedown: 109,
  insert: 110,
  delete: 111,
  del: 111,
  super: 125,
  meta: 125,
  win: 125,
  cmd: 125,
};

async function callBridge<T = Record<string, unknown>>(
  method: 'GET' | 'POST',
  pathname: string,
  body?: Record<string, unknown>,
  timeoutMs = 8000,
): Promise<T | null> {
  const baseUrl = process.env.CHAT_OVERLAY_BRIDGE_URL?.trim();
  const token = process.env.CHAT_OVERLAY_BRIDGE_TOKEN?.trim();
  if (!baseUrl || !token) return null;
  try {
    const res = await fetch(`${baseUrl}${pathname}`, {
      method,
      headers: {
        Authorization: `Bearer ${token}`,
        ...(body ? { 'Content-Type': 'application/json' } : {}),
      },
      body: body ? JSON.stringify(body) : undefined,
      signal: AbortSignal.timeout(timeoutMs),
    });
    if (!res.ok) return null;
    return (await res.json()) as T;
  } catch {
    return null;
  }
}

async function commandExists(cmd: string): Promise<boolean> {
  const extraDirs = [
    '/usr/local/bin',
    '/usr/bin',
    '/bin',
    '/opt/homebrew/bin',
    path.join(os.homedir(), '.local', 'bin'),
    '/home/linuxbrew/.linuxbrew/bin',
  ];
  const pathDirs = Array.from(
    new Set([...(process.env.PATH ?? '').split(path.delimiter).filter(Boolean), ...extraDirs]),
  );
  const exts = process.platform === 'win32' && !cmd.endsWith('.exe') ? [cmd, `${cmd}.exe`] : [cmd];
  for (const dir of pathDirs) {
    for (const c of exts) {
      try {
        const stat = await fs.stat(path.join(dir, c));
        if (stat.isFile()) return true;
      } catch {
        // continue
      }
    }
  }
  return false;
}

function getSystemPythonWithGi(): string | null {
  for (const candidate of ['/usr/bin/python3', '/bin/python3', 'python3']) {
    if (candidate.startsWith('/') && !fsSync.existsSync(candidate)) continue;
    return candidate;
  }
  return null;
}

function defaultYdotoolSocketPath(): string {
  const uid = process.getuid ? process.getuid() : 1000;
  return process.env.YDOTOOL_SOCKET || `/run/user/${uid}/.ydotool_socket`;
}

export async function ensureYdotoolSocket(): Promise<string | null> {
  const uid = process.getuid ? process.getuid() : 1000;
  const candidates = [
    process.env.YDOTOOL_SOCKET,
    `/run/user/${uid}/.ydotool_socket`,
    '/tmp/.ydotool_socket',
  ].filter(Boolean) as string[];

  for (const sock of candidates) {
    if (fsSync.existsSync(sock)) return sock;
  }

  if (!(await commandExists('ydotoold'))) return null;
  try {
    fsSync.accessSync('/dev/uinput', fsSync.constants.W_OK);
  } catch {
    return null;
  }

  const targetSock = defaultYdotoolSocketPath();
  try {
    await fs.mkdir(path.dirname(targetSock), { recursive: true });
    const child = spawn('ydotoold', ['--socket-path', targetSock], {
      detached: true,
      stdio: 'ignore',
    });
    child.unref();
    for (let i = 0; i < 15; i++) {
      await new Promise((r) => setTimeout(r, 100));
      if (fsSync.existsSync(targetSock)) {
        return targetSock;
      }
    }
  } catch {
    return null;
  }
  return fsSync.existsSync(targetSock) ? targetSock : null;
}

async function runYdotool(args: string[], timeout = 4000): Promise<void> {
  const sock = await ensureYdotoolSocket();
  if (!sock) {
    throw new Error('ydotool daemon unavailable.');
  }
  await execFileAsync('ydotool', args, {
    timeout,
    env: { ...process.env, YDOTOOL_SOCKET: sock },
  });
}

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

async function runPowerShell(script: string, timeout = 8000): Promise<string> {
  const encoded = Buffer.from(script, 'utf16le').toString('base64');
  const { stdout } = await execFileAsync(
    'powershell',
    ['-NoProfile', '-NonInteractive', '-EncodedCommand', encoded],
    { timeout, windowsHide: true, maxBuffer: 16 * 1024 * 1024 },
  );
  return stdout;
}

async function getLogicalScreenSize(): Promise<{ width: number; height: number }> {
  const bridgeInfo = await callBridge<{ width?: number; height?: number }>('GET', '/screen-info');
  if (bridgeInfo && typeof bridgeInfo.width === 'number' && typeof bridgeInfo.height === 'number') {
    return { width: bridgeInfo.width, height: bridgeInfo.height };
  }

  if (process.platform === 'linux') {
    try {
      const { stdout } = await execFileAsync('kscreen-doctor', ['-o'], { timeout: 2000 });
      const clean = stdout.replace(/\x1b\[[0-9;]*m/g, '');
      const geoMatch = clean.match(/Geometry:\s*\d+,\d+\s+(\d+)x(\d+)/i);
      if (geoMatch) {
        return { width: Number(geoMatch[1]), height: Number(geoMatch[2]) };
      }
    } catch {}
    try {
      const { stdout } = await execFileAsync('hyprctl', ['monitors', '-j'], { timeout: 2000 });
      const mons = JSON.parse(stdout) as Array<{ width?: number; height?: number; scale?: number; focused?: boolean }>;
      const m = mons.find((x) => x.focused) || mons[0];
      if (m?.width && m?.height) {
        const s = m.scale || 1;
        return { width: Math.round(m.width / s), height: Math.round(m.height / s) };
      }
    } catch {}
    try {
      const { stdout } = await execFileAsync('xrandr', ['--current'], { timeout: 2000 });
      const curMatch = stdout.match(/current\s+(\d+)\s+x\s+(\d+)/i);
      if (curMatch) {
        return { width: Number(curMatch[1]), height: Number(curMatch[2]) };
      }
    } catch {}
  } else if (process.platform === 'darwin') {
    try {
      const { stdout } = await execFileAsync(
        'osascript',
        ['-e', 'tell application "Finder" to get bounds of window of desktop'],
        { timeout: 2000 },
      );
      const parts = stdout.trim().split(',').map((s) => Number(s.trim()));
      if (parts.length === 4 && Number.isFinite(parts[2]) && Number.isFinite(parts[3])) {
        return { width: parts[2], height: parts[3] };
      }
    } catch {}
  } else if (process.platform === 'win32') {
    try {
      const out = await runPowerShell(
        'Add-Type -AssemblyName System.Windows.Forms; $b=[System.Windows.Forms.Screen]::PrimaryScreen.Bounds; Write-Output "$($b.Width)x$($b.Height)"',
        3500,
      );
      const m = out.trim().match(/^(\d+)x(\d+)$/);
      if (m) return { width: Number(m[1]), height: Number(m[2]) };
    } catch {}
  }

  return { width: 1920, height: 1080 };
}

const KWIN_CAPTURE_PY = `
import sys, os, struct, io, json, base64
import gi
gi.require_version('Gio', '2.0')
from gi.repository import Gio, GLib

max_width = int(sys.argv[1]) if len(sys.argv) > 1 else 1440
region_json = sys.argv[2] if len(sys.argv) > 2 else ""

bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
r_fd, w_fd = os.pipe()
fd_list = Gio.UnixFDList.new()
idx = fd_list.append(w_fd)
os.close(w_fd)
res, _ = bus.call_with_unix_fd_list_sync(
    "org.kde.KWin",
    "/org/kde/KWin/ScreenShot2",
    "org.kde.KWin.ScreenShot2",
    "CaptureWorkspace",
    GLib.Variant("(a{sv}h)", ({"include-cursor": GLib.Variant("b", False)}, idx)),
    GLib.VariantType("(a{sv})"),
    Gio.DBusCallFlags.NONE,
    4000,
    fd_list,
    None,
)
del fd_list
meta = res.unpack()[0]
w, h, stride, fmt = int(meta["width"]), int(meta["height"]), int(meta["stride"]), int(meta["format"])
expected = stride * h
chunks = []
got = 0
while got < expected:
    c = os.read(r_fd, expected - got)
    if not c: break
    chunks.append(c)
    got += len(c)
os.close(r_fd)
raw = b"".join(chunks)

try:
    from PIL import Image
    if fmt in (16, 17, 18):
        img = Image.frombytes("RGBA", (w, h), raw, "raw", "RGBA", stride, 1).convert("RGB")
    else:
        img = Image.frombytes("RGBA", (w, h), raw, "raw", "BGRA", stride, 1).convert("RGB")
    if region_json:
        reg = json.loads(region_json)
        rx, ry, rw, rh = int(reg["x"]), int(reg["y"]), int(reg["width"]), int(reg["height"])
        img = img.crop((max(0, rx), max(0, ry), min(w, rx + rw), min(h, ry + rh)))
    orig_w, orig_h = img.size
    if max_width > 0 and orig_w > max_width:
        new_h = int(orig_h * (max_width / float(orig_w)))
        img = img.resize((max_width, new_h), Image.Resampling.LANCZOS)
    out_w, out_h = img.size
    buf = io.BytesIO()
    img.save(buf, format="JPEG", quality=82, optimize=True)
    print(json.dumps({
        "ok": True, "screenWidth": w, "screenHeight": h,
        "imageWidth": out_w, "imageHeight": out_h,
        "mimeType": "image/jpeg", "data": base64.b64encode(buf.getvalue()).decode("ascii")
    }))
except ImportError:
    print(json.dumps({"ok": False}))
`;

async function handleScreenshot(args: Record<string, unknown>) {
  const maxWidth = typeof args.maxWidth === 'number' ? Math.max(320, Math.min(2560, args.maxWidth)) : 1440;
  const region =
    args.region && typeof args.region === 'object'
      ? (args.region as { x: number; y: number; width: number; height: number })
      : undefined;

  // 1. Primary universal path: Electron main bridge (zero external Python/PIL dependencies on Linux, Windows, macOS)
  const bridgedShot = await callBridge<{
    ok: boolean;
    screenWidth: number;
    screenHeight: number;
    logicalScreenWidth: number;
    logicalScreenHeight: number;
    imageWidth: number;
    imageHeight: number;
    mimeType: string;
    data: string;
  }>('POST', '/screenshot', { maxWidth, ...(region ? { region } : {}) }, 10_000);

  if (bridgedShot?.ok && bridgedShot.data) {
    return {
      content: [
        { type: 'image', data: bridgedShot.data, mimeType: bridgedShot.mimeType },
        {
          type: 'text',
          text: JSON.stringify({
            logicalScreenWidth: bridgedShot.logicalScreenWidth,
            logicalScreenHeight: bridgedShot.logicalScreenHeight,
            pixelWidth: bridgedShot.screenWidth,
            pixelHeight: bridgedShot.screenHeight,
            imageWidth: bridgedShot.imageWidth,
            imageHeight: bridgedShot.imageHeight,
            coordinateSystem:
              'Use normalized 0..1000 (x, y) in desktop_mouse; (0,0)=top-left, (1000,1000)=bottom-right.',
          }),
        },
      ],
    };
  }

  // 2. Standalone OS fallbacks (when MCP server runs outside Electron host)
  if (process.platform === 'linux') {
    const regionJson = region ? JSON.stringify(region) : '';
    const py = getSystemPythonWithGi();
    if (py) {
      try {
        const { stdout } = await execFileAsync(py, ['-c', KWIN_CAPTURE_PY, String(maxWidth), regionJson], {
          maxBuffer: 32 * 1024 * 1024,
          timeout: 6000,
        });
        const parsed = JSON.parse(stdout.trim()) as {
          ok: boolean;
          screenWidth: number;
          screenHeight: number;
          imageWidth: number;
          imageHeight: number;
          mimeType: string;
          data: string;
        };
        if (parsed.ok && parsed.data) {
          const logical = await getLogicalScreenSize();
          return {
            content: [
              { type: 'image', data: parsed.data, mimeType: parsed.mimeType },
              {
                type: 'text',
                text: JSON.stringify({
                  logicalScreenWidth: logical.width,
                  logicalScreenHeight: logical.height,
                  pixelWidth: parsed.screenWidth,
                  pixelHeight: parsed.screenHeight,
                  imageWidth: parsed.imageWidth,
                  imageHeight: parsed.imageHeight,
                  coordinateSystem:
                    'Use normalized 0..1000 (x, y) in desktop_mouse; (0,0)=top-left, (1000,1000)=bottom-right.',
                }),
              },
            ],
          };
        }
      } catch {}
    }

    const tmpFile = path.join(os.tmpdir(), `co-mcp-shot-${Date.now()}.png`);
    try {
      if (await commandExists('grim')) {
        await execFileAsync('grim', [tmpFile], { timeout: 5000 });
      } else if (await commandExists('spectacle')) {
        await execFileAsync('spectacle', ['-b', '-n', '-o', tmpFile], { timeout: 5000 });
      } else if (await commandExists('gnome-screenshot')) {
        await execFileAsync('gnome-screenshot', ['-f', tmpFile], { timeout: 5000 });
      } else if (await commandExists('scrot')) {
        await execFileAsync('scrot', [tmpFile], { timeout: 5000 });
      }
      if (fsSync.existsSync(tmpFile)) {
        const buf = await fs.readFile(tmpFile);
        const logical = await getLogicalScreenSize();
        return {
          content: [
            { type: 'image', data: buf.toString('base64'), mimeType: 'image/png' },
            {
              type: 'text',
              text: JSON.stringify({
                logicalScreenWidth: logical.width,
                logicalScreenHeight: logical.height,
                coordinateSystem:
                  'Use normalized 0..1000 (x, y) in desktop_mouse; (0,0)=top-left, (1000,1000)=bottom-right.',
              }),
            },
          ],
        };
      }
    } finally {
      await fs.rm(tmpFile, { force: true }).catch(() => {});
    }
  } else if (process.platform === 'darwin') {
    const tmpFile = path.join(os.tmpdir(), `co-mcp-shot-${Date.now()}.jpg`);
    try {
      await execFileAsync('screencapture', ['-x', '-t', 'jpg', tmpFile], { timeout: 5000 });
      if (fsSync.existsSync(tmpFile)) {
        const buf = await fs.readFile(tmpFile);
        const logical = await getLogicalScreenSize();
        return {
          content: [
            { type: 'image', data: buf.toString('base64'), mimeType: 'image/jpeg' },
            {
              type: 'text',
              text: JSON.stringify({
                logicalScreenWidth: logical.width,
                logicalScreenHeight: logical.height,
                coordinateSystem:
                  'Use normalized 0..1000 (x, y) in desktop_mouse; (0,0)=top-left, (1000,1000)=bottom-right.',
              }),
            },
          ],
        };
      }
    } finally {
      await fs.rm(tmpFile, { force: true }).catch(() => {});
    }
  } else if (process.platform === 'win32') {
    const tmpFile = path.join(os.tmpdir(), `co-mcp-shot-${Date.now()}.jpg`);
    try {
      await runPowerShell(
        `Add-Type -AssemblyName System.Windows.Forms,System.Drawing; ` +
          `$b=[System.Windows.Forms.Screen]::PrimaryScreen.Bounds; ` +
          `$bmp=New-Object System.Drawing.Bitmap($b.Width,$b.Height); ` +
          `$g=[System.Drawing.Graphics]::FromImage($bmp); ` +
          `$g.CopyFromScreen($b.Location,[System.Drawing.Point]::Empty,$b.Size); ` +
          `$bmp.Save('${tmpFile.replace(/'/g, "''")}',[System.Drawing.Imaging.ImageFormat]::Jpeg); ` +
          `$g.Dispose(); $bmp.Dispose()`,
        6000,
      );
      if (fsSync.existsSync(tmpFile)) {
        const buf = await fs.readFile(tmpFile);
        const logical = await getLogicalScreenSize();
        return {
          content: [
            { type: 'image', data: buf.toString('base64'), mimeType: 'image/jpeg' },
            {
              type: 'text',
              text: JSON.stringify({
                logicalScreenWidth: logical.width,
                logicalScreenHeight: logical.height,
                coordinateSystem:
                  'Use normalized 0..1000 (x, y) in desktop_mouse; (0,0)=top-left, (1000,1000)=bottom-right.',
              }),
            },
          ],
        };
      }
    } finally {
      await fs.rm(tmpFile, { force: true }).catch(() => {});
    }
  }

  throw new Error('Unable to capture screenshot on this platform/compositor.');
}

const KWIN_LIST_WINDOWS_PY = `
import gi, tempfile, os, json, uuid
gi.require_version("Gio", "2.0")
from gi.repository import Gio, GLib

bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
svc_name = f"io.chatoverlay.McpWinList{os.getpid()}"
obj_path = "/WinList"
iface_xml = '<node><interface name="io.chatoverlay.McpWinList"><method name="Report"><arg type="s" name="payload" direction="in"/></method></interface></node>'
node_info = Gio.DBusNodeInfo.new_for_xml(iface_xml)
result_holder = []
loop = GLib.MainLoop()

def on_call(conn, sender, path, iface, method, params, inv):
    if method == "Report":
        result_holder.append(params.unpack()[0])
        inv.return_value(None)
        loop.quit()

owner_id = Gio.bus_own_name_on_connection(bus, svc_name, Gio.BusNameOwnerFlags.NONE, None, None)
reg_id = bus.register_object(obj_path, node_info.interfaces[0], on_call, None, None)

js = f"""
var out = [];
var wins = workspace.windowList();
for (var i = 0; i < wins.length; i++) {{
  var w = wins[i];
  if (w.desktopWindow || w.dock) continue;
  out.push({{
    id: String(w.internalId),
    caption: String(w.caption || ""),
    resourceClass: String(w.resourceClass || ""),
    active: Boolean(w.active),
    minimized: Boolean(w.minimized),
    x: Math.round(w.x),
    y: Math.round(w.y),
    width: Math.round(w.width),
    height: Math.round(w.height)
  }});
}}
callDBus("{svc_name}", "{obj_path}", "io.chatoverlay.McpWinList", "Report", JSON.stringify(out));
"""
fd, tmp = tempfile.mkstemp(suffix=".js", prefix="kwin_lw_")
os.write(fd, js.encode("utf-8"))
os.close(fd)
pname = f"co_lw_{uuid.uuid4().hex[:8]}"
try:
    res = bus.call_sync("org.kde.KWin", "/Scripting", "org.kde.kwin.Scripting", "loadScript",
        GLib.Variant("(ss)", (tmp, pname)), GLib.VariantType("(i)"), Gio.DBusCallFlags.NONE, 2000, None)
    sid = res.unpack()[0]
    bus.call_sync("org.kde.KWin", f"/Scripting/Script{sid}", "org.kde.kwin.Script", "run",
        None, None, Gio.DBusCallFlags.NONE, 2000, None)
    GLib.timeout_add(1500, lambda: loop.quit())
    loop.run()
    bus.call_sync("org.kde.KWin", "/Scripting", "org.kde.kwin.Scripting", "unloadScript",
        GLib.Variant("(s)", (pname,)), GLib.VariantType("(b)"), Gio.DBusCallFlags.NONE, 2000, None)
    print(result_holder[0] if result_holder else "[]")
finally:
    bus.unregister_object(reg_id)
    Gio.bus_unown_name(owner_id)
    os.unlink(tmp)
`;

async function handleListWindows() {
  if (process.platform === 'linux') {
    const py = getSystemPythonWithGi();
    if (py) {
      try {
        const { stdout } = await execFileAsync(py, ['-c', KWIN_LIST_WINDOWS_PY], { timeout: 4000 });
        const windows = JSON.parse(stdout.trim() || '[]');
        return {
          content: [{ type: 'text', text: JSON.stringify({ count: windows.length, windows }, null, 2) }],
        };
      } catch {}
    }
    if (await commandExists('hyprctl')) {
      try {
        const { stdout } = await execFileAsync('hyprctl', ['clients', '-j'], { timeout: 3000 });
        const raw = JSON.parse(stdout) as Array<Record<string, any>>;
        const windows = raw.map((c) => ({
          id: String(c.address || ''),
          caption: String(c.title || ''),
          resourceClass: String(c.class || ''),
          active: false,
          minimized: false,
          x: Number(c.at?.[0] ?? 0),
          y: Number(c.at?.[1] ?? 0),
          width: Number(c.size?.[0] ?? 0),
          height: Number(c.size?.[1] ?? 0),
        }));
        return {
          content: [{ type: 'text', text: JSON.stringify({ count: windows.length, windows }, null, 2) }],
        };
      } catch {}
    }
    if (await commandExists('wmctrl')) {
      const { stdout } = await execFileAsync('wmctrl', ['-l', '-p'], { timeout: 3000 });
      return { content: [{ type: 'text', text: stdout.trim() }] };
    }
    if (await commandExists('kdotool')) {
      const { stdout } = await execFileAsync('kdotool', ['search', '--onlyvisible', '.*'], { timeout: 3000 });
      return { content: [{ type: 'text', text: stdout.trim() }] };
    }
  } else if (process.platform === 'win32') {
    const ps = `
Add-Type @"
using System;
using System.Runtime.InteropServices;
using System.Text;
public class WinEnum {
  public delegate bool EnumProc(IntPtr hWnd, IntPtr lParam);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc lpEnumFunc, IntPtr lParam);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hWnd);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr hWnd, StringBuilder sb, int max);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT r);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr hWnd);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L,T,R,B; }
}
"@
$fg = [WinEnum]::GetForegroundWindow()
$list = New-Object System.Collections.Generic.List[object]
[WinEnum]::EnumWindows({ param($h,$p)
  if ([WinEnum]::IsWindowVisible($h)) {
    $sb = New-Object System.Text.StringBuilder 256
    [void][WinEnum]::GetWindowText($h, $sb, 256)
    $t = $sb.ToString()
    if ($t) {
      $r = New-Object WinEnum+RECT
      [void][WinEnum]::GetWindowRect($h, [ref]$r)
      $list.Add([PSCustomObject]@{
        id = "$h"; caption = $t; active = ($h -eq $fg); minimized = [WinEnum]::IsIconic($h);
        x = $r.L; y = $r.T; width = ($r.R - $r.L); height = ($r.B - $r.T)
      })
    }
  }
  return $true
}, [IntPtr]::Zero) | Out-Null
$list | ConvertTo-Json -Compress
`;
    const out = await runPowerShell(ps, 5000);
    const parsed = out.trim() ? JSON.parse(out.trim()) : [];
    const windows = Array.isArray(parsed) ? parsed : [parsed];
    return {
      content: [{ type: 'text', text: JSON.stringify({ count: windows.length, windows }, null, 2) }],
    };
  } else if (process.platform === 'darwin') {
    const jxa = `
const se = Application("System Events");
const out = [];
for (const p of se.processes.whose({ backgroundOnly: false })()) {
  const name = p.name();
  const active = p.frontmost();
  try {
    for (const w of p.windows()) {
      const pos = w.position();
      const sz = w.size();
      out.push({ caption: String(w.name() || name), resourceClass: name, active, minimized: false, x: pos[0], y: pos[1], width: sz[0], height: sz[1] });
    }
  } catch {}
}
JSON.stringify({ count: out.length, windows: out }, null, 2);
`;
    const { stdout } = await execFileAsync('osascript', ['-l', 'JavaScript', '-e', jxa], { timeout: 4000 });
    return { content: [{ type: 'text', text: stdout.trim() }] };
  }

  return { content: [{ type: 'text', text: 'No window listing backend available.' }], isError: true };
}

const KWIN_CONTROL_WINDOW_PY = `
import sys, gi, tempfile, os, json, uuid
gi.require_version("Gio", "2.0")
from gi.repository import Gio, GLib

action = sys.argv[1]
match_str = sys.argv[2].lower()

bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
svc_name = f"io.chatoverlay.McpWinCtrl{os.getpid()}"
obj_path = "/WinCtrl"
iface_xml = '<node><interface name="io.chatoverlay.McpWinCtrl"><method name="Report"><arg type="s" name="payload" direction="in"/></method></interface></node>'
node_info = Gio.DBusNodeInfo.new_for_xml(iface_xml)
result_holder = []
loop = GLib.MainLoop()

def on_call(conn, sender, path, iface, method, params, inv):
    if method == "Report":
        result_holder.append(params.unpack()[0])
        inv.return_value(None)
        loop.quit()

owner_id = Gio.bus_own_name_on_connection(bus, svc_name, Gio.BusNameOwnerFlags.NONE, None, None)
reg_id = bus.register_object(obj_path, node_info.interfaces[0], on_call, None, None)

action_js = json.dumps(action)
match_js = json.dumps(match_str)

js = f"""
var act = {action_js};
var needle = {match_js};
var wins = workspace.windowList();
var target = null;
for (var i = 0; i < wins.length; i++) {{
  var w = wins[i];
  if (w.desktopWindow || w.dock) continue;
  var hay = (String(w.caption || "") + " " + String(w.resourceClass || "")).toLowerCase();
  if (hay.indexOf(needle) !== -1) {{
    target = w;
    break;
  }}
}}
if (!target) {{
  callDBus("{svc_name}", "{obj_path}", "io.chatoverlay.McpWinCtrl", "Report", JSON.stringify({{ok: false, message: "No window matched: " + needle}}));
}} else {{
  var area = workspace.clientArea(KWin.PlacementArea, target);
  if (act === "focus") {{
    if (target.minimized) target.minimized = false;
    workspace.activeWindow = target;
  }} else if (act === "minimize") {{
    target.minimized = true;
  }} else if (act === "maximize") {{
    target.minimized = false;
    workspace.activeWindow = target;
    target.setMaximize(true, true);
  }} else if (act === "unmaximize") {{
    target.setMaximize(false, false);
  }} else if (act === "snap_left") {{
    target.minimized = false;
    workspace.activeWindow = target;
    target.setMaximize(false, false);
    target.frameGeometry = {{ x: area.x, y: area.y, width: Math.floor(area.width / 2), height: area.height }};
  }} else if (act === "snap_right") {{
    target.minimized = false;
    workspace.activeWindow = target;
    target.setMaximize(false, false);
    target.frameGeometry = {{ x: area.x + Math.floor(area.width / 2), y: area.y, width: Math.ceil(area.width / 2), height: area.height }};
  }} else if (act === "close") {{
    target.closeWindow();
  }}
  callDBus("{svc_name}", "{obj_path}", "io.chatoverlay.McpWinCtrl", "Report", JSON.stringify({{
    ok: true,
    action: act,
    matchedWindow: String(target.caption || target.resourceClass)
  }}));
}}
"""
fd, tmp = tempfile.mkstemp(suffix=".js", prefix="kwin_cw_")
os.write(fd, js.encode("utf-8"))
os.close(fd)
pname = f"co_cw_{uuid.uuid4().hex[:8]}"
try:
    res = bus.call_sync("org.kde.KWin", "/Scripting", "org.kde.kwin.Scripting", "loadScript",
        GLib.Variant("(ss)", (tmp, pname)), GLib.VariantType("(i)"), Gio.DBusCallFlags.NONE, 2000, None)
    sid = res.unpack()[0]
    bus.call_sync("org.kde.KWin", f"/Scripting/Script{sid}", "org.kde.kwin.Script", "run",
        None, None, Gio.DBusCallFlags.NONE, 2000, None)
    GLib.timeout_add(1500, lambda: loop.quit())
    loop.run()
    bus.call_sync("org.kde.KWin", "/Scripting", "org.kde.kwin.Scripting", "unloadScript",
        GLib.Variant("(s)", (pname,)), GLib.VariantType("(b)"), Gio.DBusCallFlags.NONE, 2000, None)
    print(result_holder[0] if result_holder else json.dumps({"ok": False, "message": "Timeout waiting for KWin script"}))
finally:
    bus.unregister_object(reg_id)
    Gio.bus_unown_name(owner_id)
    os.unlink(tmp)
`;

async function handleWindowControl(args: Record<string, unknown>) {
  const action = String(args.action || 'focus');
  const match = String(args.match || '').trim();
  if (!match) {
    return { content: [{ type: 'text', text: 'match parameter is required.' }], isError: true };
  }

  if (process.platform === 'linux') {
    const py = getSystemPythonWithGi();
    if (py) {
      try {
        const { stdout } = await execFileAsync(py, ['-c', KWIN_CONTROL_WINDOW_PY, action, match], {
          timeout: 4500,
        });
        const parsed = JSON.parse(stdout.trim());
        return {
          content: [{ type: 'text', text: JSON.stringify(parsed) }],
          isError: !parsed.ok,
        };
      } catch {}
    }
    if (await commandExists('hyprctl')) {
      const { stdout } = await execFileAsync('hyprctl', ['clients', '-j'], { timeout: 3000 });
      const clients = JSON.parse(stdout) as Array<{ address?: string; title?: string; class?: string }>;
      const needle = match.toLowerCase();
      const target = clients.find(
        (c) =>
          (c.title || '').toLowerCase().includes(needle) ||
          (c.class || '').toLowerCase().includes(needle),
      );
      if (!target?.address) {
        return { content: [{ type: 'text', text: `No window matched: ${match}` }], isError: true };
      }
      if (action === 'focus') {
        await execFileAsync('hyprctl', ['dispatch', 'focuswindow', `address:${target.address}`], { timeout: 3000 });
      } else if (action === 'maximize' || action === 'unmaximize') {
        await execFileAsync('hyprctl', ['dispatch', 'fullscreen', '1'], { timeout: 3000 });
      } else if (action === 'close') {
        await execFileAsync('hyprctl', ['dispatch', 'closewindow', `address:${target.address}`], { timeout: 3000 });
      }
      return { content: [{ type: 'text', text: JSON.stringify({ ok: true, action, matchedWindow: target.title || target.class }) }] };
    }
    if (await commandExists('wmctrl')) {
      if (action === 'focus') {
        await execFileAsync('wmctrl', ['-a', match], { timeout: 3000 });
        return { content: [{ type: 'text', text: `Focused window matching "${match}"` }] };
      }
      if (action === 'close') {
        await execFileAsync('wmctrl', ['-c', match], { timeout: 3000 });
        return { content: [{ type: 'text', text: `Closed window matching "${match}"` }] };
      }
    }
    return { content: [{ type: 'text', text: 'Window control requires KWin, Hyprland, or wmctrl on Linux.' }], isError: true };
  }

  if (process.platform === 'win32') {
    const ps = `
Add-Type @"
using System;
using System.Runtime.InteropServices;
using System.Text;
public class WinCtrl {
  public delegate bool EnumProc(IntPtr hWnd, IntPtr lParam);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc lpEnumFunc, IntPtr lParam);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hWnd);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr hWnd, StringBuilder sb, int max);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);
  [DllImport("user32.dll")] public static extern bool MoveWindow(IntPtr hWnd, int x, int y, int w, int h, bool repaint);
  [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr hWnd, uint Msg, IntPtr wParam, IntPtr lParam);
}
"@
Add-Type -AssemblyName System.Windows.Forms
$needle = '${match.replace(/'/g, "''").toLowerCase()}'
$act = '${action.replace(/'/g, "''")}'
$target = [IntPtr]::Zero
$title = ""
[WinCtrl]::EnumWindows({ param($h,$p)
  if ([WinCtrl]::IsWindowVisible($h)) {
    $sb = New-Object System.Text.StringBuilder 256
    [void][WinCtrl]::GetWindowText($h, $sb, 256)
    $t = $sb.ToString()
    if ($t -and $t.ToLower().Contains($needle)) {
      $script:target = $h; $script:title = $t; return $false
    }
  }
  return $true
}, [IntPtr]::Zero) | Out-Null
if ($target -eq [IntPtr]::Zero) { Write-Output '{"ok":false,"message":"No matching window"}'; exit }
$wa = [System.Windows.Forms.Screen]::PrimaryScreen.WorkingArea
if ($act -eq 'focus') { [void][WinCtrl]::ShowWindow($target, 9); [void][WinCtrl]::SetForegroundWindow($target) }
elseif ($act -eq 'minimize') { [void][WinCtrl]::ShowWindow($target, 6) }
elseif ($act -eq 'maximize') { [void][WinCtrl]::ShowWindow($target, 3); [void][WinCtrl]::SetForegroundWindow($target) }
elseif ($act -eq 'unmaximize') { [void][WinCtrl]::ShowWindow($target, 9) }
elseif ($act -eq 'snap_left') { [void][WinCtrl]::ShowWindow($target, 9); [void][WinCtrl]::MoveWindow($target, $wa.X, $wa.Y, [int]($wa.Width/2), $wa.Height, $true) }
elseif ($act -eq 'snap_right') { [void][WinCtrl]::ShowWindow($target, 9); [void][WinCtrl]::MoveWindow($target, $wa.X + [int]($wa.Width/2), $wa.Y, [int]($wa.Width/2), $wa.Height, $true) }
elseif ($act -eq 'close') { [void][WinCtrl]::PostMessage($target, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero) }
@{ ok = $true; action = $act; matchedWindow = $title } | ConvertTo-Json -Compress
`;
    const out = await runPowerShell(ps, 5000);
    const parsed = JSON.parse(out.trim());
    return { content: [{ type: 'text', text: JSON.stringify(parsed) }], isError: !parsed.ok };
  }

  if (process.platform === 'darwin') {
    const jxa = `
const se = Application("System Events");
const finder = Application("Finder");
const deskBounds = finder.desktop.window().bounds();
const needle = ${JSON.stringify(match.toLowerCase())};
const act = ${JSON.stringify(action)};
let found = null;
for (const p of se.processes.whose({ backgroundOnly: false })()) {
  const pName = String(p.name() || "");
  if (pName.toLowerCase().includes(needle)) {
    found = { p, name: pName }; break;
  }
  try {
    for (const w of p.windows()) {
      const wName = String(w.name() || "");
      if (wName.toLowerCase().includes(needle)) {
        found = { p, w, name: wName || pName }; break;
      }
    }
  } catch {}
  if (found) break;
}
if (!found) {
  JSON.stringify({ ok: false, message: "No matching window" });
} else {
  if (act === "focus") found.p.frontmost = true;
  else if (act === "minimize") { try { found.p.visible = false; } catch {} }
  else if (act === "maximize") {
    found.p.frontmost = true;
    try { const w = found.w || found.p.windows[0]; w.position = [0, 25]; w.size = [deskBounds[2], deskBounds[3] - 25]; } catch {}
  } else if (act === "snap_left") {
    found.p.frontmost = true;
    try { const w = found.w || found.p.windows[0]; w.position = [0, 25]; w.size = [Math.floor(deskBounds[2]/2), deskBounds[3] - 25]; } catch {}
  } else if (act === "snap_right") {
    found.p.frontmost = true;
    try { const w = found.w || found.p.windows[0]; w.position = [Math.floor(deskBounds[2]/2), 25]; w.size = [Math.ceil(deskBounds[2]/2), deskBounds[3] - 25]; } catch {}
  } else if (act === "close") {
    try { (found.w || found.p.windows[0]).buttons.whose({ subrole: "AXCloseButton" })[0].click(); } catch {}
  }
  JSON.stringify({ ok: true, action: act, matchedWindow: found.name });
}
`;
    const { stdout } = await execFileAsync('osascript', ['-l', 'JavaScript', '-e', jxa], { timeout: 4500 });
    const parsed = JSON.parse(stdout.trim());
    return { content: [{ type: 'text', text: JSON.stringify(parsed) }], isError: !parsed.ok };
  }

  return { content: [{ type: 'text', text: 'Window control unsupported on this platform.' }], isError: true };
}

async function resolvePixelCoords(x: number, y: number, normalized = true): Promise<{ px: number; py: number }> {
  if (!normalized) {
    return { px: Math.round(x), py: Math.round(y) };
  }
  const { width, height } = await getLogicalScreenSize();
  const clampedX = Math.max(0, Math.min(1000, x));
  const clampedY = Math.max(0, Math.min(1000, y));
  return {
    px: Math.round((clampedX / 1000) * width),
    py: Math.round((clampedY / 1000) * height),
  };
}

async function handleMouse(args: Record<string, unknown>) {
  const action = String(args.action || 'click');
  const x = typeof args.x === 'number' ? args.x : 500;
  const y = typeof args.y === 'number' ? args.y : 500;
  const normalized = args.normalized !== false;
  const { px, py } = await resolvePixelCoords(x, y, normalized);
  const steps = Math.max(-25, Math.min(25, Math.round(typeof args.delta === 'number' ? args.delta : -3)));

  if (process.platform === 'linux') {
    const ydoSock = await ensureYdotoolSocket();
    if (ydoSock) {
      if (action === 'drag') {
        const endX = typeof args.endX === 'number' ? args.endX : x;
        const endY = typeof args.endY === 'number' ? args.endY : y;
        const endPt = await resolvePixelCoords(endX, endY, normalized);
        await runYdotool(['mousemove', '--absolute', '-x', String(px), '-y', String(py)]);
        await runYdotool(['click', '0x40']);
        await new Promise((r) => setTimeout(r, 80));
        await runYdotool(['mousemove', '--absolute', '-x', String(endPt.px), '-y', String(endPt.py)]);
        await new Promise((r) => setTimeout(r, 80));
        await runYdotool(['click', '0x80']);
        return { content: [{ type: 'text', text: `Dragged from (${px}, ${py}) to (${endPt.px}, ${endPt.py})` }] };
      }

      await runYdotool(['mousemove', '--absolute', '-x', String(px), '-y', String(py)]);
      if (action === 'move') return { content: [{ type: 'text', text: `Moved pointer to (${px}, ${py})` }] };
      if (action === 'click') {
        await runYdotool(['click', '0xC0']);
        return { content: [{ type: 'text', text: `Left-clicked at (${px}, ${py})` }] };
      }
      if (action === 'double_click') {
        await runYdotool(['click', '--repeat', '2', '--next-delay', '60', '0xC0']);
        return { content: [{ type: 'text', text: `Double-clicked at (${px}, ${py})` }] };
      }
      if (action === 'right_click') {
        await runYdotool(['click', '0xC1']);
        return { content: [{ type: 'text', text: `Right-clicked at (${px}, ${py})` }] };
      }
      if (action === 'middle_click') {
        await runYdotool(['click', '0xC2']);
        return { content: [{ type: 'text', text: `Middle-clicked at (${px}, ${py})` }] };
      }
      if (action === 'scroll') {
        await runYdotool(['mousemove', '-w', '--', '0', String(steps)]);
        return { content: [{ type: 'text', text: `Scrolled ${steps} clicks at (${px}, ${py})` }] };
      }
      return { content: [{ type: 'text', text: `Unknown mouse action: ${action}` }], isError: true };
    }

    if (await commandExists('xdotool')) {
      if (action === 'drag') {
        const endX = typeof args.endX === 'number' ? args.endX : x;
        const endY = typeof args.endY === 'number' ? args.endY : y;
        const endPt = await resolvePixelCoords(endX, endY, normalized);
        await execFileAsync('xdotool', ['mousemove', '--sync', String(px), String(py), 'mousedown', '1', 'mousemove', '--sync', String(endPt.px), String(endPt.py), 'mouseup', '1'], { timeout: 4000 });
        return { content: [{ type: 'text', text: `Dragged from (${px}, ${py}) to (${endPt.px}, ${endPt.py})` }] };
      }
      await execFileAsync('xdotool', ['mousemove', '--sync', String(px), String(py)], { timeout: 3000 });
      if (action === 'move') return { content: [{ type: 'text', text: `Moved pointer to (${px}, ${py})` }] };
      if (action === 'click') {
        await execFileAsync('xdotool', ['click', '1'], { timeout: 3000 });
        return { content: [{ type: 'text', text: `Left-clicked at (${px}, ${py})` }] };
      }
      if (action === 'double_click') {
        await execFileAsync('xdotool', ['click', '--repeat', '2', '1'], { timeout: 3000 });
        return { content: [{ type: 'text', text: `Double-clicked at (${px}, ${py})` }] };
      }
      if (action === 'right_click') {
        await execFileAsync('xdotool', ['click', '3'], { timeout: 3000 });
        return { content: [{ type: 'text', text: `Right-clicked at (${px}, ${py})` }] };
      }
      if (action === 'middle_click') {
        await execFileAsync('xdotool', ['click', '2'], { timeout: 3000 });
        return { content: [{ type: 'text', text: `Middle-clicked at (${px}, ${py})` }] };
      }
      if (action === 'scroll') {
        const btn = steps < 0 ? '5' : '4';
        await execFileAsync('xdotool', ['click', '--repeat', String(Math.max(1, Math.abs(steps))), btn], { timeout: 3000 });
        return { content: [{ type: 'text', text: `Scrolled ${steps} clicks at (${px}, ${py})` }] };
      }
    }

    throw new Error('Mouse automation requires ydotool/ydotoold (Wayland) or xdotool (X11).');
  }

  if (process.platform === 'win32') {
    const endX = typeof args.endX === 'number' ? args.endX : x;
    const endY = typeof args.endY === 'number' ? args.endY : y;
    const endPt = action === 'drag' ? await resolvePixelCoords(endX, endY, normalized) : { px, py };
    const ps = `
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class MouseApi {
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int X, int Y);
  [DllImport("user32.dll")] public static extern void mouse_event(int flags, int dx, int dy, int data, UIntPtr extra);
}
"@
[void][MouseApi]::SetCursorPos(${px}, ${py})
Start-Sleep -Milliseconds 30
if ('${action}' -eq 'click') {
  [MouseApi]::mouse_event(0x02, 0, 0, 0, [UIntPtr]::Zero); [MouseApi]::mouse_event(0x04, 0, 0, 0, [UIntPtr]::Zero)
} elseif ('${action}' -eq 'double_click') {
  [MouseApi]::mouse_event(0x02, 0, 0, 0, [UIntPtr]::Zero); [MouseApi]::mouse_event(0x04, 0, 0, 0, [UIntPtr]::Zero)
  Start-Sleep -Milliseconds 60
  [MouseApi]::mouse_event(0x02, 0, 0, 0, [UIntPtr]::Zero); [MouseApi]::mouse_event(0x04, 0, 0, 0, [UIntPtr]::Zero)
} elseif ('${action}' -eq 'right_click') {
  [MouseApi]::mouse_event(0x08, 0, 0, 0, [UIntPtr]::Zero); [MouseApi]::mouse_event(0x10, 0, 0, 0, [UIntPtr]::Zero)
} elseif ('${action}' -eq 'middle_click') {
  [MouseApi]::mouse_event(0x20, 0, 0, 0, [UIntPtr]::Zero); [MouseApi]::mouse_event(0x40, 0, 0, 0, [UIntPtr]::Zero)
} elseif ('${action}' -eq 'scroll') {
  [MouseApi]::mouse_event(0x0800, 0, 0, ${steps * 120}, [UIntPtr]::Zero)
} elseif ('${action}' -eq 'drag') {
  [MouseApi]::mouse_event(0x02, 0, 0, 0, [UIntPtr]::Zero)
  Start-Sleep -Milliseconds 80
  [void][MouseApi]::SetCursorPos(${endPt.px}, ${endPt.py})
  Start-Sleep -Milliseconds 80
  [MouseApi]::mouse_event(0x04, 0, 0, 0, [UIntPtr]::Zero)
}
`;
    await runPowerShell(ps, 5000);
    return { content: [{ type: 'text', text: `Executed mouse ${action} at (${px}, ${py})` }] };
  }

  if (process.platform === 'darwin') {
    if (await commandExists('cliclick')) {
      if (action === 'move') await execFileAsync('cliclick', [`m:${px},${py}`]);
      else if (action === 'click') await execFileAsync('cliclick', [`c:${px},${py}`]);
      else if (action === 'double_click') await execFileAsync('cliclick', [`dc:${px},${py}`]);
      else if (action === 'right_click') await execFileAsync('cliclick', [`rc:${px},${py}`]);
      else if (action === 'drag') {
        const endPt = await resolvePixelCoords(
          typeof args.endX === 'number' ? args.endX : x,
          typeof args.endY === 'number' ? args.endY : y,
          normalized,
        );
        await execFileAsync('cliclick', [`dd:${px},${py}`, `du:${endPt.px},${endPt.py}`]);
      } else if (action === 'scroll') {
        await execFileAsync('cliclick', [`m:${px},${py}`]);
      }
      if (action !== 'scroll') {
        return { content: [{ type: 'text', text: `Executed mouse ${action} at (${px}, ${py})` }] };
      }
    }

    const endPt =
      action === 'drag'
        ? await resolvePixelCoords(
            typeof args.endX === 'number' ? args.endX : x,
            typeof args.endY === 'number' ? args.endY : y,
            normalized,
          )
        : { px, py };
    const jxa = `
ObjC.import("CoreGraphics");
const pt = $.CGPointMake(${px}, ${py});
const post = (t, p, btn) => {
  const e = $.CGEventCreateMouseEvent(null, t, p, btn);
  $.CGEventPost($.kCGHIDEventTap, e);
};
if (${JSON.stringify(action)} === "move") {
  post($.kCGEventMouseMoved, pt, $.kCGMouseButtonLeft);
} else if (${JSON.stringify(action)} === "click") {
  post($.kCGEventLeftMouseDown, pt, $.kCGMouseButtonLeft);
  post($.kCGEventLeftMouseUp, pt, $.kCGMouseButtonLeft);
} else if (${JSON.stringify(action)} === "double_click") {
  post($.kCGEventLeftMouseDown, pt, $.kCGMouseButtonLeft);
  post($.kCGEventLeftMouseUp, pt, $.kCGMouseButtonLeft);
  delay(0.06);
  post($.kCGEventLeftMouseDown, pt, $.kCGMouseButtonLeft);
  post($.kCGEventLeftMouseUp, pt, $.kCGMouseButtonLeft);
} else if (${JSON.stringify(action)} === "right_click") {
  post($.kCGEventRightMouseDown, pt, $.kCGMouseButtonRight);
  post($.kCGEventRightMouseUp, pt, $.kCGMouseButtonRight);
} else if (${JSON.stringify(action)} === "middle_click") {
  post($.kCGEventOtherMouseDown, pt, $.kCGMouseButtonCenter);
  post($.kCGEventOtherMouseUp, pt, $.kCGMouseButtonCenter);
} else if (${JSON.stringify(action)} === "drag") {
  const endPt = $.CGPointMake(${endPt.px}, ${endPt.py});
  post($.kCGEventLeftMouseDown, pt, $.kCGMouseButtonLeft);
  delay(0.08);
  post($.kCGEventLeftMouseDragged, endPt, $.kCGMouseButtonLeft);
  delay(0.08);
  post($.kCGEventLeftMouseUp, endPt, $.kCGMouseButtonLeft);
} else if (${JSON.stringify(action)} === "scroll") {
  post($.kCGEventMouseMoved, pt, $.kCGMouseButtonLeft);
  const se = $.CGEventCreateScrollWheelEvent(null, $.kCGScrollEventUnitLine, 1, ${steps});
  $.CGEventPost($.kCGHIDEventTap, se);
}
`;
    await execFileAsync('osascript', ['-l', 'JavaScript', '-e', jxa], { timeout: 4500 });
    return { content: [{ type: 'text', text: `Executed mouse ${action} at (${px}, ${py})` }] };
  }

  return { content: [{ type: 'text', text: `Mouse automation unsupported on ${process.platform}.` }], isError: true };
}

async function readClipboardText(): Promise<string | null> {
  const bridged = await callBridge<{ ok?: boolean; text?: string }>('GET', '/clipboard');
  if (bridged?.ok && typeof bridged.text === 'string') {
    return bridged.text;
  }

  try {
    if (process.platform === 'linux') {
      if (await commandExists('wl-paste')) {
        const { stdout } = await execFileAsync('wl-paste', ['--no-newline'], { timeout: 1500 });
        return stdout;
      }
      if (await commandExists('xclip')) {
        const { stdout } = await execFileAsync('xclip', ['-o', '-selection', 'clipboard'], { timeout: 1500 });
        return stdout;
      }
    } else if (process.platform === 'darwin') {
      const { stdout } = await execFileAsync('pbpaste', [], { timeout: 1500 });
      return stdout;
    } else if (process.platform === 'win32') {
      const stdout = await runPowerShell('Get-Clipboard -Raw', 2500);
      return stdout;
    }
  } catch {
    return null;
  }
  return null;
}

async function writeClipboardText(text: string): Promise<boolean> {
  const bridged = await callBridge<{ ok?: boolean }>('POST', '/clipboard', { text });
  if (bridged?.ok) return true;

  if (process.platform === 'linux') {
    if (await commandExists('wl-copy')) {
      await new Promise<void>((resolve, reject) => {
        const child = execFile('wl-copy', [], { timeout: 2000 }, (err) => {
          if (err) reject(err);
          else resolve();
        });
        child.stdin?.write(text);
        child.stdin?.end();
      });
      return true;
    }
    if (await commandExists('xclip')) {
      await new Promise<void>((resolve, reject) => {
        const child = execFile('xclip', ['-i', '-selection', 'clipboard'], { timeout: 2000 }, (err) => {
          if (err) reject(err);
          else resolve();
        });
        child.stdin?.write(text);
        child.stdin?.end();
      });
      return true;
    }
  } else if (process.platform === 'darwin') {
    await new Promise<void>((resolve, reject) => {
      const child = execFile('pbcopy', [], { timeout: 2000 }, (err) => {
        if (err) reject(err);
        else resolve();
      });
      child.stdin?.write(text);
      child.stdin?.end();
    });
    return true;
  } else if (process.platform === 'win32') {
    await runPowerShell(`Set-Clipboard -Value '${text.replace(/'/g, "''")}'`, 3000);
    return true;
  }
  return false;
}

function toWindowsSendKeys(combo: string): string {
  const parts = combo.toLowerCase().split('+').map((p) => p.trim()).filter(Boolean);
  let mods = '';
  let mainKey = '';
  const map: Record<string, string> = {
    enter: '{ENTER}',
    return: '{ENTER}',
    tab: '{TAB}',
    esc: '{ESC}',
    escape: '{ESC}',
    backspace: '{BACKSPACE}',
    delete: '{DELETE}',
    del: '{DELETE}',
    up: '{UP}',
    down: '{DOWN}',
    left: '{LEFT}',
    right: '{RIGHT}',
    home: '{HOME}',
    end: '{END}',
    pageup: '{PGUP}',
    pagedown: '{PGDN}',
    space: ' ',
  };
  for (const p of parts) {
    if (p === 'ctrl' || p === 'control') mods += '^';
    else if (p === 'alt') mods += '%';
    else if (p === 'shift') mods += '+';
    else if (map[p]) mainKey = map[p];
    else if (/^f([1-9]|1[0-2])$/.test(p)) mainKey = `{${p.toUpperCase()}}`;
    else mainKey = p;
  }
  return `${mods}${mainKey}`;
}

async function handleKeyboard(args: Record<string, unknown>) {
  const action = String(args.action || 'type');

  if (action === 'type') {
    const text = typeof args.text === 'string' ? args.text : '';
    if (!text) {
      return { content: [{ type: 'text', text: 'text parameter is required.' }], isError: true };
    }

    if (process.platform === 'linux') {
      const isAsciiSingleLine = /^[\x20-\x7e]+$/.test(text);
      const ydoSock = await ensureYdotoolSocket();
      if (ydoSock && isAsciiSingleLine) {
        await runYdotool(['type', '--key-delay', '8', '--', text], 8000);
        return { content: [{ type: 'text', text: `Typed ${text.length} ASCII chars` }] };
      }
      if (!ydoSock && (await commandExists('xdotool'))) {
        await execFileAsync('xdotool', ['type', '--clearmodifiers', '--delay', '8', text], { timeout: 8000 });
        return { content: [{ type: 'text', text: `Typed ${text.length} chars (xdotool)` }] };
      }
    }

    // Unicode-safe atomic paste via clipboard + OS paste keystroke
    const prevClip = await readClipboardText();
    const ok = await writeClipboardText(text);
    if (!ok) {
      throw new Error('Clipboard backend required to type Unicode text.');
    }
    await new Promise((r) => setTimeout(r, 45));
    await handleKeyboard({ action: 'key', combo: process.platform === 'darwin' ? 'cmd+v' : 'ctrl+v' });
    await new Promise((r) => setTimeout(r, 95));
    if (prevClip !== null) {
      await writeClipboardText(prevClip).catch(() => {});
    }
    return { content: [{ type: 'text', text: `Typed ${text.length} chars (Unicode-safe)` }] };
  }

  if (action === 'key') {
    const combo = String(args.combo || '').trim();
    if (!combo) {
      return { content: [{ type: 'text', text: 'combo parameter is required.' }], isError: true };
    }

    if (process.platform === 'linux') {
      const seq = toYdotoolKeySequence(combo);
      const ydoSock = seq ? await ensureYdotoolSocket() : null;
      if (seq && ydoSock) {
        await runYdotool(['key', ...seq]);
        return { content: [{ type: 'text', text: `Pressed key combo: ${combo}` }] };
      }
      if (await commandExists('xdotool')) {
        const xCombo = combo
          .toLowerCase()
          .split('+')
          .map((p) => (p === 'enter' ? 'Return' : p === 'esc' ? 'Escape' : p))
          .join('+');
        await execFileAsync('xdotool', ['key', '--clearmodifiers', xCombo], { timeout: 3000 });
        return { content: [{ type: 'text', text: `Pressed key combo: ${combo}` }] };
      }
      throw new Error(`Keyboard automation unavailable for combo "${combo}".`);
    }

    if (process.platform === 'win32') {
      const sendKeysStr = toWindowsSendKeys(combo);
      await runPowerShell(
        `Add-Type -AssemblyName System.Windows.Forms; [System.Windows.Forms.SendKeys]::SendWait('${sendKeysStr.replace(/'/g, "''")}')`,
        4000,
      );
      return { content: [{ type: 'text', text: `Pressed key combo: ${combo}` }] };
    }

    if (process.platform === 'darwin') {
      const parts = combo.toLowerCase().split('+').map((p) => p.trim()).filter(Boolean);
      const mods: string[] = [];
      let mainKey = '';
      for (const p of parts) {
        if (p === 'cmd' || p === 'meta' || p === 'super' || p === 'win') mods.push('command down');
        else if (p === 'ctrl' || p === 'control') mods.push('control down');
        else if (p === 'alt' || p === 'option') mods.push('option down');
        else if (p === 'shift') mods.push('shift down');
        else mainKey = p;
      }
      const keyCodeMap: Record<string, number> = {
        enter: 36, return: 36, tab: 48, space: 49, backspace: 51, delete: 117, esc: 53, escape: 53,
        left: 123, right: 124, down: 125, up: 126,
      };
      const usingClause = mods.length > 0 ? ` using {${mods.join(', ')}}` : '';
      const stmt =
        keyCodeMap[mainKey] !== undefined
          ? `tell application "System Events" to key code ${keyCodeMap[mainKey]}${usingClause}`
          : `tell application "System Events" to keystroke ${JSON.stringify(mainKey)}${usingClause}`;
      await execFileAsync('osascript', ['-e', stmt], { timeout: 3500 });
      return { content: [{ type: 'text', text: `Pressed key combo: ${combo}` }] };
    }
  }

  return { content: [{ type: 'text', text: `Unknown keyboard action: ${action}` }], isError: true };
}

async function handleClipboard(args: Record<string, unknown>) {
  const action = String(args.action || 'read');
  if (action === 'read') {
    const text = await readClipboardText();
    return { content: [{ type: 'text', text: text ?? '' }] };
  }
  if (action === 'write') {
    const text = typeof args.text === 'string' ? args.text : '';
    const ok = await writeClipboardText(text);
    if (!ok) {
      return { content: [{ type: 'text', text: 'Failed to write clipboard.' }], isError: true };
    }
    return { content: [{ type: 'text', text: `Copied ${text.length} chars to clipboard` }] };
  }
  return { content: [{ type: 'text', text: `Unknown clipboard action: ${action}` }], isError: true };
}

async function handleSystem(args: Record<string, unknown>) {
  const act = String(args.action || '');
  const target = typeof args.target === 'string' ? args.target.trim() : '';

  if (act === 'open_url' && target) {
    if (!/^https?:\/\//i.test(target)) {
      return { content: [{ type: 'text', text: 'Only http:// and https:// URLs are allowed.' }], isError: true };
    }
    const bridged = await callBridge<{ ok?: boolean }>('POST', '/open-url', { url: target });
    if (bridged?.ok) {
      return { content: [{ type: 'text', text: `Opened URL: ${target}` }] };
    }
    if (process.platform === 'darwin') {
      await execFileAsync('open', [target], { timeout: 4000 });
    } else if (process.platform === 'win32') {
      await execFileAsync('cmd.exe', ['/c', 'start', '', target], { timeout: 4000, windowsHide: true });
    } else {
      await execFileAsync('xdg-open', [target], { timeout: 4000 });
    }
    return { content: [{ type: 'text', text: `Opened URL: ${target}` }] };
  }

  if (act === 'open_app' && target) {
    if (!/^[A-Za-z0-9 ._-]+$/.test(target)) {
      return { content: [{ type: 'text', text: 'Invalid application identifier.' }], isError: true };
    }
    if (process.platform === 'darwin') {
      await execFileAsync('open', ['-a', target], { timeout: 4000 });
      return { content: [{ type: 'text', text: `Launched ${target}` }] };
    }
    if (process.platform === 'win32') {
      const child = spawn('cmd.exe', ['/c', 'start', '', target], { detached: true, stdio: 'ignore', windowsHide: true });
      child.unref();
      return { content: [{ type: 'text', text: `Launched ${target}` }] };
    }
    if (await commandExists('gtk-launch')) {
      try {
        await execFileAsync('gtk-launch', [target], { timeout: 4000 });
        return { content: [{ type: 'text', text: `Launched ${target}` }] };
      } catch {}
    }
    if (await commandExists(target)) {
      const child = spawn(target, [], { detached: true, stdio: 'ignore', cwd: os.homedir() });
      child.unref();
      return { content: [{ type: 'text', text: `Launched ${target}` }] };
    }
    return { content: [{ type: 'text', text: `Application not found: ${target}` }], isError: true };
  }

  if (act === 'list_apps') {
    if (process.platform === 'linux') {
      const appDirs = [
        '/usr/share/applications',
        '/usr/local/share/applications',
        path.join(os.homedir(), '.local/share/applications'),
        '/var/lib/flatpak/exports/share/applications',
      ];
      const found: Array<{ name: string; exec: string; desktopId: string }> = [];
      for (const d of appDirs) {
        try {
          const files = await fs.readdir(d);
          for (const f of files) {
            if (!f.endsWith('.desktop')) continue;
            try {
              const raw = await fs.readFile(path.join(d, f), 'utf8');
              if (raw.includes('NoDisplay=true')) continue;
              const n = raw.match(/^Name=([^\n\r]+)/m)?.[1]?.trim();
              const ex = raw.match(/^Exec=([^\n\r]+)/m)?.[1]?.split(/\s+/)[0];
              if (n) {
                found.push({ name: n, exec: ex || f.replace('.desktop', ''), desktopId: f.replace('.desktop', '') });
              }
            } catch {}
          }
        } catch {}
      }
      const dedup = Array.from(new Map(found.map((a) => [a.desktopId, a])).values()).slice(0, 50);
      return { content: [{ type: 'text', text: JSON.stringify(dedup, null, 2) }] };
    }
    if (process.platform === 'darwin') {
      const entries = (await fs.readdir('/Applications').catch(() => []))
        .filter((f) => f.endsWith('.app'))
        .slice(0, 50)
        .map((f) => ({ name: f.replace(/\.app$/, ''), app: f }));
      return { content: [{ type: 'text', text: JSON.stringify(entries, null, 2) }] };
    }
    if (process.platform === 'win32') {
      const out = await runPowerShell('Get-StartApps | Select-Object -First 50 Name,AppID | ConvertTo-Json -Compress', 5000);
      return { content: [{ type: 'text', text: out.trim() }] };
    }
  }

  if (act === 'volume_get') {
    if (process.platform === 'darwin') {
      const { stdout } = await execFileAsync('osascript', ['-e', 'output volume of (get volume settings)']);
      return { content: [{ type: 'text', text: `Volume: ${stdout.trim()}%` }] };
    }
    if (process.platform === 'linux') {
      const { stdout } = await execFileAsync('wpctl', ['get-volume', '@DEFAULT_AUDIO_SINK@'], { timeout: 3000 });
      return { content: [{ type: 'text', text: stdout.trim() }] };
    }
  }

  if (act === 'volume_set') {
    const pct = Math.min(100, Math.max(0, Math.round(typeof args.value === 'number' ? args.value : 50)));
    if (process.platform === 'darwin') {
      await execFileAsync('osascript', ['-e', `set volume output volume ${pct}`]);
      return { content: [{ type: 'text', text: `Set volume to ${pct}%` }] };
    }
    if (process.platform === 'linux') {
      await execFileAsync('wpctl', ['set-volume', '@DEFAULT_AUDIO_SINK@', `${pct}%`], { timeout: 3000 });
      return { content: [{ type: 'text', text: `Set volume to ${pct}%` }] };
    }
  }

  if (act === 'volume_mute') {
    if (process.platform === 'darwin') {
      await execFileAsync('osascript', ['-e', 'set volume output muted not (output muted of (get volume settings))']);
      return { content: [{ type: 'text', text: 'Toggled audio mute' }] };
    }
    if (process.platform === 'linux') {
      await execFileAsync('wpctl', ['set-mute', '@DEFAULT_AUDIO_SINK@', 'toggle'], { timeout: 3000 });
      return { content: [{ type: 'text', text: 'Toggled audio mute' }] };
    }
  }

  if (act === 'theme_dark' || act === 'theme_light') {
    const isDark = act === 'theme_dark';
    if (process.platform === 'darwin') {
      await execFileAsync('osascript', [
        '-e',
        `tell app "System Events" to tell appearance preferences to set dark mode to ${isDark}`,
      ]);
      return { content: [{ type: 'text', text: `Switched macOS appearance to ${isDark ? 'Dark' : 'Light'}` }] };
    }
    if (process.platform === 'win32') {
      const val = isDark ? 0 : 1;
      await runPowerShell(
        `Set-ItemProperty -Path HKCU:\\Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize -Name AppsUseLightTheme -Value ${val}`,
      );
      return { content: [{ type: 'text', text: `Switched Windows app theme to ${isDark ? 'Dark' : 'Light'}` }] };
    }
    if (await commandExists('plasma-apply-colorscheme')) {
      await execFileAsync('plasma-apply-colorscheme', [isDark ? 'BreezeDark' : 'BreezeLight'], { timeout: 5000 });
      return { content: [{ type: 'text', text: `Switched KDE color scheme to ${isDark ? 'Dark' : 'Light'}` }] };
    }
    if (await commandExists('gsettings')) {
      await execFileAsync(
        'gsettings',
        ['set', 'org.gnome.desktop.interface', 'color-scheme', isDark ? 'prefer-dark' : 'prefer-light'],
        { timeout: 4000 },
      );
      return { content: [{ type: 'text', text: `Switched theme to ${isDark ? 'Dark' : 'Light'}` }] };
    }
    return { content: [{ type: 'text', text: 'No desktop theme switcher available.' }], isError: true };
  }

  if ((act === 'dnd_on' || act === 'dnd_off') && process.platform === 'linux') {
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
    return { content: [{ type: 'text', text: `Do Not Disturb ${act === 'dnd_on' ? 'enabled' : 'disabled'}` }] };
  }

  if (act === 'notify') {
    const title = typeof args.title === 'string' ? args.title : 'Notification';
    const body = typeof args.body === 'string' ? args.body : '';
    const bridged = await callBridge<{ ok?: boolean }>('POST', '/notify', { title, body });
    if (bridged?.ok) {
      return { content: [{ type: 'text', text: `Sent notification: ${title}` }] };
    }
    if (process.platform === 'darwin') {
      await execFileAsync('osascript', ['-e', `display notification ${JSON.stringify(body)} with title ${JSON.stringify(title)}`]);
    } else if (process.platform === 'linux') {
      await execFileAsync('notify-send', [title, body], { timeout: 3000 });
    }
    return { content: [{ type: 'text', text: `Sent notification: ${title}` }] };
  }

  return { content: [{ type: 'text', text: `Unsupported system action: ${act}` }], isError: true };
}

export const COMPUTER_USE_TOOLS: McpToolDef[] = [
  {
    name: 'desktop_screenshot',
    description:
      'Capture the user desktop screen and inspect current UI state. Returns the JPEG screenshot plus pixel/logical resolution and coordinate metadata.',
    inputSchema: {
      type: 'object',
      properties: {
        maxWidth: { type: 'number', description: 'Optional max output width (default 1440)' },
        region: {
          type: 'object',
          description: 'Optional crop region in pixels',
          properties: {
            x: { type: 'number' },
            y: { type: 'number' },
            width: { type: 'number' },
            height: { type: 'number' },
          },
        },
      },
    },
  },
  {
    name: 'desktop_list_windows',
    description: 'List all currently open desktop windows with their id, title (caption), application class, active/minimized state, and geometry.',
    inputSchema: { type: 'object', properties: {} },
  },
  {
    name: 'desktop_window_control',
    description: 'Focus, minimize, maximize, unmaximize, snap left/right, or close a desktop window by title or class substring.',
    inputSchema: {
      type: 'object',
      required: ['action', 'match'],
      properties: {
        action: { type: 'string', enum: ['focus', 'minimize', 'maximize', 'unmaximize', 'snap_left', 'snap_right', 'close'] },
        match: { type: 'string', description: 'Case-insensitive substring of window caption or resourceClass' },
      },
    },
  },
  {
    name: 'desktop_mouse',
    description:
      'Move, click, double click, right click, middle click, drag, or scroll the mouse pointer. Coordinates x, y use 0..1000 normalized screen space (0,0 = top-left, 1000,1000 = bottom-right) by default.',
    inputSchema: {
      type: 'object',
      required: ['action', 'x', 'y'],
      properties: {
        action: { type: 'string', enum: ['move', 'click', 'double_click', 'right_click', 'middle_click', 'drag', 'scroll'] },
        x: { type: 'number', description: 'Target X (0..1000 normalized, or pixel if normalized=false)' },
        y: { type: 'number', description: 'Target Y (0..1000 normalized, or pixel if normalized=false)' },
        endX: { type: 'number', description: 'Drag end X (0..1000 normalized)' },
        endY: { type: 'number', description: 'Drag end Y (0..1000 normalized)' },
        delta: { type: 'number', description: 'Scroll clicks (negative = down, positive = up)' },
        normalized: { type: 'boolean', description: 'Default true (0..1000 normalized)' },
      },
    },
  },
  {
    name: 'desktop_keyboard',
    description: 'Type Unicode/ASCII text or press a key combination (e.g. "ctrl+s", "alt+tab", "enter", "super+left").',
    inputSchema: {
      type: 'object',
      required: ['action'],
      properties: {
        action: { type: 'string', enum: ['type', 'key'] },
        text: { type: 'string', description: 'Text to type when action="type"' },
        combo: { type: 'string', description: 'Key combination like "ctrl+l" when action="key"' },
      },
    },
  },
  {
    name: 'desktop_clipboard',
    description: 'Read or write plain text on the system clipboard.',
    inputSchema: {
      type: 'object',
      required: ['action'],
      properties: {
        action: { type: 'string', enum: ['read', 'write'] },
        text: { type: 'string', description: 'Text to copy when action="write"' },
      },
    },
  },
  {
    name: 'desktop_system',
    description: 'Launch apps, open URLs, list installed apps, read/change volume, toggle dark/light theme or Do Not Disturb, or send desktop notifications.',
    inputSchema: {
      type: 'object',
      required: ['action'],
      properties: {
        action: {
          type: 'string',
          enum: ['open_app', 'open_url', 'list_apps', 'volume_get', 'volume_set', 'volume_mute', 'theme_dark', 'theme_light', 'dnd_on', 'dnd_off', 'notify'],
        },
        target: { type: 'string' },
        value: { type: 'number' },
        title: { type: 'string' },
        body: { type: 'string' },
      },
    },
  },
  {
    name: 'desktop_wait',
    description: 'Pause execution for ms milliseconds (50..10000) to allow UI animations or applications to finish loading.',
    inputSchema: {
      type: 'object',
      required: ['ms'],
      properties: {
        ms: { type: 'number' },
      },
    },
  },
];

async function executeToolCall(name: string, args: Record<string, unknown>) {
  switch (name) {
    case 'desktop_screenshot':
      return handleScreenshot(args);
    case 'desktop_list_windows':
      return handleListWindows();
    case 'desktop_window_control':
      return handleWindowControl(args);
    case 'desktop_mouse':
      return handleMouse(args);
    case 'desktop_keyboard':
      return handleKeyboard(args);
    case 'desktop_clipboard':
      return handleClipboard(args);
    case 'desktop_system':
      return handleSystem(args);
    case 'desktop_wait': {
      const ms = Math.max(50, Math.min(10_000, Number(args.ms) || 500));
      await new Promise((r) => setTimeout(r, ms));
      return { content: [{ type: 'text', text: `Waited ${ms}ms` }] };
    }
    default:
      return { content: [{ type: 'text', text: `Unknown tool: ${name}` }], isError: true };
  }
}

function sendJson(obj: Record<string, unknown>): void {
  process.stdout.write(`${JSON.stringify(obj)}\n`);
}

function startStdioMcpServer(): void {
  const rl = readline.createInterface({ input: process.stdin, crlfDelay: Infinity });
  rl.on('line', async (line) => {
    const trimmed = line.trim();
    if (!trimmed) return;
    let msg: Record<string, unknown>;
    try {
      msg = JSON.parse(trimmed);
    } catch {
      return;
    }

    const id = msg.id;
    const method = typeof msg.method === 'string' ? msg.method : '';
    const params =
      msg.params && typeof msg.params === 'object' ? (msg.params as Record<string, unknown>) : {};

    if (id === undefined || id === null) {
      return;
    }

    if (method === 'initialize') {
      const reqProto =
        typeof params.protocolVersion === 'string' ? params.protocolVersion : '2024-11-05';
      sendJson({
        jsonrpc: '2.0',
        id,
        result: {
          protocolVersion: reqProto,
          capabilities: {
            tools: { listChanged: false },
          },
          serverInfo: {
            name: 'computer-use',
            version: '4.0.0',
          },
        },
      });
      return;
    }

    if (method === 'ping') {
      sendJson({ jsonrpc: '2.0', id, result: {} });
      return;
    }

    if (method === 'tools/list') {
      sendJson({
        jsonrpc: '2.0',
        id,
        result: { tools: COMPUTER_USE_TOOLS },
      });
      return;
    }

    if (method === 'tools/call') {
      const toolName = typeof params.name === 'string' ? params.name : '';
      const toolArgs =
        params.arguments && typeof params.arguments === 'object'
          ? (params.arguments as Record<string, unknown>)
          : {};
      try {
        const res = await executeToolCall(toolName, toolArgs);
        sendJson({ jsonrpc: '2.0', id, result: res });
      } catch (err) {
        sendJson({
          jsonrpc: '2.0',
          id,
          result: {
            content: [{ type: 'text', text: err instanceof Error ? err.message : String(err) }],
            isError: true,
          },
        });
      }
      return;
    }

    sendJson({
      jsonrpc: '2.0',
      id,
      error: { code: -32601, message: `Method not found: ${method}` },
    });
  });
}

if (require.main === module) {
  startStdioMcpServer();
}
