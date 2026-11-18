import { execFile } from 'node:child_process';
import fs from 'node:fs/promises';
import fsSync from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { promisify } from 'node:util';
import { desktopCapturer, nativeImage, screen } from 'electron';
import type { ScreenshotQuality } from '../shared/types';

const execFileAsync = promisify(execFile);

export type CaptureKind = 'fullscreen' | 'region' | 'window';

export function encodeNativeImage(
  img: Electron.NativeImage,
  quality: ScreenshotQuality,
): { dataUrl: string; mimeType: string; name: string } {
  const size = img.getSize();
  if (quality === 'compressed') {
    const resized =
      size.width > 1600 ? img.resize({ width: 1600, quality: 'good' }) : img;
    const jpg = resized.toJPEG(84);
    return {
      dataUrl: `data:image/jpeg;base64,${jpg.toString('base64')}`,
      mimeType: 'image/jpeg',
      name: `screenshot-${Date.now()}.jpg`,
    };
  }
  const png = img.toPNG();
  return {
    dataUrl: `data:image/png;base64,${png.toString('base64')}`,
    mimeType: 'image/png',
    name: `screenshot-${Date.now()}.png`,
  };
}

/**
 * Converts a KWin ScreenShot2 framebuffer (RGBA/RGBX or ARGB/RGB32 + optional stride padding)
 * into a tight little-endian BGRA buffer compatible with Electron `nativeImage.createFromBitmap`.
 */
export function kwinPixelsToBgra(
  raw: Buffer,
  width: number,
  height: number,
  stride: number,
  format: number,
): Buffer {
  const rowBytes = width * 4;
  const out = Buffer.allocUnsafe(rowBytes * height);
  // Qt QImage formats: 16=RGBX8888, 17=RGBA8888, 18=RGBA8888_Premultiplied
  const isRgbaOrder = format === 16 || format === 17 || format === 18;
  const forceOpaque = format === 16 || format === 4;

  for (let y = 0; y < height; y++) {
    const srcRow = y * stride;
    const dstRow = y * rowBytes;
    if (!isRgbaOrder && !forceOpaque) {
      raw.copy(out, dstRow, srcRow, srcRow + rowBytes);
      continue;
    }
    for (let x = 0; x < rowBytes; x += 4) {
      const s = srcRow + x;
      const d = dstRow + x;
      if (isRgbaOrder) {
        out[d] = raw[s + 2];
        out[d + 1] = raw[s + 1];
        out[d + 2] = raw[s];
        out[d + 3] = forceOpaque ? 255 : raw[s + 3];
      } else {
        out[d] = raw[s];
        out[d + 1] = raw[s + 1];
        out[d + 2] = raw[s + 2];
        out[d + 3] = 255;
      }
    }
  }
  return out;
}

const KWIN_SHOT_SCRIPT = `
import sys, os, struct, gi
gi.require_version('Gio', '2.0')
from gi.repository import Gio, GLib
method = sys.argv[1]
bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
r_fd, w_fd = os.pipe()
fd_list = Gio.UnixFDList.new()
idx = fd_list.append(w_fd)
os.close(w_fd)
res, _ = bus.call_with_unix_fd_list_sync(
    "org.kde.KWin",
    "/org/kde/KWin/ScreenShot2",
    "org.kde.KWin.ScreenShot2",
    method,
    GLib.Variant("(a{sv}h)", ({"include-cursor": GLib.Variant("b", False)}, idx)),
    GLib.VariantType("(a{sv})"),
    Gio.DBusCallFlags.NONE,
    3000,
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
sys.stdout.buffer.write(struct.pack("<IIII", w, h, stride, fmt))
for c in chunks:
    sys.stdout.buffer.write(c)
sys.stdout.buffer.flush()
`;

let cachedGiPython: string | null | undefined;

async function resolveSystemPythonWithGi(): Promise<string | null> {
  if (cachedGiPython !== undefined) return cachedGiPython;
  // Prefer system `/usr/bin/python3` first so mise / homebrew / conda shims on PATH
  // never shadow distro `python3-gobject` (`gi`).
  const candidates = ['/usr/bin/python3', '/bin/python3', 'python3'];
  for (const bin of candidates) {
    if (bin.startsWith('/') && !fsSync.existsSync(bin)) continue;
    try {
      await execFileAsync(bin, ['-c', 'import gi; gi.require_version("Gio", "2.0")'], {
        timeout: 1500,
      });
      cachedGiPython = bin;
      return bin;
    } catch {
      // try next candidate
    }
  }
  cachedGiPython = null;
  return null;
}

async function runKwinMethod(
  pythonBin: string,
  method: 'CaptureActiveScreen' | 'CaptureActiveWindow' | 'CaptureWorkspace',
): Promise<Electron.NativeImage | null> {
  try {
    const out = await new Promise<Buffer>((resolve, reject) => {
      const child = execFile(
        pythonBin,
        ['-c', KWIN_SHOT_SCRIPT, method],
        { encoding: 'buffer', maxBuffer: 64 * 1024 * 1024, timeout: 3500 },
        (err, stdout) => {
          if (err) reject(err);
          else resolve(Buffer.isBuffer(stdout) ? stdout : Buffer.from(stdout));
        },
      );
      child.stdin?.end();
    });
    if (out.length < 16) return null;
    const width = out.readUInt32LE(0);
    const height = out.readUInt32LE(4);
    const stride = out.readUInt32LE(8);
    const format = out.readUInt32LE(12);
    const raw = out.subarray(16);
    if (width <= 0 || height <= 0 || raw.length < stride * height) return null;
    const bgra = kwinPixelsToBgra(raw, width, height, stride, format);
    const img = nativeImage.createFromBitmap(bgra, { width, height });
    return img.isEmpty() ? null : img;
  } catch {
    return null;
  }
}

async function captureViaKwinDBus(kind: 'fullscreen' | 'window'): Promise<Electron.NativeImage | null> {
  const pythonBin = await resolveSystemPythonWithGi();
  if (!pythonBin) return null;

  if (kind === 'window') {
    const winImg = await runKwinMethod(pythonBin, 'CaptureActiveWindow');
    if (winImg) return winImg;
  }
  const screenImg = await runKwinMethod(pythonBin, 'CaptureActiveScreen');
  if (screenImg) return screenImg;
  return runKwinMethod(pythonBin, 'CaptureWorkspace');
}

async function commandExists(cmd: string): Promise<boolean> {
  const pathDirs = (process.env.PATH ?? '').split(path.delimiter).filter(Boolean);
  for (const dir of [...pathDirs, '/usr/bin', '/bin', '/usr/local/bin']) {
    try {
      const stat = await fs.stat(path.join(dir, cmd));
      if (stat.isFile()) return true;
    } catch {
      // keep searching
    }
  }
  return false;
}

export async function captureScreenNativeImage(
  kind: CaptureKind,
  excludeWindow?: Electron.BrowserWindow | null,
  restoreOpacity = 1,
): Promise<Electron.NativeImage> {
  const wasVisible = Boolean(
    excludeWindow && !excludeWindow.isDestroyed() && excludeWindow.isVisible(),
  );
  if (wasVisible && excludeWindow && !excludeWindow.isDestroyed()) {
    excludeWindow.setOpacity(0);
    excludeWindow.hide();
    // Allow compositor frame + focus handoff to underlying window before grabbing pixels
    await new Promise((r) => setTimeout(r, kind === 'window' ? 90 : 55));
  }

  try {
    const isLinuxWayland = process.platform === 'linux' && Boolean(process.env.WAYLAND_DISPLAY);

    if (isLinuxWayland) {
      // 1. Direct KDE KWin ScreenShot2 D-Bus capture (~40ms, zero temp files, zero portal popup)
      const kwinMode = kind === 'window' ? 'window' : 'fullscreen';
      const kwinImg = await captureViaKwinDBus(kwinMode);
      if (kwinImg) return kwinImg;

      // 2. Fast wlroots grim (silent, zero portal popup)
      if (await commandExists('grim')) {
        const tmpPath = path.join(os.tmpdir(), `chat-overlay-${Date.now()}.png`);
        try {
          await execFileAsync('grim', [tmpPath], { timeout: 4000 });
          const buf = await fs.readFile(tmpPath);
          await fs.unlink(tmpPath).catch(() => {});
          const img = nativeImage.createFromBuffer(buf);
          if (!img.isEmpty()) return img;
        } catch {
          await fs.unlink(tmpPath).catch(() => {});
        }
      }

      // 3. Silent KDE Spectacle background CLI (-b -n) BEFORE desktopCapturer so KDE never shows ScreenCast portal
      if (await commandExists('spectacle')) {
        const tmpPath = path.join(os.tmpdir(), `chat-overlay-${Date.now()}.png`);
        const modeFlag = kind === 'window' ? '-a' : '-f';
        try {
          await execFileAsync('spectacle', ['-b', '-n', modeFlag, '-o', tmpPath], {
            timeout: 5000,
          });
          const buf = await fs.readFile(tmpPath);
          await fs.unlink(tmpPath).catch(() => {});
          const img = nativeImage.createFromBuffer(buf);
          if (!img.isEmpty()) return img;
        } catch {
          await fs.unlink(tmpPath).catch(() => {});
        }
      }

      // 4. Silent GNOME screenshot CLI if available
      if (await commandExists('gnome-screenshot')) {
        const tmpPath = path.join(os.tmpdir(), `chat-overlay-${Date.now()}.png`);
        const args = kind === 'window' ? ['-w', '-f', tmpPath] : ['-f', tmpPath];
        try {
          await execFileAsync('gnome-screenshot', args, { timeout: 5000 });
          const buf = await fs.readFile(tmpPath);
          await fs.unlink(tmpPath).catch(() => {});
          const img = nativeImage.createFromBuffer(buf);
          if (!img.isEmpty()) return img;
        } catch {
          await fs.unlink(tmpPath).catch(() => {});
        }
      }
    }

    // 5. Electron desktopCapturer (instant & silent on Windows, macOS, X11; absolute last resort on Wayland)
    const primary = screen.getPrimaryDisplay();
    const width = Math.round(primary.size.width * primary.scaleFactor);
    const height = Math.round(primary.size.height * primary.scaleFactor);
    const sources = await desktopCapturer.getSources({
      types: kind === 'window' ? ['window', 'screen'] : ['screen'],
      thumbnailSize: { width: Math.max(width, 1280), height: Math.max(height, 720) },
    });

    const chosen = sources[0];
    if (chosen && !chosen.thumbnail.isEmpty()) {
      return chosen.thumbnail;
    }

    throw new Error('No screen capture source available.');
  } finally {
    if (wasVisible && excludeWindow && !excludeWindow.isDestroyed()) {
      excludeWindow.setOpacity(Math.min(1, Math.max(0.5, restoreOpacity)));
      excludeWindow.showInactive?.();
    }
  }
}

export async function captureScreenDataUrl(
  kind: CaptureKind,
  excludeWindow?: Electron.BrowserWindow | null,
  quality: ScreenshotQuality = 'compressed',
  restoreOpacity = 1,
): Promise<{ dataUrl: string; mimeType: string; name: string }> {
  const img = await captureScreenNativeImage(kind, excludeWindow, restoreOpacity);
  return encodeNativeImage(img, quality);
}
