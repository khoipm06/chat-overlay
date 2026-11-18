import type { MarkupAnnotation, ScreenshotQuality } from '../shared/types';

type Tool = 'box' | 'arrow' | 'pen' | 'highlighter' | 'badge';

interface DrawnStroke {
  tool: Tool;
  color: string;
  x1: number;
  y1: number;
  x2: number;
  y2: number;
  points?: Array<{ x: number; y: number }>;
  step?: number;
}

const canvas = document.getElementById('markup-canvas') as HTMLCanvasElement;
const ctx = canvas.getContext('2d')!;
const annotateToolbar = document.getElementById('annotate-toolbar')!;
const regionToolbar = document.getElementById('region-toolbar')!;
const regionHint = document.getElementById('region-hint')!;
const agentToolbar = document.getElementById('agent-toolbar')!;
const agentCount = document.getElementById('agent-markup-count')!;

let mode: 'annotate' | 'region' | 'agent' = 'annotate';
let quality: ScreenshotQuality = 'compressed';
let bgImage: HTMLImageElement | null = null;
let currentTool: Tool = 'box';
let currentColor = '#ef4444';
let strokes: DrawnStroke[] = [];
let activeStroke: DrawnStroke | null = null;
let regionDrag: { x1: number; y1: number; x2: number; y2: number } | null = null;
let agentAnnotations: MarkupAnnotation[] = [];
let pulseRafId = 0;
let lastPulseFrame = 0;

function stopPulseAnimation(): void {
  if (pulseRafId) {
    cancelAnimationFrame(pulseRafId);
    pulseRafId = 0;
  }
}

function startPulseAnimationIfNeeded(): void {
  stopPulseAnimation();
  if (mode !== 'agent' || agentAnnotations.length === 0) return;
  const shouldPulse = agentAnnotations.some((a) => a.pulse !== false);
  if (!shouldPulse) return;
  const tick = (ts: number) => {
    if (ts - lastPulseFrame >= 33) {
      lastPulseFrame = ts;
      redraw();
    }
    pulseRafId = requestAnimationFrame(tick);
  };
  pulseRafId = requestAnimationFrame(tick);
}

function resizeCanvas(): void {
  const dpr = window.devicePixelRatio || 1;
  canvas.width = Math.floor(window.innerWidth * dpr);
  canvas.height = Math.floor(window.innerHeight * dpr);
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  redraw();
}

function drawArrow(targetCtx: CanvasRenderingContext2D, x1: number, y1: number, x2: number, y2: number): void {
  const headLen = 14;
  const angle = Math.atan2(y2 - y1, x2 - x1);
  targetCtx.beginPath();
  targetCtx.moveTo(x1, y1);
  targetCtx.lineTo(x2, y2);
  targetCtx.stroke();

  targetCtx.beginPath();
  targetCtx.moveTo(x2, y2);
  targetCtx.lineTo(
    x2 - headLen * Math.cos(angle - Math.PI / 6),
    y2 - headLen * Math.sin(angle - Math.PI / 6),
  );
  targetCtx.lineTo(
    x2 - headLen * Math.cos(angle + Math.PI / 6),
    y2 - headLen * Math.sin(angle + Math.PI / 6),
  );
  targetCtx.closePath();
  targetCtx.fill();
}

function drawLabelPill(x: number, y: number, text: string, color = '#0284c7'): void {
  ctx.font = '600 12px system-ui, sans-serif';
  const metrics = ctx.measureText(text);
  const w = metrics.width + 14;
  const h = 23;
  const py = Math.max(4, y - h - 6);
  ctx.fillStyle = color;
  ctx.beginPath();
  ctx.roundRect(x, py, w, h, 6);
  ctx.fill();
  ctx.fillStyle = '#ffffff';
  ctx.fillText(text, x + 7, py + 15);
}

function drawStepBadge(x: number, y: number, stepNum: number, color = '#0284c7'): void {
  ctx.save();
  ctx.fillStyle = color;
  ctx.beginPath();
  ctx.arc(x, y, 11, 0, Math.PI * 2);
  ctx.fill();
  ctx.lineWidth = 2;
  ctx.strokeStyle = '#ffffff';
  ctx.stroke();
  ctx.fillStyle = '#ffffff';
  ctx.font = '700 11px system-ui, sans-serif';
  ctx.textAlign = 'center';
  ctx.textBaseline = 'middle';
  ctx.fillText(String(stepNum), x, y + 0.5);
  ctx.restore();
}

function redraw(): void {
  const w = window.innerWidth;
  const h = window.innerHeight;
  ctx.clearRect(0, 0, w, h);

  if ((mode === 'annotate' || mode === 'region') && bgImage) {
    ctx.drawImage(bgImage, 0, 0, w, h);
    ctx.fillStyle = mode === 'region' ? 'rgba(0, 0, 0, 0.42)' : 'rgba(0, 0, 0, 0.12)';
    ctx.fillRect(0, 0, w, h);
  }

  if (mode === 'region' && regionDrag && bgImage) {
    const rx = Math.min(regionDrag.x1, regionDrag.x2);
    const ry = Math.min(regionDrag.y1, regionDrag.y2);
    const rw = Math.abs(regionDrag.x2 - regionDrag.x1);
    const rh = Math.abs(regionDrag.y2 - regionDrag.y1);
    if (rw > 2 && rh > 2) {
      // Paint bright un-dimmed cutout inside selected marquee
      const sx = (rx / w) * bgImage.naturalWidth;
      const sy = (ry / h) * bgImage.naturalHeight;
      const sw = (rw / w) * bgImage.naturalWidth;
      const sh = (rh / h) * bgImage.naturalHeight;
      ctx.drawImage(bgImage, sx, sy, sw, sh, rx, ry, rw, rh);
      ctx.strokeStyle = '#38bdf8';
      ctx.lineWidth = 2;
      ctx.strokeRect(rx, ry, rw, rh);
      regionHint.textContent = `${Math.round(sw)} × ${Math.round(sh)} px`;
    }
    return;
  }

  ctx.strokeStyle = '#ff3b30';
  ctx.fillStyle = '#ff3b30';
  ctx.lineWidth = 3;
  ctx.lineCap = 'round';
  ctx.lineJoin = 'round';

  const drawOne = (s: DrawnStroke, targetCtx: CanvasRenderingContext2D = ctx) => {
    const strokeColor = s.color || '#ef4444';
    if (s.tool === 'box') {
      const rx = Math.min(s.x1, s.x2);
      const ry = Math.min(s.y1, s.y2);
      const rw = Math.abs(s.x2 - s.x1);
      const rh = Math.abs(s.y2 - s.y1);
      targetCtx.save();
      targetCtx.strokeStyle = strokeColor;
      targetCtx.lineWidth = 3;
      targetCtx.fillStyle = `${strokeColor}18`;
      targetCtx.fillRect(rx, ry, rw, rh);
      targetCtx.strokeRect(rx, ry, rw, rh);
      targetCtx.restore();
    } else if (s.tool === 'arrow') {
      targetCtx.save();
      targetCtx.strokeStyle = strokeColor;
      targetCtx.fillStyle = strokeColor;
      targetCtx.lineWidth = 3;
      drawArrow(targetCtx, s.x1, s.y1, s.x2, s.y2);
      targetCtx.restore();
    } else if (s.tool === 'pen' && s.points && s.points.length > 1) {
      targetCtx.save();
      targetCtx.strokeStyle = strokeColor;
      targetCtx.lineWidth = 3;
      targetCtx.beginPath();
      targetCtx.moveTo(s.points[0].x, s.points[0].y);
      for (let i = 1; i < s.points.length; i++) {
        targetCtx.lineTo(s.points[i].x, s.points[i].y);
      }
      targetCtx.stroke();
      targetCtx.restore();
    } else if (s.tool === 'highlighter' && s.points && s.points.length > 0) {
      targetCtx.save();
      targetCtx.strokeStyle = strokeColor;
      targetCtx.globalAlpha = 0.35;
      targetCtx.lineWidth = 18;
      targetCtx.beginPath();
      targetCtx.moveTo(s.points[0].x, s.points[0].y);
      for (let i = 1; i < s.points.length; i++) {
        targetCtx.lineTo(s.points[i].x, s.points[i].y);
      }
      targetCtx.stroke();
      targetCtx.restore();
    } else if (s.tool === 'badge') {
      drawStepBadge(s.x1, s.y1, s.step || 1, strokeColor);
    }
  };

  for (const s of strokes) drawOne(s);
  if (activeStroke) drawOne(activeStroke);

  if (mode === 'agent') {
    const phase = (Math.sin(performance.now() / 280) + 1) / 2;
    const accent = '#38bdf8';
    const badgeColor = '#0284c7';
    agentAnnotations.forEach((ann, idx) => {
      const stepNum = ann.step ?? (agentAnnotations.length > 1 ? idx + 1 : undefined);
      const pulsePad = ann.pulse === false ? 0 : 2 + phase * 6;
      if (ann.kind === 'box') {
        const bx = ((ann.x ?? 0) / 100) * w;
        const by = ((ann.y ?? 0) / 100) * h;
        const bw = ((ann.w ?? 10) / 100) * w;
        const bh = ((ann.h ?? 8) / 100) * h;
        if (pulsePad > 0) {
          ctx.save();
          ctx.strokeStyle = `rgba(56, 189, 248, ${(0.55 - phase * 0.4).toFixed(2)})`;
          ctx.lineWidth = 2.5;
          ctx.beginPath();
          ctx.roundRect(bx - pulsePad, by - pulsePad, bw + pulsePad * 2, bh + pulsePad * 2, 8);
          ctx.stroke();
          ctx.restore();
        }
        ctx.fillStyle = 'rgba(56, 189, 248, 0.12)';
        ctx.strokeStyle = accent;
        ctx.lineWidth = 2.5;
        ctx.beginPath();
        ctx.roundRect(bx, by, bw, bh, 6);
        ctx.fill();
        ctx.stroke();
        const labelX = stepNum !== undefined ? bx + 16 : bx;
        if (ann.label) drawLabelPill(labelX, by, ann.label, badgeColor);
        if (stepNum !== undefined) drawStepBadge(bx, by, stepNum, badgeColor);
      } else if (ann.kind === 'arrow') {
        const x1 = ((ann.x1 ?? 50) / 100) * w;
        const y1 = ((ann.y1 ?? 50) / 100) * h;
        const x2 = ((ann.x2 ?? 25) / 100) * w;
        const y2 = ((ann.y2 ?? 25) / 100) * h;
        ctx.fillStyle = accent;
        ctx.strokeStyle = accent;
        ctx.lineWidth = 3;
        drawArrow(ctx, x1, y1, x2, y2);
        if (ann.pulse !== false) {
          ctx.save();
          ctx.strokeStyle = `rgba(56, 189, 248, ${(0.6 - phase * 0.45).toFixed(2)})`;
          ctx.lineWidth = 2;
          ctx.beginPath();
          ctx.arc(x2, y2, 8 + phase * 10, 0, Math.PI * 2);
          ctx.stroke();
          ctx.restore();
        }
        if (ann.label) drawLabelPill(x2 + 14, y2, ann.label, badgeColor);
        if (stepNum !== undefined) drawStepBadge(x2, y2, stepNum, badgeColor);
      }
    });
  }
}

function encodeOutputCanvas(out: HTMLCanvasElement): void {
  if (quality === 'compressed') {
    window.markupBridge.submitImage(out.toDataURL('image/jpeg', 0.84), 'image/jpeg');
  } else {
    window.markupBridge.submitImage(out.toDataURL('image/png'), 'image/png');
  }
}

function submitRegionCrop(drag: { x1: number; y1: number; x2: number; y2: number }): void {
  if (!bgImage) {
    window.markupBridge.cancel();
    return;
  }
  const w = window.innerWidth;
  const h = window.innerHeight;
  const rx = Math.min(drag.x1, drag.x2);
  const ry = Math.min(drag.y1, drag.y2);
  const rw = Math.abs(drag.x2 - drag.x1);
  const rh = Math.abs(drag.y2 - drag.y1);
  if (rw < 10 || rh < 10) return;

  const sx = Math.max(0, Math.round((rx / w) * bgImage.naturalWidth));
  const sy = Math.max(0, Math.round((ry / h) * bgImage.naturalHeight));
  const sw = Math.min(bgImage.naturalWidth - sx, Math.max(1, Math.round((rw / w) * bgImage.naturalWidth)));
  const sh = Math.min(bgImage.naturalHeight - sy, Math.max(1, Math.round((rh / h) * bgImage.naturalHeight)));

  const scale = quality === 'compressed' && sw > 1600 ? 1600 / sw : 1;
  const out = document.createElement('canvas');
  out.width = Math.max(1, Math.round(sw * scale));
  out.height = Math.max(1, Math.round(sh * scale));
  const octx = out.getContext('2d')!;
  octx.drawImage(bgImage, sx, sy, sw, sh, 0, 0, out.width, out.height);
  encodeOutputCanvas(out);
}

function submitAnnotated(): void {
  const srcW = bgImage?.naturalWidth || canvas.width;
  const srcH = bgImage?.naturalHeight || canvas.height;
  const scaleDown = quality === 'compressed' && srcW > 1600 ? 1600 / srcW : 1;

  const out = document.createElement('canvas');
  out.width = Math.max(1, Math.round(srcW * scaleDown));
  out.height = Math.max(1, Math.round(srcH * scaleDown));
  const octx = out.getContext('2d')!;
  if (bgImage) {
    octx.drawImage(bgImage, 0, 0, out.width, out.height);
  }
  const scaleX = out.width / window.innerWidth;
  const scaleY = out.height / window.innerHeight;
  octx.scale(scaleX, scaleY);
  for (const s of strokes) {
    const strokeColor = s.color || '#ef4444';
    if (s.tool === 'box') {
      const rx = Math.min(s.x1, s.x2);
      const ry = Math.min(s.y1, s.y2);
      const rw = Math.abs(s.x2 - s.x1);
      const rh = Math.abs(s.y2 - s.y1);
      octx.save();
      octx.strokeStyle = strokeColor;
      octx.lineWidth = 3;
      octx.fillStyle = `${strokeColor}18`;
      octx.fillRect(rx, ry, rw, rh);
      octx.strokeRect(rx, ry, rw, rh);
      octx.restore();
    } else if (s.tool === 'arrow') {
      octx.save();
      octx.strokeStyle = strokeColor;
      octx.fillStyle = strokeColor;
      octx.lineWidth = 3;
      drawArrow(octx, s.x1, s.y1, s.x2, s.y2);
      octx.restore();
    } else if (s.tool === 'pen' && s.points && s.points.length > 1) {
      octx.save();
      octx.strokeStyle = strokeColor;
      octx.lineWidth = 3;
      octx.beginPath();
      octx.moveTo(s.points[0].x, s.points[0].y);
      for (let i = 1; i < s.points.length; i++) octx.lineTo(s.points[i].x, s.points[i].y);
      octx.stroke();
      octx.restore();
    } else if (s.tool === 'highlighter' && s.points && s.points.length > 0) {
      octx.save();
      octx.strokeStyle = strokeColor;
      octx.globalAlpha = 0.35;
      octx.lineWidth = 18;
      octx.beginPath();
      octx.moveTo(s.points[0].x, s.points[0].y);
      for (let i = 1; i < s.points.length; i++) octx.lineTo(s.points[i].x, s.points[i].y);
      octx.stroke();
      octx.restore();
    } else if (s.tool === 'badge') {
      octx.save();
      octx.fillStyle = strokeColor;
      octx.beginPath();
      octx.arc(s.x1, s.y1, 11, 0, Math.PI * 2);
      octx.fill();
      octx.lineWidth = 2;
      octx.strokeStyle = '#ffffff';
      octx.stroke();
      octx.fillStyle = '#ffffff';
      octx.font = '700 11px system-ui, sans-serif';
      octx.textAlign = 'center';
      octx.textBaseline = 'middle';
      octx.fillText(String(s.step || 1), s.x1, s.y1 + 0.5);
      octx.restore();
    }
  }

  encodeOutputCanvas(out);
}

window.markupBridge.onInit((payload) => {
  mode = payload.mode;
  quality = payload.quality ?? 'compressed';
  strokes = [];
  regionDrag = null;
  activeStroke = null;
  agentAnnotations = payload.annotations ?? [];

  annotateToolbar.classList.toggle('hidden', mode !== 'annotate');
  regionToolbar.classList.toggle('hidden', mode !== 'region');
  agentToolbar.classList.toggle('hidden', mode !== 'agent');
  if (mode === 'region') {
    regionHint.textContent = 'Drag to select area · Esc to cancel';
  }

  if (mode === 'annotate' || mode === 'region') {
    window.markupBridge.setClickThrough(false);
    if (payload.screenshotDataUrl) {
      const img = new Image();
      img.onload = () => {
        bgImage = img;
        redraw();
      };
      img.src = payload.screenshotDataUrl;
    }
  } else {
    agentCount.textContent = `${agentAnnotations.length} highlighted on screen`;
    window.markupBridge.setClickThrough(true);
    agentToolbar.addEventListener('mouseenter', () => window.markupBridge.setClickThrough(false));
    agentToolbar.addEventListener('mouseleave', () => window.markupBridge.setClickThrough(true));
  }
  resizeCanvas();
  startPulseAnimationIfNeeded();
});

window.addEventListener('beforeunload', stopPulseAnimation);

canvas.addEventListener('mousedown', (e) => {
  if (e.button !== 0) return;
  if (mode === 'region') {
    regionDrag = { x1: e.clientX, y1: e.clientY, x2: e.clientX, y2: e.clientY };
    redraw();
    return;
  }
  if (mode !== 'annotate') return;
  if (currentTool === 'badge') {
    const badgeStep = strokes.filter((s) => s.tool === 'badge').length + 1;
    strokes.push({
      tool: 'badge',
      color: currentColor,
      x1: e.clientX,
      y1: e.clientY,
      x2: e.clientX,
      y2: e.clientY,
      step: badgeStep,
    });
    redraw();
    return;
  }
  activeStroke = {
    tool: currentTool,
    color: currentColor,
    x1: e.clientX,
    y1: e.clientY,
    x2: e.clientX,
    y2: e.clientY,
    points: (currentTool === 'pen' || currentTool === 'highlighter')
      ? [{ x: e.clientX, y: e.clientY }]
      : undefined,
  };
  redraw();
});

canvas.addEventListener('mousemove', (e) => {
  if (mode === 'region' && regionDrag) {
    regionDrag.x2 = e.clientX;
    regionDrag.y2 = e.clientY;
    redraw();
    return;
  }
  if (!activeStroke) return;
  activeStroke.x2 = e.clientX;
  activeStroke.y2 = e.clientY;
  if (activeStroke.tool === 'pen' || activeStroke.tool === 'highlighter') {
    activeStroke.points?.push({ x: e.clientX, y: e.clientY });
  }
  redraw();
});

window.addEventListener('mouseup', () => {
  if (mode === 'region' && regionDrag) {
    const finished = regionDrag;
    regionDrag = null;
    redraw();
    submitRegionCrop(finished);
    return;
  }
  if (!activeStroke) return;
  strokes.push(activeStroke);
  activeStroke = null;
  redraw();
});

function selectTool(tool: Tool): void {
  currentTool = tool;
  document.querySelectorAll<HTMLButtonElement>('[data-tool]').forEach((b) => {
    b.classList.toggle('active', b.dataset.tool === tool);
  });
}

document.querySelectorAll<HTMLButtonElement>('[data-tool]').forEach((btn) => {
  btn.addEventListener('click', () => {
    selectTool((btn.dataset.tool as Tool) || 'box');
  });
});

document.getElementById('btn-undo')?.addEventListener('click', () => {
  strokes.pop();
  redraw();
});

document.getElementById('btn-clear-all')?.addEventListener('click', () => {
  strokes = [];
  redraw();
});

document.getElementById('btn-cancel-markup')?.addEventListener('click', () => {
  window.markupBridge.cancel();
});

document.getElementById('btn-cancel-region')?.addEventListener('click', () => {
  window.markupBridge.cancel();
});

document.getElementById('btn-submit-markup')?.addEventListener('click', submitAnnotated);

document.getElementById('btn-dismiss-agent-markup')?.addEventListener('click', () => {
  window.agentOverlay.clearAgentMarkup();
});

window.addEventListener('keydown', (e) => {
  if (e.key === 'Escape') {
    if (mode === 'annotate' || mode === 'region') window.markupBridge.cancel();
    else void window.agentOverlay.clearAgentMarkup();
  } else if (e.key === 'Enter' && mode === 'annotate') {
    submitAnnotated();
  } else if (mode === 'annotate' && !e.ctrlKey && !e.metaKey) {
    if (e.key === '1') selectTool('box');
    else if (e.key === '2') selectTool('arrow');
    else if (e.key === '3') selectTool('pen');
    else if (e.key === '4') selectTool('highlighter');
    else if (e.key === '5') selectTool('badge');
  } else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'z' && mode === 'annotate') {
    strokes.pop();
    redraw();
  }
});

// Color palette
document.querySelectorAll<HTMLButtonElement>('.color-swatch').forEach((btn) => {
  btn.addEventListener('click', () => {
    currentColor = btn.dataset.color || '#ef4444';
    document.querySelectorAll('.color-swatch').forEach((s) => s.classList.remove('active'));
    btn.classList.add('active');
  });
});
// Activate first swatch by default
(document.querySelector('.color-swatch') as HTMLButtonElement | null)?.classList.add('active');

window.addEventListener('resize', resizeCanvas);
resizeCanvas();
