import Koa from 'koa';
import bodyParser from 'koa-bodyparser';
import type { AppConfig } from './config.js';
import { createDefaultStoreDraftProxy, type DraftProxy } from './proxy-instance.js';

export function createApp(config: AppConfig, proxy: DraftProxy = createDefaultStoreDraftProxy(config)): Koa {
  const app = new Koa();

  app.use(bodyParser());
  app.use(async (ctx) => {
    const response = await proxy.processRequest({
      method: ctx.method,
      path: ctx.path,
      headers: ctx.request.headers,
      body: ctx.request.body,
    });

    ctx.status = response.status;
    for (const [name, value] of Object.entries(response.headers ?? {})) {
      ctx.set(name, value);
    }
    ctx.body = response.body;
  });

  return app;
}
