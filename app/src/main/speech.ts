import { spawn } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import type { AppSettings, SpeechMode } from '../shared/types';
import { bundledBinDir } from './app-paths';
import { configDir } from './settings';
import {
  ensureManagedWhisperServer,
  resolveActiveModelPath,
  speechBinDir,
  speechModelsDir,
} from './speech-manager';

export { SpeechMode };

export interface TranscribeOptions {
  data: string;
  mimeType: string;
  language?: string;
  model?: string;
  mode?: SpeechMode;
  localUrl?: string;
  localCommand?: string;
}

export interface TranscribeResult {
  ok: boolean;
  text?: string;
  error?: string;
  engine?: 'cloud' | 'local' | 'browser';
}

export type SpeechProgressCallback = (token: string, accumulatedText: string) => void;

export function resolveSpeechApiKey(): string | null {
  if (process.env.GEMINI_API_KEY?.trim()) return process.env.GEMINI_API_KEY.trim();
  if (process.env.GOOGLE_API_KEY?.trim()) return process.env.GOOGLE_API_KEY.trim();

  // Primary: ~/.config/chat-overlay/voice.json
  const voiceJson = path.join(configDir(), 'voice.json');
  if (fs.existsSync(voiceJson)) {
    try {
      const parsed = JSON.parse(fs.readFileSync(voiceJson, 'utf8'));
      if (parsed.apiKey && typeof parsed.apiKey === 'string' && parsed.apiKey.trim()) {
        return parsed.apiKey.trim();
      }
      if (parsed.geminiApiKey && typeof parsed.geminiApiKey === 'string' && parsed.geminiApiKey.trim()) {
        return parsed.geminiApiKey.trim();
      }
    } catch {}
  }

  const authJson = path.join(configDir(), 'auth.json');
  if (fs.existsSync(authJson)) {
    try {
      const parsed = JSON.parse(fs.readFileSync(authJson, 'utf8'));
      if (parsed.google?.key && typeof parsed.google.key === 'string' && parsed.google.key.trim()) {
        return parsed.google.key.trim();
      }
      if (parsed.gemini?.key && typeof parsed.gemini.key === 'string' && parsed.gemini.key.trim()) {
        return parsed.gemini.key.trim();
      }
    } catch {}
  }

  return null;
}

export function resolveOpenAiKey(): string | null {
  if (process.env.OPENAI_API_KEY?.trim()) return process.env.OPENAI_API_KEY.trim();

  // Primary: ~/.config/chat-overlay/voice.json
  const voiceJson = path.join(configDir(), 'voice.json');
  if (fs.existsSync(voiceJson)) {
    try {
      const parsed = JSON.parse(fs.readFileSync(voiceJson, 'utf8'));
      if (parsed.openaiApiKey && typeof parsed.openaiApiKey === 'string' && parsed.openaiApiKey.trim()) {
        return parsed.openaiApiKey.trim();
      }
      if (parsed.openaiKey && typeof parsed.openaiKey === 'string' && parsed.openaiKey.trim()) {
        return parsed.openaiKey.trim();
      }
    } catch {}
  }

  const authJson = path.join(configDir(), 'auth.json');
  if (fs.existsSync(authJson)) {
    try {
      const parsed = JSON.parse(fs.readFileSync(authJson, 'utf8'));
      if (
        parsed.openai?.key &&
        typeof parsed.openai.key === 'string' &&
        parsed.openai.key.trim() &&
        parsed.openai.type !== 'oauth'
      ) {
        return parsed.openai.key.trim();
      }
    } catch {}
  }

  return null;
}

export function cleanAudioMimeType(rawMime: string): string {
  if (!rawMime) return 'audio/webm';
  const base = rawMime.split(';')[0].trim().toLowerCase();
  if (base === 'audio/mpeg') return 'audio/mp3';
  if (base === 'audio/x-wav') return 'audio/wav';
  if (['audio/webm', 'audio/wav', 'audio/ogg', 'audio/mp3', 'audio/aac', 'audio/flac', 'audio/mp4'].includes(base)) {
    return base;
  }
  return 'audio/webm';
}

export function sanitizeTranscription(text: string): string {
  if (!text) return '';
  let trimmed = text.trim();

  // Strip enclosing markdown code block fences e.g. ```markdown ... ``` or ``` ... ```
  const codeBlockMatch = trimmed.match(/^```(?:[a-zA-Z0-9_-]+)?\s*([\s\S]*?)\s*```$/);
  if (codeBlockMatch) {
    trimmed = codeBlockMatch[1].trim();
  }

  // Strip leading and trailing quotes or backticks
  trimmed = trimmed.replace(/^["'`]+|["'`]+$/g, '').trim();

  // Filter out silence or non-speech markers
  const normalizedMarker = trimmed.toLowerCase().replace(/^[[(<{\s]+|[\])>}.\s]+$/g, '');
  if (
    normalizedMarker === 'no_speech' ||
    normalizedMarker === 'no speech' ||
    normalizedMarker === 'silence' ||
    normalizedMarker === 'silent' ||
    normalizedMarker === 'blank' ||
    normalizedMarker === 'none' ||
    normalizedMarker === 'music' ||
    normalizedMarker === 'applause' ||
    normalizedMarker === 'laughter' ||
    normalizedMarker === 'cough'
  ) {
    return '';
  }

  return trimmed;
}

export function findExecutableInPath(names: string[]): string | null {
  const pathEnv = process.env.PATH || '';
  const delimiter = path.delimiter;
  const standardDirs = [
    speechBinDir(),
    '/usr/local/bin',
    '/usr/bin',
    '/bin',
    path.join(os.homedir(), '.local', 'bin'),
    path.join(os.homedir(), '.cargo', 'bin'),
    path.join(os.homedir(), '.opencode', 'bin'),
    path.join(os.homedir(), '.bun', 'bin'),
    path.join(os.homedir(), 'miniconda3', 'bin'),
    path.join(os.homedir(), 'anaconda3', 'bin'),
    '/home/linuxbrew/.linuxbrew/bin',
  ];
  const bundled = bundledBinDir();
  const searchDirs = Array.from(
    new Set([...(bundled ? [bundled] : []), speechBinDir(), ...pathEnv.split(delimiter).filter(Boolean), ...standardDirs]),
  );

  for (const name of names) {
    const nameCandidates =
      process.platform === 'win32' && !name.endsWith('.exe') ? [name, `${name}.exe`] : [name];

    for (const candidate of nameCandidates) {
      if (path.isAbsolute(candidate)) {
        if (fs.existsSync(candidate)) {
          if (process.platform === 'win32') return candidate;
          try {
            fs.accessSync(candidate, fs.constants.X_OK);
            return candidate;
          } catch {}
        }
        continue;
      }
      for (const dir of searchDirs) {
        const fullPath = path.join(dir, candidate);
        if (fs.existsSync(fullPath)) {
          if (process.platform === 'win32') return fullPath;
          try {
            fs.accessSync(fullPath, fs.constants.X_OK);
            return fullPath;
          } catch {}
        }
      }
    }
  }
  return null;
}

export function isPcm16Mono16kWav(buf: Buffer): boolean {
  if (!buf || buf.length < 44) return false;
  if (buf.toString('ascii', 0, 4) !== 'RIFF' || buf.toString('ascii', 8, 12) !== 'WAVE') {
    return false;
  }
  let offset = 12;
  let validFmt = false;
  let hasData = false;
  while (offset + 8 <= buf.length) {
    const chunkId = buf.toString('ascii', offset, offset + 4);
    const chunkSize = buf.readUInt32LE(offset + 4);
    const bodyStart = offset + 8;
    if (chunkId === 'fmt ' && bodyStart + 16 <= buf.length) {
      const audioFormat = buf.readUInt16LE(bodyStart);
      const channels = buf.readUInt16LE(bodyStart + 2);
      const sampleRate = buf.readUInt32LE(bodyStart + 4);
      const bitsPerSample = buf.readUInt16LE(bodyStart + 14);
      validFmt = audioFormat === 1 && channels === 1 && sampleRate === 16000 && bitsPerSample === 16;
    } else if (chunkId === 'data' && chunkSize > 0) {
      hasData = true;
    }
    if (chunkSize === 0 && chunkId !== 'data') break;
    offset = bodyStart + chunkSize + (chunkSize % 2);
  }
  return validFmt && hasData;
}

export function findWhisperCppModel(preferredModelId?: string): string | null {
  if (process.env.WHISPER_CPP_MODEL && fs.existsSync(process.env.WHISPER_CPP_MODEL)) {
    return process.env.WHISPER_CPP_MODEL;
  }

  const managed = resolveActiveModelPath(preferredModelId);
  if (managed) return managed;

  const candidateDirs = [
    speechModelsDir(),
    path.join(os.homedir(), '.cache', 'whisper'),
    path.join(os.homedir(), '.local', 'share', 'whisper'),
    path.join(os.homedir(), '.local', 'share', 'whisper.cpp'),
    path.join(os.homedir(), 'whisper.cpp', 'models'),
    path.join(os.homedir(), '.whisper'),
    '/usr/share/whisper',
    '/usr/local/share/whisper',
    '/usr/share/whisper-cpp',
    '/usr/local/share/whisper-cpp',
  ];

  for (const dir of candidateDirs) {
    if (!fs.existsSync(dir)) continue;
    try {
      const files = fs.readdirSync(dir);
      const match = files.find((f) => f.endsWith('.bin') || f.includes('ggml'));
      if (match) {
        return path.join(dir, match);
      }
    } catch {}
  }
  return null;
}

export function runProcessWithTimeout(
  cmd: string,
  args: string[],
  timeoutMs: number,
  cwd?: string,
  onStdoutChunk?: (chunk: string) => void,
  extraEnv?: Record<string, string>,
): Promise<{ ok: boolean; stdout: string; stderr: string; exitCode: number | null }> {
  return new Promise((resolve) => {
    let stdout = '';
    let stderr = '';
    let finished = false;

    let proc: ReturnType<typeof spawn>;
    try {
      proc = spawn(cmd, args, {
        cwd,
        env: { ...process.env, ...(extraEnv || {}) },
        stdio: ['ignore', 'pipe', 'pipe'],
      });
    } catch (err: any) {
      resolve({ ok: false, stdout: '', stderr: err?.message || String(err), exitCode: -1 });
      return;
    }

    const timer = setTimeout(() => {
      if (!finished) {
        finished = true;
        try {
          proc.kill('SIGKILL');
        } catch {}
        resolve({ ok: false, stdout, stderr: stderr + '\nProcess timed out', exitCode: null });
      }
    }, timeoutMs);

    proc.stdout?.on('data', (d) => {
      const chunk = d.toString();
      stdout += chunk;
      onStdoutChunk?.(chunk);
    });

    proc.stderr?.on('data', (d) => {
      stderr += d.toString();
    });

    proc.on('error', (err) => {
      if (!finished) {
        finished = true;
        clearTimeout(timer);
        resolve({ ok: false, stdout, stderr: err.message, exitCode: null });
      }
    });

    proc.on('close', (code) => {
      if (!finished) {
        finished = true;
        clearTimeout(timer);
        resolve({ ok: code === 0, stdout, stderr, exitCode: code });
      }
    });
  });
}

export async function convertAudioToWav(inputPath: string, outputPath: string): Promise<boolean> {
  const ffmpeg = findExecutableInPath(['ffmpeg']);
  if (!ffmpeg) return false;
  const res = await runProcessWithTimeout(
    ffmpeg,
    ['-y', '-threads', '0', '-i', inputPath, '-ar', '16000', '-ac', '1', '-c:a', 'pcm_s16le', outputPath],
    10_000,
  );
  return res.ok && fs.existsSync(outputPath);
}

export async function transcribeViaHttpServer(
  audioBuffer: Buffer,
  mimeType: string,
  options: TranscribeOptions,
): Promise<TranscribeResult> {
  const customUrl =
    options.localUrl?.trim() ||
    process.env.LOCAL_STT_URL?.trim() ||
    process.env.WHISPER_SERVER_URL?.trim();

  const candidateUrls = customUrl
    ? [customUrl]
    : [
        'http://127.0.0.1:8080/inference',
        'http://127.0.0.1:8000/v1/audio/transcriptions',
        'http://127.0.0.1:10300/v1/audio/transcriptions',
        'http://127.0.0.1:5000/transcribe',
      ];

  const cleanMime = cleanAudioMimeType(mimeType);
  const ext = cleanMime.includes('wav') ? 'wav' : cleanMime.includes('ogg') ? 'ogg' : 'webm';

  for (const url of candidateUrls) {
    try {
      const formData = new FormData();
      formData.append(
        'file',
        new Blob([new Uint8Array(audioBuffer)], { type: cleanMime }),
        `audio.${ext}`,
      );

      if (url.includes('/inference')) {
        formData.append('response_format', 'json');
        formData.append('temperature', '0.0');
        if (options.language && options.language !== 'auto') {
          formData.append('language', options.language.split('-')[0]);
        }
      } else {
        formData.append('model', 'whisper-1');
        if (options.language && options.language !== 'auto') {
          formData.append('language', options.language.split('-')[0]);
        }
      }

      const timeoutMs = customUrl ? 25_000 : 300;
      const res = await fetch(url, {
        method: 'POST',
        body: formData,
        signal: AbortSignal.timeout(timeoutMs),
      });

      if (res.ok) {
        const data = (await res.json()) as any;
        const rawText =
          data.text ??
          data.transcription ??
          data.result ??
          (Array.isArray(data.results) ? data.results.map((r: any) => r.text).join(' ') : '');
        const text = sanitizeTranscription(String(rawText || ''));
        return { ok: true, text, engine: 'local' };
      }
      if (customUrl) {
        const errText = await res.text().catch(() => '');
        return {
          ok: false,
          error: `Local STT server returned HTTP ${res.status}: ${errText.slice(0, 150)}`,
        };
      }
    } catch (err: any) {
      if (customUrl) {
        return {
          ok: false,
          error: `Failed to connect to local STT server at ${customUrl}: ${err.message}`,
        };
      }
    }
  }

  return { ok: false, error: 'No local HTTP speech server found' };
}

export async function transcribeViaCli(
  wavPath: string,
  options: TranscribeOptions,
  onProgress?: SpeechProgressCallback,
): Promise<TranscribeResult> {
  // 1. Custom CLI command if provided
  const customCmd = options.localCommand?.trim() || process.env.LOCAL_STT_COMMAND?.trim();
  if (customCmd) {
    try {
      const expanded = customCmd.includes('{file}')
        ? customCmd.replace(/\{file\}/g, `"${wavPath}"`)
        : `${customCmd} "${wavPath}"`;
      const cmd = process.platform === 'win32' ? 'cmd.exe' : '/bin/sh';
      const args = process.platform === 'win32' ? ['/c', expanded] : ['-c', expanded];
      let liveOutput = '';
      const res = await runProcessWithTimeout(cmd, args, 30_000, undefined, (chunk) => {
        liveOutput += chunk;
        const cleaned = sanitizeTranscription(liveOutput);
        if (cleaned && onProgress) {
          onProgress(chunk, cleaned);
        }
      });
      if (res.ok) {
        const text = sanitizeTranscription(res.stdout);
        return { ok: true, text, engine: 'local' };
      }
      return {
        ok: false,
        error: `Custom local STT command exited with code ${res.exitCode}: ${(res.stderr || res.stdout).slice(0, 180)}`,
      };
    } catch (err: any) {
      return { ok: false, error: `Custom local STT command failed: ${err.message}` };
    }
  }

  // 2. whisper-cli / whisper-cpp / whisper.cpp
  const whisperCpp = findExecutableInPath(['whisper-cli', 'whisper-cpp', 'whisper.cpp']);
  if (whisperCpp) {
    const model = findWhisperCppModel(options.model);
    if (model) {
      const args: string[] = ['-m', model];
      if (options.language && options.language !== 'auto') {
        args.push('-l', options.language.split('-')[0]);
      }
      const cpus = Math.min(8, Math.max(2, os.cpus().length || 4));
      args.push('-f', wavPath, '-nt', '-t', String(cpus), '--beam-size', '1', '--best-of', '1', '--no-context');
      let liveOutput = '';
      const res = await runProcessWithTimeout(whisperCpp, args, 30_000, undefined, (chunk) => {
        liveOutput += chunk;
        const cleaned = sanitizeTranscription(liveOutput);
        if (cleaned && onProgress) {
          onProgress(chunk, cleaned);
        }
      });
      if (res.ok) {
        return { ok: true, text: sanitizeTranscription(res.stdout), engine: 'local' };
      }
    }
  }

  // 3. OpenAI Python whisper CLI
  const whisperPy = findExecutableInPath(['whisper', 'whisper-fast', 'faster-whisper']);
  if (whisperPy) {
    const tmpOut = fs.mkdtempSync(path.join(os.tmpdir(), 'chat-overlay-whpy-'));
    try {
      const cpus = Math.min(8, Math.max(2, os.cpus().length || 4));
      const args = [
        wavPath,
        '--model',
        'base',
        '--output_format',
        'txt',
        '--output_dir',
        tmpOut,
        '--fp16',
        'False',
        '--threads',
        String(cpus),
        '--beam_size',
        '1',
        '--best_of',
        '1',
        '--condition_on_previous_text',
        'False',
      ];
      if (options.language && options.language !== 'auto') {
        args.push('--language', options.language.split('-')[0]);
      }
      let liveOutput = '';
      const res = await runProcessWithTimeout(
        whisperPy,
        args,
        45_000,
        undefined,
        (chunk) => {
          liveOutput += chunk;
          const cleaned = sanitizeTranscription(liveOutput);
          if (cleaned && onProgress) {
            onProgress(chunk, cleaned);
          }
        },
        { PYTHONUNBUFFERED: '1' },
      );
      const base = path.basename(wavPath, path.extname(wavPath));
      const txtFile = path.join(tmpOut, `${base}.txt`);
      if (fs.existsSync(txtFile)) {
        const content = fs.readFileSync(txtFile, 'utf8');
        return { ok: true, text: sanitizeTranscription(content), engine: 'local' };
      }
      if (res.ok && res.stdout.trim()) {
        return { ok: true, text: sanitizeTranscription(res.stdout), engine: 'local' };
      }
    } finally {
      try {
        fs.rmSync(tmpOut, { recursive: true, force: true });
      } catch {}
    }
  }

  // 4. sherpa-onnx-offline / sherpa-onnx
  const sherpa = findExecutableInPath(['sherpa-onnx-offline', 'sherpa-onnx']);
  if (sherpa) {
    const res = await runProcessWithTimeout(sherpa, [wavPath], 30_000);
    if (res.ok) {
      return { ok: true, text: sanitizeTranscription(res.stdout), engine: 'local' };
    }
  }

  // 5. vosk-transcriber
  const vosk = findExecutableInPath(['vosk-transcriber']);
  if (vosk) {
    const tmpTxt = path.join(os.tmpdir(), `vosk-out-${Date.now()}.txt`);
    try {
      const args = ['-i', wavPath, '-o', tmpTxt];
      if (options.language && options.language !== 'auto') {
        args.push('-l', options.language.split('-')[0]);
      }
      const res = await runProcessWithTimeout(vosk, args, 30_000);
      if (fs.existsSync(tmpTxt)) {
        const content = fs.readFileSync(tmpTxt, 'utf8');
        return { ok: true, text: sanitizeTranscription(content), engine: 'local' };
      }
      if (res.ok && res.stdout.trim()) {
        return { ok: true, text: sanitizeTranscription(res.stdout), engine: 'local' };
      }
    } finally {
      try {
        fs.unlinkSync(tmpTxt);
      } catch {}
    }
  }

  return { ok: false, error: 'No supported local speech CLI tool found' };
}

export async function transcribeLocalSpeech(
  options: TranscribeOptions,
  settings?: { localSpeechUrl?: string; localSpeechCommand?: string; speechModel?: string },
  onProgress?: SpeechProgressCallback,
): Promise<TranscribeResult> {
  const mergedOptions: TranscribeOptions = {
    ...options,
    model: options.model || settings?.speechModel,
    localUrl: options.localUrl || settings?.localSpeechUrl,
    localCommand: options.localCommand || settings?.localSpeechCommand,
  };

  const audioBuffer = Buffer.from(options.data, 'base64');
  if (audioBuffer.length === 0) {
    return { ok: false, error: 'Empty audio data' };
  }

  // Prepare temporary audio file for conversion & CLI tools
  const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'chat-overlay-speech-'));
  const cleanMime = cleanAudioMimeType(options.mimeType);
  const ext = cleanMime.includes('wav') ? 'wav' : cleanMime.includes('ogg') ? 'ogg' : 'webm';
  const inputPath = path.join(tmpDir, `input.${ext}`);
  const wavPath = path.join(tmpDir, 'input-16k.wav');

  let targetWav = inputPath;
  let targetBuffer = audioBuffer;
  let targetMime = options.mimeType;

  try {
    if (isPcm16Mono16kWav(audioBuffer)) {
      fs.writeFileSync(wavPath, audioBuffer);
      targetWav = wavPath;
      targetBuffer = audioBuffer;
      targetMime = 'audio/wav';
    } else {
      fs.writeFileSync(inputPath, audioBuffer);
      const converted = await convertAudioToWav(inputPath, wavPath);
      if (converted && fs.existsSync(wavPath)) {
        targetWav = wavPath;
        targetBuffer = fs.readFileSync(wavPath);
        targetMime = 'audio/wav';
      }
    }

    // 1. If custom local command is configured, run it directly without delay
    if (mergedOptions.localCommand) {
      const cliRes = await transcribeViaCli(targetWav, mergedOptions, onProgress);
      return cliRes;
    }

    // 2. If custom local STT server URL is configured, run it directly
    if (mergedOptions.localUrl) {
      const httpRes = await transcribeViaHttpServer(targetBuffer, targetMime, mergedOptions);
      if (httpRes.ok && httpRes.text && onProgress) {
        onProgress(httpRes.text, httpRes.text);
      }
      return httpRes;
    }

    // 3. Managed warm whisper-server (sub-second latency, keeps model loaded with 5m idle sleep)
    const activeModel = findWhisperCppModel(mergedOptions.model);
    if (activeModel) {
      const managedUrl = await ensureManagedWhisperServer(activeModel);
      if (managedUrl) {
        const srvRes = await transcribeViaHttpServer(targetBuffer, targetMime, {
          ...mergedOptions,
          localUrl: managedUrl,
        });
        if (srvRes.ok) {
          if (srvRes.text && onProgress) {
            onProgress(srvRes.text, srvRes.text);
          }
          return srvRes;
        }
      }
    }

    // 4. Try fastest installed CLI tool (whisper-cli / whisper / sherpa-onnx / vosk)
    const cliRes = await transcribeViaCli(targetWav, mergedOptions, onProgress);
    if (cliRes.ok) {
      return cliRes;
    }

    // 5. Try local HTTP server default ports (short probe timeout)
    try {
      const httpRes = await transcribeViaHttpServer(targetBuffer, targetMime, mergedOptions);
      if (httpRes.ok) {
        if (httpRes.text && onProgress) {
          onProgress(httpRes.text, httpRes.text);
        }
        return httpRes;
      }
    } catch {}

    if (cliRes.error && !cliRes.error.includes('No supported local speech CLI tool found')) {
      return cliRes;
    }
  } finally {
    try {
      fs.rmSync(tmpDir, { recursive: true, force: true });
    } catch {}
  }

  return {
    ok: false,
    error:
      'No local speech-to-text engine responded. Start a local STT server (e.g. whisper.cpp at http://127.0.0.1:8080) or install a local CLI (whisper, whisper-cli, sherpa-onnx, or vosk).',
  };
}

export async function detectLocalSpeechBackend(
  settings?: { localSpeechUrl?: string; localSpeechCommand?: string },
): Promise<{ available: boolean; engine: string; target?: string }> {
  const customUrl =
    settings?.localSpeechUrl?.trim() ||
    process.env.LOCAL_STT_URL?.trim() ||
    process.env.WHISPER_SERVER_URL?.trim();
  if (customUrl) {
    return { available: true, engine: 'http-custom', target: customUrl };
  }

  // Check default HTTP ports (must exist and not return 404 Not Found)
  for (const url of ['http://127.0.0.1:8080/inference', 'http://127.0.0.1:8000/v1/audio/transcriptions']) {
    try {
      const res = await fetch(url, { method: 'HEAD', signal: AbortSignal.timeout(400) }).catch(() => null);
      if (res && res.status !== 404 && res.status < 500) {
        return { available: true, engine: 'http-server', target: url };
      }
    } catch {}
  }

  const customCmd = settings?.localSpeechCommand?.trim() || process.env.LOCAL_STT_COMMAND?.trim();
  if (customCmd) {
    return { available: true, engine: 'cli-custom', target: customCmd };
  }

  const whisperCpp = findExecutableInPath(['whisper-server', 'whisper-cli', 'whisper-cpp', 'whisper.cpp']);
  if (whisperCpp) {
    return { available: true, engine: path.basename(whisperCpp), target: whisperCpp };
  }

  const whisperPy = findExecutableInPath(['whisper', 'whisper-fast', 'faster-whisper']);
  if (whisperPy) {
    return { available: true, engine: 'whisper', target: whisperPy };
  }

  const sherpa = findExecutableInPath(['sherpa-onnx-offline', 'sherpa-onnx']);
  if (sherpa) {
    return { available: true, engine: 'sherpa-onnx', target: sherpa };
  }

  const vosk = findExecutableInPath(['vosk-transcriber']);
  if (vosk) {
    return { available: true, engine: 'vosk', target: vosk };
  }

  return { available: false, engine: 'none' };
}

async function readSseStream(
  body: any,
  onToken?: (token: string, accumulated: string) => void,
): Promise<string> {
  let accumulated = '';
  let buffer = '';

  const processChunk = (chunkText: string) => {
    buffer += chunkText;
    const lines = buffer.split('\n');
    buffer = lines.pop() || '';

    for (const line of lines) {
      const trimmed = line.trim();
      if (!trimmed.startsWith('data:')) continue;
      const jsonStr = trimmed.slice(5).trim();
      if (!jsonStr || jsonStr === '[DONE]') continue;
      try {
        const parsed = JSON.parse(jsonStr);
        const token = parsed.candidates?.[0]?.content?.parts?.[0]?.text;
        if (typeof token === 'string' && token) {
          accumulated += token;
          const cleaned = sanitizeTranscription(accumulated);
          if (cleaned) {
            onToken?.(token, cleaned);
          }
        }
      } catch {}
    }
  };

  if (body && typeof body.getReader === 'function') {
    const reader = body.getReader();
    const decoder = new TextDecoder();
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      processChunk(decoder.decode(value, { stream: true }));
    }
  } else if (body && typeof body[Symbol.asyncIterator] === 'function') {
    for await (const chunk of body) {
      const text = typeof chunk === 'string' ? chunk : chunk.toString();
      processChunk(text);
    }
  }

  if (buffer.trim().startsWith('data:')) {
    const jsonStr = buffer.trim().slice(5).trim();
    if (jsonStr && jsonStr !== '[DONE]') {
      try {
        const parsed = JSON.parse(jsonStr);
        const token = parsed.candidates?.[0]?.content?.parts?.[0]?.text;
        if (typeof token === 'string' && token) {
          accumulated += token;
          const cleaned = sanitizeTranscription(accumulated);
          if (cleaned) {
            onToken?.(token, cleaned);
          }
        }
      } catch {}
    }
  }

  return accumulated;
}

export async function transcribeAudio(
  options: TranscribeOptions,
  settings?: Partial<AppSettings>,
  onProgress?: SpeechProgressCallback,
): Promise<TranscribeResult> {
  if (!options?.data || typeof options.data !== 'string') {
    return { ok: false, error: 'No audio data provided' };
  }

  const mode: SpeechMode = options.mode || settings?.speechMode || 'auto';
  const effectiveOptions: TranscribeOptions = {
    ...options,
    localUrl: options.localUrl || settings?.localSpeechUrl,
    localCommand: options.localCommand || settings?.localSpeechCommand,
  };

  // If explicitly set to 'local', run local STT directly
  if (mode === 'local') {
    const localRes = await transcribeLocalSpeech(effectiveOptions, settings, onProgress);
    if (localRes.ok && localRes.text && onProgress) {
      onProgress(localRes.text, localRes.text);
    }
    return localRes;
  }

  // Cloud transcription (Gemini -> OpenAI Whisper)
  let cloudError: string | null = null;
  const geminiKey = resolveSpeechApiKey();

  if (geminiKey) {
    const mimeType = cleanAudioMimeType(effectiveOptions.mimeType);
    const candidateModels = effectiveOptions.model
      ? [effectiveOptions.model]
      : ['gemini-2.0-flash', 'gemini-1.5-flash'];

    let systemPrompt =
      'You are an accurate speech-to-text transcriber for a coding assistant. ' +
      'Transcribe the audio speech verbatim across all languages without translation. ' +
      'Output ONLY the raw transcribed text. Do NOT include markdown code blocks, commentary, quotes, or conversational explanations. ' +
      'If the audio is silent or contains only non-speech noise without spoken words, output exactly: [NO_SPEECH].';

    if (effectiveOptions.language && effectiveOptions.language !== 'auto') {
      systemPrompt += `\nSpoken language hint: ${effectiveOptions.language}.`;
    }

    const payload = {
      contents: [
        {
          parts: [
            {
              inlineData: {
                mimeType,
                data: effectiveOptions.data,
              },
            },
            {
              text: 'Transcribe the audio input verbatim following the rules.',
            },
          ],
        },
      ],
      systemInstruction: {
        parts: [{ text: systemPrompt }],
      },
      generationConfig: {
        temperature: 0.0,
        maxOutputTokens: 256,
        thinkingConfig: {
          thinkingBudget: 0,
        },
      },
    };

    for (const model of candidateModels) {
      try {
        const streamEndpoint = `https://generativelanguage.googleapis.com/v1beta/models/${model}:streamGenerateContent?alt=sse&key=${geminiKey}`;
        const streamRes = await fetch(streamEndpoint, {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify(payload),
          signal: AbortSignal.timeout(15_000),
        });

        if (streamRes.ok && streamRes.body) {
          const rawStreamText = await readSseStream(streamRes.body, (token, acc) => {
            onProgress?.(token, acc);
          });
          const cleaned = sanitizeTranscription(rawStreamText);
          if (cleaned) {
            return { ok: true, text: cleaned, engine: 'cloud' };
          }
          if (rawStreamText.includes('[NO_SPEECH]') || !rawStreamText.trim()) {
            return { ok: true, text: '', engine: 'cloud' };
          }
        }

        // Fallback to standard generateContent for this model
        const endpoint = `https://generativelanguage.googleapis.com/v1beta/models/${model}:generateContent?key=${geminiKey}`;
        const res = await fetch(endpoint, {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify(payload),
          signal: AbortSignal.timeout(15_000),
        });

        if (res.ok) {
          const data = (await res.json()) as any;
          const candidateText = data.candidates?.[0]?.content?.parts?.[0]?.text?.trim() || '';
          const cleaned = sanitizeTranscription(candidateText);
          if (cleaned && onProgress) {
            onProgress(cleaned, cleaned);
          }
          return { ok: true, text: cleaned, engine: 'cloud' };
        }

        const errorText = await res.text().catch(() => '');
        cloudError = `Gemini API (${model}) error (${res.status}): ${errorText.slice(0, 160)}`;
      } catch (err: any) {
        cloudError = err?.message || `Failed to call Gemini (${model})`;
      }
    }
  }

  // Secondary fallback: OpenAI Whisper
  const openAiKey = resolveOpenAiKey();
  if (openAiKey) {
    try {
      const mimeType = cleanAudioMimeType(effectiveOptions.mimeType);
      const ext = mimeType.includes('wav') ? 'wav' : mimeType.includes('ogg') ? 'ogg' : 'webm';
      const audioBuffer = Buffer.from(effectiveOptions.data, 'base64');

      const formData = new FormData();
      formData.append(
        'file',
        new Blob([new Uint8Array(audioBuffer)], { type: mimeType }),
        `audio.${ext}`,
      );
      formData.append('model', 'whisper-1');
      formData.append('temperature', '0.0');
      if (effectiveOptions.language && effectiveOptions.language !== 'auto') {
        formData.append('language', effectiveOptions.language.split('-')[0]);
      }

      const res = await fetch('https://api.openai.com/v1/audio/transcriptions', {
        method: 'POST',
        headers: {
          Authorization: `Bearer ${openAiKey}`,
        },
        body: formData,
        signal: AbortSignal.timeout(20_000),
      });

      if (res.ok) {
        const data = (await res.json()) as any;
        const text = sanitizeTranscription(data.text || '');
        if (text && onProgress) {
          onProgress(text, text);
        }
        return { ok: true, text, engine: 'cloud' };
      }

      const errorText = await res.text();
      cloudError = `OpenAI Whisper API error (${res.status}): ${errorText.slice(0, 180)}`;
    } catch (err: any) {
      cloudError = err?.message || 'Failed to call Whisper transcription service';
    }
  }

  // If cloud mode was explicitly requested, do not fall back to local
  if (mode === 'cloud') {
    return {
      ok: false,
      error:
        cloudError ||
        'No speech recognition API key configured. Set GEMINI_API_KEY in environment or configure ~/.config/chat-overlay/voice.json.',
    };
  }

  // Auto mode fallback to Local STT!
  // If API was attempted but failed, or if no API key is available, run local speech recognition
  const localRes = await transcribeLocalSpeech(effectiveOptions, settings, onProgress);
  if (localRes.ok) {
    if (localRes.text && onProgress) {
      onProgress(localRes.text, localRes.text);
    }
    return localRes;
  }

  // If both failed or no API key and local failed:
  if (cloudError) {
    return {
      ok: false,
      error: `Cloud API failed (${cloudError}). Local STT fallback failed: ${localRes.error}`,
    };
  }

  return {
    ok: false,
    error: `No speech recognition API key configured, and local STT fallback failed: ${localRes.error}`,
  };
}
