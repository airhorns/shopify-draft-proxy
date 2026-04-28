import Router from '@koa/router';
import type Koa from 'koa';
import { logger } from '../logger.js';
import { parseOperation, type ParsedOperation } from '../graphql/parse-operation.js';
import { isProxySyntheticGid, makeSyntheticGid, makeSyntheticTimestamp } from '../state/synthetic-identity.js';
import { store } from '../state/store.js';
import type { MutationLogEntry, MutationLogInterpretedMetadata } from '../state/types.js';
import type { AppConfig } from '../config.js';
import { createUpstreamGraphQLClient } from '../shopify/upstream-client.js';
import { requestUpstreamGraphQL } from '../shopify/upstream-request.js';
import {
  ADMIN_PLATFORM_MUTATION_ROOTS,
  ADMIN_PLATFORM_QUERY_ROOTS,
  FLOW_UTILITY_MUTATION_ROOTS,
  handleAdminPlatformMutation,
  handleAdminPlatformQuery,
} from './admin-platform.js';
import {
  APP_MUTATION_ROOTS,
  APP_QUERY_ROOTS,
  handleAppMutation,
  handleAppQuery,
  hydrateAppsFromUpstreamResponse,
} from './apps.js';
import { handleB2BMutation, handleB2BQuery } from './b2b.js';
import {
  handleBulkOperationMutation,
  handleBulkOperationQuery,
  type BulkOperationImportLogEntry,
} from './bulk-operations.js';
import { getOperationCapability, type OperationCapability } from './capabilities.js';
import { findOperationRegistryEntry } from './operation-registry.js';
import { handleMediaMutation, handleMediaQuery } from './media.js';
import {
  handleMarketingMutation,
  handleMarketingQuery,
  hydrateMarketingFromUpstreamResponse,
  MARKETING_MUTATION_ROOTS,
} from './marketing.js';
import { handleCustomerMutation, handleCustomerQuery, hydrateCustomersFromUpstreamResponse } from './customers.js';
import { handleDeliverySettingsQuery } from './delivery-settings.js';
import { handleDeliveryProfileMutation, handleDeliveryProfileQuery } from './delivery-profiles.js';
import { handleDiscountMutation, handleDiscountQuery } from './discounts.js';
import { handleEventsQuery } from './events.js';
import { handleInventoryShipmentMutation, handleInventoryShipmentQuery } from './inventory-shipments.js';
import {
  FUNCTION_MUTATION_ROOTS,
  FUNCTION_QUERY_ROOTS,
  handleFunctionMutation,
  handleFunctionQuery,
} from './functions.js';
import { handleGiftCardMutation, handleGiftCardQuery } from './gift-cards.js';
import { handleMarketMutation, handleMarketsQuery, hydrateMarketsFromUpstreamResponse } from './markets.js';
import {
  handleLocalizationMutation,
  handleLocalizationQuery,
  hydrateLocalizationFromUpstreamResponse,
} from './localization.js';
import { handleOrderMutation, handleOrderQuery, shouldServeDraftOrderCatalogLocally } from './orders.js';
import {
  handleOnlineStoreMutation,
  handleOnlineStoreQuery,
  hydrateOnlineStoreFromUpstreamResponse,
  isOnlineStoreContentQueryRoot,
} from './online-store.js';
import { handleProductMutation, handleProductQuery, hydrateProductsFromUpstreamResponse } from './products.js';
import {
  handleSavedSearchMutation,
  handleSavedSearchQuery,
  hydrateSavedSearchesFromUpstreamResponse,
  isSavedSearchQueryRoot,
} from './saved-searches.js';
import { handleMetafieldDefinitionMutation, handleMetafieldDefinitionQuery } from './metafield-definitions.js';
import {
  handleMetaobjectDefinitionMutation,
  handleMetaobjectQuery,
  hydrateMetaobjectsFromUpstreamResponse,
} from './metaobject-definitions.js';
import { handlePaymentMutation, handlePaymentQuery } from './payments.js';
import { handleSegmentMutation, handleSegmentsQuery, hydrateSegmentsFromUpstreamResponse } from './segments.js';
import { handleStorePropertiesMutation, handleStorePropertiesQuery } from './store-properties.js';
import {
  handleWebhookSubscriptionMutation,
  handleWebhookSubscriptionQuery,
  hydrateWebhookSubscriptionsFromUpstreamResponse,
} from './webhooks.js';

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

const APP_BILLING_ACCESS_MUTATION_ROOTS = new Set([
  'appPurchaseOneTimeCreate',
  'appSubscriptionCreate',
  'appSubscriptionCancel',
  'appSubscriptionLineItemUpdate',
  'appSubscriptionTrialExtend',
  'appUsageRecordCreate',
  'appRevokeAccessScopes',
  'appUninstall',
  'delegateAccessTokenCreate',
  'delegateAccessTokenDestroy',
]);

const ORDER_PAYMENT_MUTATION_ROOTS = new Set(['orderCapture', 'transactionVoid', 'orderCreateMandatePayment']);
const PAYMENT_TERMS_MUTATION_ROOTS = new Set(['paymentTermsCreate', 'paymentTermsUpdate', 'paymentTermsDelete']);
const ORDER_ACCESS_DENIED_GUARDRAIL_MUTATION_ROOTS = new Set(['orderCreateManualPayment', 'taxSummaryCreate']);
const ORDER_RETURN_MUTATION_ROOTS = new Set([
  'returnCreate',
  'returnRequest',
  'returnApproveRequest',
  'returnDeclineRequest',
  'returnCancel',
  'returnClose',
  'returnReopen',
  'removeFromReturn',
  'returnProcess',
  'reverseDeliveryCreateWithShipping',
  'reverseDeliveryShippingUpdate',
  'reverseFulfillmentOrderDispose',
]);
const ORDER_BACKED_REVERSE_LOGISTICS_QUERY_ROOTS = new Set(['reverseDelivery', 'reverseFulfillmentOrder']);
const DRAFT_ORDER_LOCAL_HELPER_QUERY_ROOTS = new Set(['draftOrderAvailableDeliveryOptions', 'draftOrderTag']);
const NO_LOG_ERROR_MUTATION_ROOTS = new Set([
  'orderCapture',
  'transactionVoid',
  'orderCreateMandatePayment',
  'orderClose',
  'orderOpen',
  'orderMarkAsPaid',
  'orderCreateManualPayment',
  'orderCustomerSet',
  'orderCustomerRemove',
  'taxSummaryCreate',
  'orderCancel',
  'orderDelete',
  'returnCreate',
  'returnRequest',
  'returnApproveRequest',
  'returnDeclineRequest',
  'returnCancel',
  'returnClose',
  'returnReopen',
  'removeFromReturn',
  'returnProcess',
  'reverseDeliveryCreateWithShipping',
  'reverseDeliveryShippingUpdate',
  'reverseFulfillmentOrderDispose',
  'paymentTermsCreate',
  'paymentTermsUpdate',
  'paymentTermsDelete',
]);

const PAYMENT_CUSTOMIZATION_MUTATION_ROOTS = new Set([
  'customerPaymentMethodCreateFromDuplicationData',
  'customerPaymentMethodCreditCardCreate',
  'customerPaymentMethodCreditCardUpdate',
  'customerPaymentMethodGetDuplicationData',
  'customerPaymentMethodGetUpdateUrl',
  'customerPaymentMethodPaypalBillingAgreementCreate',
  'customerPaymentMethodPaypalBillingAgreementUpdate',
  'customerPaymentMethodRemoteCreate',
  'customerPaymentMethodRevoke',
  'paymentCustomizationActivation',
  'paymentCustomizationCreate',
  'paymentCustomizationDelete',
  'paymentCustomizationUpdate',
  'paymentReminderSend',
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

const DELIVERY_FUNCTION_MUTATION_ROOTS = new Set([
  'deliveryCustomizationActivation',
  'deliveryCustomizationCreate',
  'deliveryCustomizationDelete',
  'deliveryCustomizationUpdate',
]);

const DELIVERY_PROMISE_MUTATION_ROOTS = new Set(['deliveryPromiseParticipantsUpdate', 'deliveryPromiseProviderUpsert']);

const DELIVERY_SETTINGS_MUTATION_ROOTS = new Set(['deliverySettingUpdate']);

const CARRIER_SERVICE_MUTATION_ROOTS = new Set([
  'carrierServiceCreate',
  'carrierServiceUpdate',
  'carrierServiceDelete',
]);

const SHIPPING_SETTINGS_MUTATION_ROOTS = new Set([
  'locationLocalPickupEnable',
  'locationLocalPickupDisable',
  'shippingPackageUpdate',
  'shippingPackageMakeDefault',
  'shippingPackageDelete',
]);

const FULFILLMENT_ORDER_LIFECYCLE_MUTATION_ROOTS = new Set([
  'fulfillmentOrderHold',
  'fulfillmentOrderReleaseHold',
  'fulfillmentOrderMove',
  'fulfillmentOrderOpen',
  'fulfillmentOrderCancel',
  'fulfillmentOrderReportProgress',
  'fulfillmentOrderReschedule',
  'fulfillmentOrderClose',
  'fulfillmentOrderMerge',
  'fulfillmentOrderSplit',
  'fulfillmentOrdersReroute',
  'fulfillmentOrdersSetFulfillmentDeadline',
]);

const PRODUCT_FEED_QUERY_ROOTS = new Set(['productFeed', 'productFeeds']);

const INVENTORY_SHIPMENT_MUTATION_ROOTS = new Set([
  'inventoryShipmentCreate',
  'inventoryShipmentCreateInTransit',
  'inventoryShipmentAddItems',
  'inventoryShipmentRemoveItems',
  'inventoryShipmentUpdateItemQuantities',
  'inventoryShipmentSetTracking',
  'inventoryShipmentMarkInTransit',
  'inventoryShipmentReceive',
  'inventoryShipmentDelete',
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

function hasLocalMutationErrors(value: unknown): boolean {
  if (!value || typeof value !== 'object') {
    return false;
  }

  if (Array.isArray(value)) {
    return value.some((item) => hasLocalMutationErrors(item));
  }

  for (const [key, child] of Object.entries(value)) {
    if (key === 'errors' && Array.isArray(child) && child.length > 0) {
      return true;
    }

    if ((key === 'userErrors' || key.endsWith('UserErrors')) && Array.isArray(child) && child.length > 0) {
      return true;
    }

    if (hasLocalMutationErrors(child)) {
      return true;
    }
  }

  return false;
}

function shouldAppendLocalMutationLog(primaryRootField: string | null | undefined, responseBody: unknown): boolean {
  if (isRejectedCreateMutationResponse(primaryRootField, responseBody)) {
    return false;
  }

  if (!hasLocalMutationErrors(responseBody)) {
    return true;
  }

  return !NO_LOG_ERROR_MUTATION_ROOTS.has(primaryRootField ?? '');
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

  if (APP_BILLING_ACCESS_MUTATION_ROOTS.has(primaryRootField)) {
    return {
      registeredOperation,
      safety: {
        classification: 'unsupported-app-billing-access-mutation',
        wouldProxyToShopify: true,
        reason:
          'App billing, access-scope, delegated-token, and uninstall mutations can alter merchant billing, installation state, or app access; local staging and commit replay semantics are required before support.',
      },
    };
  }

  if (DELIVERY_FUNCTION_MUTATION_ROOTS.has(primaryRootField)) {
    return {
      registeredOperation,
      safety: {
        classification: 'unsupported-delivery-customization-function-mutation',
        wouldProxyToShopify: true,
        reason:
          'Delivery customization mutations are backed by Shopify Functions and external function IDs; local staging requires captured function ownership, validation, activation, metafield, and downstream read behavior before support.',
      },
    };
  }

  if (DELIVERY_PROMISE_MUTATION_ROOTS.has(primaryRootField)) {
    return {
      registeredOperation,
      safety: {
        classification: 'unsupported-delivery-promise-mutation',
        wouldProxyToShopify: true,
        reason:
          'Delivery promise mutations require delivery-promise access scopes, location/owner eligibility, provider state, and participant read-after-write semantics before they can be staged locally.',
      },
    };
  }

  if (DELIVERY_SETTINGS_MUTATION_ROOTS.has(primaryRootField)) {
    return {
      registeredOperation,
      safety: {
        classification: 'unsupported-delivery-settings-mutation',
        wouldProxyToShopify: true,
        reason:
          'Delivery setting mutations alter shop delivery configuration and legacy-mode behavior; local staging needs conformance-backed setting transitions and downstream delivery read effects before support.',
      },
    };
  }

  if (primaryRootField && FLOW_UTILITY_MUTATION_ROOTS.has(primaryRootField)) {
    return {
      registeredOperation,
      safety: {
        classification: 'unsupported-flow-side-effect-mutation',
        wouldProxyToShopify: true,
        reason:
          'Flow utility mutations can generate signatures or deliver Flow triggers; local signing/trigger semantics and commit replay are required before support.',
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

  if (primaryRootField && APP_BILLING_ACCESS_MUTATION_ROOTS.has(primaryRootField)) {
    return 'Unsupported app billing/access mutation would be proxied to Shopify. These roots can alter merchant billing, installation state, app scopes, or delegated tokens and require conformance-backed local staging plus raw commit replay before support.';
  }

  if (primaryRootField && DELIVERY_FUNCTION_MUTATION_ROOTS.has(primaryRootField)) {
    return 'Unsupported delivery customization mutation would be proxied to Shopify. Delivery customizations are Shopify Function-backed and require conformance-backed local staging for function ownership, activation, metafields, and downstream reads before support.';
  }

  if (primaryRootField && DELIVERY_PROMISE_MUTATION_ROOTS.has(primaryRootField)) {
    return 'Unsupported delivery promise mutation would be proxied to Shopify. Delivery promise provider and participant roots require delivery-promise scope evidence, owner/location eligibility, and local read-after-write modeling before support.';
  }

  if (primaryRootField && DELIVERY_SETTINGS_MUTATION_ROOTS.has(primaryRootField)) {
    return 'Unsupported delivery settings mutation would be proxied to Shopify. Delivery setting changes alter shop delivery configuration and require conformance-backed transition and downstream read modeling before support.';
  }

  if (primaryRootField && FLOW_UTILITY_MUTATION_ROOTS.has(primaryRootField)) {
    return 'Unsupported Flow utility mutation would be proxied to Shopify. Flow signature generation and trigger delivery require local signing/trigger semantics plus raw commit replay before support.';
  }

  const registryEntry = findOperationRegistryEntry(parsed.type, [...parsed.rootFields, parsed.name]);
  if (registryEntry?.domain === 'discounts') {
    return 'Unsupported discount mutation lifecycle branch would be proxied to Shopify. Captured validation failures are handled locally only; full local emulation is required before this root can be supported.';
  }

  return 'Mutation passthrough placeholder until supported local staging is implemented.';
}

function hasOwnKey(value: unknown, key: string): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && key in value;
}

function hasSelectedUserErrors(payload: unknown): boolean {
  return hasOwnKey(payload, 'userErrors') && Array.isArray(payload['userErrors']) && payload['userErrors'].length > 0;
}

function isRejectedCreateMutationResponse(rootField: string | null | undefined, responseBody: unknown): boolean {
  if (rootField !== 'orderCreate' && rootField !== 'draftOrderCreate') {
    return false;
  }

  if (hasOwnKey(responseBody, 'errors') && Array.isArray(responseBody['errors']) && responseBody['errors'].length > 0) {
    return true;
  }

  const data = hasOwnKey(responseBody, 'data') ? responseBody['data'] : null;
  const payload = hasOwnKey(data, rootField) ? data[rootField] : null;
  if (hasSelectedUserErrors(payload)) {
    return true;
  }

  const payloads = typeof data === 'object' && data !== null ? Object.values(data) : [];
  return payloads.length > 0 && payloads.every(hasSelectedUserErrors);
}

function isProductLocalMutationCapability(capability: OperationCapability): boolean {
  return (
    capability.execution === 'stage-locally' &&
    (capability.domain === 'products' ||
      (capability.domain === 'store-properties' && capability.operationName?.startsWith('publishable') === true))
  );
}

type ProxyLogger = Pick<typeof logger, 'debug'>;
type UpstreamGraphQLClient = ReturnType<typeof createUpstreamGraphQLClient>;

interface ProxyDispatchRequest {
  ctx: Koa.Context;
  body: { query: string };
  variables: Record<string, unknown>;
  requestBody: Record<string, unknown>;
  parsed: ParsedOperation;
  capability: OperationCapability;
  primaryRootField: string | null;
  apiVersion: string | null;
  config: AppConfig;
  upstream: UpstreamGraphQLClient;
  proxyLogger: ProxyLogger;
}

interface DomainDispatcher {
  name: string;
  canHandle(request: ProxyDispatchRequest): boolean;
  handleMutation?(request: ProxyDispatchRequest): boolean | Promise<boolean>;
  handleQuery?(request: ProxyDispatchRequest): boolean | Promise<boolean>;
}

interface StagedMutationLogOptions {
  id?: string | undefined;
  receivedAt?: string | undefined;
  operationName?: string | null | undefined;
  responseBody?: unknown;
  stagedResourceIds?: string[] | undefined;
  status?: MutationLogEntry['status'] | undefined;
  interpreted?: MutationLogInterpretedMetadata | undefined;
  notes?: string | null | undefined;
}

const ORDER_BACKED_FULFILLMENT_QUERY_ROOTS = new Set([
  'fulfillment',
  'fulfillmentOrder',
  'fulfillmentOrders',
  'assignedFulfillmentOrders',
  'manualHoldsFulfillmentOrders',
]);

const ORDER_BACKED_LOCAL_FULFILLMENT_MUTATION_ROOTS = new Set([
  'fulfillmentEventCreate',
  'fulfillmentOrderSubmitFulfillmentRequest',
  'fulfillmentOrderAcceptFulfillmentRequest',
  'fulfillmentOrderRejectFulfillmentRequest',
  'fulfillmentOrderSubmitCancellationRequest',
  'fulfillmentOrderAcceptCancellationRequest',
  'fulfillmentOrderRejectCancellationRequest',
]);

const ORDER_EDIT_SHIPPING_MUTATION_ROOTS = new Set([
  'orderEditAddShippingLine',
  'orderEditRemoveShippingLine',
  'orderEditUpdateShippingLine',
]);

const LIVE_HYBRID_LOCAL_ORDER_MUTATION_ROOTS = new Set([
  'orderCreate',
  'refundCreate',
  'orderUpdate',
  'orderClose',
  'orderOpen',
  'orderMarkAsPaid',
  'orderCustomerSet',
  'orderCustomerRemove',
  'orderInvoiceSend',
  'orderCancel',
  'orderDelete',
  'orderEditBegin',
  'orderEditAddVariant',
  'orderEditAddCustomItem',
  'orderEditAddLineItemDiscount',
  'orderEditAddShippingLine',
  'orderEditRemoveDiscount',
  'orderEditRemoveShippingLine',
  'orderEditSetQuantity',
  'orderEditUpdateShippingLine',
  'orderEditCommit',
  'draftOrderComplete',
  'draftOrderUpdate',
  'draftOrderDuplicate',
  'draftOrderDelete',
  'draftOrderBulkAddTags',
  'draftOrderBulkRemoveTags',
  'draftOrderBulkDelete',
  'draftOrderCalculate',
  'draftOrderInvoicePreview',
  'draftOrderInvoiceSend',
  'draftOrderCreateFromOrder',
  'abandonmentUpdateActivitiesDeliveryStatuses',
  'fulfillmentCreate',
  'fulfillmentTrackingInfoUpdate',
  'fulfillmentCancel',
]);

const LIVE_HYBRID_ORDER_MUTATION_NOTES: Record<string, string> = {
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
  orderDelete: 'Locally staged orderDelete in live-hybrid mode for a synthetic/local order.',
  orderEditBegin:
    'Locally staged the first calculated-order edit session in live-hybrid mode for a synthetic/local order.',
  orderEditAddVariant: 'Locally staged a calculated-order variant add in live-hybrid mode for a synthetic/local order.',
  orderEditAddCustomItem:
    'Locally staged a calculated-order custom item add in live-hybrid mode for a synthetic/local order.',
  orderEditAddLineItemDiscount:
    'Locally staged a calculated-order line-item discount in live-hybrid mode for a synthetic/local order.',
  orderEditAddShippingLine:
    'Locally staged a calculated-order shipping-line add in live-hybrid mode for a synthetic/local order.',
  orderEditRemoveDiscount:
    'Locally staged a calculated-order discount removal in live-hybrid mode for a synthetic/local order.',
  orderEditRemoveShippingLine:
    'Locally staged a calculated-order shipping-line removal in live-hybrid mode for a synthetic/local order.',
  orderEditSetQuantity:
    'Locally staged a calculated-order quantity edit in live-hybrid mode for a synthetic/local order.',
  orderEditUpdateShippingLine:
    'Locally staged a calculated-order shipping-line update in live-hybrid mode for a synthetic/local order.',
  orderEditCommit: 'Locally committed a calculated-order edit back onto a synthetic/local order in live-hybrid mode.',
  draftOrderComplete:
    'Locally handled draftOrderComplete in live-hybrid mode for captured validation branches or a synthetic/local staged draft order.',
  draftOrderUpdate: 'Locally staged draftOrderUpdate in live-hybrid mode for a synthetic/local staged draft order.',
  draftOrderDuplicate:
    'Locally staged draftOrderDuplicate in live-hybrid mode for a synthetic/local staged draft order.',
  draftOrderDelete: 'Locally staged draftOrderDelete in live-hybrid mode for a synthetic/local staged draft order.',
  draftOrderBulkAddTags:
    'Locally staged draftOrderBulkAddTags in live-hybrid mode for synthetic/local staged draft orders.',
  draftOrderBulkRemoveTags:
    'Locally staged draftOrderBulkRemoveTags in live-hybrid mode for synthetic/local staged draft orders.',
  draftOrderBulkDelete:
    'Locally staged draftOrderBulkDelete in live-hybrid mode for synthetic/local staged draft orders.',
  draftOrderCalculate: 'Locally calculated draftOrderCalculate in live-hybrid mode without writing to Shopify.',
  draftOrderInvoicePreview:
    'Locally handled draftOrderInvoicePreview in live-hybrid mode without sending invoice email.',
  draftOrderInvoiceSend: 'Locally handled draftOrderInvoiceSend in live-hybrid mode without sending invoice email.',
  draftOrderCreateFromOrder:
    'Locally staged draftOrderCreateFromOrder in live-hybrid mode for a synthetic/local order.',
  abandonmentUpdateActivitiesDeliveryStatuses:
    'Locally staged abandonmentUpdateActivitiesDeliveryStatuses in live-hybrid mode for a synthetic/local abandonment.',
  fulfillmentCreate: 'Locally short-circuited captured fulfillmentCreate validation in live-hybrid mode.',
  fulfillmentEventCreate:
    'Locally staged fulfillmentEventCreate in live-hybrid mode for an order-backed local fulfillment.',
  fulfillmentOrderSubmitFulfillmentRequest:
    'Locally staged fulfillment-order fulfillment request without invoking fulfillment-service callbacks.',
  fulfillmentOrderAcceptFulfillmentRequest:
    'Locally staged fulfillment-order fulfillment request acceptance without invoking fulfillment-service callbacks.',
  fulfillmentOrderRejectFulfillmentRequest:
    'Locally staged fulfillment-order fulfillment request rejection without invoking fulfillment-service callbacks.',
  fulfillmentOrderSubmitCancellationRequest:
    'Locally staged fulfillment-order cancellation request without invoking fulfillment-service callbacks.',
  fulfillmentOrderAcceptCancellationRequest:
    'Locally staged fulfillment-order cancellation request acceptance without invoking fulfillment-service callbacks.',
  fulfillmentOrderRejectCancellationRequest:
    'Locally staged fulfillment-order cancellation request rejection without invoking fulfillment-service callbacks.',
  returnCreate: 'Locally staged returnCreate in live-hybrid mode for a synthetic/local order.',
  returnRequest: 'Locally staged returnRequest in live-hybrid mode for a synthetic/local order.',
  returnCancel: 'Locally staged returnCancel in live-hybrid mode for a synthetic/local return.',
  returnClose: 'Locally staged returnClose in live-hybrid mode for a synthetic/local return.',
  returnReopen: 'Locally staged returnReopen in live-hybrid mode for a synthetic/local return.',
};

function setGraphQLResponse(request: ProxyDispatchRequest, status: number, responseBody: unknown): void {
  request.ctx.status = status;
  request.ctx.body = responseBody;
}

function appendStagedMutationLog(request: ProxyDispatchRequest, options: StagedMutationLogOptions): void {
  store.appendLog({
    id: options.id ?? makeSyntheticGid('MutationLogEntry'),
    receivedAt: options.receivedAt ?? makeSyntheticTimestamp(),
    operationName: options.operationName ?? request.capability.operationName,
    path: request.ctx.path,
    query: request.body.query,
    variables: request.variables,
    requestBody: request.requestBody,
    ...(options.stagedResourceIds !== undefined
      ? { stagedResourceIds: options.stagedResourceIds }
      : options.responseBody !== undefined
        ? { stagedResourceIds: collectProxySyntheticGids(options.responseBody) }
        : {}),
    status: options.status ?? 'staged',
    interpreted: options.interpreted ?? interpretMutationLogEntry(request.parsed, request.capability),
    ...(options.notes ? { notes: options.notes } : {}),
  });
}

function appendBulkOperationImportLog(request: ProxyDispatchRequest, entry: BulkOperationImportLogEntry): void {
  store.appendLog({
    id: makeSyntheticGid('MutationLogEntry'),
    receivedAt: makeSyntheticTimestamp(),
    operationName: entry.operationName,
    path: request.ctx.path,
    query: entry.query,
    variables: entry.variables,
    requestBody: entry.requestBody,
    stagedResourceIds: entry.stagedResourceIds,
    status: 'staged',
    interpreted: {
      operationType: 'mutation',
      operationName: entry.operationName,
      rootFields: [entry.rootField],
      primaryRootField: entry.rootField,
      capability: {
        operationName: entry.operationName,
        domain: entry.domain,
        execution: 'stage-locally',
      },
      bulkOperationImport: {
        bulkOperationId: entry.bulkOperationId,
        lineNumber: entry.lineNumber,
        stagedUploadPath: entry.stagedUploadPath,
        outerRequestBody: request.requestBody,
        innerMutation: entry.innerMutation,
      },
    },
    notes:
      'Staged locally from bulkOperationRunMutation JSONL import; commit replay uses this original inner mutation and line variables.',
  });
}

async function proxyUpstreamGraphQL(request: ProxyDispatchRequest): Promise<{ status: number; body: unknown }> {
  const response = await requestUpstreamGraphQL(request.upstream, request.ctx, {
    body: {
      query: request.body.query,
      variables: request.variables,
    },
  });

  return {
    status: response.status,
    body: await response.json(),
  };
}

function isOrderBackedLocalFulfillmentMutation(rootField: string | null): boolean {
  return rootField !== null && ORDER_BACKED_LOCAL_FULFILLMENT_MUTATION_ROOTS.has(rootField);
}

function isOrderEditShippingMutation(rootField: string | null): boolean {
  return rootField !== null && ORDER_EDIT_SHIPPING_MUTATION_ROOTS.has(rootField);
}

function isOrderBackedReverseLogisticsMutation(rootField: string | null): boolean {
  return rootField !== null && ORDER_RETURN_MUTATION_ROOTS.has(rootField);
}

function shouldTryLocalOrderMutation(request: ProxyDispatchRequest): boolean {
  return (
    request.capability.execution === 'stage-locally' &&
    (request.capability.domain === 'orders' ||
      (request.capability.domain === 'shipping-fulfillments' &&
        (isOrderBackedLocalFulfillmentMutation(request.primaryRootField) ||
          isOrderEditShippingMutation(request.primaryRootField) ||
          isOrderBackedReverseLogisticsMutation(request.primaryRootField))))
  );
}

const DOMAIN_DISPATCHERS: DomainDispatcher[] = [
  {
    name: 'apps',
    canHandle: (request) =>
      request.capability.domain === 'apps' ||
      (request.primaryRootField !== null &&
        (APP_QUERY_ROOTS.has(request.primaryRootField) || APP_MUTATION_ROOTS.has(request.primaryRootField))),
    async handleQuery(request) {
      if (request.capability.execution !== 'overlay-read' && !APP_QUERY_ROOTS.has(request.primaryRootField ?? '')) {
        return false;
      }

      if (request.config.readMode === 'snapshot' || store.hasAppDomainState()) {
        setGraphQLResponse(request, 200, handleAppQuery(request.body.query, request.variables));
        return true;
      }

      if (request.config.readMode === 'live-hybrid') {
        const upstreamResponse = await proxyUpstreamGraphQL(request);
        hydrateAppsFromUpstreamResponse(upstreamResponse.body);
        setGraphQLResponse(
          request,
          upstreamResponse.status,
          store.hasAppDomainState() ? handleAppQuery(request.body.query, request.variables) : upstreamResponse.body,
        );
        return true;
      }

      return false;
    },
    handleMutation(request) {
      if (
        request.primaryRootField === null ||
        !APP_MUTATION_ROOTS.has(request.primaryRootField) ||
        (request.capability.execution !== 'stage-locally' && request.capability.domain !== 'unknown')
      ) {
        return false;
      }

      const responseBody = handleAppMutation(request.body.query, request.variables, request.config.shopifyAdminOrigin);
      if (!responseBody) {
        return false;
      }

      appendStagedMutationLog(request, {
        responseBody,
        notes:
          'Staged locally in the in-memory app billing/access draft store; no billing, uninstall, scope, or delegated-token side effect was sent to Shopify at runtime.',
      });
      setGraphQLResponse(request, 200, responseBody);
      return true;
    },
  },
  {
    name: 'discounts',
    canHandle: (request) =>
      request.parsed.type === 'mutation' ||
      (request.capability.execution === 'overlay-read' && request.capability.domain === 'discounts'),
    handleMutation(request) {
      const discountMutation = handleDiscountMutation(request.body.query, request.variables);
      if (!discountMutation) {
        return false;
      }

      request.proxyLogger.debug(
        {
          operationName: request.capability.operationName,
          operationType: request.parsed.type,
          rootFields: request.parsed.rootFields,
        },
        discountMutation.staged
          ? 'staging supported discount mutation locally'
          : 'returning captured discount validation response locally',
      );

      if (discountMutation.staged) {
        const discountLogMetadata = buildLocalDiscountMutationLogMetadata(request.parsed, request.capability);
        appendStagedMutationLog(request, {
          operationName: discountLogMetadata.operationName,
          interpreted: discountLogMetadata.interpreted,
          stagedResourceIds: discountMutation.stagedResourceIds,
          notes: discountMutation.notes,
        });
      }

      setGraphQLResponse(request, 200, discountMutation.response);
      return true;
    },
    handleQuery(request) {
      if (request.config.readMode === 'snapshot') {
        setGraphQLResponse(request, 200, handleDiscountQuery(request.body.query, request.variables));
        return true;
      }

      if (request.config.readMode === 'live-hybrid' && store.hasDiscounts()) {
        setGraphQLResponse(request, 200, handleDiscountQuery(request.body.query, request.variables));
        return true;
      }

      return false;
    },
  },
  {
    name: 'bulk-operations',
    canHandle: (request) => request.capability.domain === 'bulk-operations',
    handleMutation(request) {
      if (
        request.capability.execution !== 'stage-locally' ||
        (request.primaryRootField !== 'bulkOperationCancel' &&
          request.primaryRootField !== 'bulkOperationRunQuery' &&
          request.primaryRootField !== 'bulkOperationRunMutation')
      ) {
        return false;
      }

      const bulkOperationMutation = handleBulkOperationMutation(request.body.query, request.variables, {
        readMode: request.config.readMode,
        shopifyAdminOrigin: request.config.shopifyAdminOrigin,
        apiVersion: request.apiVersion,
      });
      if (!bulkOperationMutation) {
        return false;
      }

      for (const entry of bulkOperationMutation.innerMutationLogs ?? []) {
        appendBulkOperationImportLog(request, entry);
      }

      if (request.primaryRootField === 'bulkOperationRunMutation') {
        if ((bulkOperationMutation.innerMutationLogs ?? []).length === 0) {
          appendStagedMutationLog(request, {
            stagedResourceIds: bulkOperationMutation.stagedResourceIds,
            status: 'failed',
            notes: bulkOperationMutation.notes,
          });
        }
      } else {
        appendStagedMutationLog(request, {
          stagedResourceIds: bulkOperationMutation.stagedResourceIds,
          notes: bulkOperationMutation.notes,
        });
      }
      setGraphQLResponse(request, 200, bulkOperationMutation.response);
      return true;
    },
    handleQuery(request) {
      if (request.capability.execution !== 'overlay-read') {
        return false;
      }

      if (
        request.config.readMode === 'snapshot' ||
        (request.config.readMode === 'live-hybrid' && store.hasBulkOperations())
      ) {
        setGraphQLResponse(request, 200, handleBulkOperationQuery(request.body.query, request.variables));
        return true;
      }

      return false;
    },
  },
  {
    name: 'products',
    canHandle: (request) =>
      isProductLocalMutationCapability(request.capability) ||
      (request.capability.execution === 'overlay-read' && request.capability.domain === 'products'),
    async handleQuery(request) {
      if (request.primaryRootField === 'inventoryShipment') {
        if (request.config.readMode === 'snapshot') {
          setGraphQLResponse(request, 200, handleInventoryShipmentQuery(request.body.query, request.variables));
          return true;
        }

        if (request.config.readMode === 'live-hybrid' && store.hasInventoryShipments()) {
          setGraphQLResponse(request, 200, handleInventoryShipmentQuery(request.body.query, request.variables));
          return true;
        }
      }

      if (request.config.readMode === 'snapshot') {
        setGraphQLResponse(
          request,
          200,
          handleProductQuery(request.body.query, request.variables, request.config.readMode),
        );
        return true;
      }

      const upstreamResponse = await proxyUpstreamGraphQL(request);

      if (
        request.primaryRootField &&
        PRODUCT_FEED_QUERY_ROOTS.has(request.primaryRootField) &&
        !store.hasStagedProducts()
      ) {
        setGraphQLResponse(request, upstreamResponse.status, upstreamResponse.body);
        return true;
      }

      hydrateProductsFromUpstreamResponse(request.body.query, request.variables, upstreamResponse.body);
      setGraphQLResponse(
        request,
        upstreamResponse.status,
        store.hasStagedProducts() ||
          store.hasStagedSellingPlanGroups() ||
          store.hasStagedInventoryTransfers() ||
          store.hasInventoryShipments()
          ? handleProductQuery(request.body.query, request.variables, request.config.readMode)
          : upstreamResponse.body,
      );
      return true;
    },
    handleMutation(request) {
      if (request.primaryRootField && INVENTORY_SHIPMENT_MUTATION_ROOTS.has(request.primaryRootField)) {
        const inventoryShipmentMutation = handleInventoryShipmentMutation(request.body.query, request.variables);
        if (!inventoryShipmentMutation) {
          return false;
        }

        if (inventoryShipmentMutation.staged) {
          appendStagedMutationLog(request, {
            stagedResourceIds: inventoryShipmentMutation.stagedResourceIds,
            notes: inventoryShipmentMutation.notes,
          });
        }

        setGraphQLResponse(request, 200, inventoryShipmentMutation.response);
        return true;
      }

      request.proxyLogger.debug(
        {
          execution: request.capability.execution,
          operationName: request.capability.operationName,
          operationType: request.parsed.type,
          rootFields: request.parsed.rootFields,
        },
        'staging supported mutation locally',
      );

      const logEntryId = makeSyntheticGid('MutationLogEntry');
      const receivedAt = makeSyntheticTimestamp();
      const responseBody = handleProductMutation(
        request.body.query,
        request.variables,
        request.config.readMode,
        request.apiVersion,
      );
      appendStagedMutationLog(request, {
        id: logEntryId,
        receivedAt,
        responseBody,
        notes: 'Staged locally in the in-memory product draft store.',
      });

      setGraphQLResponse(request, 200, responseBody);
      return true;
    },
  },
  {
    name: 'shipping-fulfillments',
    canHandle: (request) =>
      request.capability.domain === 'shipping-fulfillments' ||
      (request.primaryRootField !== null && FULFILLMENT_ORDER_LIFECYCLE_MUTATION_ROOTS.has(request.primaryRootField)),
    handleMutation(request) {
      if (request.primaryRootField && FULFILLMENT_ORDER_LIFECYCLE_MUTATION_ROOTS.has(request.primaryRootField)) {
        const responseBody = handleOrderMutation(
          request.body.query,
          request.variables,
          request.config.readMode,
          request.config.shopifyAdminOrigin,
        );
        if (!responseBody) {
          return false;
        }

        appendStagedMutationLog(request, {
          operationName: request.primaryRootField,
          responseBody,
          notes:
            request.primaryRootField === 'fulfillmentOrderReschedule' ||
            request.primaryRootField === 'fulfillmentOrderClose' ||
            request.primaryRootField === 'fulfillmentOrdersReroute'
              ? 'Returned a captured fulfillment-order lifecycle guardrail locally without proxying upstream; full local lifecycle support remains unimplemented for this root.'
              : 'Staged locally in the in-memory fulfillment-order lifecycle draft store.',
        });
        setGraphQLResponse(request, 200, responseBody);
        return true;
      }

      if (request.primaryRootField && isOrderEditShippingMutation(request.primaryRootField)) {
        const responseBody = handleOrderMutation(
          request.body.query,
          request.variables,
          request.config.readMode,
          request.config.shopifyAdminOrigin,
        );
        if (!responseBody) {
          return false;
        }

        if (shouldAppendLocalMutationLog(request.primaryRootField, responseBody)) {
          appendStagedMutationLog(request, {
            operationName: request.primaryRootField,
            responseBody,
            notes:
              LIVE_HYBRID_ORDER_MUTATION_NOTES[request.primaryRootField] ??
              'Staged locally in the in-memory calculated-order shipping-line draft store.',
          });
        }
        setGraphQLResponse(request, 200, responseBody);
        return true;
      }

      if (
        request.capability.execution === 'stage-locally' &&
        request.primaryRootField &&
        (FULFILLMENT_SERVICE_MUTATION_ROOTS.has(request.primaryRootField) ||
          CARRIER_SERVICE_MUTATION_ROOTS.has(request.primaryRootField) ||
          SHIPPING_SETTINGS_MUTATION_ROOTS.has(request.primaryRootField))
      ) {
        const isCarrierServiceMutation = CARRIER_SERVICE_MUTATION_ROOTS.has(request.primaryRootField);
        const isShippingSettingsMutation = SHIPPING_SETTINGS_MUTATION_ROOTS.has(request.primaryRootField);
        request.proxyLogger.debug(
          {
            execution: request.capability.execution,
            operationName: request.capability.operationName,
            operationType: request.parsed.type,
            rootFields: request.parsed.rootFields,
          },
          isShippingSettingsMutation
            ? 'staging supported shipping settings mutation locally'
            : isCarrierServiceMutation
              ? 'staging supported carrier service mutation locally'
              : 'staging supported fulfillment service mutation locally',
        );

        const responseBody = handleStorePropertiesMutation(request.body.query, request.variables);
        if (!isShippingSettingsMutation || !hasLocalMutationErrors(responseBody)) {
          appendStagedMutationLog(request, {
            responseBody,
            notes: isShippingSettingsMutation
              ? 'Staged locally in the in-memory shipping settings draft store; no Shopify delivery settings or package configuration are mutated at runtime.'
              : isCarrierServiceMutation
                ? 'Staged locally in the in-memory carrier service draft store; callback URL and service-discovery endpoints are not invoked.'
                : 'Staged locally in the in-memory fulfillment service draft store; callback, inventory, tracking, and fulfillment-order notification endpoints are not invoked.',
          });
        }
        setGraphQLResponse(request, 200, responseBody);
        return true;
      }

      if (
        request.capability.execution === 'stage-locally' &&
        request.primaryRootField &&
        DELIVERY_PROFILE_MUTATION_ROOTS.has(request.primaryRootField)
      ) {
        request.proxyLogger.debug(
          {
            execution: request.capability.execution,
            operationName: request.capability.operationName,
            operationType: request.parsed.type,
            rootFields: request.parsed.rootFields,
          },
          'staging supported delivery profile mutation locally',
        );

        const deliveryProfileMutation = handleDeliveryProfileMutation(request.body.query, request.variables);
        if (!deliveryProfileMutation) {
          return false;
        }

        if (deliveryProfileMutation.staged) {
          appendStagedMutationLog(request, {
            stagedResourceIds: deliveryProfileMutation.stagedResourceIds,
            notes: deliveryProfileMutation.notes,
          });
        }

        setGraphQLResponse(request, 200, deliveryProfileMutation.response);
        return true;
      }

      return false;
    },
    async handleQuery(request) {
      if (request.config.readMode === 'snapshot') {
        if (request.primaryRootField === 'deliverySettings' || request.primaryRootField === 'deliveryPromiseSettings') {
          setGraphQLResponse(request, 200, handleDeliverySettingsQuery(request.body.query));
          return true;
        }

        if (request.primaryRootField === 'deliveryProfile' || request.primaryRootField === 'deliveryProfiles') {
          setGraphQLResponse(request, 200, handleDeliveryProfileQuery(request.body.query, request.variables));
          return true;
        }

        setGraphQLResponse(
          request,
          200,
          request.primaryRootField !== null && ORDER_BACKED_FULFILLMENT_QUERY_ROOTS.has(request.primaryRootField)
            ? handleOrderQuery(request.body.query, request.variables)
            : handleStorePropertiesQuery(request.body.query, request.variables),
        );
        return true;
      }

      if (
        request.config.readMode === 'live-hybrid' &&
        request.primaryRootField !== null &&
        ORDER_BACKED_FULFILLMENT_QUERY_ROOTS.has(request.primaryRootField)
      ) {
        const upstreamResponse = await proxyUpstreamGraphQL(request);
        setGraphQLResponse(request, upstreamResponse.status, upstreamResponse.body);
        return true;
      }

      if (
        request.config.readMode === 'live-hybrid' &&
        ((request.primaryRootField === 'fulfillmentService' &&
          typeof request.variables['id'] === 'string' &&
          store.getEffectiveFulfillmentServiceById(request.variables['id']) !== null) ||
          (request.primaryRootField === 'carrierService' &&
            typeof request.variables['id'] === 'string' &&
            store.getEffectiveCarrierServiceById(request.variables['id']) !== null) ||
          (request.primaryRootField === 'carrierServices' && store.hasStagedCarrierServices()) ||
          (request.primaryRootField === 'availableCarrierServices' &&
            (store.hasStagedCarrierServices() || store.hasStagedLocations())) ||
          (request.primaryRootField === 'locationsAvailableForDeliveryProfilesConnection' &&
            store.hasStagedLocations()))
      ) {
        setGraphQLResponse(request, 200, handleStorePropertiesQuery(request.body.query, request.variables));
        return true;
      }

      if (
        request.config.readMode === 'live-hybrid' &&
        (request.primaryRootField === 'deliveryProfile' || request.primaryRootField === 'deliveryProfiles') &&
        store.hasStagedDeliveryProfiles()
      ) {
        setGraphQLResponse(request, 200, handleDeliveryProfileQuery(request.body.query, request.variables));
        return true;
      }

      return false;
    },
  },
  {
    name: 'customers',
    canHandle: (request) =>
      request.capability.domain === 'customers' ||
      request.primaryRootField === 'dataSaleOptOut' ||
      request.parsed.rootFields.includes('customerPaymentMethod'),
    async handleQuery(request) {
      if (request.parsed.rootFields.includes('customerPaymentMethod')) {
        if (request.config.readMode === 'snapshot') {
          setGraphQLResponse(request, 200, handleCustomerQuery(request.body.query, request.variables));
          return true;
        }

        if (request.config.readMode === 'live-hybrid' && store.hasCustomerPaymentMethods()) {
          setGraphQLResponse(request, 200, handleCustomerQuery(request.body.query, request.variables));
          return true;
        }
      }

      if (request.capability.execution !== 'overlay-read') {
        return false;
      }

      if (request.config.readMode === 'snapshot') {
        setGraphQLResponse(request, 200, handleCustomerQuery(request.body.query, request.variables));
        return true;
      }

      if (request.config.readMode === 'live-hybrid') {
        const upstreamResponse = await proxyUpstreamGraphQL(request);
        hydrateCustomersFromUpstreamResponse(request.body.query, request.variables, upstreamResponse.body);

        if (request.primaryRootField === 'customerAccountPage' || request.primaryRootField === 'customerAccountPages') {
          setGraphQLResponse(request, upstreamResponse.status, upstreamResponse.body);
          return true;
        }

        setGraphQLResponse(
          request,
          upstreamResponse.status,
          store.hasBaseCustomers() || store.hasStagedCustomers() || store.hasCustomerAccountPages()
            ? handleCustomerQuery(request.body.query, request.variables)
            : upstreamResponse.body,
        );
        return true;
      }

      return false;
    },
    handleMutation(request) {
      if (
        request.capability.execution !== 'stage-locally' ||
        (request.capability.domain !== 'customers' && request.primaryRootField !== 'dataSaleOptOut')
      ) {
        return false;
      }

      const logEntryId = makeSyntheticGid('MutationLogEntry');
      const receivedAt = makeSyntheticTimestamp();
      const responseBody = handleCustomerMutation(request.body.query, request.variables);
      appendStagedMutationLog(request, {
        id: logEntryId,
        receivedAt,
        responseBody,
        notes:
          request.primaryRootField === 'dataSaleOptOut'
            ? 'Staged locally in the in-memory customer privacy draft store.'
            : 'Staged locally in the in-memory customer draft store.',
      });
      setGraphQLResponse(request, 200, responseBody);
      return true;
    },
  },
  {
    name: 'media',
    canHandle: (request) => request.capability.domain === 'media',
    handleMutation(request) {
      if (request.capability.execution !== 'stage-locally') {
        return false;
      }

      appendStagedMutationLog(request, {
        notes: 'Staged locally in the in-memory media draft store.',
      });
      setGraphQLResponse(request, 200, handleMediaMutation(request.body.query, request.variables));
      return true;
    },
    handleQuery(request) {
      if (request.capability.execution !== 'overlay-read') {
        return false;
      }

      if (request.config.readMode === 'snapshot') {
        setGraphQLResponse(request, 200, handleMediaQuery(request.body.query, request.variables));
        return true;
      }

      if (request.config.readMode === 'live-hybrid' && store.listEffectiveFiles().length > 0) {
        setGraphQLResponse(request, 200, handleMediaQuery(request.body.query, request.variables));
        return true;
      }

      return false;
    },
  },
  {
    name: 'metafields',
    canHandle: (request) => request.capability.domain === 'metafields',
    async handleQuery(request) {
      if (request.capability.execution !== 'overlay-read') {
        return false;
      }

      if (request.config.readMode === 'snapshot') {
        setGraphQLResponse(request, 200, handleMetafieldDefinitionQuery(request.body.query, request.variables));
        return true;
      }

      const upstreamResponse = await proxyUpstreamGraphQL(request);
      setGraphQLResponse(request, upstreamResponse.status, upstreamResponse.body);
      return true;
    },
    handleMutation(request) {
      if (request.capability.execution !== 'stage-locally') {
        return false;
      }

      const responseBody = handleMetafieldDefinitionMutation(request.body.query, request.variables);
      appendStagedMutationLog(request, {
        responseBody,
        notes: 'Staged locally in the in-memory metafield definition draft store.',
      });
      setGraphQLResponse(request, 200, responseBody);
      return true;
    },
  },
  {
    name: 'metaobjects',
    canHandle: (request) => request.capability.domain === 'metaobjects',
    async handleQuery(request) {
      if (request.capability.execution !== 'overlay-read') {
        return false;
      }

      if (request.config.readMode === 'snapshot') {
        setGraphQLResponse(request, 200, handleMetaobjectQuery(request.body.query, request.variables));
        return true;
      }

      if (request.config.readMode === 'live-hybrid') {
        const hadLocalDefinitions = store.hasEffectiveMetaobjectDefinitions();
        const hadLocalMetaobjects = store.hasEffectiveMetaobjects();
        const upstreamResponse = await proxyUpstreamGraphQL(request);
        hydrateMetaobjectsFromUpstreamResponse(request.body.query, request.variables, upstreamResponse.body);
        setGraphQLResponse(
          request,
          upstreamResponse.status,
          (store.hasEffectiveMetaobjectDefinitions() &&
            (hadLocalDefinitions || store.hasStagedMetaobjectDefinitions())) ||
            (store.hasEffectiveMetaobjects() && (hadLocalMetaobjects || store.hasStagedMetaobjects()))
            ? handleMetaobjectQuery(request.body.query, request.variables)
            : upstreamResponse.body,
        );
        return true;
      }

      return false;
    },
    handleMutation(request) {
      if (request.capability.execution !== 'stage-locally') {
        return false;
      }

      const responseBody = handleMetaobjectDefinitionMutation(request.body.query, request.variables);
      appendStagedMutationLog(request, {
        responseBody,
        notes: 'Staged locally in the in-memory metaobject definition/entry draft store.',
      });
      setGraphQLResponse(request, 200, responseBody);
      return true;
    },
  },
  {
    name: 'orders',
    canHandle: (request) =>
      request.capability.domain === 'orders' ||
      (request.capability.domain === 'payments' && ORDER_PAYMENT_MUTATION_ROOTS.has(request.primaryRootField ?? '')) ||
      ORDER_ACCESS_DENIED_GUARDRAIL_MUTATION_ROOTS.has(request.primaryRootField ?? '') ||
      (request.capability.domain === 'shipping-fulfillments' &&
        (isOrderBackedLocalFulfillmentMutation(request.primaryRootField) ||
          isOrderBackedReverseLogisticsMutation(request.primaryRootField))) ||
      (request.parsed.type === 'query' &&
        request.parsed.rootFields.some(
          (rootField) =>
            DRAFT_ORDER_LOCAL_HELPER_QUERY_ROOTS.has(rootField) ||
            ORDER_BACKED_REVERSE_LOGISTICS_QUERY_ROOTS.has(rootField),
        )),
    async handleQuery(request) {
      if (request.capability.execution !== 'overlay-read') {
        return false;
      }

      if (request.config.readMode === 'snapshot') {
        setGraphQLResponse(request, 200, handleOrderQuery(request.body.query, request.variables));
        return true;
      }

      if (request.config.readMode === 'live-hybrid') {
        const liveHybridOrderId = typeof request.variables['id'] === 'string' ? request.variables['id'] : null;
        const liveHybridAbandonedCheckoutId =
          typeof request.variables['abandonedCheckoutId'] === 'string'
            ? request.variables['abandonedCheckoutId']
            : null;
        const hasStagedOrders = store.getOrders().length > 0;
        const hasStagedDraftOrders = store.getDraftOrders().length > 0;
        const hasLocalAbandonedCheckouts = store.getAbandonedCheckouts().length > 0;
        const canServeLocalOrderDetail =
          request.primaryRootField === 'order' &&
          liveHybridOrderId !== null &&
          (store.getOrderById(liveHybridOrderId) !== null || store.hasDeletedOrder(liveHybridOrderId));
        const canServeLocalReturnDetail =
          request.primaryRootField === 'return' &&
          liveHybridOrderId !== null &&
          store.getOrders().some((order) => order.returns.some((orderReturn) => orderReturn.id === liveHybridOrderId));
        const canServeLocalReverseLogisticsDetail =
          (request.primaryRootField === 'reverseDelivery' || request.primaryRootField === 'reverseFulfillmentOrder') &&
          liveHybridOrderId !== null;
        const canServeLocalOrderCatalog =
          (request.primaryRootField === 'orders' || request.primaryRootField === 'ordersCount') &&
          hasStagedOrders &&
          typeof request.variables['query'] !== 'string';
        const canServeLocalDraftOrderDetail =
          request.primaryRootField === 'draftOrder' &&
          liveHybridOrderId !== null &&
          (store.getDraftOrderById(liveHybridOrderId) !== null || store.hasDeletedDraftOrder(liveHybridOrderId));
        const canServeLocalDraftOrderCatalog =
          (request.primaryRootField === 'draftOrders' || request.primaryRootField === 'draftOrdersCount') &&
          hasStagedDraftOrders &&
          shouldServeDraftOrderCatalogLocally(request.variables['query'], request.variables['savedSearchId']);
        const canServeLocalDraftOrderHelper =
          request.primaryRootField === 'draftOrderAvailableDeliveryOptions' ||
          request.primaryRootField === 'draftOrderSavedSearches' ||
          request.primaryRootField === 'draftOrderTag';
        const canServeLocalAbandonedCheckoutCatalog =
          (request.primaryRootField === 'abandonedCheckouts' ||
            request.primaryRootField === 'abandonedCheckoutsCount') &&
          hasLocalAbandonedCheckouts &&
          typeof request.variables['query'] !== 'string' &&
          typeof request.variables['savedSearchId'] !== 'string';
        const canServeLocalAbandonmentDetail =
          request.primaryRootField === 'abandonment' &&
          liveHybridOrderId !== null &&
          store.getAbandonmentById(liveHybridOrderId) !== null;
        const canServeLocalAbandonmentByCheckout =
          request.primaryRootField === 'abandonmentByAbandonedCheckoutId' &&
          liveHybridAbandonedCheckoutId !== null &&
          store.getAbandonmentByAbandonedCheckoutId(liveHybridAbandonedCheckoutId) !== null;

        if (
          canServeLocalOrderDetail ||
          canServeLocalReturnDetail ||
          canServeLocalReverseLogisticsDetail ||
          canServeLocalOrderCatalog ||
          canServeLocalDraftOrderDetail ||
          canServeLocalDraftOrderCatalog ||
          canServeLocalDraftOrderHelper ||
          canServeLocalAbandonedCheckoutCatalog ||
          canServeLocalAbandonmentDetail ||
          canServeLocalAbandonmentByCheckout
        ) {
          setGraphQLResponse(request, 200, handleOrderQuery(request.body.query, request.variables));
          return true;
        }

        const upstreamResponse = await proxyUpstreamGraphQL(request);
        setGraphQLResponse(request, upstreamResponse.status, upstreamResponse.body);
        return true;
      }

      return false;
    },
    handleMutation(request) {
      if (
        request.capability.execution === 'stage-locally' &&
        request.capability.domain === 'payments' &&
        ORDER_PAYMENT_MUTATION_ROOTS.has(request.primaryRootField ?? '')
      ) {
        const responseBody = handleOrderMutation(
          request.body.query,
          request.variables,
          request.config.readMode,
          request.config.shopifyAdminOrigin,
        );
        if (!responseBody) {
          return false;
        }

        const logEntryId = makeSyntheticGid('MutationLogEntry');
        const receivedAt = makeSyntheticTimestamp();
        if (shouldAppendLocalMutationLog(request.primaryRootField, responseBody)) {
          appendStagedMutationLog(request, {
            id: logEntryId,
            receivedAt,
            responseBody,
            notes: 'Staged locally in the in-memory order payment draft store.',
          });
        }

        setGraphQLResponse(request, 200, responseBody);
        return true;
      }

      if (
        ORDER_ACCESS_DENIED_GUARDRAIL_MUTATION_ROOTS.has(request.primaryRootField ?? '') &&
        (request.config.readMode === 'snapshot' || request.config.readMode === 'live-hybrid')
      ) {
        const responseBody = handleOrderMutation(
          request.body.query,
          request.variables,
          request.config.readMode,
          request.config.shopifyAdminOrigin,
        );
        if (!responseBody) {
          return false;
        }

        setGraphQLResponse(request, 200, responseBody);
        return true;
      }

      if (
        shouldTryLocalOrderMutation(request) &&
        (request.config.readMode === 'snapshot' || request.primaryRootField === 'draftOrderCreate')
      ) {
        const logEntryId = makeSyntheticGid('MutationLogEntry');
        const receivedAt = makeSyntheticTimestamp();
        const responseBody = handleOrderMutation(
          request.body.query,
          request.variables,
          request.config.readMode,
          request.config.shopifyAdminOrigin,
        ) ?? { data: {} };

        if (shouldAppendLocalMutationLog(request.primaryRootField, responseBody)) {
          appendStagedMutationLog(request, {
            id: logEntryId,
            receivedAt,
            responseBody,
            notes: isOrderBackedLocalFulfillmentMutation(request.primaryRootField)
              ? 'Staged locally in the in-memory order-backed fulfillment store.'
              : isOrderBackedReverseLogisticsMutation(request.primaryRootField)
                ? 'Staged locally in the in-memory order-backed return/reverse-logistics store.'
                : 'Staged locally in the in-memory order draft store.',
          });
        }

        setGraphQLResponse(request, 200, responseBody);
        return true;
      }

      if (
        shouldTryLocalOrderMutation(request) &&
        request.config.readMode === 'live-hybrid' &&
        (LIVE_HYBRID_LOCAL_ORDER_MUTATION_ROOTS.has(request.primaryRootField ?? '') ||
          isOrderBackedLocalFulfillmentMutation(request.primaryRootField) ||
          isOrderBackedReverseLogisticsMutation(request.primaryRootField) ||
          (request.primaryRootField !== null && ORDER_RETURN_MUTATION_ROOTS.has(request.primaryRootField)))
      ) {
        const responseBody = handleOrderMutation(
          request.body.query,
          request.variables,
          request.config.readMode,
          request.config.shopifyAdminOrigin,
        );
        if (!responseBody) {
          return false;
        }

        const logEntryId = makeSyntheticGid('MutationLogEntry');
        const receivedAt = makeSyntheticTimestamp();
        if (shouldAppendLocalMutationLog(request.primaryRootField, responseBody)) {
          appendStagedMutationLog(request, {
            id: logEntryId,
            receivedAt,
            responseBody,
            notes:
              LIVE_HYBRID_ORDER_MUTATION_NOTES[request.primaryRootField ?? ''] ??
              'Locally short-circuited captured order mutation validation in live-hybrid mode.',
          });
        }

        setGraphQLResponse(request, 200, responseBody);
        return true;
      }

      return false;
    },
  },
  {
    name: 'payments',
    canHandle: (request) => request.capability.domain === 'payments',
    handleMutation(request) {
      if (PAYMENT_TERMS_MUTATION_ROOTS.has(request.primaryRootField ?? '')) {
        const responseBody = handleOrderMutation(
          request.body.query,
          request.variables,
          request.config.readMode,
          request.config.shopifyAdminOrigin,
        );
        if (responseBody === null) {
          return false;
        }

        if (shouldAppendLocalMutationLog(request.primaryRootField, responseBody)) {
          appendStagedMutationLog(request, {
            operationName: request.primaryRootField,
            responseBody,
            notes:
              'Staged payment terms locally on the order/draft-order graph; runtime Shopify payment terms writes are not sent upstream.',
          });
        }
        setGraphQLResponse(request, 200, responseBody);
        return true;
      }

      if (
        request.capability.execution !== 'stage-locally' ||
        !PAYMENT_CUSTOMIZATION_MUTATION_ROOTS.has(request.primaryRootField ?? '')
      ) {
        return false;
      }

      const responseBody = handlePaymentMutation(request.body.query, request.variables);
      appendStagedMutationLog(request, {
        responseBody,
        notes: request.primaryRootField?.startsWith('customerPaymentMethod')
          ? 'Staged locally in the in-memory customer payment-method draft store; payment credentials, gateway secrets, and customer-facing update URLs are scrubbed or synthetic.'
          : request.primaryRootField === 'paymentReminderSend'
            ? 'Staged a local payment reminder intent only; no customer email is sent at runtime.'
            : 'Staged locally in the in-memory payment customization draft store; Shopify Functions and checkout payment behavior are not invoked.',
      });
      setGraphQLResponse(request, 200, responseBody);
      return true;
    },
    handleQuery(request) {
      if (request.capability.execution !== 'overlay-read') {
        return false;
      }

      if (request.primaryRootField === 'shopifyPaymentsAccount') {
        if (request.config.readMode === 'snapshot') {
          setGraphQLResponse(request, 200, handleStorePropertiesQuery(request.body.query, request.variables));
          return true;
        }

        if (request.config.readMode === 'live-hybrid') {
          const primaryBusinessEntity = store.getPrimaryBusinessEntity();
          const hasLocalShopifyPaymentsAccount =
            Boolean(primaryBusinessEntity?.shopifyPaymentsAccount) ||
            store
              .listEffectiveBusinessEntities()
              .some((businessEntity) => businessEntity.shopifyPaymentsAccount !== null);

          if (hasLocalShopifyPaymentsAccount) {
            setGraphQLResponse(request, 200, handleStorePropertiesQuery(request.body.query, request.variables));
            return true;
          }
        }
      }

      if (request.config.readMode === 'snapshot') {
        setGraphQLResponse(request, 200, handlePaymentQuery(request.body.query, request.variables));
        return true;
      }

      if (
        request.config.readMode === 'live-hybrid' &&
        (store.hasPaymentCustomizations() || request.primaryRootField === 'paymentTermsTemplates')
      ) {
        setGraphQLResponse(request, 200, handlePaymentQuery(request.body.query, request.variables));
        return true;
      }

      return false;
    },
  },
  {
    name: 'localization',
    canHandle: (request) => request.capability.domain === 'localization',
    async handleQuery(request) {
      if (request.capability.execution !== 'overlay-read') {
        return false;
      }

      if (request.config.readMode === 'snapshot') {
        setGraphQLResponse(request, 200, handleLocalizationQuery(request.body.query, request.variables));
        return true;
      }

      if (request.config.readMode === 'live-hybrid') {
        const upstreamResponse = await proxyUpstreamGraphQL(request);
        hydrateLocalizationFromUpstreamResponse(upstreamResponse.body);
        setGraphQLResponse(
          request,
          upstreamResponse.status,
          store.hasStagedLocalizationState()
            ? handleLocalizationQuery(request.body.query, request.variables)
            : upstreamResponse.body,
        );
        return true;
      }

      return false;
    },
    handleMutation(request) {
      if (request.capability.execution !== 'stage-locally') {
        return false;
      }

      const responseBody = handleLocalizationMutation(request.body.query, request.variables);
      appendStagedMutationLog(request, {
        responseBody,
        notes: 'Staged locally in the in-memory localization draft store.',
      });
      setGraphQLResponse(request, 200, responseBody);
      return true;
    },
  },
  {
    name: 'markets',
    canHandle: (request) => request.capability.domain === 'markets',
    async handleQuery(request) {
      if (request.capability.execution !== 'overlay-read') {
        return false;
      }

      if (request.config.readMode === 'snapshot') {
        setGraphQLResponse(request, 200, handleMarketsQuery(request.body.query, request.variables));
        return true;
      }

      if (request.config.readMode === 'live-hybrid') {
        const upstreamResponse = await proxyUpstreamGraphQL(request);
        hydrateMarketsFromUpstreamResponse(request.body.query, request.variables, upstreamResponse.body);
        setGraphQLResponse(
          request,
          upstreamResponse.status,
          store.hasStagedMarkets() || store.hasStagedPriceLists()
            ? handleMarketsQuery(request.body.query, request.variables)
            : upstreamResponse.body,
        );
        return true;
      }

      return false;
    },
    handleMutation(request) {
      if (request.capability.execution !== 'stage-locally') {
        return false;
      }

      const responseBody = handleMarketMutation(request.body.query, request.variables);
      appendStagedMutationLog(request, {
        responseBody,
        notes: 'Staged locally in the in-memory Markets draft store.',
      });
      setGraphQLResponse(request, 200, responseBody);
      return true;
    },
  },
  {
    name: 'segments',
    canHandle: (request) => request.capability.domain === 'segments',
    async handleQuery(request) {
      if (request.capability.execution !== 'overlay-read') {
        return false;
      }

      if (request.config.readMode === 'snapshot') {
        setGraphQLResponse(request, 200, handleSegmentsQuery(request.body.query, request.variables));
        return true;
      }

      if (request.config.readMode === 'live-hybrid') {
        if (
          request.primaryRootField !== null &&
          ['customerSegmentMembers', 'customerSegmentMembersQuery', 'customerSegmentMembership'].includes(
            request.primaryRootField,
          ) &&
          (store.hasCustomerSegmentMembersQueries() || store.hasStagedSegments() || store.hasStagedCustomers())
        ) {
          setGraphQLResponse(request, 200, handleSegmentsQuery(request.body.query, request.variables));
          return true;
        }

        const upstreamResponse = await proxyUpstreamGraphQL(request);
        hydrateSegmentsFromUpstreamResponse(request.body.query, request.variables, upstreamResponse.body);
        setGraphQLResponse(
          request,
          upstreamResponse.status,
          store.hasStagedSegments()
            ? handleSegmentsQuery(request.body.query, request.variables)
            : upstreamResponse.body,
        );
        return true;
      }

      return false;
    },
    handleMutation(request) {
      if (request.capability.execution !== 'stage-locally') {
        return false;
      }

      const responseBody = handleSegmentMutation(request.body.query, request.variables);
      appendStagedMutationLog(request, {
        responseBody,
        notes: 'Staged locally in the in-memory segment draft store.',
      });
      setGraphQLResponse(request, 200, responseBody);
      return true;
    },
  },
  {
    name: 'saved-searches',
    canHandle: (request) => request.capability.domain === 'saved-searches',
    async handleQuery(request) {
      if (request.capability.execution !== 'overlay-read') {
        return false;
      }

      if (!request.parsed.rootFields.every((rootField) => isSavedSearchQueryRoot(rootField))) {
        return false;
      }

      if (request.config.readMode === 'snapshot') {
        setGraphQLResponse(request, 200, handleSavedSearchQuery(request.body.query, request.variables));
        return true;
      }

      if (request.config.readMode === 'live-hybrid') {
        const upstreamResponse = await proxyUpstreamGraphQL(request);
        hydrateSavedSearchesFromUpstreamResponse(request.body.query, upstreamResponse.body);
        setGraphQLResponse(
          request,
          upstreamResponse.status,
          store.hasStagedSavedSearches() ||
            (isSavedSearchQueryRoot(request.primaryRootField) && store.hasSavedSearches())
            ? handleSavedSearchQuery(request.body.query, request.variables)
            : upstreamResponse.body,
        );
        return true;
      }

      return false;
    },
    handleMutation(request) {
      if (request.capability.execution !== 'stage-locally') {
        return false;
      }

      const savedSearchMutation = handleSavedSearchMutation(request.body.query, request.variables);
      if (!savedSearchMutation) {
        return false;
      }

      appendStagedMutationLog(request, {
        stagedResourceIds: savedSearchMutation.stagedResourceIds,
        notes:
          'Staged locally in the in-memory saved-search draft store; URL redirect saved-search branches remain blocked until online-store navigation conformance is captured.',
      });
      setGraphQLResponse(request, 200, savedSearchMutation.response);
      return true;
    },
  },
  {
    name: 'marketing',
    canHandle: (request) => request.capability.domain === 'marketing',
    async handleQuery(request) {
      if (request.capability.execution !== 'overlay-read') {
        return false;
      }

      if (request.config.readMode === 'snapshot') {
        setGraphQLResponse(request, 200, handleMarketingQuery(request.body.query, request.variables));
        return true;
      }

      if (request.config.readMode === 'live-hybrid') {
        const upstreamResponse = await proxyUpstreamGraphQL(request);
        hydrateMarketingFromUpstreamResponse(request.body.query, request.variables, upstreamResponse.body);
        setGraphQLResponse(
          request,
          upstreamResponse.status,
          store.hasStagedMarketingRecords()
            ? handleMarketingQuery(request.body.query, request.variables)
            : upstreamResponse.body,
        );
        return true;
      }

      return false;
    },
    handleMutation(request) {
      if (
        request.capability.execution !== 'stage-locally' ||
        request.primaryRootField === null ||
        !MARKETING_MUTATION_ROOTS.has(request.primaryRootField)
      ) {
        return false;
      }

      const marketingMutation = handleMarketingMutation(request.body.query, request.variables);
      if (!marketingMutation) {
        return false;
      }

      if (marketingMutation.shouldLog) {
        appendStagedMutationLog(request, {
          stagedResourceIds: marketingMutation.stagedResourceIds,
          notes: marketingMutation.notes,
        });
      }

      setGraphQLResponse(request, 200, marketingMutation.response);
      return true;
    },
  },
  {
    name: 'webhooks',
    canHandle: (request) => request.capability.domain === 'webhooks',
    async handleQuery(request) {
      if (request.capability.execution !== 'overlay-read') {
        return false;
      }

      if (request.config.readMode === 'snapshot') {
        setGraphQLResponse(request, 200, handleWebhookSubscriptionQuery(request.body.query, request.variables));
        return true;
      }

      if (request.config.readMode === 'live-hybrid') {
        const upstreamResponse = await proxyUpstreamGraphQL(request);
        hydrateWebhookSubscriptionsFromUpstreamResponse(request.body.query, request.variables, upstreamResponse.body);
        setGraphQLResponse(
          request,
          upstreamResponse.status,
          store.hasWebhookSubscriptions() || store.hasStagedWebhookSubscriptions()
            ? handleWebhookSubscriptionQuery(request.body.query, request.variables)
            : upstreamResponse.body,
        );
        return true;
      }

      return false;
    },
    handleMutation(request) {
      if (
        request.capability.execution !== 'stage-locally' ||
        (request.primaryRootField !== 'webhookSubscriptionCreate' &&
          request.primaryRootField !== 'webhookSubscriptionUpdate' &&
          request.primaryRootField !== 'webhookSubscriptionDelete')
      ) {
        return false;
      }

      const webhookSubscriptionMutation = handleWebhookSubscriptionMutation(request.body.query, request.variables);
      if (!webhookSubscriptionMutation) {
        return false;
      }

      if (webhookSubscriptionMutation.staged) {
        appendStagedMutationLog(request, {
          stagedResourceIds: webhookSubscriptionMutation.stagedResourceIds,
          notes: webhookSubscriptionMutation.notes,
        });
      }

      setGraphQLResponse(request, 200, webhookSubscriptionMutation.response);
      return true;
    },
  },
  {
    name: 'functions',
    canHandle: (request) =>
      request.capability.domain === 'functions' ||
      (request.primaryRootField !== null &&
        (FUNCTION_QUERY_ROOTS.has(request.primaryRootField) || FUNCTION_MUTATION_ROOTS.has(request.primaryRootField))),
    async handleQuery(request) {
      if (request.capability.execution !== 'overlay-read') {
        return false;
      }

      if (request.config.readMode === 'snapshot') {
        setGraphQLResponse(request, 200, handleFunctionQuery(request.body.query, request.variables));
        return true;
      }

      if (request.config.readMode === 'live-hybrid') {
        if (store.hasFunctionMetadata()) {
          setGraphQLResponse(request, 200, handleFunctionQuery(request.body.query, request.variables));
          return true;
        }

        const upstreamResponse = await proxyUpstreamGraphQL(request);
        setGraphQLResponse(request, upstreamResponse.status, upstreamResponse.body);
        return true;
      }

      return false;
    },
    handleMutation(request) {
      if (request.capability.execution !== 'stage-locally') {
        return false;
      }

      const responseBody = handleFunctionMutation(request.body.query, request.variables);
      appendStagedMutationLog(request, {
        responseBody,
        notes:
          request.primaryRootField === 'taxAppConfigure'
            ? 'Staged locally in the in-memory tax app configuration metadata store; no tax calculation app callbacks are invoked.'
            : 'Staged locally in the in-memory Shopify Functions metadata store; external Shopify Function code is not executed.',
      });
      setGraphQLResponse(request, 200, responseBody);
      return true;
    },
  },
  {
    name: 'gift-cards',
    canHandle: (request) => request.capability.domain === 'gift-cards',
    async handleQuery(request) {
      if (request.capability.execution !== 'overlay-read') {
        return false;
      }

      if (request.config.readMode === 'snapshot') {
        setGraphQLResponse(request, 200, handleGiftCardQuery(request.body.query, request.variables));
        return true;
      }

      if (request.config.readMode === 'live-hybrid') {
        const upstreamResponse = await proxyUpstreamGraphQL(request);
        setGraphQLResponse(
          request,
          upstreamResponse.status,
          store.hasGiftCards() || store.hasStagedGiftCards()
            ? handleGiftCardQuery(request.body.query, request.variables)
            : upstreamResponse.body,
        );
        return true;
      }

      return false;
    },
    handleMutation(request) {
      if (request.capability.execution !== 'stage-locally') {
        return false;
      }

      const responseBody = handleGiftCardMutation(request.body.query, request.variables);
      appendStagedMutationLog(request, {
        responseBody,
        notes:
          request.primaryRootField === 'giftCardSendNotificationToCustomer' ||
          request.primaryRootField === 'giftCardSendNotificationToRecipient'
            ? 'Short-circuited locally in the in-memory gift-card draft store; no customer-visible notification is sent at runtime.'
            : 'Staged locally in the in-memory gift-card draft store.',
      });
      setGraphQLResponse(request, 200, responseBody);
      return true;
    },
  },
  {
    name: 'online-store',
    canHandle: (request) => request.capability.domain === 'online-store',
    async handleQuery(request) {
      if (request.capability.execution !== 'overlay-read') {
        return false;
      }

      if (request.config.readMode === 'snapshot') {
        setGraphQLResponse(request, 200, handleOnlineStoreQuery(request.body.query, request.variables));
        return true;
      }

      if (request.config.readMode === 'live-hybrid') {
        const upstreamResponse = await proxyUpstreamGraphQL(request);
        hydrateOnlineStoreFromUpstreamResponse(request.body.query, upstreamResponse.body);
        setGraphQLResponse(
          request,
          upstreamResponse.status,
          store.hasStagedOnlineStoreContent() ||
            store.hasStagedOnlineStoreIntegrations() ||
            (request.primaryRootField !== null &&
              isOnlineStoreContentQueryRoot(request.primaryRootField) &&
              (store.hasOnlineStoreContent() || store.hasOnlineStoreIntegrations()))
            ? handleOnlineStoreQuery(request.body.query, request.variables)
            : upstreamResponse.body,
        );
        return true;
      }

      return false;
    },
    handleMutation(request) {
      if (request.capability.execution !== 'stage-locally') {
        return false;
      }

      const onlineStoreMutation = handleOnlineStoreMutation(request.body.query, request.variables);
      if (!onlineStoreMutation) {
        return false;
      }

      appendStagedMutationLog(request, {
        stagedResourceIds: onlineStoreMutation.stagedResourceIds,
        notes: 'Staged locally in the in-memory online-store content draft store.',
      });
      setGraphQLResponse(request, 200, onlineStoreMutation.response);
      return true;
    },
  },
  {
    name: 'store-properties',
    canHandle: (request) => request.capability.domain === 'store-properties',
    async handleQuery(request) {
      if (request.capability.execution !== 'overlay-read') {
        return false;
      }

      if (request.config.readMode === 'snapshot') {
        setGraphQLResponse(request, 200, handleStorePropertiesQuery(request.body.query, request.variables));
        return true;
      }

      if (request.config.readMode === 'live-hybrid') {
        if (request.primaryRootField === 'shop' && store.getEffectiveShop() !== null) {
          setGraphQLResponse(request, 200, handleStorePropertiesQuery(request.body.query, request.variables));
          return true;
        }

        if (
          (request.primaryRootField === 'location' || request.primaryRootField === 'locationByIdentifier') &&
          store.hasStagedLocations()
        ) {
          setGraphQLResponse(request, 200, handleStorePropertiesQuery(request.body.query, request.variables));
          return true;
        }

        const upstreamResponse = await proxyUpstreamGraphQL(request);
        setGraphQLResponse(request, upstreamResponse.status, upstreamResponse.body);
        return true;
      }

      return false;
    },
    handleMutation(request) {
      if (
        request.capability.execution !== 'stage-locally' ||
        (request.primaryRootField !== 'shopPolicyUpdate' &&
          request.primaryRootField !== 'locationAdd' &&
          request.primaryRootField !== 'locationEdit' &&
          request.primaryRootField !== 'locationActivate' &&
          request.primaryRootField !== 'locationDeactivate' &&
          request.primaryRootField !== 'locationDelete')
      ) {
        return false;
      }

      request.proxyLogger.debug(
        {
          execution: request.capability.execution,
          operationName: request.capability.operationName,
          operationType: request.parsed.type,
          rootFields: request.parsed.rootFields,
        },
        'staging supported store properties mutation locally',
      );

      const responseBody = handleStorePropertiesMutation(request.body.query, request.variables);
      appendStagedMutationLog(request, {
        responseBody,
        notes:
          request.primaryRootField === 'shopPolicyUpdate'
            ? 'Staged locally in the in-memory Store properties legal policy draft store.'
            : 'Staged locally in the in-memory Store properties location draft store.',
      });
      setGraphQLResponse(request, 200, responseBody);
      return true;
    },
  },
  {
    name: 'events',
    canHandle: (request) => request.capability.domain === 'events',
    handleQuery(request) {
      if (request.capability.execution === 'overlay-read' && request.config.readMode === 'snapshot') {
        setGraphQLResponse(request, 200, handleEventsQuery(request.body.query));
        return true;
      }

      return false;
    },
  },
  {
    name: 'b2b',
    canHandle: (request) => request.capability.domain === 'b2b',
    handleMutation(request) {
      if (request.capability.execution !== 'stage-locally') {
        return false;
      }

      const b2bMutation = handleB2BMutation(request.body.query, request.variables);
      if (!b2bMutation) {
        return false;
      }

      request.proxyLogger.debug(
        {
          operationName: request.capability.operationName,
          operationType: request.parsed.type,
          rootFields: request.parsed.rootFields,
        },
        b2bMutation.staged
          ? 'staging supported B2B mutation locally'
          : 'returning captured B2B validation response locally',
      );

      if (b2bMutation.staged) {
        appendStagedMutationLog(request, {
          stagedResourceIds: b2bMutation.stagedResourceIds,
          notes: b2bMutation.notes,
        });
      }

      setGraphQLResponse(request, 200, b2bMutation.response);
      return true;
    },
    async handleQuery(request) {
      if (request.capability.execution !== 'overlay-read') {
        return false;
      }

      if (request.config.readMode === 'snapshot') {
        setGraphQLResponse(request, 200, handleB2BQuery(request.body.query, request.variables));
        return true;
      }

      if (request.config.readMode === 'live-hybrid') {
        const upstreamResponse = await proxyUpstreamGraphQL(request);
        setGraphQLResponse(request, upstreamResponse.status, upstreamResponse.body);
        return true;
      }

      return false;
    },
  },
  {
    name: 'admin-platform',
    canHandle: (request) =>
      (request.parsed.type === 'query' &&
        request.config.readMode === 'snapshot' &&
        request.parsed.rootFields.some((rootField) => ADMIN_PLATFORM_QUERY_ROOTS.has(rootField))) ||
      (request.parsed.type === 'mutation' &&
        request.capability.execution === 'stage-locally' &&
        request.parsed.rootFields.some((rootField) => ADMIN_PLATFORM_MUTATION_ROOTS.has(rootField))),
    handleQuery(request) {
      setGraphQLResponse(request, 200, handleAdminPlatformQuery(request.body.query, request.variables));
      return true;
    },
    handleMutation(request) {
      const result = handleAdminPlatformMutation(request.body.query, request.variables);
      if (!result) {
        return false;
      }

      if (result.staged) {
        appendStagedMutationLog(request, {
          stagedResourceIds: result.stagedResourceIds ?? [],
          notes: result.notes ?? 'Staged locally in the in-memory Admin platform utility store.',
        });
      }

      setGraphQLResponse(request, 200, result.response);
      return true;
    },
  },
  {
    name: 'snapshot-order-mutation-fallback',
    canHandle: (request) => request.parsed.type === 'mutation' && request.config.readMode === 'snapshot',
    handleMutation(request) {
      const responseBody = handleOrderMutation(
        request.body.query,
        request.variables,
        request.config.readMode,
        request.config.shopifyAdminOrigin,
      );
      if (!responseBody) {
        return false;
      }

      setGraphQLResponse(request, 200, responseBody);
      return true;
    },
  },
];

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
    const routeParams = ctx['params'] as Record<string, unknown> | undefined;
    const apiVersion = typeof routeParams?.['version'] === 'string' ? routeParams['version'] : null;
    const dispatchRequest: ProxyDispatchRequest = {
      ctx,
      body: { query: body.query },
      variables,
      requestBody,
      parsed,
      capability,
      primaryRootField,
      apiVersion,
      config,
      upstream,
      proxyLogger,
    };

    for (const dispatcher of DOMAIN_DISPATCHERS) {
      if (!dispatcher.canHandle(dispatchRequest)) {
        continue;
      }

      const handled =
        parsed.type === 'mutation'
          ? await dispatcher.handleMutation?.(dispatchRequest)
          : await dispatcher.handleQuery?.(dispatchRequest);

      if (handled) {
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

    const response = await requestUpstreamGraphQL(upstream, ctx, {
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
