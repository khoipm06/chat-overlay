import { spawn, type ChildProcess } from 'node:child_process';
import fs from 'node:fs';
import net from 'node:net';
import os from 'node:os';
import path from 'node:path';
import type {
  AppSettings,
  CustomSpeechModelInput,
  SpeechInstallProgress,
  SpeechModelEntry,
  SpeechStatus,
  WhisperModelSpec,
} from '../shared/types';
import { appDataDir } from './app-paths';
import { extractArchive, findFileRecursive } from './archive-utils';
import { configDir } from './settings';
import { findExecutableInPath, resolveOpenAiKey, resolveSpeechApiKey } from './speech';

export const WHISPER_MODELS: WhisperModelSpec[] = [
  {
    id: 'large-v3-turbo-q5_0',
    fileName: 'ggml-large-v3-turbo-q5_0.bin',
    sizeBytes: 574_041_195,
    url: 'https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo-q5_0.bin',
    label: 'Whisper Large-v3-Turbo (q5_0 · ~574 MB)',
    description: 'Best multilingual & mixed-language (code-switching) accuracy.',
    recommended: true,
  },
  {
    id: 'small-q5_1',
    fileName: 'ggml-small-q5_1.bin',
    sizeBytes: 190_085_487,
    url: 'https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small-q5_1.bin',
    label: 'Whisper Small (q5_1 · ~190 MB)',
    description: 'Lightweight — Lower disk & RAM footprint for modest hardware.',
    recommended: false,
  },
];

interface StoredCustomSpeechModel {
  id: string;
  label: string;
  description: string;
  source: 'custom_url' | 'custom_local';
  fileName: string;
  url?: string;
  localPath?: string;
}

export function speechDataDir(): string {
  return path.join(appDataDir(), 'speech');
}

export function speechModelsDir(): string {
  return path.join(speechDataDir(), 'models');
}

export function speechBinDir(): string {
  return path.join(speechDataDir(), 'bin');
}

export function speechStagingDir(): string {
  return path.join(speechDataDir(), '.staging');
}

export function customSpeechModelsFilePath(): string {
  return path.join(configDir(), 'speech_models.json');
}

function loadCustomSpeechModels(): StoredCustomSpeechModel[] {
  const target = customSpeechModelsFilePath();
  if (!fs.existsSync(target)) return [];
  try {
    const parsed = JSON.parse(fs.readFileSync(target, 'utf8'));
    if (!Array.isArray(parsed)) return [];
    return parsed.filter(
      (item): item is StoredCustomSpeechModel =>
        Boolean(item && typeof item.id === 'string' && typeof item.label === 'string'),
    );
  } catch {
    return [];
  }
}

function saveCustomSpeechModels(models: StoredCustomSpeechModel[]): void {
  fs.mkdirSync(configDir(), { recursive: true });
  const target = customSpeechModelsFilePath();
  const tmp = `${target}.tmp`;
  fs.writeFileSync(tmp, JSON.stringify(models, null, 2), 'utf8');
  fs.renameSync(tmp, target);
}

function deriveSafeBinFileName(labelOrUrl: string, fallbackId: string): string {
  try {
    const u = new URL(labelOrUrl);
    const base = path.basename(u.pathname).replace(/[^A-Za-z0-9._-]/g, '');
    if (base && (base.endsWith('.bin') || base.includes('ggml'))) {
      return base;
    }
  } catch {
    // not a URL
  }
  const slug = labelOrUrl
    .toLowerCase()
    .replace(/[^a-z0-9._-]+/g, '-')
    .replace(/^-+|-+$/g, '')
    .slice(0, 48);
  const safe = slug || fallbackId;
  return safe.endsWith('.bin') ? safe : `${safe}.bin`;
}

export interface WhisperBinaryAsset {
  fileName: string;
  url: string;
  binaryName: string;
}

export function resolveWhisperBinaryAsset(
  platform: NodeJS.Platform = process.platform,
  arch: string = process.arch,
): WhisperBinaryAsset | null {
  let fileName = '';
  let binaryName = 'whisper-server';

  if (platform === 'linux') {
    fileName = arch === 'arm64' ? 'whisper-server-linux-arm64.tar.gz' : 'whisper-server-linux-x64.tar.gz';
  } else if (platform === 'darwin') {
    fileName = arch === 'arm64' ? 'whisper-server-darwin-arm64.tar.gz' : 'whisper-server-darwin-x64.tar.gz';
  } else if (platform === 'win32') {
    binaryName = 'whisper-server.exe';
    if (arch !== 'x64') return null;
    fileName = 'whisper-server-win32-x64.zip';
  } else {
    return null;
  }

  return {
    fileName,
    binaryName,
    url: `https://github.com/khoipm06/chat-overlay/releases/latest/download/${fileName}`,
  };
}

export function findManagedWhisperBinary(
  names: string[] = ['whisper-server', 'whisper-cli', 'whisper-cpp', 'whisper.cpp'],
): string | null {
  const binDir = speechBinDir();
  for (const name of names) {
    const ext = process.platform === 'win32' && !name.endsWith('.exe') ? '.exe' : '';
    const candidate = path.join(binDir, name + ext);
    if (fs.existsSync(candidate)) {
      if (process.platform === 'win32') return candidate;
      try {
        fs.accessSync(candidate, fs.constants.X_OK);
        return candidate;
      } catch {
        // ignore
      }
    }
  }
  const found = findExecutableInPath(names);
  if (found && found.includes('/tmp/.mount_')) return null;
  return found;
}

export function findManagedOnlyRuntimePath(): string | null {
  const binDir = speechBinDir();
  for (const name of ['whisper-server', 'whisper-cli']) {
    const ext = process.platform === 'win32' ? '.exe' : '';
    const candidate = path.join(binDir, name + ext);
    if (fs.existsSync(candidate)) {
      return candidate;
    }
  }
  return null;
}

export function listSpeechModels(preferredModelId?: string): SpeechModelEntry[] {
  const entries: SpeechModelEntry[] = [];
  const modelsDir = speechModelsDir();
  const seenFiles = new Set<string>();

  for (const spec of WHISPER_MODELS) {
    const candidate = path.join(modelsDir, spec.fileName);
    const exists = fs.existsSync(candidate);
    if (exists) seenFiles.add(spec.fileName);
    entries.push({
      id: spec.id,
      label: spec.label,
      description: spec.description,
      fileName: spec.fileName,
      sizeBytes: spec.sizeBytes,
      source: 'builtin',
      url: spec.url,
      installed: exists,
      resolvedPath: exists ? candidate : null,
      active: false,
      recommended: spec.recommended,
    });
  }

  for (const custom of loadCustomSpeechModels()) {
    if (custom.source === 'custom_local') {
      const p = custom.localPath ? path.resolve(custom.localPath) : '';
      const exists = Boolean(p && fs.existsSync(p));
      let sizeBytes: number | undefined;
      if (exists) {
        try {
          sizeBytes = fs.statSync(p).size;
        } catch {}
      }
      entries.push({
        id: custom.id,
        label: custom.label,
        description: custom.description || `Local file: ${p}`,
        fileName: custom.fileName || path.basename(p),
        sizeBytes,
        source: 'custom_local',
        localPath: custom.localPath,
        installed: exists,
        resolvedPath: exists ? p : null,
        active: false,
      });
    } else {
      const candidate = path.join(modelsDir, path.basename(custom.fileName));
      const exists = fs.existsSync(candidate);
      if (exists) seenFiles.add(path.basename(candidate));
      let sizeBytes: number | undefined;
      if (exists) {
        try {
          sizeBytes = fs.statSync(candidate).size;
        } catch {}
      }
      entries.push({
        id: custom.id,
        label: custom.label,
        description: custom.description || `Custom URL model (${custom.fileName})`,
        fileName: custom.fileName,
        sizeBytes,
        source: 'custom_url',
        url: custom.url,
        installed: exists,
        resolvedPath: exists ? candidate : null,
        active: false,
      });
    }
  }

  if (fs.existsSync(modelsDir)) {
    try {
      for (const f of fs.readdirSync(modelsDir)) {
        if ((!f.endsWith('.bin') && !f.includes('ggml')) || f.endsWith('.part')) continue;
        if (seenFiles.has(f)) continue;
        const full = path.join(modelsDir, f);
        let sizeBytes: number | undefined;
        try {
          const st = fs.statSync(full);
          if (!st.isFile()) continue;
          sizeBytes = st.size;
        } catch {
          continue;
        }
        entries.push({
          id: `local:${f}`,
          label: f,
          description: `Discovered model file in ${modelsDir}`,
          fileName: f,
          sizeBytes,
          source: 'custom_local',
          localPath: full,
          installed: true,
          resolvedPath: full,
          active: false,
        });
      }
    } catch {
      // ignore
    }
  }

  let activeEntry: SpeechModelEntry | undefined;
  if (preferredModelId) {
    activeEntry = entries.find(
      (e) =>
        e.installed &&
        (e.id === preferredModelId ||
          e.fileName === preferredModelId ||
          e.id.includes(preferredModelId)),
    );
  }
  if (!activeEntry) {
    activeEntry = entries.find((e) => e.installed);
  }
  if (activeEntry) {
    activeEntry.active = true;
  }

  return entries;
}

export function listInstalledWhisperModelFiles(): string[] {
  const fromEntries = listSpeechModels()
    .filter((m) => m.installed && m.resolvedPath)
    .map((m) => m.resolvedPath!);
  const extraDirs = [
    path.join(os.homedir(), '.cache', 'whisper'),
    path.join(os.homedir(), '.local', 'share', 'whisper'),
    path.join(os.homedir(), '.local', 'share', 'whisper.cpp'),
    path.join(os.homedir(), 'whisper.cpp', 'models'),
  ];
  const out = [...fromEntries];
  const seen = new Set(out);
  for (const d of extraDirs) {
    if (!fs.existsSync(d)) continue;
    try {
      for (const f of fs.readdirSync(d)) {
        if ((f.endsWith('.bin') || f.includes('ggml')) && !f.endsWith('.part')) {
          const full = path.join(d, f);
          if (!seen.has(full)) {
            seen.add(full);
            out.push(full);
          }
        }
      }
    } catch {
      // ignore
    }
  }
  return out;
}

export function resolveActiveModelPath(preferredModelId?: string): string | null {
  const models = listSpeechModels(preferredModelId);
  const active = models.find((m) => m.active && m.resolvedPath);
  if (active?.resolvedPath) return active.resolvedPath;

  const extra = listInstalledWhisperModelFiles();
  return extra[0] || null;
}

export function addCustomSpeechModel(
  input: CustomSpeechModelInput,
): { ok: boolean; model?: SpeechModelEntry; error?: string } {
  const label = (input.label || '').trim();
  if (!label) {
    return { ok: false, error: 'Model name is required.' };
  }
  const description = (input.description || '').trim();
  const id = `custom_${Date.now().toString(36)}_${Math.random().toString(36).slice(2, 6)}`;
  const customList = loadCustomSpeechModels();

  if (input.source === 'custom_url') {
    const rawUrl = (input.url || '').trim();
    if (!/^https?:\/\//i.test(rawUrl)) {
      return { ok: false, error: 'A valid http:// or https:// download URL is required.' };
    }
    let fileName = deriveSafeBinFileName(rawUrl, id);
    if (
      WHISPER_MODELS.some((m) => m.fileName === fileName) ||
      customList.some((m) => m.fileName === fileName)
    ) {
      fileName = `${id}-${fileName}`;
    }
    customList.push({
      id,
      label,
      description: description || `Custom model from URL`,
      source: 'custom_url',
      fileName,
      url: rawUrl,
    });
    saveCustomSpeechModels(customList);
  } else {
    const rawPath = (input.localPath || '').trim();
    if (!rawPath) {
      return { ok: false, error: 'Local model file path is required.' };
    }
    const resolved = path.resolve(rawPath);
    if (!fs.existsSync(resolved)) {
      return { ok: false, error: `Model file does not exist: ${resolved}` };
    }
    try {
      if (!fs.statSync(resolved).isFile()) {
        return { ok: false, error: `Selected path is not a file: ${resolved}` };
      }
    } catch {
      return { ok: false, error: `Cannot read file: ${resolved}` };
    }
    customList.push({
      id,
      label,
      description: description || `Local model file`,
      source: 'custom_local',
      fileName: path.basename(resolved),
      localPath: resolved,
    });
    saveCustomSpeechModels(customList);
  }

  const created = listSpeechModels().find((m) => m.id === id);
  return { ok: true, model: created };
}

export function updateCustomSpeechModel(
  id: string,
  patch: Partial<CustomSpeechModelInput>,
): { ok: boolean; model?: SpeechModelEntry; error?: string } {
  const customList = loadCustomSpeechModels();
  const idx = customList.findIndex((m) => m.id === id);
  if (idx === -1) {
    return { ok: false, error: 'Only custom models can be edited.' };
  }
  const existing = customList[idx];
  const nextLabel = patch.label !== undefined ? patch.label.trim() : existing.label;
  if (!nextLabel) {
    return { ok: false, error: 'Model name cannot be empty.' };
  }
  const nextDesc = patch.description !== undefined ? patch.description.trim() : existing.description;

  if (existing.source === 'custom_url') {
    const nextUrl = patch.url !== undefined ? patch.url.trim() : existing.url || '';
    if (!/^https?:\/\//i.test(nextUrl)) {
      return { ok: false, error: 'A valid http:// or https:// download URL is required.' };
    }
    existing.label = nextLabel;
    existing.description = nextDesc;
    existing.url = nextUrl;
  } else {
    const nextPath = patch.localPath !== undefined ? patch.localPath.trim() : existing.localPath || '';
    const resolved = path.resolve(nextPath);
    if (!nextPath || !fs.existsSync(resolved)) {
      return { ok: false, error: `Local model file does not exist: ${resolved}` };
    }
    existing.label = nextLabel;
    existing.description = nextDesc;
    existing.localPath = resolved;
    existing.fileName = path.basename(resolved);
  }

  customList[idx] = existing;
  saveCustomSpeechModels(customList);
  const updated = listSpeechModels().find((m) => m.id === id);
  return { ok: true, model: updated };
}

export function getSpeechStatus(settings?: Partial<AppSettings>): SpeechStatus {
  const hasGemini = Boolean(resolveSpeechApiKey());
  const hasOpenAi = Boolean(resolveOpenAiKey());
  const hasCloudKey = hasGemini || hasOpenAi;

  const serverOrCli = findManagedWhisperBinary([
    'whisper-server',
    'whisper-cli',
    'whisper-cpp',
    'whisper.cpp',
    'whisper',
    'faster-whisper',
    'sherpa-onnx-offline',
    'vosk-transcriber',
  ]);
  const managedRuntimePath = findManagedOnlyRuntimePath();
  const models = listSpeechModels(settings?.speechModel);
  const activeEntry = models.find((m) => m.active) || null;
  const activeModelPath = activeEntry?.resolvedPath || resolveActiveModelPath(settings?.speechModel);
  const installedIds = models.filter((m) => m.installed).map((m) => m.id);
  const hasCustomLocalBackend = Boolean(
    settings?.localSpeechUrl?.trim() || settings?.localSpeechCommand?.trim(),
  );

  return {
    hasCloudKey,
    cloudProviders: {
      gemini: hasGemini,
      openai: hasOpenAi,
    },
    hasLocalEngine: Boolean(serverOrCli || hasCustomLocalBackend),
    hasCustomLocalBackend,
    hasLocalModel: Boolean(activeModelPath || hasCustomLocalBackend),
    managedRuntimeInstalled: Boolean(managedRuntimePath),
    managedRuntimePath,
    installedModels: installedIds,
    activeModelId: activeEntry?.id || null,
    activeModelPath,
    engineBinary: serverOrCli,
    catalog: WHISPER_MODELS,
    models,
  };
}

export async function downloadModelFile(
  url: string,
  destPath: string,
  onProgress?: (info: { downloadedBytes: number; totalBytes: number; percent: number }) => void,
  signal?: AbortSignal,
): Promise<void> {
  fs.mkdirSync(path.dirname(destPath), { recursive: true });
  const partPath = `${destPath}.part`;
  if (fs.existsSync(partPath)) {
    fs.rmSync(partPath, { force: true });
  }

  const res = await fetch(url, { signal, redirect: 'follow' });
  if (!res.ok || !res.body) {
    throw new Error(`HTTP ${res.status} downloading ${url}`);
  }

  const totalHeader = Number(res.headers.get('content-length') || 0);
  const totalBytes = Number.isFinite(totalHeader) ? totalHeader : 0;

  const outStream = fs.createWriteStream(partPath);
  const reader = res.body.getReader();
  let downloadedBytes = 0;
  let lastEmitAt = 0;

  try {
    while (true) {
      if (signal?.aborted) {
        throw new Error('Download cancelled');
      }
      const { done, value } = await reader.read();
      if (done) break;
      if (value && value.byteLength > 0) {
        await new Promise<void>((resolve, reject) => {
          const ok = outStream.write(Buffer.from(value), (err) => {
            if (err) reject(err);
          });
          if (ok) resolve();
          else outStream.once('drain', resolve);
        });
        downloadedBytes += value.byteLength;
        const now = Date.now();
        if (now - lastEmitAt >= 120) {
          lastEmitAt = now;
          const pct =
            totalBytes > 0 ? Math.min(99, Math.round((downloadedBytes / totalBytes) * 100)) : 0;
          onProgress?.({ downloadedBytes, totalBytes, percent: pct });
        }
      }
    }
    await new Promise<void>((resolve, reject) => {
      outStream.on('error', reject);
      outStream.end(() => resolve());
    });
    fs.renameSync(partPath, destPath);
    onProgress?.({ downloadedBytes, totalBytes: totalBytes || downloadedBytes, percent: 100 });
  } catch (err) {
    try {
      outStream.destroy();
    } catch {}
    try {
      if (fs.existsSync(partPath)) fs.rmSync(partPath, { force: true });
    } catch {}
    throw err;
  }
}

let activeInstallAbort: AbortController | null = null;

export function cancelLocalWhisperInstall(): void {
  if (activeInstallAbort) {
    activeInstallAbort.abort();
    activeInstallAbort = null;
  }
}

function runStreamingCommand(
  cmd: string,
  args: string[],
  cwd: string | undefined,
  signal: AbortSignal,
  onLine: (line: string) => void,
): Promise<boolean> {
  return new Promise((resolve) => {
    const proc = spawn(cmd, args, {
      cwd,
      env: {
        ...process.env,
        PATH: `${path.join(os.homedir(), '.local', 'bin')}:/home/linuxbrew/.linuxbrew/bin:${process.env.PATH || ''}`,
      },
      stdio: ['ignore', 'pipe', 'pipe'],
    });
    const onAbort = () => {
      try {
        proc.kill('SIGTERM');
      } catch {}
    };
    signal.addEventListener('abort', onAbort, { once: true });

    const forward = (chunk: Buffer) => {
      const lines = chunk.toString('utf8').split(/\r?\n/).filter(Boolean);
      for (const l of lines) onLine(l.trim());
    };
    proc.stdout?.on('data', forward);
    proc.stderr?.on('data', forward);
    proc.on('error', () => {
      signal.removeEventListener('abort', onAbort);
      resolve(false);
    });
    proc.on('close', (code) => {
      signal.removeEventListener('abort', onAbort);
      resolve(code === 0);
    });
  });
}

function createPersistentStagingDir(prefix: string): string {
  const base = speechStagingDir();
  fs.mkdirSync(base, { recursive: true });
  return fs.mkdtempSync(path.join(base, prefix));
}

async function ensureWhisperEngineBinary(
  onProgress: (p: SpeechInstallProgress) => void,
  modelId: string,
  signal: AbortSignal,
): Promise<string | null> {
  const existing = findManagedWhisperBinary(['whisper-server', 'whisper-cli']);
  if (existing) return existing;

  const asset = resolveWhisperBinaryAsset();
  if (asset) {
    const stagingDir = createPersistentStagingDir('runtime-');
    try {
      const archivePath = path.join(stagingDir, asset.fileName);
      const extractDir = path.join(stagingDir, 'out');
      onProgress({
        modelId,
        stage: 'installing_engine',
        percent: 10,
        detail: `Downloading optional runtime (${asset.binaryName})...`,
      });
      await downloadModelFile(
        asset.url,
        archivePath,
        ({ percent }) => {
          onProgress({
            modelId,
            stage: 'installing_engine',
            percent: Math.min(75, Math.max(10, Math.round(percent * 0.75))),
            detail: `Downloading optional runtime (${percent}%)...`,
          });
        },
        signal,
      );
      onProgress({
        modelId,
        stage: 'installing_engine',
        percent: 85,
        detail: 'Extracting optional offline speech runtime...',
      });
      await extractArchive(archivePath, extractDir);
      const extractedBin = findFileRecursive(extractDir, asset.binaryName);
      if (extractedBin) {
        fs.mkdirSync(speechBinDir(), { recursive: true });
        const destBin = path.join(speechBinDir(), asset.binaryName);
        fs.copyFileSync(extractedBin, destBin);
        if (process.platform !== 'win32') {
          fs.chmodSync(destBin, 0o755);
        }
        const verified = findManagedWhisperBinary(['whisper-server', 'whisper-cli']);
        if (verified) return verified;
      }
    } catch (err) {
      if (signal.aborted) throw err;
    } finally {
      try {
        fs.rmSync(stagingDir, { recursive: true, force: true });
      } catch {}
    }
  }

  const brew = findExecutableInPath(['brew']);
  if (brew) {
    onProgress({
      modelId,
      stage: 'installing_engine',
      percent: 10,
      detail: 'Installing whisper-cpp via Homebrew...',
    });
    const ok = await runStreamingCommand(brew, ['install', 'whisper-cpp'], undefined, signal, (line) => {
      onProgress({
        modelId,
        stage: 'installing_engine',
        percent: 45,
        detail: line.slice(0, 100),
      });
    });
    if (ok) {
      const bin = findManagedWhisperBinary(['whisper-server', 'whisper-cli']);
      if (bin) return bin;
    }
  }

  const cmake = findExecutableInPath(['cmake']);
  const tarBin = findExecutableInPath(['tar']);
  const makeBin = findExecutableInPath(['make']);
  const cxx = findExecutableInPath(['g++', 'clang++', 'c++']);
  if (cmake && tarBin && makeBin && cxx) {
    const stagingSrc = createPersistentStagingDir('src-build-');
    try {
      const tgz = path.join(stagingSrc, 'whisper.tar.gz');
      onProgress({
        modelId,
        stage: 'installing_engine',
        percent: 15,
        detail: 'Downloading whisper.cpp source v1.9.6...',
      });
      await downloadModelFile(
        'https://github.com/ggml-org/whisper.cpp/archive/refs/tags/v1.9.6.tar.gz',
        tgz,
        undefined,
        signal,
      );
      await runStreamingCommand(tarBin, ['-xzf', tgz, '-C', stagingSrc], undefined, signal, () => {});
      const srcDir = fs.readdirSync(stagingSrc).find((d) => d.startsWith('whisper.cpp-'));
      if (srcDir) {
        const fullSrc = path.join(stagingSrc, srcDir);
        const buildDir = path.join(fullSrc, 'build');
        onProgress({
          modelId,
          stage: 'installing_engine',
          percent: 35,
          detail: 'Configuring whisper.cpp CMake build...',
        });
        const cfgOk = await runStreamingCommand(
          cmake,
          [
            '-B',
            buildDir,
            '-DWHISPER_BUILD_EXAMPLES=ON',
            '-DWHISPER_BUILD_SERVER=ON',
            '-DBUILD_SHARED_LIBS=OFF',
            '-DCMAKE_BUILD_TYPE=Release',
          ],
          fullSrc,
          signal,
          (line) => {
            onProgress({ modelId, stage: 'installing_engine', percent: 45, detail: line.slice(0, 90) });
          },
        );
        if (cfgOk) {
          const jobs = String(Math.min(8, Math.max(2, os.cpus().length || 4)));
          onProgress({
            modelId,
            stage: 'installing_engine',
            percent: 60,
            detail: `Compiling whisper-server & whisper-cli (${jobs} threads)...`,
          });
          const buildOk = await runStreamingCommand(
            cmake,
            ['--build', buildDir, '--target', 'whisper-server', 'whisper-cli', '-j', jobs],
            fullSrc,
            signal,
            (line) => {
              onProgress({ modelId, stage: 'installing_engine', percent: 80, detail: line.slice(0, 90) });
            },
          );
          if (buildOk) {
            fs.mkdirSync(speechBinDir(), { recursive: true });
            for (const name of ['whisper-server', 'whisper-cli']) {
              const built = path.join(buildDir, 'bin', name);
              if (fs.existsSync(built)) {
                fs.copyFileSync(built, path.join(speechBinDir(), name));
                fs.chmodSync(path.join(speechBinDir(), name), 0o755);
              }
            }
            const bin = findManagedWhisperBinary(['whisper-server', 'whisper-cli']);
            if (bin) return bin;
          }
        }
      }
    } finally {
      try {
        fs.rmSync(stagingSrc, { recursive: true, force: true });
      } catch {}
    }
  }

  return null;
}

export async function installLocalWhisper(
  modelId: string,
  onProgress: (p: SpeechInstallProgress) => void,
): Promise<{ ok: boolean; modelPath?: string; engineBinary?: string | null; error?: string }> {
  const entry = listSpeechModels().find((m) => m.id === modelId) ?? listSpeechModels()[0];
  if (!entry) {
    return { ok: false, error: `Unknown speech model: ${modelId}` };
  }
  if (activeInstallAbort) {
    activeInstallAbort.abort();
  }
  const abort = new AbortController();
  activeInstallAbort = abort;

  try {
    const engineBinary = await ensureWhisperEngineBinary(onProgress, entry.id, abort.signal);
    if (!engineBinary) {
      const msg =
        'Unable to set up optional whisper.cpp runtime (check internet access or configure an external local/cloud speech provider).';
      onProgress({ modelId: entry.id, stage: 'error', percent: 0, detail: msg });
      return { ok: false, error: msg };
    }

    if (entry.source === 'custom_local') {
      const target = entry.resolvedPath || entry.localPath || '';
      if (!target || !fs.existsSync(target)) {
        const msg = `Local model file missing: ${target}`;
        onProgress({ modelId: entry.id, stage: 'error', percent: 0, detail: msg });
        return { ok: false, error: msg };
      }
      onProgress({ modelId: entry.id, stage: 'done', percent: 100, detail: `${entry.label} ready` });
      return { ok: true, modelPath: target, engineBinary };
    }

    const downloadUrl = entry.url;
    if (!downloadUrl) {
      return { ok: false, error: 'Model entry has no download URL.' };
    }

    const destPath = path.join(speechModelsDir(), path.basename(entry.fileName));
    if (!fs.existsSync(destPath)) {
      const expectedBytes = entry.sizeBytes || 0;
      onProgress({
        modelId: entry.id,
        stage: 'downloading_model',
        percent: 0,
        downloadedBytes: 0,
        totalBytes: expectedBytes,
        detail: `Downloading ${entry.label}...`,
      });
      await downloadModelFile(
        downloadUrl,
        destPath,
        ({ downloadedBytes, totalBytes, percent }) => {
          const dlMb = (downloadedBytes / (1024 * 1024)).toFixed(1);
          const denom = totalBytes || expectedBytes;
          const totLabel = denom > 0 ? ` / ${(denom / (1024 * 1024)).toFixed(0)} MB` : ' MB';
          onProgress({
            modelId: entry.id,
            stage: 'downloading_model',
            percent,
            downloadedBytes,
            totalBytes: denom,
            detail: `Downloading model: ${dlMb}${totLabel} (${percent}%)`,
          });
        },
        abort.signal,
      );
    }

    onProgress({
      modelId: entry.id,
      stage: 'done',
      percent: 100,
      detail: `${entry.label} ready`,
    });
    return { ok: true, modelPath: destPath, engineBinary };
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err);
    onProgress({ modelId: entry.id, stage: 'error', percent: 0, detail: msg });
    return { ok: false, error: msg };
  } finally {
    if (activeInstallAbort === abort) {
      activeInstallAbort = null;
    }
  }
}

export function deleteLocalWhisperModel(modelId: string, removeEntry = false): boolean {
  const customList = loadCustomSpeechModels();
  const customIdx = customList.findIndex((m) => m.id === modelId);
  if (customIdx !== -1) {
    const custom = customList[customIdx];
    if (custom.source === 'custom_local') {
      // Never delete user's external local model file; remove metadata entry only
      if (managedServerModelPath === custom.localPath) {
        shutdownManagedWhisperServer();
      }
      customList.splice(customIdx, 1);
      saveCustomSpeechModels(customList);
      return true;
    }
    const target = path.join(speechModelsDir(), path.basename(custom.fileName));
    if (managedServerModelPath === target) {
      shutdownManagedWhisperServer();
    }
    let deleted = false;
    if (fs.existsSync(target)) {
      fs.rmSync(target, { force: true });
      deleted = true;
    }
    if (removeEntry) {
      customList.splice(customIdx, 1);
      saveCustomSpeechModels(customList);
      deleted = true;
    }
    return deleted;
  }

  if (modelId.startsWith('local:')) {
    const fileName = path.basename(modelId.slice('local:'.length));
    const target = path.join(speechModelsDir(), fileName);
    if (managedServerModelPath === target) {
      shutdownManagedWhisperServer();
    }
    if (fs.existsSync(target)) {
      fs.rmSync(target, { force: true });
      return true;
    }
    return false;
  }

  const spec = WHISPER_MODELS.find((m) => m.id === modelId);
  const fileName = spec ? spec.fileName : modelId;
  const target = path.join(speechModelsDir(), path.basename(fileName));
  if (managedServerModelPath === target) {
    shutdownManagedWhisperServer();
  }
  if (fs.existsSync(target)) {
    fs.rmSync(target, { force: true });
    return true;
  }
  return false;
}

export function uninstallLocalWhisperRuntime(): boolean {
  shutdownManagedWhisperServer();
  const binDir = speechBinDir();
  if (!fs.existsSync(binDir)) return false;
  let removed = false;
  for (const name of ['whisper-server', 'whisper-server.exe', 'whisper-cli', 'whisper-cli.exe']) {
    const target = path.join(binDir, name);
    if (fs.existsSync(target)) {
      fs.rmSync(target, { force: true });
      removed = true;
    }
  }
  return removed;
}

let managedServerProc: ChildProcess | null = null;
let managedServerPort = 0;
let managedServerModelPath = '';
let managedServerIdleTimer: NodeJS.Timeout | null = null;
let managedServerStarting: Promise<string | null> | null = null;

function resetManagedServerIdleTimer(): void {
  if (managedServerIdleTimer) clearTimeout(managedServerIdleTimer);
  managedServerIdleTimer = setTimeout(() => {
    shutdownManagedWhisperServer();
  }, 5 * 60 * 1000);
}

export function shutdownManagedWhisperServer(): void {
  if (managedServerIdleTimer) {
    clearTimeout(managedServerIdleTimer);
    managedServerIdleTimer = null;
  }
  const proc = managedServerProc;
  managedServerProc = null;
  managedServerPort = 0;
  managedServerModelPath = '';
  if (proc) {
    try {
      proc.kill('SIGTERM');
    } catch {
      // ignore
    }
  }
}

function getFreePort(): Promise<number> {
  return new Promise((resolve, reject) => {
    const srv = net.createServer();
    srv.unref();
    srv.on('error', reject);
    srv.listen(0, '127.0.0.1', () => {
      const addr = srv.address();
      const port = typeof addr === 'object' && addr ? addr.port : 0;
      srv.close(() => resolve(port));
    });
  });
}

export async function ensureManagedWhisperServer(modelPath: string): Promise<string | null> {
  if (!modelPath || !fs.existsSync(modelPath)) return null;
  const serverBin = findManagedWhisperBinary(['whisper-server']);
  if (!serverBin) return null;

  if (managedServerProc && managedServerModelPath === modelPath && managedServerPort > 0) {
    resetManagedServerIdleTimer();
    return `http://127.0.0.1:${managedServerPort}/inference`;
  }

  if (managedServerStarting) {
    return managedServerStarting;
  }

  managedServerStarting = (async () => {
    shutdownManagedWhisperServer();
    try {
      const port = await getFreePort();
      if (!port) return null;
      const cpus = Math.min(8, Math.max(2, os.cpus().length || 4));
      const child = spawn(
        serverBin,
        ['-m', modelPath, '--host', '127.0.0.1', '--port', String(port), '-t', String(cpus), '-l', 'auto'],
        {
          stdio: 'ignore',
          detached: false,
          windowsHide: true,
        },
      );
      managedServerProc = child;
      managedServerPort = port;
      managedServerModelPath = modelPath;
      child.once('exit', () => {
        if (managedServerProc === child) {
          managedServerProc = null;
          managedServerPort = 0;
          managedServerModelPath = '';
        }
      });

      const baseUrl = `http://127.0.0.1:${port}`;
      for (let i = 0; i < 60; i++) {
        if (!managedServerProc) return null;
        try {
          const probe = await fetch(baseUrl, { signal: AbortSignal.timeout(400) });
          if (probe.status < 500) {
            resetManagedServerIdleTimer();
            return `${baseUrl}/inference`;
          }
        } catch {
          // still loading model into RAM
        }
        await new Promise((r) => setTimeout(r, 250));
      }
      shutdownManagedWhisperServer();
      return null;
    } catch {
      shutdownManagedWhisperServer();
      return null;
    } finally {
      managedServerStarting = null;
    }
  })();

  return managedServerStarting;
}
