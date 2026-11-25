import crypto from 'node:crypto';
import http from 'node:http';

export interface ScreenInfoPayload {
  width: number;
  height: number;
  scaleFactor: number;
  pixelWidth: number;
  pixelHeight: number;
}

export interface ScreenshotRequestPayload {
  maxWidth?: number;
  region?: {
    x: number;
    y: number;
    width: number;
    height: number;
  };
}

export interface ScreenshotResponsePayload {
  ok: boolean;
  screenWidth: number;
  screenHeight: number;
  logicalScreenWidth: number;
  logicalScreenHeight: number;
  imageWidth: number;
  imageHeight: number;
  mimeType: string;
  data: string;
}

export interface ComputerUseBridgeDeps {
  getScreenInfo: () => ScreenInfoPayload;
  captureScreenshot: (opts: ScreenshotRequestPayload) => Promise<ScreenshotResponsePayload>;
  readClipboard: () => string;
  writeClipboard: (text: string) => void;
  openUrl: (url: string) => Promise<void> | void;
  notify: (title: string, body: string) => void;
}

export interface ComputerUseBridgeHandle {
  url: string;
  token: string;
  close: () => Promise<void>;
}

function readJsonBody(req: http.IncomingMessage): Promise<Record<string, unknown>> {
  return new Promise((resolve, reject) => {
    const chunks: Buffer[] = [];
    let total = 0;
    req.on('data', (chunk: Buffer) => {
      total += chunk.length;
      if (total > 2 * 1024 * 1024) {
        reject(new Error('Request body too large'));
        req.destroy();
        return;
      }
      chunks.push(chunk);
    });
    req.on('end', () => {
      const raw = Buffer.concat(chunks).toString('utf8').trim();
      if (!raw) {
        resolve({});
        return;
      }
      try {
        const parsed = JSON.parse(raw);
        resolve(parsed && typeof parsed === 'object' ? (parsed as Record<string, unknown>) : {});
      } catch (err) {
        reject(err);
      }
    });
    req.on('error', reject);
  });
}

function sendJson(res: http.ServerResponse, status: number, payload: unknown): void {
  const body = JSON.stringify(payload);
  res.writeHead(status, {
    'Content-Type': 'application/json',
    'Content-Length': Buffer.byteLength(body),
  });
  res.end(body);
}

export async function startComputerUseBridge(
  deps: ComputerUseBridgeDeps,
): Promise<ComputerUseBridgeHandle> {
  const token = crypto.randomBytes(32).toString('hex');

  const server = http.createServer(async (req, res) => {
    const auth = req.headers.authorization || '';
    if (auth !== `Bearer ${token}`) {
      sendJson(res, 401, { ok: false, error: 'Unauthorized' });
      return;
    }

    const reqUrl = new URL(req.url || '/', 'http://127.0.0.1');
    const pathname = reqUrl.pathname;
    const method = (req.method || 'GET').toUpperCase();

    try {
      if (method === 'GET' && pathname === '/screen-info') {
        sendJson(res, 200, deps.getScreenInfo());
        return;
      }

      if (method === 'POST' && pathname === '/screenshot') {
        const body = await readJsonBody(req);
        const maxWidth = typeof body.maxWidth === 'number' ? body.maxWidth : undefined;
        const rawRegion = body.region && typeof body.region === 'object'
          ? (body.region as Record<string, unknown>)
          : null;
        const region =
          rawRegion &&
          typeof rawRegion.x === 'number' &&
          typeof rawRegion.y === 'number' &&
          typeof rawRegion.width === 'number' &&
          typeof rawRegion.height === 'number'
            ? {
                x: rawRegion.x,
                y: rawRegion.y,
                width: rawRegion.width,
                height: rawRegion.height,
              }
            : undefined;
        const shot = await deps.captureScreenshot({ maxWidth, region });
        sendJson(res, 200, shot);
        return;
      }

      if (method === 'GET' && pathname === '/clipboard') {
        sendJson(res, 200, { ok: true, text: deps.readClipboard() });
        return;
      }

      if (method === 'POST' && pathname === '/clipboard') {
        const body = await readJsonBody(req);
        const text = typeof body.text === 'string' ? body.text : '';
        deps.writeClipboard(text);
        sendJson(res, 200, { ok: true });
        return;
      }

      if (method === 'POST' && pathname === '/open-url') {
        const body = await readJsonBody(req);
        const target = typeof body.url === 'string' ? body.url.trim() : '';
        if (!/^https?:\/\//i.test(target)) {
          sendJson(res, 400, { ok: false, error: 'Only http:// and https:// URLs are allowed.' });
          return;
        }
        await deps.openUrl(target);
        sendJson(res, 200, { ok: true });
        return;
      }

      if (method === 'POST' && pathname === '/notify') {
        const body = await readJsonBody(req);
        const title = typeof body.title === 'string' ? body.title : 'Notification';
        const msg = typeof body.body === 'string' ? body.body : '';
        deps.notify(title, msg);
        sendJson(res, 200, { ok: true });
        return;
      }

      sendJson(res, 404, { ok: false, error: 'Not found' });
    } catch (err) {
      sendJson(res, 500, {
        ok: false,
        error: err instanceof Error ? err.message : String(err),
      });
    }
  });

  await new Promise<void>((resolve, reject) => {
    server.once('error', reject);
    server.listen(0, '127.0.0.1', () => {
      server.removeListener('error', reject);
      resolve();
    });
  });

  const addr = server.address();
  const port = typeof addr === 'object' && addr ? addr.port : 0;

  return {
    url: `http://127.0.0.1:${port}`,
    token,
    close: () =>
      new Promise<void>((resolve) => {
        server.close(() => resolve());
      }),
  };
}
