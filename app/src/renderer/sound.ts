let audioCtx: AudioContext | null = null;

function getCtx(): AudioContext | null {
  try {
    const Ctx = window.AudioContext || (window as unknown as { webkitAudioContext?: typeof AudioContext }).webkitAudioContext;
    if (!Ctx) return null;
    if (!audioCtx) audioCtx = new Ctx();
    if (audioCtx.state === 'suspended') {
      void audioCtx.resume().catch(() => {});
    }
    return audioCtx;
  } catch {
    return null;
  }
}

function playTone(freq: number, startOffsetSec: number, durationSec: number, gainPeak: number): void {
  const ctx = getCtx();
  if (!ctx) return;
  const now = ctx.currentTime + startOffsetSec;
  const osc = ctx.createOscillator();
  const gain = ctx.createGain();
  osc.type = 'sine';
  osc.frequency.setValueAtTime(freq, now);
  gain.gain.setValueAtTime(0.0001, now);
  gain.gain.exponentialRampToValueAtTime(gainPeak, now + 0.012);
  gain.gain.exponentialRampToValueAtTime(0.0001, now + durationSec);
  osc.connect(gain);
  gain.connect(ctx.destination);
  osc.start(now);
  osc.stop(now + durationSec + 0.02);
}

export function playDoneChime(): void {
  playTone(587.33, 0, 0.11, 0.05);
  playTone(880.0, 0.08, 0.14, 0.055);
}

export function playAttentionChime(): void {
  playTone(523.25, 0, 0.09, 0.06);
  playTone(659.25, 0.08, 0.09, 0.06);
  playTone(783.99, 0.16, 0.15, 0.065);
}
