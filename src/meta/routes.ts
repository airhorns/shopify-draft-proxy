import Router from '@koa/router';
import type Koa from 'koa';
import { store } from '../state/store.js';
import { resetSyntheticIdentity } from '../state/synthetic-identity.js';

export function createMetaRouter(): Router {
  const router = new Router();

  router.get('/__meta/health', (ctx: Koa.Context) => {
    ctx.body = {
      ok: true,
      message: 'shopify-draft-proxy is running',
    };
  });

  router.get('/__meta/log', (ctx: Koa.Context) => {
    ctx.body = {
      entries: store.getLog(),
    };
  });

  router.get('/__meta/state', (ctx: Koa.Context) => {
    ctx.body = store.getState();
  });

  router.post('/__meta/reset', (ctx: Koa.Context) => {
    store.reset();
    resetSyntheticIdentity();
    ctx.body = {
      ok: true,
      message: 'state reset',
    };
  });

  router.post('/__meta/commit', (ctx: Koa.Context) => {
    ctx.status = 501;
    ctx.body = {
      ok: false,
      message: 'commit not implemented yet',
      entries: store.getLog(),
    };
  });

  return router;
}
