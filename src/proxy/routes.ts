import Router from '@koa/router';
import type Koa from 'koa';
import { parseOperation } from '../graphql/parse-operation.js';
import { makeSyntheticGid, makeSyntheticTimestamp } from '../state/synthetic-identity.js';
import { store } from '../state/store.js';
import type { AppConfig } from '../config.js';
import { createUpstreamGraphQLClient } from '../shopify/upstream-client.js';
import { getOperationCapability } from './capabilities.js';
import { handleProductMutation, handleProductQuery, hydrateProductsFromUpstreamResponse } from './products.js';

interface GraphQLBody {
  query?: unknown;
  variables?: unknown;
}

function readVariables(raw: unknown): Record<string, unknown> {
  return typeof raw === 'object' && raw !== null ? (raw as Record<string, unknown>) : {};
}

export function createProxyRouter(config: AppConfig): Router {
  const router = new Router();
  const upstream = createUpstreamGraphQLClient(config.shopifyAdminOrigin);

  router.post('/admin/api/:version/graphql.json', async (ctx: Koa.Context) => {
    const body = ctx.request.body as GraphQLBody;

    if (typeof body?.query !== 'string') {
      ctx.status = 400;
      ctx.body = { errors: [{ message: 'Expected string GraphQL query' }] };
      return;
    }

    const variables = readVariables(body.variables);
    const parsed = parseOperation(body.query);
    const capability = getOperationCapability(parsed);

    if (capability.execution === 'stage-locally' && capability.domain === 'products') {
      store.appendLog({
        id: makeSyntheticGid('MutationLogEntry'),
        receivedAt: makeSyntheticTimestamp(),
        operationName: capability.operationName,
        query: body.query,
        variables,
        status: 'staged',
        notes: 'Staged locally in the in-memory product draft store.',
      });

      ctx.status = 200;
      ctx.body = handleProductMutation(body.query, variables);
      return;
    }

    if (capability.execution === 'overlay-read' && capability.domain === 'products') {
      if (config.readMode === 'snapshot') {
        ctx.status = 200;
        ctx.body = handleProductQuery(body.query, variables, config.readMode);
        return;
      }

      const response = await upstream.request({
        path: ctx.path,
        headers: {
          'content-type': 'application/json',
          'x-shopify-access-token': ctx.get('x-shopify-access-token'),
        },
        body: {
          query: body.query,
          variables,
        },
      });

      const upstreamBody = await response.json();
      hydrateProductsFromUpstreamResponse(upstreamBody);

      ctx.status = response.status;
      ctx.body = store.hasStagedProducts() ? handleProductQuery(body.query, variables, config.readMode) : upstreamBody;
      return;
    }

    if (parsed.type === 'mutation') {
      store.appendLog({
        id: makeSyntheticGid('MutationLogEntry'),
        receivedAt: makeSyntheticTimestamp(),
        operationName: capability.operationName,
        query: body.query,
        variables,
        status: 'proxied',
        notes: 'Mutation passthrough placeholder until supported local staging is implemented.',
      });
    }

    const response = await upstream.request({
      path: ctx.path,
      headers: {
        'content-type': 'application/json',
        'x-shopify-access-token': ctx.get('x-shopify-access-token'),
      },
      body: {
        query: body.query,
        variables,
      },
    });

    ctx.status = response.status;
    ctx.body = await response.json();
  });

  return router;
}
