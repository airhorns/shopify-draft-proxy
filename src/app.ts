import Koa from 'koa';
import bodyParser from 'koa-bodyparser';
import type { AppConfig } from './config.js';
import { createMetaRouter } from './meta/routes.js';
import { createProxyRouter } from './proxy/routes.js';
import { loadNormalizedStateSnapshot } from './state/snapshot-loader.js';
import { store } from './state/store.js';

export function createApp(config: AppConfig): Koa {
  if (config.snapshotPath) {
    store.installSnapshot(loadNormalizedStateSnapshot(config.snapshotPath));
  }

  const app = new Koa();
  const metaRouter = createMetaRouter(config);
  const proxyRouter = createProxyRouter(config);

  app.use(bodyParser());
  app.use(metaRouter.routes());
  app.use(metaRouter.allowedMethods());
  app.use(proxyRouter.routes());
  app.use(proxyRouter.allowedMethods());

  return app;
}
