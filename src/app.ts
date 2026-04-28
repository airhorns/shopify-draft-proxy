import Koa from 'koa';
import bodyParser from 'koa-bodyparser';
import type { AppConfig } from './config.js';
import { createDraftProxy, type DraftProxy } from './proxy-instance.js';

async function readRequestText(ctx: Koa.Context): Promise<string> {
  const parsedBody = ctx.request.body;
  if (typeof parsedBody === 'string') {
    return parsedBody;
  }
  if (Buffer.isBuffer(parsedBody)) {
    return parsedBody.toString('utf8');
  }
  if (parsedBody && typeof parsedBody === 'object' && Object.keys(parsedBody).length > 0) {
    return JSON.stringify(parsedBody);
  }

  const chunks: Buffer[] = [];
  for await (const chunk of ctx.req) {
    chunks.push(Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk));
  }
  return Buffer.concat(chunks).toString('utf8');
}

export function createApp(config: AppConfig, proxy: DraftProxy = createDraftProxy(config)): Koa {
  const app = new Koa();

  app.use(bodyParser());
  app.use(async (ctx) => {
    const response = await proxy.processRequest({
      method: ctx.method,
      path: ctx.path,
      headers: ctx.request.headers,
      body: /^\/staged-uploads\/[^/]+\/.+/u.test(ctx.path) ? await readRequestText(ctx) : ctx.request.body,
    });

    ctx.status = response.status;
    for (const [name, value] of Object.entries(response.headers ?? {})) {
      ctx.set(name, value);
    }
    ctx.body = response.body;
  });

  return app;
}
