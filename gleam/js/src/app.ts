import { createServer, type IncomingMessage, type Server, type ServerResponse } from 'node:http';

import { createDraftProxy, type DraftProxy } from './runtime.js';
import type { AppConfig, DraftProxyHeaderValue, DraftProxyHttpResponse } from './types.js';

type RequestHandler = (req: IncomingMessage, res: ServerResponse) => void;

const LEGACY_BULK_OPERATION_RESULT_ROUTE_PATTERN = /^\/__bulk_operations\/([^/]+)\/result\.jsonl$/u;
const META_BULK_OPERATION_RESULT_ROUTE_PATTERN = /^\/__meta\/bulk-operations\/(.+)\/result\.jsonl$/u;
const STAGED_UPLOAD_ROUTE_PATTERN = /^\/staged-uploads\/([^/]+)\/(.+)$/u;

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

function methodIs(req: IncomingMessage, method: string): boolean {
  return (req.method ?? 'GET').toUpperCase() === method;
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

function sendMethodNotAllowed(res: ServerResponse): void {
  sendJsonError(res, 405, 'Method not allowed');
}

function sendBulkOperationResult(res: ServerResponse, jsonl: string | null): void {
  if (jsonl === null) {
    sendResponse(res, {
      status: 404,
      body: 'Bulk operation result not found',
    });
    return;
  }

  sendResponse(res, {
    status: 200,
    headers: { 'content-type': 'application/jsonl; charset=utf-8' },
    body: jsonl,
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
    const path = requestPath(req);
    const rawBody = await readRequestText(req);

    const stagedUploadMatch = STAGED_UPLOAD_ROUTE_PATTERN.exec(path);
    if (stagedUploadMatch) {
      if (!methodIs(req, 'POST') && !methodIs(req, 'PUT')) {
        sendMethodNotAllowed(res);
        return;
      }

      sendResponse(res, {
        status: 201,
        body: this.proxy.stageStagedUpload(stagedUploadMatch[1] ?? '', stagedUploadMatch[2] ?? '', rawBody),
      });
      return;
    }

    const legacyBulkResultMatch = LEGACY_BULK_OPERATION_RESULT_ROUTE_PATTERN.exec(path);
    if (legacyBulkResultMatch) {
      if (!methodIs(req, 'GET')) {
        sendMethodNotAllowed(res);
        return;
      }

      const numericId = legacyBulkResultMatch[1];
      sendBulkOperationResult(
        res,
        numericId ? this.proxy.getBulkOperationResultJsonl(`gid://shopify/BulkOperation/${numericId}`) : null,
      );
      return;
    }

    const metaBulkResultMatch = META_BULK_OPERATION_RESULT_ROUTE_PATTERN.exec(path);
    if (metaBulkResultMatch) {
      if (!methodIs(req, 'GET')) {
        sendMethodNotAllowed(res);
        return;
      }

      const encodedOperationId = metaBulkResultMatch[1];
      sendBulkOperationResult(
        res,
        encodedOperationId === undefined
          ? null
          : this.proxy.getBulkOperationResultJsonl(decodeURIComponent(encodedOperationId)),
      );
      return;
    }

    let body: unknown;
    try {
      body = parseRequestBody(req, rawBody);
    } catch {
      sendJsonError(res, 400, 'Invalid JSON request body');
      return;
    }

    const response = await this.proxy.processRequest({
      method: req.method ?? 'GET',
      path,
      headers: requestHeaders(req),
      body,
    });
    sendResponse(res, response);
  }
}

export function createApp(config: AppConfig, proxy?: DraftProxy): DraftProxyHttpApp {
  return new DraftProxyHttpApp(config, proxy);
}
