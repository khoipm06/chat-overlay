import type { MarkupAnnotation } from '../shared/types';

type Tool = 'box' | 'arrow' | 'pen';

interface DrawnStroke {
  tool: Tool;
  x1: number;
  y1: number;
  x2: number;
  y2: number;
  points?: Array<{ x: number; y: number }>;
}

const canvas = document.getElementById('markup-canvas') as HTMLCanvasElement;
const ctx = canvas.getContext('2d')!;
const annotateToolbar = document.getElementById('annotate-toolbar')!;
const agentToolbar = document.getElementById('agent-toolbar')!;
const agentCount = document.getElementById('agent-markup-count')!;

let mode: 'annotate' | 'agent' = 'annotate';
let bgImage: HTMLImageElement | null = null;
let currentTool: Tool = 'box';
let strokes: DrawnStroke[] = [];
let activeStroke: DrawnStroke | null = null;
let agentAnnotations: MarkupAnnotation[] = [];

function resizeCanvas(): void {
  const dpr = window.devicePixelRatio || 1;
  canvas.width = Math.floor(window.innerWidth * dpr);
  canvas.height = Math.floor(window.innerHeight * dpr);
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  redraw();
}

function drawArrow(x1: number, y1: number, x2: number, y2: number): void {
  const headLen = 14;
  const angle = Math.atan2(y2 - y1, x2 - x1);
  ctx.beginPath();
  ctx.moveTo(x1, y1);
  ctx.lineTo(x2, y2);
  ctx.stroke();

  ctx.beginPath();
  ctx.moveTo(x2, y2);
  ctx.lineTo(
    x2 - headLen * Math.cos(angle - Math.PI / 6),
    y2 - headLen * Math.sin(angle - Math.PI / 6),
  );
  ctx.lineTo(
    x2 - headLen * Math.cos(angle + Math.PI / 6),
    y2 - headLen * Math.sin(angle + Math.PI / 6),
  );
  ctx.closePath();
  ctx.fill();
}

function drawLabelPill(x: number, y: number, text: string): void {
  ctx.font = '600 12px system-ui, sans-serif';
  const metrics = ctx.measureText(text);
  const w = metrics.width + 12;
  const h = 22;
  const py = Math.max(4, y - h - 4);
  ctx.fillStyle = '#e11d48';
  ctx.beginPath();
  ctx.roundRect(x, py, w, h, 5);
  ctx.fill();
  ctx.fillStyle = '#ffffff';
  ctx.fillText(text, x + 6, py + 15);
}

function redraw(): void {
  const w = window.innerWidth;
  const h = window.innerHeight;
  ctx.clearRect(0, 0, w, h);

  if (mode === 'annotate' && bgImage) {
    ctx.drawImage(bgImage, 0, 0, w, h);
    ctx.fillStyle = 'rgba(0, 0, 0, 0.12)';
    ctx.fillRect(0, 0, w, h);
  }

  ctx.strokeStyle = '#ff3b30';
  ctx.fillStyle = '#ff3b30';
  ctx.lineWidth = 3;
  ctx.lineCap = 'round';
  ctx.lineJoin = 'round';

  const drawOne = (s: DrawnStroke) => {
    if (s.tool === 'box') {
      const rx = Math.min(s.x1, s.x2);
      const ry = Math.min(s.y1, s.y2);
      const rw = Math.abs(s.x2 - s.x1);
      const rh = Math.abs(s.y2 - s.y1);
      ctx.fillStyle = 'rgba(255, 59, 48, 0.08)';
      ctx.fillRect(rx, ry, rw, rh);
      ctx.strokeRect(rx, ry, rw, rh);
      ctx.fillStyle = '#ff3b30';
    } else if (s.tool === 'arrow') {
      drawArrow(s.x1, s.y1, s.x2, s.y2);
    } else if (s.tool === 'pen' && s.points && s.points.length > 1) {
      ctx.beginPath();
      ctx.moveTo(s.points[0].x, s.points[0].y);
      for (let i = 1; i < s.points.length; i++) {
        ctx.lineTo(s.points[i].x, s.points[i].y);
      }
      ctx.stroke();
    }
  };

  for (const s of strokes) drawOne(s);
  if (activeStroke) drawOne(activeStroke);

  if (mode === 'agent') {
    for (const ann of agentAnnotations) {
      if (ann.kind === 'box') {
        const bx = ((ann.x ?? 0) / 100) * w;
        const by = ((ann.y ?? 0) / 100) * h;
        const bw = ((ann.w ?? 10) / 100) * w;
        const bh = ((ann.h ?? 8) / 100) * h;
        ctx.fillStyle = 'rgba(255, 59, 48, 0.1)';
        ctx.fillRect(bx, by, bw, bh);
        ctx.strokeRect(bx, by, bw, bh);
        if (ann.label) drawLabelPill(bx, by, ann.label);
      } else if (ann.kind === 'arrow') {
        const x1 = ((ann.x1 ?? 50) / 100) * w;
        const y1 = ((ann.y1 ?? 50) / 100) * h;
        const x2 = ((ann.x2 ?? 25) / 100) * w;
        const y2 = ((ann.y2 ?? 25) / 100) * h;
        ctx.fillStyle = '#ff3b30';
        drawArrow(x1, y1, x2, y2);
        if (ann.label) drawLabelPill(x2, y2, ann.label);
      }
    }
  }
}

function submitAnnotated(): void {
  // Draw clean final export without dim overlay
  const out = document.createElement('canvas');
  out.width = canvas.width;
  out.height = canvas.height;
  const octx = out.getContext('2d')!;
  if (bgImage) {
    octx.drawImage(bgImage, 0, 0, out.width, out.height);
  }
  const scaleX = out.width / window.innerWidth;
  const scaleY = out.height / window.innerHeight;
  octx.scale(scaleX, scaleY);
  octx.strokeStyle = '#ff3b30';
  octx.fillStyle = '#ff3b30';
  octx.lineWidth = 3;
  octx.lineCap = 'round';
  octx.lineJoin = 'round';

  for (const s of strokes) {
    if (s.tool === 'box') {
      octx.strokeRect(
        Math.min(s.x1, s.x2),
        Math.min(s.y1, s.y2),
        Math.abs(s.x2 - s.x1),
        Math.abs(s.y2 - s.y1),
      );
    } else if (s.tool === 'arrow') {
      const headLen = 14;
      const angle = Math.atan2(s.y2 - s.y1, s.x2 - s.x1);
      octx.beginPath();
      octx.moveTo(s.x1, s.y1);
      octx.lineTo(s.x2, s.y2);
      octx.stroke();
      octx.beginPath();
      octx.moveTo(s.x2, s.y2);
      octx.lineTo(
        s.x2 - headLen * Math.cos(angle - Math.PI / 6),
        s.y2 - headLen * Math.sin(angle - Math.PI / 6),
      );
      octx.lineTo(
        s.x2 - headLen * Math.cos(angle + Math.PI / 6),
        s.y2 - headLen * Math.sin(angle + Math.PI / 6),
      );
      octx.closePath();
      octx.fill();
    } else if (s.tool === 'pen' && s.points && s.points.length > 1) {
      octx.beginPath();
      octx.moveTo(s.points[0].x, s.points[0].y);
      for (let i = 1; i < s.points.length; i++) octx.lineTo(s.points[i].x, s.points[i].y);
      octx.stroke();
    }
  }

  window.markupBridge.submitImage(out.toDataURL('image/png'));
}

window.markupBridge.onInit((payload) => {
  mode = payload.mode;
  strokes = [];
  agentAnnotations = payload.annotations ?? [];

  if (mode === 'annotate') {
    annotateToolbar.classList.remove('hidden');
    agentToolbar.classList.add('hidden');
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
    annotateToolbar.classList.add('hidden');
    agentToolbar.classList.remove('hidden');
    agentCount.textContent = `${agentAnnotations.length} highlighted on screen`;
    window.markupBridge.setClickThrough(true);
    agentToolbar.addEventListener('mouseenter', () => window.markupBridge.setClickThrough(false));
    agentToolbar.addEventListener('mouseleave', () => window.markupBridge.setClickThrough(true));
  }
  resizeCanvas();
});

canvas.addEventListener('mousedown', (e) => {
  if (mode !== 'annotate' || e.button !== 0) return;
  activeStroke = {
    tool: currentTool,
    x1: e.clientX,
    y1: e.clientY,
    x2: e.clientX,
    y2: e.clientY,
    points: currentTool === 'pen' ? [{ x: e.clientX, y: e.clientY }] : undefined,
  };
  redraw();
});

canvas.addEventListener('mousemove', (e) => {
  if (!activeStroke) return;
  activeStroke.x2 = e.clientX;
  activeStroke.y2 = e.clientY;
  if (activeStroke.tool === 'pen') {
    activeStroke.points?.push({ x: e.clientX, y: e.clientY });
  }
  redraw();
});

window.addEventListener('mouseup', () => {
  if (!activeStroke) return;
  strokes.push(activeStroke);
  activeStroke = null;
  redraw();
});

document.querySelectorAll<HTMLButtonElement>('[data-tool]').forEach((btn) => {
  btn.addEventListener('click', () => {
    currentTool = (btn.dataset.tool as Tool) || 'box';
    document.querySelectorAll('[data-tool]').forEach((b) => b.classList.remove('active'));
    btn.classList.add('active');
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

document.getElementById('btn-submit-markup')?.addEventListener('click', submitAnnotated);

document.getElementById('btn-dismiss-agent-markup')?.addEventListener('click', () => {
  window.agentOverlay.clearAgentMarkup();
});

window.addEventListener('keydown', (e) => {
  if (e.key === 'Escape') {
    if (mode === 'annotate') window.markupBridge.cancel();
    else void window.agentOverlay.clearAgentMarkup();
  } else if (e.key === 'Enter' && mode === 'annotate') {
    submitAnnotated();
  } else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'z' && mode === 'annotate') {
    strokes.pop();
    redraw();
  }
});

window.addEventListener('resize', resizeCanvas);
resizeCanvas();
