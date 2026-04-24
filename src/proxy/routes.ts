import Router from '@koa/router';
import type Koa from 'koa';
import { logger } from '../logger.js';
import { parseOperation, type ParsedOperation } from '../graphql/parse-operation.js';
import { isProxySyntheticGid, makeSyntheticGid, makeSyntheticTimestamp } from '../state/synthetic-identity.js';
import { store } from '../state/store.js';
import type { MutationLogInterpretedMetadata } from '../state/types.js';
import type { AppConfig } from '../config.js';
import { createUpstreamGraphQLClient } from '../shopify/upstream-client.js';
import { getOperationCapability, type OperationCapability } from './capabilities.js';
import { handleMediaMutation } from './media.js';
import { handleCustomerMutation, handleCustomerQuery, hydrateCustomersFromUpstreamResponse } from './customers.js';
import { handleOrderMutation, handleOrderQuery, shouldServeDraftOrderCatalogLocally } from './orders.js';
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

function collectProxySyntheticGids(value: unknown, seen = new Set<string>()): string[] {
  if (typeof value === 'string') {
    if (isProxySyntheticGid(value)) {
      seen.add(value);
    }
    return [...seen];
  }

  if (!value || typeof value !== 'object') {
    return [...seen];
  }

  if (Array.isArray(value)) {
    for (const item of value) {
      collectProxySyntheticGids(item, seen);
    }
    return [...seen];
  }

  for (const item of Object.values(value)) {
    collectProxySyntheticGids(item, seen);
  }

  return [...seen];
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
    const primaryRootField = parsed.rootFields[0] ?? capability.operationName;

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

      const logEntryId = makeSyntheticGid('MutationLogEntry');
      const receivedAt = makeSyntheticTimestamp();
      const responseBody = handleProductMutation(body.query, variables, config.readMode);

      store.appendLog({
        id: logEntryId,
        receivedAt,
        operationName: capability.operationName,
        path: ctx.path,
        query: body.query,
        variables,
        requestBody,
        stagedResourceIds: collectProxySyntheticGids(responseBody),
        status: 'staged',
        interpreted: interpretMutationLogEntry(parsed, capability),
        notes: 'Staged locally in the in-memory product draft store.',
      });

      ctx.status = 200;
      ctx.body = responseBody;
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
          primaryRootField === 'order' && liveHybridOrderId !== null && store.getOrderById(liveHybridOrderId) !== null;
        const canServeLocalOrderCatalog =
          (primaryRootField === 'orders' || primaryRootField === 'ordersCount') &&
          hasStagedOrders &&
          typeof variables['query'] !== 'string';
        const canServeLocalDraftOrderDetail =
          primaryRootField === 'draftOrder' &&
          liveHybridOrderId !== null &&
          store.getDraftOrderById(liveHybridOrderId) !== null;
        const canServeLocalDraftOrderCatalog =
          (primaryRootField === 'draftOrders' || primaryRootField === 'draftOrdersCount') &&
          hasStagedDraftOrders &&
          shouldServeDraftOrderCatalogLocally(variables['query'], variables['savedSearchId']);

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
      (config.readMode === 'snapshot' || primaryRootField === 'draftOrderCreate')
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
      (primaryRootField === 'orderCreate' ||
        primaryRootField === 'refundCreate' ||
        primaryRootField === 'orderUpdate' ||
        primaryRootField === 'orderClose' ||
        primaryRootField === 'orderOpen' ||
        primaryRootField === 'orderMarkAsPaid' ||
        primaryRootField === 'orderCreateManualPayment' ||
        primaryRootField === 'orderCustomerSet' ||
        primaryRootField === 'orderCustomerRemove' ||
        primaryRootField === 'orderInvoiceSend' ||
        primaryRootField === 'taxSummaryCreate' ||
        primaryRootField === 'orderCancel' ||
        primaryRootField === 'orderEditBegin' ||
        primaryRootField === 'orderEditAddVariant' ||
        primaryRootField === 'orderEditSetQuantity' ||
        primaryRootField === 'orderEditCommit' ||
        primaryRootField === 'draftOrderComplete' ||
        primaryRootField === 'draftOrderUpdate' ||
        primaryRootField === 'draftOrderDuplicate' ||
        primaryRootField === 'draftOrderDelete' ||
        primaryRootField === 'draftOrderInvoiceSend' ||
        primaryRootField === 'draftOrderCreateFromOrder' ||
        primaryRootField === 'fulfillmentCreate' ||
        primaryRootField === 'fulfillmentTrackingInfoUpdate' ||
        primaryRootField === 'fulfillmentCancel')
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
          refundCreate:
            'Locally staged refundCreate in live-hybrid mode for a synthetic/local staged order or captured validation branch.',
          orderUpdate:
            'Locally handled orderUpdate in live-hybrid mode for captured validation branches or a synthetic/local staged order.',
          orderClose: 'Locally staged orderClose in live-hybrid mode for a synthetic/local order.',
          orderOpen: 'Locally staged orderOpen in live-hybrid mode for a synthetic/local order.',
          orderMarkAsPaid: 'Locally staged orderMarkAsPaid in live-hybrid mode for a synthetic/local order.',
          orderCreateManualPayment:
            'Locally mirrored the captured orderCreateManualPayment access-denied branch without proxying the mutation upstream.',
          orderCustomerSet: 'Locally staged orderCustomerSet in live-hybrid mode for a synthetic/local order.',
          orderCustomerRemove: 'Locally staged orderCustomerRemove in live-hybrid mode for a synthetic/local order.',
          orderInvoiceSend: 'Locally handled orderInvoiceSend in live-hybrid mode without sending invoice email.',
          taxSummaryCreate:
            'Locally mirrored the captured taxSummaryCreate access-denied branch without proxying the mutation upstream.',
          orderCancel: 'Locally staged orderCancel in live-hybrid mode for a synthetic/local order.',
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
          draftOrderUpdate:
            'Locally staged draftOrderUpdate in live-hybrid mode for a synthetic/local staged draft order.',
          draftOrderDuplicate:
            'Locally staged draftOrderDuplicate in live-hybrid mode for a synthetic/local staged draft order.',
          draftOrderDelete:
            'Locally staged draftOrderDelete in live-hybrid mode for a synthetic/local staged draft order.',
          draftOrderInvoiceSend:
            'Locally handled draftOrderInvoiceSend in live-hybrid mode without sending invoice email.',
          draftOrderCreateFromOrder:
            'Locally staged draftOrderCreateFromOrder in live-hybrid mode for a synthetic/local order.',
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
            shortCircuitNotesByOperation[primaryRootField ?? ''] ??
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
