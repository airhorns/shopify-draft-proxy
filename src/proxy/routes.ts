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
import { findOperationRegistryEntry } from './operation-registry.js';
import { handleMediaMutation } from './media.js';
import { handleMarketingQuery, hydrateMarketingFromUpstreamResponse } from './marketing.js';
import { handleCustomerMutation, handleCustomerQuery, hydrateCustomersFromUpstreamResponse } from './customers.js';
import { handleDeliveryProfileMutation, handleDeliveryProfileQuery } from './delivery-profiles.js';
import { handleDiscountMutation, handleDiscountQuery } from './discounts.js';
import { handleMarketMutation, handleMarketsQuery, hydrateMarketsFromUpstreamResponse } from './markets.js';
import { handleOrderMutation, handleOrderQuery, shouldServeDraftOrderCatalogLocally } from './orders.js';
import { handleProductMutation, handleProductQuery, hydrateProductsFromUpstreamResponse } from './products.js';
import { handleMetafieldDefinitionMutation, handleMetafieldDefinitionQuery } from './metafield-definitions.js';
import {
  handleMetaobjectDefinitionQuery,
  hydrateMetaobjectDefinitionsFromUpstreamResponse,
} from './metaobject-definitions.js';
import { handlePaymentMutation, handlePaymentQuery } from './payments.js';
import { handleSegmentMutation, handleSegmentsQuery, hydrateSegmentsFromUpstreamResponse } from './segments.js';
import { handleStorePropertiesMutation, handleStorePropertiesQuery } from './store-properties.js';

interface GraphQLBody {
  query?: unknown;
  variables?: unknown;
  operationName?: unknown;
  extensions?: unknown;
}

const APP_DISCOUNT_MUTATION_ROOTS = new Set([
  'discountCodeAppCreate',
  'discountCodeAppUpdate',
  'discountAutomaticAppCreate',
  'discountAutomaticAppUpdate',
]);

const ORDER_PAYMENT_MUTATION_ROOTS = new Set(['orderCapture', 'transactionVoid', 'orderCreateMandatePayment']);

const PAYMENT_CUSTOMIZATION_MUTATION_ROOTS = new Set([
  'paymentCustomizationActivation',
  'paymentCustomizationCreate',
  'paymentCustomizationDelete',
  'paymentCustomizationUpdate',
]);

const FULFILLMENT_SERVICE_MUTATION_ROOTS = new Set([
  'fulfillmentServiceCreate',
  'fulfillmentServiceUpdate',
  'fulfillmentServiceDelete',
]);

const DELIVERY_PROFILE_MUTATION_ROOTS = new Set([
  'deliveryProfileCreate',
  'deliveryProfileUpdate',
  'deliveryProfileRemove',
]);

const CARRIER_SERVICE_MUTATION_ROOTS = new Set([
  'carrierServiceCreate',
  'carrierServiceUpdate',
  'carrierServiceDelete',
]);

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

function registeredOperationMetadata(
  registryEntry: NonNullable<ReturnType<typeof findOperationRegistryEntry>>,
): NonNullable<MutationLogInterpretedMetadata['registeredOperation']> {
  return {
    name: registryEntry.name,
    domain: registryEntry.domain,
    execution: registryEntry.execution,
    implemented: registryEntry.implemented,
    ...(registryEntry.supportNotes ? { supportNotes: registryEntry.supportNotes } : {}),
  };
}

function buildLocalDiscountMutationLogMetadata(
  parsed: ParsedOperation,
  fallbackCapability: OperationCapability,
): { operationName: string | null; interpreted: MutationLogInterpretedMetadata } {
  const registryEntry = findOperationRegistryEntry(parsed.type, [...parsed.rootFields, parsed.name]);
  if (!registryEntry || registryEntry.domain !== 'discounts') {
    return {
      operationName: fallbackCapability.operationName,
      interpreted: interpretMutationLogEntry(parsed, fallbackCapability),
    };
  }

  const operationName =
    parsed.rootFields.find((rootField) => registryEntry.matchNames.includes(rootField)) ?? registryEntry.name;
  const effectiveCapability: OperationCapability = {
    type: parsed.type,
    operationName,
    domain: registryEntry.domain,
    execution: registryEntry.execution,
  };
  const interpreted = interpretMutationLogEntry(parsed, effectiveCapability);

  if (!registryEntry.implemented) {
    interpreted.registeredOperation = registeredOperationMetadata(registryEntry);
  }

  return { operationName, interpreted };
}

function buildUnsupportedMutationObservability(parsed: ParsedOperation): Partial<MutationLogInterpretedMetadata> {
  const registryEntry = findOperationRegistryEntry(parsed.type, [...parsed.rootFields, parsed.name]);
  if (!registryEntry || registryEntry.implemented) {
    return {};
  }

  const primaryRootField = parsed.rootFields[0] ?? registryEntry.name;
  const registeredOperation = registeredOperationMetadata(registryEntry);

  if (APP_DISCOUNT_MUTATION_ROOTS.has(primaryRootField)) {
    return {
      registeredOperation,
      safety: {
        classification: 'unsupported-app-discount-function-mutation',
        wouldProxyToShopify: true,
        reason:
          'App-managed discount mutations are backed by Shopify Functions and external function IDs; local staging requires captured fixtures and an explicit model that does not execute external Function logic at runtime.',
      },
    };
  }

  return { registeredOperation };
}

function unsupportedMutationNotes(parsed: ParsedOperation): string {
  const primaryRootField = parsed.rootFields[0] ?? null;
  if (primaryRootField && APP_DISCOUNT_MUTATION_ROOTS.has(primaryRootField)) {
    return 'Unsupported app-managed discount mutation would be proxied to Shopify. Shopify Functions app-discount roots require conformance-backed local staging before they can be supported without executing external Function logic.';
  }

  const registryEntry = findOperationRegistryEntry(parsed.type, [...parsed.rootFields, parsed.name]);
  if (registryEntry?.domain === 'discounts') {
    return 'Unsupported discount mutation lifecycle branch would be proxied to Shopify. Captured validation failures are handled locally only; full local emulation is required before this root can be supported.';
  }

  return 'Mutation passthrough placeholder until supported local staging is implemented.';
}

function isProductLocalMutationCapability(capability: OperationCapability): boolean {
  return (
    capability.execution === 'stage-locally' &&
    (capability.domain === 'products' ||
      (capability.domain === 'store-properties' && capability.operationName?.startsWith('publishable') === true))
  );
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

    if (parsed.type === 'mutation') {
      const discountMutation = handleDiscountMutation(body.query, variables);
      if (discountMutation) {
        proxyLogger.debug(
          {
            operationName: capability.operationName,
            operationType: parsed.type,
            rootFields: parsed.rootFields,
          },
          discountMutation.staged
            ? 'staging supported discount mutation locally'
            : 'returning captured discount validation response locally',
        );

        if (discountMutation.staged) {
          const discountLogMetadata = buildLocalDiscountMutationLogMetadata(parsed, capability);
          store.appendLog({
            id: makeSyntheticGid('MutationLogEntry'),
            receivedAt: makeSyntheticTimestamp(),
            operationName: discountLogMetadata.operationName,
            path: ctx.path,
            query: body.query,
            variables,
            requestBody,
            stagedResourceIds: discountMutation.stagedResourceIds,
            status: 'staged',
            interpreted: discountLogMetadata.interpreted,
            ...(discountMutation.notes ? { notes: discountMutation.notes } : {}),
          });
        }

        ctx.status = 200;
        ctx.body = discountMutation.response;
        return;
      }
    }

    if (isProductLocalMutationCapability(capability)) {
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

    if (
      capability.execution === 'stage-locally' &&
      capability.domain === 'store-properties' &&
      (primaryRootField === 'publishablePublish' || primaryRootField === 'publishableUnpublish')
    ) {
      proxyLogger.debug(
        {
          execution: capability.execution,
          operationName: capability.operationName,
          operationType: parsed.type,
          rootFields: parsed.rootFields,
        },
        'staging supported publishable mutation locally',
      );

      const responseBody = handleProductMutation(body.query, variables, config.readMode);

      store.appendLog({
        id: makeSyntheticGid('MutationLogEntry'),
        receivedAt: makeSyntheticTimestamp(),
        operationName: capability.operationName,
        path: ctx.path,
        query: body.query,
        variables,
        requestBody,
        stagedResourceIds: collectProxySyntheticGids(responseBody),
        status: 'staged',
        interpreted: interpretMutationLogEntry(parsed, capability),
        notes: 'Staged locally in the in-memory publishable draft store.',
      });

      ctx.status = 200;
      ctx.body = responseBody;
      return;
    }

    if (
      capability.execution === 'stage-locally' &&
      capability.domain === 'shipping-fulfillments' &&
      primaryRootField &&
      (FULFILLMENT_SERVICE_MUTATION_ROOTS.has(primaryRootField) || CARRIER_SERVICE_MUTATION_ROOTS.has(primaryRootField))
    ) {
      const isCarrierServiceMutation = CARRIER_SERVICE_MUTATION_ROOTS.has(primaryRootField);
      proxyLogger.debug(
        {
          execution: capability.execution,
          operationName: capability.operationName,
          operationType: parsed.type,
          rootFields: parsed.rootFields,
        },
        isCarrierServiceMutation
          ? 'staging supported carrier service mutation locally'
          : 'staging supported fulfillment service mutation locally',
      );

      const responseBody = handleStorePropertiesMutation(body.query, variables);

      store.appendLog({
        id: makeSyntheticGid('MutationLogEntry'),
        receivedAt: makeSyntheticTimestamp(),
        operationName: capability.operationName,
        path: ctx.path,
        query: body.query,
        variables,
        requestBody,
        stagedResourceIds: collectProxySyntheticGids(responseBody),
        status: 'staged',
        interpreted: interpretMutationLogEntry(parsed, capability),
        notes: isCarrierServiceMutation
          ? 'Staged locally in the in-memory carrier service draft store; callback URL and service-discovery endpoints are not invoked.'
          : 'Staged locally in the in-memory fulfillment service draft store; callback, inventory, tracking, and fulfillment-order notification endpoints are not invoked.',
      });

      ctx.status = 200;
      ctx.body = responseBody;
      return;
    }

    if (
      capability.execution === 'stage-locally' &&
      capability.domain === 'shipping-fulfillments' &&
      primaryRootField &&
      DELIVERY_PROFILE_MUTATION_ROOTS.has(primaryRootField)
    ) {
      proxyLogger.debug(
        {
          execution: capability.execution,
          operationName: capability.operationName,
          operationType: parsed.type,
          rootFields: parsed.rootFields,
        },
        'staging supported delivery profile mutation locally',
      );

      const deliveryProfileMutation = handleDeliveryProfileMutation(body.query, variables);
      if (deliveryProfileMutation) {
        if (deliveryProfileMutation.staged) {
          store.appendLog({
            id: makeSyntheticGid('MutationLogEntry'),
            receivedAt: makeSyntheticTimestamp(),
            operationName: capability.operationName,
            path: ctx.path,
            query: body.query,
            variables,
            requestBody,
            stagedResourceIds: deliveryProfileMutation.stagedResourceIds,
            status: 'staged',
            interpreted: interpretMutationLogEntry(parsed, capability),
            notes: deliveryProfileMutation.notes,
          });
        }

        ctx.status = 200;
        ctx.body = deliveryProfileMutation.response;
        return;
      }
    }

    if (capability.execution === 'stage-locally' && capability.domain === 'customers') {
      const logEntryId = makeSyntheticGid('MutationLogEntry');
      const receivedAt = makeSyntheticTimestamp();
      const responseBody = handleCustomerMutation(body.query, variables);
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
        notes: 'Staged locally in the in-memory customer draft store.',
      });

      ctx.status = 200;
      ctx.body = responseBody;
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

    if (capability.execution === 'stage-locally' && capability.domain === 'metafields') {
      const responseBody = handleMetafieldDefinitionMutation(body.query, variables);

      store.appendLog({
        id: makeSyntheticGid('MutationLogEntry'),
        receivedAt: makeSyntheticTimestamp(),
        operationName: capability.operationName,
        path: ctx.path,
        query: body.query,
        variables,
        requestBody,
        stagedResourceIds: collectProxySyntheticGids(responseBody),
        status: 'staged',
        interpreted: interpretMutationLogEntry(parsed, capability),
        notes: 'Staged locally in the in-memory metafield definition draft store.',
      });

      ctx.status = 200;
      ctx.body = responseBody;
      return;
    }

    if (
      capability.execution === 'stage-locally' &&
      capability.domain === 'payments' &&
      ORDER_PAYMENT_MUTATION_ROOTS.has(primaryRootField ?? '')
    ) {
      const responseBody = handleOrderMutation(body.query, variables, config.readMode, config.shopifyAdminOrigin);
      if (responseBody) {
        store.appendLog({
          id: makeSyntheticGid('MutationLogEntry'),
          receivedAt: makeSyntheticTimestamp(),
          operationName: capability.operationName,
          path: ctx.path,
          query: body.query,
          variables,
          requestBody,
          stagedResourceIds: collectProxySyntheticGids(responseBody),
          status: 'staged',
          interpreted: interpretMutationLogEntry(parsed, capability),
          notes: 'Staged locally in the in-memory order payment draft store.',
        });

        ctx.status = 200;
        ctx.body = responseBody;
        return;
      }
    }

    if (
      capability.execution === 'stage-locally' &&
      capability.domain === 'payments' &&
      PAYMENT_CUSTOMIZATION_MUTATION_ROOTS.has(primaryRootField ?? '')
    ) {
      const responseBody = handlePaymentMutation(body.query, variables);
      store.appendLog({
        id: makeSyntheticGid('MutationLogEntry'),
        receivedAt: makeSyntheticTimestamp(),
        operationName: capability.operationName,
        path: ctx.path,
        query: body.query,
        variables,
        requestBody,
        stagedResourceIds: collectProxySyntheticGids(responseBody),
        status: 'staged',
        interpreted: interpretMutationLogEntry(parsed, capability),
        notes:
          'Staged locally in the in-memory payment customization draft store; Shopify Functions and checkout payment behavior are not invoked.',
      });

      ctx.status = 200;
      ctx.body = responseBody;
      return;
    }

    if (capability.execution === 'stage-locally' && capability.domain === 'markets') {
      const responseBody = handleMarketMutation(body.query, variables);

      store.appendLog({
        id: makeSyntheticGid('MutationLogEntry'),
        receivedAt: makeSyntheticTimestamp(),
        operationName: capability.operationName,
        path: ctx.path,
        query: body.query,
        variables,
        requestBody,
        stagedResourceIds: collectProxySyntheticGids(responseBody),
        status: 'staged',
        interpreted: interpretMutationLogEntry(parsed, capability),
        notes: 'Staged locally in the in-memory Markets draft store.',
      });

      ctx.status = 200;
      ctx.body = responseBody;
      return;
    }

    if (capability.execution === 'stage-locally' && capability.domain === 'segments') {
      const responseBody = handleSegmentMutation(body.query, variables);

      store.appendLog({
        id: makeSyntheticGid('MutationLogEntry'),
        receivedAt: makeSyntheticTimestamp(),
        operationName: capability.operationName,
        path: ctx.path,
        query: body.query,
        variables,
        requestBody,
        stagedResourceIds: collectProxySyntheticGids(responseBody),
        status: 'staged',
        interpreted: interpretMutationLogEntry(parsed, capability),
        notes: 'Staged locally in the in-memory segment draft store.',
      });

      ctx.status = 200;
      ctx.body = responseBody;
      return;
    }

    if (
      capability.execution === 'stage-locally' &&
      capability.domain === 'metafields' &&
      (primaryRootField === 'metafieldDefinitionPin' || primaryRootField === 'metafieldDefinitionUnpin')
    ) {
      const responseBody = handleMetafieldDefinitionMutation(body.query, variables);

      store.appendLog({
        id: makeSyntheticGid('MutationLogEntry'),
        receivedAt: makeSyntheticTimestamp(),
        operationName: capability.operationName,
        path: ctx.path,
        query: body.query,
        variables,
        requestBody,
        stagedResourceIds: collectProxySyntheticGids(responseBody),
        status: 'staged',
        interpreted: interpretMutationLogEntry(parsed, capability),
        notes: 'Staged locally in the in-memory metafield definition draft store.',
      });

      ctx.status = 200;
      ctx.body = responseBody;
      return;
    }

    if (
      capability.execution === 'stage-locally' &&
      capability.domain === 'store-properties' &&
      (primaryRootField === 'shopPolicyUpdate' ||
        primaryRootField === 'locationAdd' ||
        primaryRootField === 'locationEdit' ||
        primaryRootField === 'locationActivate' ||
        primaryRootField === 'locationDeactivate' ||
        primaryRootField === 'locationDelete')
    ) {
      proxyLogger.debug(
        {
          execution: capability.execution,
          operationName: capability.operationName,
          operationType: parsed.type,
          rootFields: parsed.rootFields,
        },
        'staging supported store properties mutation locally',
      );

      const responseBody = handleStorePropertiesMutation(body.query, variables);

      store.appendLog({
        id: makeSyntheticGid('MutationLogEntry'),
        receivedAt: makeSyntheticTimestamp(),
        operationName: capability.operationName,
        path: ctx.path,
        query: body.query,
        variables,
        requestBody,
        stagedResourceIds: collectProxySyntheticGids(responseBody),
        status: 'staged',
        interpreted: interpretMutationLogEntry(parsed, capability),
        notes:
          primaryRootField === 'shopPolicyUpdate'
            ? 'Staged locally in the in-memory Store properties legal policy draft store.'
            : 'Staged locally in the in-memory Store properties location draft store.',
      });

      ctx.status = 200;
      ctx.body = responseBody;
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
        ctx.body =
          store.hasBaseCustomers() || store.hasStagedCustomers()
            ? handleCustomerQuery(body.query, variables)
            : upstreamBody;
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

    if (capability.execution === 'overlay-read' && capability.domain === 'discounts') {
      if (config.readMode === 'snapshot') {
        ctx.status = 200;
        ctx.body = handleDiscountQuery(body.query, variables);
        return;
      }

      if (config.readMode === 'live-hybrid' && store.hasDiscounts()) {
        ctx.status = 200;
        ctx.body = handleDiscountQuery(body.query, variables);
        return;
      }
    }

    if (capability.execution === 'overlay-read' && capability.domain === 'store-properties') {
      if (config.readMode === 'snapshot') {
        ctx.status = 200;
        ctx.body = handleStorePropertiesQuery(body.query, variables);
        return;
      }

      if (config.readMode === 'live-hybrid') {
        if (primaryRootField === 'shop' && store.getEffectiveShop() !== null) {
          ctx.status = 200;
          ctx.body = handleStorePropertiesQuery(body.query, variables);
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

    if (capability.execution === 'overlay-read' && capability.domain === 'shipping-fulfillments') {
      const orderBackedFulfillmentRoots = new Set([
        'fulfillment',
        'fulfillmentOrder',
        'fulfillmentOrders',
        'assignedFulfillmentOrders',
        'manualHoldsFulfillmentOrders',
      ]);

      if (config.readMode === 'snapshot') {
        if (primaryRootField === 'deliveryProfile' || primaryRootField === 'deliveryProfiles') {
          ctx.status = 200;
          ctx.body = handleDeliveryProfileQuery(body.query, variables);
          return;
        }

        ctx.status = 200;
        ctx.body =
          primaryRootField !== null && orderBackedFulfillmentRoots.has(primaryRootField)
            ? handleOrderQuery(body.query, variables)
            : handleStorePropertiesQuery(body.query, variables);
        return;
      }

      if (
        config.readMode === 'live-hybrid' &&
        primaryRootField !== null &&
        orderBackedFulfillmentRoots.has(primaryRootField)
      ) {
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

      if (
        config.readMode === 'live-hybrid' &&
        ((primaryRootField === 'fulfillmentService' &&
          typeof variables['id'] === 'string' &&
          store.getEffectiveFulfillmentServiceById(variables['id']) !== null) ||
          (primaryRootField === 'carrierService' &&
            typeof variables['id'] === 'string' &&
            store.getEffectiveCarrierServiceById(variables['id']) !== null) ||
          (primaryRootField === 'carrierServices' && store.hasStagedCarrierServices()))
      ) {
        ctx.status = 200;
        ctx.body = handleStorePropertiesQuery(body.query, variables);
        return;
      }

      if (
        config.readMode === 'live-hybrid' &&
        (primaryRootField === 'deliveryProfile' || primaryRootField === 'deliveryProfiles') &&
        store.hasStagedDeliveryProfiles()
      ) {
        ctx.status = 200;
        ctx.body = handleDeliveryProfileQuery(body.query, variables);
        return;
      }
    }

    if (
      capability.execution === 'overlay-read' &&
      capability.domain === 'payments' &&
      primaryRootField === 'shopifyPaymentsAccount'
    ) {
      if (config.readMode === 'snapshot') {
        ctx.status = 200;
        ctx.body = handleStorePropertiesQuery(body.query, variables);
        return;
      }

      if (config.readMode === 'live-hybrid') {
        const primaryBusinessEntity = store.getPrimaryBusinessEntity();
        const hasLocalShopifyPaymentsAccount =
          Boolean(primaryBusinessEntity?.shopifyPaymentsAccount) ||
          store
            .listEffectiveBusinessEntities()
            .some((businessEntity) => businessEntity.shopifyPaymentsAccount !== null);

        if (hasLocalShopifyPaymentsAccount) {
          ctx.status = 200;
          ctx.body = handleStorePropertiesQuery(body.query, variables);
          return;
        }
      }
    }

    if (capability.execution === 'overlay-read' && capability.domain === 'markets') {
      if (config.readMode === 'snapshot') {
        ctx.status = 200;
        ctx.body = handleMarketsQuery(body.query, variables);
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
        hydrateMarketsFromUpstreamResponse(body.query, variables, upstreamBody);

        ctx.status = response.status;
        ctx.body = store.hasStagedMarkets() ? handleMarketsQuery(body.query, variables) : upstreamBody;
        return;
      }
    }

    if (capability.execution === 'overlay-read' && capability.domain === 'metafields') {
      if (config.readMode === 'snapshot') {
        ctx.status = 200;
        ctx.body = handleMetafieldDefinitionQuery(body.query, variables);
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

    if (capability.execution === 'overlay-read' && capability.domain === 'metaobjects') {
      if (config.readMode === 'snapshot') {
        ctx.status = 200;
        ctx.body = handleMetaobjectDefinitionQuery(body.query, variables);
        return;
      }

      if (config.readMode === 'live-hybrid') {
        const hadLocalDefinitions = store.hasEffectiveMetaobjectDefinitions();
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
        hydrateMetaobjectDefinitionsFromUpstreamResponse(body.query, variables, upstreamBody);

        ctx.status = response.status;
        ctx.body =
          store.hasEffectiveMetaobjectDefinitions() && (hadLocalDefinitions || store.hasStagedMetaobjectDefinitions())
            ? handleMetaobjectDefinitionQuery(body.query, variables)
            : upstreamBody;
        return;
      }
    }

    if (capability.execution === 'overlay-read' && capability.domain === 'payments') {
      if (config.readMode === 'snapshot') {
        ctx.status = 200;
        ctx.body = handlePaymentQuery(body.query, variables);
        return;
      }

      if (config.readMode === 'live-hybrid' && store.hasPaymentCustomizations()) {
        ctx.status = 200;
        ctx.body = handlePaymentQuery(body.query, variables);
        return;
      }
    }

    if (capability.execution === 'overlay-read' && capability.domain === 'segments') {
      if (config.readMode === 'snapshot') {
        ctx.status = 200;
        ctx.body = handleSegmentsQuery(body.query, variables);
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
        hydrateSegmentsFromUpstreamResponse(body.query, variables, upstreamBody);

        ctx.status = response.status;
        ctx.body = store.hasStagedSegments() ? handleSegmentsQuery(body.query, variables) : upstreamBody;
        return;
      }
    }

    if (capability.execution === 'overlay-read' && capability.domain === 'marketing') {
      if (config.readMode === 'snapshot') {
        ctx.status = 200;
        ctx.body = handleMarketingQuery(body.query, variables);
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
        hydrateMarketingFromUpstreamResponse(body.query, variables, upstreamBody);

        ctx.status = response.status;
        ctx.body = upstreamBody;
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
      const unsupportedObservability = buildUnsupportedMutationObservability(parsed);
      proxyLogger.warn(
        {
          execution: capability.execution,
          operationName: capability.operationName,
          operationType: parsed.type,
          rootFields: parsed.rootFields,
          registeredOperation: unsupportedObservability.registeredOperation,
          safety: unsupportedObservability.safety,
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
        interpreted: {
          ...interpretMutationLogEntry(parsed, capability),
          ...unsupportedObservability,
        },
        notes: unsupportedMutationNotes(parsed),
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
