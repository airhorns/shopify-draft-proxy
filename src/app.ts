import Koa from 'koa';
import bodyParser from 'koa-bodyparser';
import type { AppConfig } from './config.js';
import { createMetaRouter } from './meta/routes.js';
import { createProxyRouter } from './proxy/routes.js';

export function createApp(config: AppConfig): Koa {
  const app = new Koa();
  const metaRouter = createMetaRouter();
  const proxyRouter = createProxyRouter(config);

  app.use(bodyParser());
  app.use(metaRouter.routes());
  app.use(metaRouter.allowedMethods());
  app.use(proxyRouter.routes());
  app.use(proxyRouter.allowedMethods());

  return app;
}
