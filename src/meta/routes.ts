import Router from '@koa/router';
import type Koa from 'koa';
import type { AppConfig } from '../config.js';
import { createUpstreamGraphQLClient } from '../shopify/upstream-client.js';
import { store } from '../state/store.js';
import { resetSyntheticIdentity } from '../state/synthetic-identity.js';
import type { MutationLogEntry } from '../state/types.js';

interface CommitAttempt {
  logEntryId: string;
  operationName: string | null;
  path: string;
  status: MutationLogEntry['status'];
  upstreamStatus: number | null;
  responseBody: unknown;
}

function logEntryRequiresCommit(entry: MutationLogEntry): boolean {
  return entry.status === 'staged' || entry.status === 'proxied';
}

function responseBodyHasGraphQLErrors(body: unknown): boolean {
  if (!body || typeof body !== 'object') {
    return false;
  }

  const errors = (body as Record<string, unknown>)['errors'];
  return Array.isArray(errors) && errors.length > 0;
}

export function createMetaRouter(config: AppConfig): Router {
  const router = new Router();
  const upstream = createUpstreamGraphQLClient(config.shopifyAdminOrigin);

  router.get('/__meta/health', (ctx: Koa.Context) => {
    ctx.body = {
      ok: true,
      message: 'shopify-draft-proxy is running',
    };
  });

  router.get('/__meta/config', (ctx: Koa.Context) => {
    ctx.body = {
      runtime: {
        readMode: config.readMode,
      },
      proxy: {
        port: config.port,
        shopifyAdminOrigin: config.shopifyAdminOrigin,
      },
      snapshot: {
        enabled: Boolean(config.snapshotPath),
        path: config.snapshotPath ?? null,
      },
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
    store.restoreInitialState();
    resetSyntheticIdentity();
    ctx.body = {
      ok: true,
      message: 'state reset',
    };
  });

  router.post('/__meta/commit', async (ctx: Koa.Context) => {
    const pendingEntries = store.getLog().filter(logEntryRequiresCommit);
    const attempts: CommitAttempt[] = [];
    let stopIndex: number | null = null;

    for (const [index, entry] of pendingEntries.entries()) {
      try {
        const response = await upstream.request({
          path: entry.path,
          headers: {
            'content-type': 'application/json',
            'x-shopify-access-token': ctx.get('x-shopify-access-token'),
          },
          body: {
            query: entry.query,
            variables: entry.variables,
          },
        });
        const responseBody = await response.json();
        const failed = response.status >= 400 || responseBodyHasGraphQLErrors(responseBody);
        const nextStatus: MutationLogEntry['status'] = failed ? 'failed' : 'committed';

        store.updateLogEntry(entry.id, {
          status: nextStatus,
          notes: failed
            ? 'Commit replay failed against upstream Shopify.'
            : 'Committed to upstream Shopify via __meta/commit replay.',
        });

        attempts.push({
          logEntryId: entry.id,
          operationName: entry.operationName,
          path: entry.path,
          status: nextStatus,
          upstreamStatus: response.status,
          responseBody,
        });

        if (failed) {
          stopIndex = index;
          break;
        }
      } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        store.updateLogEntry(entry.id, {
          status: 'failed',
          notes: `Commit replay failed before an upstream response was received: ${message}`,
        });
        attempts.push({
          logEntryId: entry.id,
          operationName: entry.operationName,
          path: entry.path,
          status: 'failed',
          upstreamStatus: null,
          responseBody: { errors: [{ message }] },
        });
        stopIndex = index;
        break;
      }
    }

    ctx.body = {
      ok: stopIndex === null,
      stopIndex,
      attempts,
    };
  });

  return router;
}
