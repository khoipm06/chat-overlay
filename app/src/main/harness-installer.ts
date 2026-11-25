import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { managedBinDir } from './app-paths';
import { extractArchive, findFileRecursive } from './archive-utils';
import { downloadModelFile } from './speech-manager';

export interface StandaloneBinaryAsset {
  fileName: string;
  url: string;
  binaryName: string;
}

export function resolveOpenCodeAsset(
  platform: NodeJS.Platform = process.platform,
  arch: string = process.arch,
): StandaloneBinaryAsset | null {
  let fileName = '';
  let binaryName = 'opencode';

  if (platform === 'linux') {
    fileName = arch === 'arm64' ? 'opencode-linux-arm64.tar.gz' : 'opencode-linux-x64.tar.gz';
  } else if (platform === 'darwin') {
    fileName = arch === 'arm64' ? 'opencode-darwin-arm64.zip' : 'opencode-darwin-x64.zip';
  } else if (platform === 'win32') {
    binaryName = 'opencode.exe';
    fileName = arch === 'arm64' ? 'opencode-windows-arm64.zip' : 'opencode-windows-x64.zip';
  } else {
    return null;
  }

  return {
    fileName,
    binaryName,
    url: `https://github.com/anomalyco/opencode/releases/latest/download/${fileName}`,
  };
}

export async function installStandaloneOpenCode(
  onProgress?: (detail: string, percent: number) => void,
): Promise<{ ok: boolean; binaryPath?: string; message: string }> {
  const asset = resolveOpenCodeAsset();
  if (!asset) {
    return {
      ok: false,
      message: `Standalone OpenCode installer is unavailable for ${process.platform}/${process.arch}.`,
    };
  }

  const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'co-opencode-install-'));
  try {
    const archivePath = path.join(tmpDir, asset.fileName);
    const extractDir = path.join(tmpDir, 'extracted');
    onProgress?.(`Downloading ${asset.fileName}...`, 5);

    await downloadModelFile(asset.url, archivePath, ({ percent }) => {
      onProgress?.(`Downloading OpenCode (${percent}%)...`, Math.min(85, Math.max(5, Math.round(percent * 0.85))));
    });

    onProgress?.('Extracting OpenCode binary...', 90);
    await extractArchive(archivePath, extractDir);

    const extractedBin = findFileRecursive(extractDir, asset.binaryName);
    if (!extractedBin) {
      return {
        ok: false,
        message: `Downloaded archive did not contain ${asset.binaryName}.`,
      };
    }

    const binDir = managedBinDir();
    fs.mkdirSync(binDir, { recursive: true });
    const destBin = path.join(binDir, asset.binaryName);
    const tmpDest = `${destBin}.tmp`;
    fs.copyFileSync(extractedBin, tmpDest);
    if (process.platform !== 'win32') {
      fs.chmodSync(tmpDest, 0o755);
    }
    fs.renameSync(tmpDest, destBin);
    onProgress?.('OpenCode installed successfully.', 100);
    return {
      ok: true,
      binaryPath: destBin,
      message: `Installed OpenCode to ${destBin}`,
    };
  } catch (err) {
    return {
      ok: false,
      message: `Standalone OpenCode install failed: ${err instanceof Error ? err.message : String(err)}`,
    };
  } finally {
    try {
      fs.rmSync(tmpDir, { recursive: true, force: true });
    } catch {
      // ignore cleanup errors
    }
  }
}
