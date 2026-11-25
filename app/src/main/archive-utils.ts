import { execFile } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { promisify } from 'node:util';

const execFileAsync = promisify(execFile);

export function findFileRecursive(rootDir: string, targetName: string): string | null {
  if (!fs.existsSync(rootDir)) return null;
  const entries = fs.readdirSync(rootDir, { withFileTypes: true });
  for (const entry of entries) {
    const full = path.join(rootDir, entry.name);
    if (entry.isFile() && entry.name.toLowerCase() === targetName.toLowerCase()) {
      return full;
    }
    if (entry.isDirectory()) {
      const nested = findFileRecursive(full, targetName);
      if (nested) return nested;
    }
  }
  return null;
}

export async function extractArchive(
  archivePath: string,
  destDir: string,
  platform: NodeJS.Platform = process.platform,
): Promise<void> {
  fs.mkdirSync(destDir, { recursive: true });
  if (archivePath.endsWith('.tar.gz') || archivePath.endsWith('.tgz')) {
    await execFileAsync(platform === 'win32' ? 'tar.exe' : 'tar', ['-xzf', archivePath, '-C', destDir], {
      timeout: 60_000,
    });
    return;
  }

  if (archivePath.endsWith('.zip')) {
    if (platform === 'win32') {
      try {
        await execFileAsync('tar.exe', ['-xf', archivePath, '-C', destDir], { timeout: 60_000 });
        return;
      } catch {
        const psCmd = `Expand-Archive -LiteralPath '${archivePath.replace(/'/g, "''")}' -DestinationPath '${destDir.replace(/'/g, "''")}' -Force`;
        await execFileAsync('powershell', ['-NoProfile', '-NonInteractive', '-Command', psCmd], {
          timeout: 60_000,
          windowsHide: true,
        });
        return;
      }
    }
    if (platform === 'darwin') {
      await execFileAsync('ditto', ['-xk', archivePath, destDir], { timeout: 60_000 });
      return;
    }
    try {
      await execFileAsync('unzip', ['-oq', archivePath, '-d', destDir], { timeout: 60_000 });
      return;
    } catch {
      await execFileAsync('tar', ['-xf', archivePath, '-C', destDir], { timeout: 60_000 });
    }
    return;
  }

  throw new Error(`Unsupported archive format: ${archivePath}`);
}
