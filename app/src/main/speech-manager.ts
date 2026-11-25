import { spawn, type ChildProcess } from 'node:child_process';
import fs from 'node:fs';
import net from 'node:net';
import os from 'node:os';
import path from 'node:path';
import type {
  AppSettings,
  SpeechInstallProgress,
  SpeechStatus,
  WhisperModelId,
  WhisperModelSpec,
} from '../shared/types';
import { findExecutableInPath, resolveOpenAiKey, resolveSpeechApiKey } from './speech';

export const WHISPER_MODELS: WhisperModelSpec[] = [
  {
    id: 'large-v3-turbo-q5_0',
    fileName: 'ggml-large-v3-turbo-q5_0.bin',
    sizeBytes: 574_041_195,
    url: 'https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo-q5_0.bin',
    label: 'Whisper Large-v3-Turbo (q5_0 · ~574 MB)',
    description: 'Recommended — Best multilingual & mixed-language (code-switching) accuracy.',
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

export function speechDataDir(): string {
  const dataHome = process.env.XDG_DATA_HOME || path.join(os.homedir(), '.local', 'share');
  return path.join(dataHome, 'chat-overlay', 'speech');
}

export function speechModelsDir(): string {
  return path.join(speechDataDir(), 'models');
}

export function speechBinDir(): string {
  return path.join(speechDataDir(), 'bin');
}

export function findManagedWhisperBinary(
  names: string[] = ['whisper-server', 'whisper-cli', 'whisper-cpp', 'whisper.cpp'],
): string | null {
  const binDir = speechBinDir();
  for (const name of names) {
    const ext = process.platform === 'win32' && !name.endsWith('.exe') ? '.exe' : '';
    const candidate = path.join(binDir, name + ext);
    if (fs.existsSync(candidate)) {
      try {
        fs.accessSync(candidate, fs.constants.X_OK);
        return candidate;
      } catch {
        // ignore
      }
    }
  }
  return findExecutableInPath(names);
}

export function listInstalledWhisperModelFiles(): string[] {
  const dirs = Array.from(
    new Set([
      speechModelsDir(),
      path.join(os.homedir(), '.cache', 'whisper'),
      path.join(os.homedir(), '.local', 'share', 'whisper'),
      path.join(os.homedir(), '.local', 'share', 'whisper.cpp'),
      path.join(os.homedir(), 'whisper.cpp', 'models'),
    ]),
  );
  const found: string[] = [];
  for (const d of dirs) {
    if (!fs.existsSync(d)) continue;
    try {
      for (const f of fs.readdirSync(d)) {
        if (f.endsWith('.bin') || f.includes('ggml')) {
          found.push(path.join(d, f));
        }
      }
    } catch {
      // ignore
    }
  }
  return found;
}

export function resolveActiveModelPath(preferredModelId?: string): string | null {
  const installed = listInstalledWhisperModelFiles();
  if (installed.length === 0) return null;

  if (preferredModelId) {
    const spec = WHISPER_MODELS.find((m) => m.id === preferredModelId);
    if (spec) {
      const exact = installed.find((p) => path.basename(p) === spec.fileName);
      if (exact) return exact;
    }
    const partial = installed.find((p) => path.basename(p).includes(preferredModelId));
    if (partial) return partial;
  }

  for (const spec of WHISPER_MODELS) {
    const exact = installed.find((p) => path.basename(p) === spec.fileName);
    if (exact) return exact;
  }
  return installed[0];
}

export function getSpeechStatus(settings?: Partial<AppSettings>): SpeechStatus {
  const hasCloudKey = Boolean(resolveSpeechApiKey() || resolveOpenAiKey());
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
  const installedPaths = listInstalledWhisperModelFiles();
  const activeModelPath = resolveActiveModelPath(settings?.speechModel);

  const installedIds: string[] = [];
  for (const spec of WHISPER_MODELS) {
    if (installedPaths.some((p) => path.basename(p) === spec.fileName)) {
      installedIds.push(spec.id);
    }
  }
  if (installedIds.length === 0 && installedPaths.length > 0) {
    installedIds.push(path.basename(installedPaths[0]));
  }

  const hasCustomRemote = Boolean(settings?.localSpeechUrl?.trim() || settings?.localSpeechCommand?.trim());

  return {
    hasCloudKey,
    hasLocalEngine: Boolean(serverOrCli || hasCustomRemote),
    hasLocalModel: Boolean(activeModelPath || hasCustomRemote),
    installedModels: installedIds,
    activeModelPath,
    engineBinary: serverOrCli,
    catalog: WHISPER_MODELS,
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
          const pct = totalBytes > 0 ? Math.min(99, Math.round((downloadedBytes / totalBytes) * 100)) : 0;
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

async function ensureWhisperEngineBinary(
  onProgress: (p: SpeechInstallProgress) => void,
  modelId: string,
  signal: AbortSignal,
): Promise<string | null> {
  const existing = findManagedWhisperBinary(['whisper-server', 'whisper-cli']);
  if (existing) return existing;

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
    const tmpSrc = fs.mkdtempSync(path.join(os.tmpdir(), 'whisper-cpp-build-'));
    try {
      const tgz = path.join(tmpSrc, 'whisper.tar.gz');
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
      await runStreamingCommand(tarBin, ['-xzf', tgz, '-C', tmpSrc], undefined, signal, () => {});
      const srcDir = fs.readdirSync(tmpSrc).find((d) => d.startsWith('whisper.cpp-'));
      if (srcDir) {
        const fullSrc = path.join(tmpSrc, srcDir);
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
        fs.rmSync(tmpSrc, { recursive: true, force: true });
      } catch {}
    }
  }

  return null;
}

export async function installLocalWhisper(
  modelId: WhisperModelId,
  onProgress: (p: SpeechInstallProgress) => void,
): Promise<{ ok: boolean; modelPath?: string; engineBinary?: string | null; error?: string }> {
  const spec = WHISPER_MODELS.find((m) => m.id === modelId) ?? WHISPER_MODELS[0];
  if (activeInstallAbort) {
    activeInstallAbort.abort();
  }
  const abort = new AbortController();
  activeInstallAbort = abort;

  try {
    const engineBinary = await ensureWhisperEngineBinary(onProgress, spec.id, abort.signal);
    if (!engineBinary) {
      const msg =
        'Unable to install whisper.cpp automatically (install Homebrew or cmake + g++, or install whisper-cpp manually).';
      onProgress({ modelId: spec.id, stage: 'error', percent: 0, detail: msg });
      return { ok: false, error: msg };
    }

    const destPath = path.join(speechModelsDir(), spec.fileName);
    if (!fs.existsSync(destPath)) {
      onProgress({
        modelId: spec.id,
        stage: 'downloading_model',
        percent: 0,
        downloadedBytes: 0,
        totalBytes: spec.sizeBytes,
        detail: `Downloading ${spec.label}...`,
      });
      await downloadModelFile(
        spec.url,
        destPath,
        ({ downloadedBytes, totalBytes, percent }) => {
          const dlMb = (downloadedBytes / (1024 * 1024)).toFixed(1);
          const totMb = ((totalBytes || spec.sizeBytes) / (1024 * 1024)).toFixed(0);
          onProgress({
            modelId: spec.id,
            stage: 'downloading_model',
            percent,
            downloadedBytes,
            totalBytes: totalBytes || spec.sizeBytes,
            detail: `Downloading model: ${dlMb} / ${totMb} MB (${percent}%)`,
          });
        },
        abort.signal,
      );
    }

    onProgress({
      modelId: spec.id,
      stage: 'done',
      percent: 100,
      detail: `${spec.label} ready`,
    });
    return { ok: true, modelPath: destPath, engineBinary };
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err);
    onProgress({ modelId: spec.id, stage: 'error', percent: 0, detail: msg });
    return { ok: false, error: msg };
  } finally {
    if (activeInstallAbort === abort) {
      activeInstallAbort = null;
    }
  }
}

export function deleteLocalWhisperModel(modelId: string): boolean {
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
          // still warming model into RAM
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
