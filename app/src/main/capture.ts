import { execFile } from 'node:child_process';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { promisify } from 'node:util';
import { desktopCapturer, screen } from 'electron';

const execFileAsync = promisify(execFile);

export type CaptureKind = 'fullscreen' | 'region' | 'window';

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

export async function captureScreenDataUrl(
  kind: CaptureKind,
  excludeWindow?: Electron.BrowserWindow | null,
): Promise<{ dataUrl: string; mimeType: string; name: string }> {
  const wasVisible = Boolean(excludeWindow && !excludeWindow.isDestroyed() && excludeWindow.isVisible());
  if (wasVisible && kind === 'fullscreen') {
    excludeWindow?.hide();
    await new Promise((r) => setTimeout(r, 120));
  }

  try {
    // On Linux Wayland (KDE Spectacle or wlroots grim/slurp), native tools avoid repeated portal prompts
    // and provide native region/window selection when available.
    if (process.platform === 'linux' && process.env.WAYLAND_DISPLAY) {
      const tmpPath = path.join(os.tmpdir(), `agent-overlay-${Date.now()}.png`);
      if (await commandExists('spectacle')) {
        const modeFlag = kind === 'region' ? '-r' : kind === 'window' ? '-a' : '-f';
        try {
          await execFileAsync('spectacle', ['-b', '-n', '-p', modeFlag, '-o', tmpPath]);
          const buf = await fs.readFile(tmpPath);
          await fs.unlink(tmpPath).catch(() => {});
          return {
            dataUrl: `data:image/png;base64,${buf.toString('base64')}`,
            mimeType: 'image/png',
            name: `screenshot-${Date.now()}.png`,
          };
        } catch {
          await fs.unlink(tmpPath).catch(() => {});
        }
      }

      if (await commandExists('grim')) {
        try {
          if (kind === 'region' && (await commandExists('slurp'))) {
            const { stdout: geom } = await execFileAsync('slurp', []);
            await execFileAsync('grim', ['-g', geom.trim(), tmpPath]);
          } else {
            await execFileAsync('grim', [tmpPath]);
          }
          const buf = await fs.readFile(tmpPath);
          await fs.unlink(tmpPath).catch(() => {});
          return {
            dataUrl: `data:image/png;base64,${buf.toString('base64')}`,
            mimeType: 'image/png',
            name: `screenshot-${Date.now()}.png`,
          };
        } catch {
          await fs.unlink(tmpPath).catch(() => {});
        }
      }
    }

    // Universal Electron desktopCapturer fallback (Windows, macOS, X11, Wayland w/ portal)
    const primary = screen.getPrimaryDisplay();
    const width = Math.round(primary.size.width * primary.scaleFactor);
    const height = Math.round(primary.size.height * primary.scaleFactor);
    const sources = await desktopCapturer.getSources({
      types: kind === 'window' ? ['window', 'screen'] : ['screen'],
      thumbnailSize: { width: Math.max(width, 1280), height: Math.max(height, 720) },
    });

    const chosen = sources[0];
    if (!chosen || chosen.thumbnail.isEmpty()) {
      throw new Error('No screen capture source available.');
    }

    const png = chosen.thumbnail.toPNG();
    return {
      dataUrl: `data:image/png;base64,${png.toString('base64')}`,
      mimeType: 'image/png',
      name: `screenshot-${Date.now()}.png`,
    };
  } finally {
    if (wasVisible && excludeWindow && !excludeWindow.isDestroyed()) {
      excludeWindow.showInactive?.();
    }
  }
}
