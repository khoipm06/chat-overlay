export function downsampleLinear(
  samples: Float32Array,
  fromRate: number,
  toRate: number,
): Float32Array {
  if (samples.length === 0 || fromRate <= 0 || toRate <= 0) {
    return new Float32Array(0);
  }
  if (fromRate === toRate) {
    return new Float32Array(samples);
  }
  const ratio = fromRate / toRate;
  const outLength = Math.max(1, Math.floor(samples.length / ratio));
  const out = new Float32Array(outLength);
  for (let i = 0; i < outLength; i++) {
    const srcPos = i * ratio;
    const idx = Math.floor(srcPos);
    const frac = srcPos - idx;
    const s0 = samples[idx] ?? 0;
    const s1 = samples[Math.min(samples.length - 1, idx + 1)] ?? s0;
    out[i] = s0 + (s1 - s0) * frac;
  }
  return out;
}

export function encodePcm16Wav(
  chunks: Float32Array[],
  sourceSampleRate: number,
  targetSampleRate = 16000,
): Uint8Array {
  let totalLen = 0;
  for (const c of chunks) {
    totalLen += c.length;
  }
  const merged = new Float32Array(totalLen);
  let offset = 0;
  for (const c of chunks) {
    merged.set(c, offset);
    offset += c.length;
  }

  const resampled = downsampleLinear(merged, sourceSampleRate, targetSampleRate);
  const dataBytes = resampled.length * 2;
  const buffer = new ArrayBuffer(44 + dataBytes);
  const view = new DataView(buffer);

  const writeAscii = (pos: number, str: string) => {
    for (let i = 0; i < str.length; i++) {
      view.setUint8(pos + i, str.charCodeAt(i));
    }
  };

  writeAscii(0, 'RIFF');
  view.setUint32(4, 36 + dataBytes, true);
  writeAscii(8, 'WAVE');
  writeAscii(12, 'fmt ');
  view.setUint32(16, 16, true); // PCM chunk size
  view.setUint16(20, 1, true); // PCM linear quantization
  view.setUint16(22, 1, true); // Mono
  view.setUint32(24, targetSampleRate, true);
  view.setUint32(28, targetSampleRate * 2, true); // Byte rate = sampleRate * channels * bytesPerSample
  view.setUint16(32, 2, true); // Block align = channels * bytesPerSample
  view.setUint16(34, 16, true); // Bits per sample
  writeAscii(36, 'data');
  view.setUint32(40, dataBytes, true);

  let writePos = 44;
  for (let i = 0; i < resampled.length; i++) {
    const clamped = Math.max(-1, Math.min(1, resampled[i]));
    const int16 = clamped < 0 ? Math.round(clamped * 32768) : Math.round(clamped * 32767);
    view.setInt16(writePos, int16, true);
    writePos += 2;
  }

  return new Uint8Array(buffer);
}

export function uint8ToBase64(bytes: Uint8Array): string {
  let binary = '';
  const chunkSize = 0x8000;
  for (let i = 0; i < bytes.length; i += chunkSize) {
    const slice = bytes.subarray(i, i + chunkSize);
    binary += String.fromCharCode(...slice);
  }
  return btoa(binary);
}

export class PcmWavRecorder {
  private audioCtx: AudioContext | null = null;
  private source: MediaStreamAudioSourceNode | null = null;
  private processor: ScriptProcessorNode | null = null;
  private silentGain: GainNode | null = null;
  private chunks: Float32Array[] = [];
  private sampleRate = 48000;
  private stream: MediaStream | null = null;
  private totalSamples = 0;

  async start(stream: MediaStream): Promise<void> {
    this.stream = stream;
    const AudioCtx =
      window.AudioContext ||
      (window as unknown as { webkitAudioContext: typeof AudioContext }).webkitAudioContext;
    if (!AudioCtx) {
      throw new Error('AudioContext is not supported');
    }
    const ctx = new AudioCtx();
    if (ctx.state === 'suspended') {
      await ctx.resume();
    }
    this.audioCtx = ctx;
    this.sampleRate = ctx.sampleRate || 48000;
    this.chunks = [];
    this.totalSamples = 0;

    const source = ctx.createMediaStreamSource(stream);
    const processor = ctx.createScriptProcessor(4096, 1, 1);
    const silentGain = ctx.createGain();
    silentGain.gain.value = 0;

    processor.onaudioprocess = (ev) => {
      const channelData = ev.inputBuffer.getChannelData(0);
      if (channelData && channelData.length > 0) {
        this.chunks.push(new Float32Array(channelData));
        this.totalSamples += channelData.length;
      }
    };

    source.connect(processor);
    processor.connect(silentGain);
    silentGain.connect(ctx.destination);

    this.source = source;
    this.processor = processor;
    this.silentGain = silentGain;
  }

  hasAudio(): boolean {
    const durationSec = this.totalSamples / Math.max(1, this.sampleRate);
    return durationSec >= 0.15;
  }

  getSnapshotBase64(): string | null {
    if (!this.hasAudio()) return null;
    const wav = encodePcm16Wav([...this.chunks], this.sampleRate, 16000);
    return uint8ToBase64(wav);
  }

  async stopAndGetBase64(): Promise<string | null> {
    const wav = this.hasAudio() ? encodePcm16Wav([...this.chunks], this.sampleRate, 16000) : null;
    this.cleanup();
    return wav ? uint8ToBase64(wav) : null;
  }

  cancel(): void {
    this.chunks = [];
    this.totalSamples = 0;
    this.cleanup();
  }

  private cleanup(): void {
    try {
      this.processor?.disconnect();
    } catch {}
    try {
      this.source?.disconnect();
    } catch {}
    try {
      this.silentGain?.disconnect();
    } catch {}
    if (this.audioCtx) {
      try {
        void this.audioCtx.close();
      } catch {}
    }
    if (this.stream) {
      try {
        for (const t of this.stream.getTracks()) t.stop();
      } catch {}
    }
    this.processor = null;
    this.source = null;
    this.silentGain = null;
    this.audioCtx = null;
    this.stream = null;
  }
}
