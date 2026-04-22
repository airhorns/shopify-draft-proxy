import Router from '@koa/router';
import type Koa from 'koa';
import { logger } from '../logger.js';
import { parseOperation, type ParsedOperation } from '../graphql/parse-operation.js';
import { makeSyntheticGid, makeSyntheticTimestamp } from '../state/synthetic-identity.js';
import { store } from '../state/store.js';
import type { MutationLogInterpretedMetadata } from '../state/types.js';
import type { AppConfig } from '../config.js';
import { createUpstreamGraphQLClient } from '../shopify/upstream-client.js';
import { getOperationCapability, type OperationCapability } from './capabilities.js';
import { handleMediaMutation } from './media.js';
import { handleCustomerMutation, handleCustomerQuery, hydrateCustomersFromUpstreamResponse } from './customers.js';
import { handleOrderMutation, handleOrderQuery, shouldServeDraftOrderSearchLocally } from './orders.js';
import { handleProductMutation, handleProductQuery, hydrateProductsFromUpstreamResponse } from './products.js';

interface GraphQLBody {
  query?: unknown;
  variables?: unknown;
  operationName?: unknown;
  extensions?: unknown;
}

function readVariables(raw: unknown): Record<string, unknown> {
  return typeof raw === 'object' && raw !== null ? (raw as Record<string, unknown>) : {};
}

function hasOwnProperty(value: object, key: string): boolean {
  return Object.prototype.hasOwnProperty.call(value, key);
}

function readOriginalRequestBody(body: GraphQLBody): Record<string, unknown> {
  const requestBody: Record<string, unknown> = {
    query: body.query,
  };

  if (hasOwnProperty(body, 'variables')) {
    requestBody['variables'] = body.variables;
  }

  if (hasOwnProperty(body, 'operationName')) {
    requestBody['operationName'] = body.operationName;
  }

  if (hasOwnProperty(body, 'extensions')) {
    requestBody['extensions'] = body.extensions;
  }

  return structuredClone(requestBody);
}

function interpretMutationLogEntry(
  parsed: ParsedOperation,
  capability: OperationCapability,
): MutationLogInterpretedMetadata {
  return {
    operationType: parsed.type,
    operationName: parsed.name,
    rootFields: parsed.rootFields,
    primaryRootField: parsed.rootFields[0] ?? null,
    capability: {
      operationName: capability.operationName,
      domain: capability.domain,
      execution: capability.execution,
    },
  };
}

export function createProxyRouter(config: AppConfig): Router {
  const router = new Router();
  const upstream = createUpstreamGraphQLClient(config.shopifyAdminOrigin);
  const proxyLogger = logger.child({ component: 'proxy' });

  router.post('/admin/api/:version/graphql.json', async (ctx: Koa.Context) => {
    const body = ctx.request.body as GraphQLBody;

    if (typeof body?.query !== 'string') {
      ctx.status = 400;
      ctx.body = { errors: [{ message: 'Expected string GraphQL query' }] };
      return;
    }

    const variables = readVariables(body.variables);
    const requestBody = readOriginalRequestBody(body);
    const parsed = parseOperation(body.query);
    const capability = getOperationCapability(parsed);

    if (capability.execution === 'stage-locally' && capability.domain === 'products') {
      proxyLogger.debug(
        {
          execution: capability.execution,
          operationName: capability.operationName,
          operationType: parsed.type,
          rootFields: parsed.rootFields,
        },
        'staging supported mutation locally',
      );

      store.appendLog({
        id: makeSyntheticGid('MutationLogEntry'),
        receivedAt: makeSyntheticTimestamp(),
        operationName: capability.operationName,
        path: ctx.path,
        query: body.query,
        variables,
        requestBody,
        status: 'staged',
        interpreted: interpretMutationLogEntry(parsed, capability),
        notes: 'Staged locally in the in-memory product draft store.',
      });

      ctx.status = 200;
      ctx.body = handleProductMutation(body.query, variables, config.readMode);
      return;
    }

    if (capability.execution === 'stage-locally' && capability.domain === 'customers') {
      store.appendLog({
        id: makeSyntheticGid('MutationLogEntry'),
        receivedAt: makeSyntheticTimestamp(),
        operationName: capability.operationName,
        path: ctx.path,
        query: body.query,
        variables,
        requestBody,
        status: 'staged',
        interpreted: interpretMutationLogEntry(parsed, capability),
        notes: 'Staged locally in the in-memory customer draft store.',
      });

      ctx.status = 200;
      ctx.body = handleCustomerMutation(body.query, variables);
      return;
    }

    if (capability.execution === 'stage-locally' && capability.domain === 'media') {
      store.appendLog({
        id: makeSyntheticGid('MutationLogEntry'),
        receivedAt: makeSyntheticTimestamp(),
        operationName: capability.operationName,
        path: ctx.path,
        query: body.query,
        variables,
        requestBody,
        status: 'staged',
        interpreted: interpretMutationLogEntry(parsed, capability),
        notes: 'Staged locally in the in-memory media draft store.',
      });

      ctx.status = 200;
      ctx.body = handleMediaMutation(body.query, variables);
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
      hydrateProductsFromUpstreamResponse(body.query, variables, upstreamBody);

      ctx.status = response.status;
      ctx.body = store.hasStagedProducts() ? handleProductQuery(body.query, variables, config.readMode) : upstreamBody;
      return;
    }

    if (capability.execution === 'overlay-read' && capability.domain === 'customers') {
      if (config.readMode === 'snapshot') {
        ctx.status = 200;
        ctx.body = handleCustomerQuery(body.query, variables);
        return;
      }

      if (config.readMode === 'live-hybrid') {
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
        hydrateCustomersFromUpstreamResponse(body.query, variables, upstreamBody);

        ctx.status = response.status;
        ctx.body = store.hasBaseCustomers() ? handleCustomerQuery(body.query, variables) : upstreamBody;
        return;
      }
    }

    if (capability.execution === 'overlay-read' && capability.domain === 'orders') {
      if (config.readMode === 'snapshot') {
        ctx.status = 200;
        ctx.body = handleOrderQuery(body.query, variables);
        return;
      }

      if (config.readMode === 'live-hybrid') {
        const liveHybridOrderId = typeof variables['id'] === 'string' ? variables['id'] : null;
        const hasStagedOrders = store.getOrders().length > 0;
        const hasStagedDraftOrders = store.getDraftOrders().length > 0;
        const canServeLocalOrderDetail =
          capability.operationName === 'order' &&
          liveHybridOrderId !== null &&
          store.getOrderById(liveHybridOrderId) !== null;
        const canServeLocalOrderCatalog =
          (capability.operationName === 'orders' || capability.operationName === 'ordersCount') &&
          hasStagedOrders &&
          typeof variables['query'] !== 'string';
        const canServeLocalDraftOrderDetail =
          capability.operationName === 'draftOrder' &&
          liveHybridOrderId !== null &&
          store.getDraftOrderById(liveHybridOrderId) !== null;
        const canServeLocalDraftOrderCatalog =
          (capability.operationName === 'draftOrders' || capability.operationName === 'draftOrdersCount') &&
          hasStagedDraftOrders &&
          (typeof variables['query'] !== 'string' || shouldServeDraftOrderSearchLocally(variables['query']));

        if (
          canServeLocalOrderDetail ||
          canServeLocalOrderCatalog ||
          canServeLocalDraftOrderDetail ||
          canServeLocalDraftOrderCatalog
        ) {
          ctx.status = 200;
          ctx.body = handleOrderQuery(body.query, variables);
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

        ctx.status = response.status;
        ctx.body = await response.json();
        return;
      }
    }

    if (
      capability.execution === 'stage-locally' &&
      capability.domain === 'orders' &&
      (config.readMode === 'snapshot' || capability.operationName === 'draftOrderCreate')
    ) {
      store.appendLog({
        id: makeSyntheticGid('MutationLogEntry'),
        receivedAt: makeSyntheticTimestamp(),
        operationName: capability.operationName,
        path: ctx.path,
        query: body.query,
        variables,
        requestBody,
        status: 'staged',
        interpreted: interpretMutationLogEntry(parsed, capability),
        notes: 'Staged locally in the in-memory order draft store.',
      });

      ctx.status = 200;
      ctx.body = handleOrderMutation(body.query, variables, config.readMode, config.shopifyAdminOrigin) ?? { data: {} };
      return;
    }

    if (
      capability.execution === 'stage-locally' &&
      capability.domain === 'orders' &&
      config.readMode === 'live-hybrid' &&
      (capability.operationName === 'orderCreate' ||
        capability.operationName === 'orderUpdate' ||
        capability.operationName === 'orderEditBegin' ||
        capability.operationName === 'orderEditAddVariant' ||
        capability.operationName === 'orderEditSetQuantity' ||
        capability.operationName === 'orderEditCommit' ||
        capability.operationName === 'draftOrderComplete' ||
        capability.operationName === 'fulfillmentCreate' ||
        capability.operationName === 'fulfillmentTrackingInfoUpdate' ||
        capability.operationName === 'fulfillmentCancel')
    ) {
      const orderMutationResponse = handleOrderMutation(
        body.query,
        variables,
        config.readMode,
        config.shopifyAdminOrigin,
      );
      if (orderMutationResponse) {
        const shortCircuitNotesByOperation: Record<string, string> = {
          orderCreate: 'Locally short-circuited captured orderCreate validation in live-hybrid mode.',
          orderUpdate:
            'Locally handled orderUpdate in live-hybrid mode for captured validation branches or a synthetic/local staged order.',
          orderEditBegin:
            'Locally staged the first calculated-order edit session in live-hybrid mode for a synthetic/local order.',
          orderEditAddVariant:
            'Locally staged a calculated-order variant add in live-hybrid mode for a synthetic/local order.',
          orderEditSetQuantity:
            'Locally staged a calculated-order quantity edit in live-hybrid mode for a synthetic/local order.',
          orderEditCommit:
            'Locally committed a calculated-order edit back onto a synthetic/local order in live-hybrid mode.',
          draftOrderComplete:
            'Locally handled draftOrderComplete in live-hybrid mode for captured validation branches or a synthetic/local staged draft order.',
          fulfillmentCreate: 'Locally short-circuited captured fulfillmentCreate validation in live-hybrid mode.',
        };

        store.appendLog({
          id: makeSyntheticGid('MutationLogEntry'),
          receivedAt: makeSyntheticTimestamp(),
          operationName: capability.operationName,
          path: ctx.path,
          query: body.query,
          variables,
          requestBody,
          status: 'staged',
          interpreted: interpretMutationLogEntry(parsed, capability),
          notes:
            shortCircuitNotesByOperation[capability.operationName] ??
            'Locally short-circuited captured order mutation validation in live-hybrid mode.',
        });

        ctx.status = 200;
        ctx.body = orderMutationResponse;
        return;
      }
    }

    if (parsed.type === 'mutation' && config.readMode === 'snapshot') {
      const orderMutationResponse = handleOrderMutation(
        body.query,
        variables,
        config.readMode,
        config.shopifyAdminOrigin,
      );
      if (orderMutationResponse) {
        ctx.status = 200;
        ctx.body = orderMutationResponse;
        return;
      }
    }

    if (parsed.type === 'mutation') {
      proxyLogger.warn(
        {
          execution: capability.execution,
          operationName: capability.operationName,
          operationType: parsed.type,
          rootFields: parsed.rootFields,
        },
        'proxying unsupported mutation upstream',
      );

      store.appendLog({
        id: makeSyntheticGid('MutationLogEntry'),
        receivedAt: makeSyntheticTimestamp(),
        operationName: capability.operationName,
        path: ctx.path,
        query: body.query,
        variables,
        requestBody,
        status: 'proxied',
        interpreted: interpretMutationLogEntry(parsed, capability),
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
