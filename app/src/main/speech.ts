import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

export interface TranscribeOptions {
  data: string;
  mimeType: string;
  language?: string;
  model?: string;
}

export interface TranscribeResult {
  ok: boolean;
  text?: string;
  error?: string;
}

export function resolveSpeechApiKey(): string | null {
  if (process.env.GEMINI_API_KEY?.trim()) return process.env.GEMINI_API_KEY.trim();
  if (process.env.GOOGLE_API_KEY?.trim()) return process.env.GOOGLE_API_KEY.trim();

  const voiceJson = path.join(os.homedir(), '.pi', 'agent', 'voice.json');
  if (fs.existsSync(voiceJson)) {
    try {
      const parsed = JSON.parse(fs.readFileSync(voiceJson, 'utf8'));
      if (parsed.apiKey && typeof parsed.apiKey === 'string' && parsed.apiKey.trim()) {
        return parsed.apiKey.trim();
      }
    } catch {}
  }

  const authJson = path.join(os.homedir(), '.pi', 'agent', 'auth.json');
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

  const voiceJson = path.join(os.homedir(), '.pi', 'agent', 'voice.json');
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

  const authJson = path.join(os.homedir(), '.pi', 'agent', 'auth.json');
  if (fs.existsSync(authJson)) {
    try {
      const parsed = JSON.parse(fs.readFileSync(authJson, 'utf8'));
      // Only accept actual API keys (sk-...), not OAuth tokens (type === 'oauth')
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

export async function transcribeAudio(options: TranscribeOptions): Promise<TranscribeResult> {
  if (!options?.data || typeof options.data !== 'string') {
    return { ok: false, error: 'No audio data provided' };
  }

  let lastError: string | null = null;
  const geminiKey = resolveSpeechApiKey();

  if (geminiKey) {
    try {
      const mimeType = cleanAudioMimeType(options.mimeType);
      const model = options.model || 'gemini-2.5-flash';
      const endpoint = `https://generativelanguage.googleapis.com/v1beta/models/${model}:generateContent?key=${geminiKey}`;

      let systemPrompt =
        'You are an accurate speech-to-text transcriber for a coding assistant. ' +
        'Transcribe the audio speech verbatim across all languages without translation. ' +
        'Output ONLY the raw transcribed text. Do NOT include markdown code blocks, commentary, quotes, or conversational explanations. ' +
        'If the audio is silent or contains only non-speech noise without spoken words, output exactly: [NO_SPEECH].';

      if (options.language && options.language !== 'auto') {
        systemPrompt += `\nSpoken language hint: ${options.language}.`;
      }

      const payload = {
        contents: [
          {
            parts: [
              {
                inlineData: {
                  mimeType,
                  data: options.data,
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
          thinkingConfig: {
            thinkingBudget: 0,
          },
        },
      };

      const res = await fetch(endpoint, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(payload),
        signal: AbortSignal.timeout(20_000),
      });

      if (res.ok) {
        const data = (await res.json()) as any;
        const candidateText = data.candidates?.[0]?.content?.parts?.[0]?.text?.trim() || '';
        const cleaned = sanitizeTranscription(candidateText);
        return { ok: true, text: cleaned };
      }

      const errorText = await res.text();
      lastError = `Gemini API error (${res.status}): ${errorText.slice(0, 180)}`;
    } catch (err: any) {
      lastError = err?.message || 'Failed to call Gemini speech transcription service';
    }
  }

  // Secondary fallback: OpenAI Whisper
  const openAiKey = resolveOpenAiKey();
  if (openAiKey) {
    try {
      const mimeType = cleanAudioMimeType(options.mimeType);
      const ext = mimeType.includes('wav') ? 'wav' : mimeType.includes('ogg') ? 'ogg' : 'webm';
      const audioBuffer = Buffer.from(options.data, 'base64');

      const formData = new FormData();
      formData.append('file', new Blob([audioBuffer], { type: mimeType }), `audio.${ext}`);
      formData.append('model', 'whisper-1');
      if (options.language && options.language !== 'auto') {
        formData.append('language', options.language.split('-')[0]);
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
        return { ok: true, text };
      }

      const errorText = await res.text();
      lastError = `OpenAI Whisper API error (${res.status}): ${errorText.slice(0, 180)}`;
    } catch (err: any) {
      lastError = err?.message || 'Failed to call Whisper transcription service';
    }
  }

  return {
    ok: false,
    error:
      lastError ||
      'No speech recognition API key configured. Set GEMINI_API_KEY in environment or configure ~/.pi/agent/voice.json.',
  };
}
