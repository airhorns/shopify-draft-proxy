import { createServer, type IncomingMessage, type Server, type ServerResponse } from 'node:http';

import { createDraftProxy, type DraftProxy } from './runtime.js';
import type { AppConfig, DraftProxyHeaderValue, DraftProxyHttpResponse } from './types.js';

type RequestHandler = (req: IncomingMessage, res: ServerResponse) => void;

function requestPath(req: IncomingMessage): string {
  return new URL(req.url ?? '/', 'http://localhost').pathname;
}

function headerValue(value: string | string[] | undefined): DraftProxyHeaderValue {
  return value;
}

function requestHeaders(req: IncomingMessage): Record<string, DraftProxyHeaderValue> {
  return Object.fromEntries(Object.entries(req.headers).map(([name, value]) => [name, headerValue(value)]));
}

async function readRequestText(req: IncomingMessage): Promise<string> {
  const chunks: Buffer[] = [];
  for await (const chunk of req) {
    chunks.push(Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk));
  }
  return Buffer.concat(chunks).toString('utf8');
}

function isJsonRequest(req: IncomingMessage): boolean {
  const contentType = req.headers['content-type'];
  const value = Array.isArray(contentType) ? contentType.join(',') : contentType;
  return /\bapplication\/(?:[\w.+-]+\+)?json\b/iu.test(value ?? '');
}

function parseRequestBody(req: IncomingMessage, rawBody: string): unknown {
  if (rawBody.length === 0) {
    return undefined;
  }
  if (!isJsonRequest(req)) {
    return rawBody;
  }
  return JSON.parse(rawBody);
}

function hasContentType(response: ServerResponse): boolean {
  return response.hasHeader('content-type') || response.hasHeader('Content-Type');
}

function sendResponse(res: ServerResponse, response: DraftProxyHttpResponse): void {
  res.statusCode = response.status;
  for (const [name, value] of Object.entries(response.headers ?? {})) {
    res.setHeader(name, value);
  }

  if (typeof response.body === 'string') {
    if (!hasContentType(res)) {
      res.setHeader('content-type', 'text/plain; charset=utf-8');
    }
    res.end(response.body);
    return;
  }

  if (!hasContentType(res)) {
    res.setHeader('content-type', 'application/json; charset=utf-8');
  }
  res.end(JSON.stringify(response.body));
}

function sendJsonError(res: ServerResponse, status: number, message: string): void {
  sendResponse(res, {
    status,
    body: { errors: [{ message }] },
  });
}

export class DraftProxyHttpApp {
  readonly proxy: DraftProxy;
  readonly config: AppConfig;

  constructor(config: AppConfig, proxy: DraftProxy = createDraftProxy(config)) {
    this.config = config;
    this.proxy = proxy;
  }

  callback(): RequestHandler {
    return (req, res) => {
      void this.handle(req, res).catch((error: unknown) => {
        sendJsonError(res, 500, error instanceof Error ? error.message : 'Internal server error');
      });
    };
  }

  listen(port: number = this.config.port, hostnameOrListener?: string | (() => void), listener?: () => void): Server {
    const server = createServer(this.callback());
    if (typeof hostnameOrListener === 'function') {
      return server.listen(port, hostnameOrListener);
    }
    if (hostnameOrListener === undefined) {
      return server.listen(port, listener);
    }
    return server.listen(port, hostnameOrListener, listener);
  }

  async handle(req: IncomingMessage, res: ServerResponse): Promise<void> {
    let body: unknown;
    try {
      body = parseRequestBody(req, await readRequestText(req));
    } catch {
      sendJsonError(res, 400, 'Invalid JSON request body');
      return;
    }

    const response = await this.proxy.processRequest({
      method: req.method ?? 'GET',
      path: requestPath(req),
      headers: requestHeaders(req),
      body,
    });
    sendResponse(res, response);
  }
}

export function createApp(config: AppConfig, proxy?: DraftProxy): DraftProxyHttpApp {
  return new DraftProxyHttpApp(config, proxy);
}
