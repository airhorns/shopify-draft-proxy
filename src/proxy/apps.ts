import type { ProxyRuntimeContext } from './runtime-context.js';
import { createHash } from 'node:crypto';
import { Kind, type FieldNode } from 'graphql';

import { getFieldArguments, getRootFields } from '../graphql/root-field.js';
import type {
  AccessScopeRecord,
  AppInstallationRecord,
  AppOneTimePurchaseRecord,
  AppRecord,
  AppSubscriptionLineItemRecord,
  AppSubscriptionRecord,
  AppUsageRecord,
} from '../state/types.js';
import {
  getDocumentFragments,
  getFieldResponseKey,
  getSelectedChildFields,
  isPlainObject,
  paginateConnectionItems,
  projectGraphqlObject,
  readStringValue,
  serializeConnection,
} from './graphql-helpers.js';

export const APP_QUERY_ROOTS = new Set([
  'app',
  'appByHandle',
  'appByKey',
  'appInstallation',
  'appInstallations',
  'currentAppInstallation',
]);

export const APP_MUTATION_ROOTS = new Set([
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

type MoneyInput = { amount: string; currencyCode: string };
type UserError = { field: string[] | null; message: string; code?: string | null };

function asString(value: unknown): string | null {
  return typeof value === 'string' && value.length > 0 ? value : null;
}

function asBoolean(value: unknown, fallback = false): boolean {
  return typeof value === 'boolean' ? value : fallback;
}

function asNumber(value: unknown): number | null {
  return typeof value === 'number' && Number.isFinite(value) ? value : null;
}

function asRecordArray(value: unknown): Record<string, unknown>[] {
  return Array.isArray(value) ? value.filter(isPlainObject) : [];
}

function readMoneyInput(value: unknown): MoneyInput {
  if (!isPlainObject(value)) {
    return { amount: '0.0', currencyCode: 'USD' };
  }

  const amount = value['amount'];
  const currencyCode = value['currencyCode'];
  return {
    amount: typeof amount === 'number' ? String(amount) : typeof amount === 'string' ? amount : '0.0',
    currencyCode: typeof currencyCode === 'string' && currencyCode.length > 0 ? currencyCode : 'USD',
  };
}

function accessScope(handle: string): AccessScopeRecord {
  return { handle, description: null };
}

function defaultApp(runtime: ProxyRuntimeContext): AppRecord {
  return {
    id: runtime.syntheticIdentity.makeSyntheticGid('App'),
    apiKey: 'shopify-draft-proxy-local-app',
    handle: 'shopify-draft-proxy',
    title: 'shopify-draft-proxy',
    developerName: 'shopify-draft-proxy',
    embedded: true,
    previouslyInstalled: false,
    requestedAccessScopes: [accessScope('read_products'), accessScope('write_products')],
  };
}

function ensureCurrentInstallation(runtime: ProxyRuntimeContext, origin: string): AppInstallationRecord {
  const existing = runtime.store.getCurrentAppInstallation();
  if (existing) {
    return existing;
  }

  const app = defaultApp(runtime);
  runtime.store.stageApp(app);
  return runtime.store.stageAppInstallation({
    id: runtime.syntheticIdentity.makeSyntheticGid('AppInstallation'),
    appId: app.id,
    launchUrl: `${origin}/admin/apps/${app.handle ?? 'shopify-draft-proxy'}`,
    uninstallUrl: null,
    accessScopes: app.requestedAccessScopes,
    activeSubscriptionIds: [],
    allSubscriptionIds: [],
    oneTimePurchaseIds: [],
    uninstalledAt: null,
  });
}

function confirmationUrl(origin: string, kind: string, id: string): string {
  const numericId = id.split('/').at(-1) ?? 'local';
  return `${origin}/admin/charges/shopify-draft-proxy/${numericId}/${kind}/confirm?signature=shopify-draft-proxy-local-redacted`;
}

function tokenHash(accessToken: string): string {
  return createHash('sha256').update(accessToken).digest('hex');
}

function tokenPreview(accessToken: string): string {
  return accessToken.length <= 8 ? '[redacted]' : `[redacted]${accessToken.slice(-4)}`;
}

function userError(field: string[] | null, message: string, code?: string): UserError {
  return { field, message, ...(code ? { code } : {}) };
}

function serializeUserErrors(errors: UserError[], field: FieldNode): unknown[] {
  return errors.map((error) => serializePlainObject(error, field));
}

function serializePlainObject(source: Record<string, unknown>, field: FieldNode): Record<string, unknown> {
  return projectGraphqlObject(
    source,
    field.selectionSet?.selections ?? [],
    getDocumentFragments('query X { __typename }'),
  );
}

function serializeAccessScope(scope: AccessScopeRecord, field: FieldNode): Record<string, unknown> {
  return serializePlainObject(scope, field);
}

function getAppRecord(runtime: ProxyRuntimeContext, appId: string): AppRecord | null {
  return runtime.store.getEffectiveAppById(appId);
}

function nodeField(selectedFields: readonly FieldNode[]): FieldNode {
  return {
    kind: Kind.FIELD,
    name: {
      kind: Kind.NAME,
      value: 'node',
    },
    selectionSet: {
      kind: Kind.SELECTION_SET,
      selections: [...selectedFields],
    },
  };
}

function objectOrNull(value: unknown): Record<string, unknown> | null {
  return isPlainObject(value) ? value : null;
}

function serializeApp(app: AppRecord | null, field: FieldNode): unknown {
  if (!app) {
    return null;
  }

  const result: Record<string, unknown> = {};
  for (const child of getSelectedChildFields(field, { includeInlineFragments: true })) {
    const key = getFieldResponseKey(child);
    switch (child.name.value) {
      case '__typename':
        result[key] = 'App';
        break;
      case 'requestedAccessScopes':
        result[key] = app.requestedAccessScopes.map((scope) => serializeAccessScope(scope, child));
        break;
      default:
        result[key] = (app as unknown as Record<string, unknown>)[child.name.value] ?? null;
    }
  }
  return result;
}

export function serializeAppNodeById(
  runtime: ProxyRuntimeContext,
  id: string,
  selectedFields: readonly FieldNode[],
): Record<string, unknown> | null {
  return objectOrNull(serializeApp(runtime.store.getEffectiveAppById(id), nodeField(selectedFields)));
}

function serializeAppSubscriptionLineItem(
  runtime: ProxyRuntimeContext,
  lineItem: AppSubscriptionLineItemRecord,
  field: FieldNode,
): unknown {
  const source = {
    id: lineItem.id,
    plan: lineItem.plan,
    usageRecords: {
      nodes: runtime.store.listEffectiveAppUsageRecordsForLineItem(lineItem.id),
    },
  };
  const result: Record<string, unknown> = {};
  for (const child of getSelectedChildFields(field, { includeInlineFragments: true })) {
    const key = getFieldResponseKey(child);
    if (child.name.value === '__typename') {
      result[key] = 'AppSubscriptionLineItem';
    } else if (child.name.value === 'usageRecords') {
      result[key] = serializeUsageRecordConnection(
        runtime,
        runtime.store.listEffectiveAppUsageRecordsForLineItem(lineItem.id),
        child,
      );
    } else {
      result[key] = child.selectionSet
        ? projectGraphqlObject(source, [child], getDocumentFragments('query X { __typename }'))[key]
        : ((source as Record<string, unknown>)[child.name.value] ?? null);
    }
  }
  return result;
}

function serializeAppSubscription(
  runtime: ProxyRuntimeContext,
  subscription: AppSubscriptionRecord | null,
  field: FieldNode,
): unknown {
  if (!subscription) {
    return null;
  }

  const result: Record<string, unknown> = {};
  for (const child of getSelectedChildFields(field, { includeInlineFragments: true })) {
    const key = getFieldResponseKey(child);
    switch (child.name.value) {
      case '__typename':
        result[key] = 'AppSubscription';
        break;
      case 'lineItems':
        result[key] = subscription.lineItemIds
          .map((lineItemId) => runtime.store.getEffectiveAppSubscriptionLineItemById(lineItemId))
          .filter((lineItem): lineItem is AppSubscriptionLineItemRecord => lineItem !== null)
          .map((lineItem) => serializeAppSubscriptionLineItem(runtime, lineItem, child));
        break;
      default:
        result[key] = (subscription as unknown as Record<string, unknown>)[child.name.value] ?? null;
    }
  }
  return result;
}

export function serializeAppSubscriptionNodeById(
  runtime: ProxyRuntimeContext,
  id: string,
  selectedFields: readonly FieldNode[],
): Record<string, unknown> | null {
  return objectOrNull(
    serializeAppSubscription(runtime, runtime.store.getEffectiveAppSubscriptionById(id), nodeField(selectedFields)),
  );
}

function serializeOneTimePurchase(purchase: AppOneTimePurchaseRecord | null, field: FieldNode): unknown {
  if (!purchase) {
    return null;
  }

  return projectGraphqlObject(
    { __typename: 'AppPurchaseOneTime', ...purchase },
    field.selectionSet?.selections ?? [],
    getDocumentFragments('query X { __typename }'),
  );
}

export function serializeAppOneTimePurchaseNodeById(
  runtime: ProxyRuntimeContext,
  id: string,
  selectedFields: readonly FieldNode[],
): Record<string, unknown> | null {
  return objectOrNull(
    serializeOneTimePurchase(runtime.store.getEffectiveAppOneTimePurchaseById(id), nodeField(selectedFields)),
  );
}

function serializeUsageRecord(runtime: ProxyRuntimeContext, record: AppUsageRecord | null, field: FieldNode): unknown {
  if (!record) {
    return null;
  }

  const lineItem = runtime.store.getEffectiveAppSubscriptionLineItemById(record.subscriptionLineItemId);
  const source = { __typename: 'AppUsageRecord', ...record, subscriptionLineItem: lineItem };
  return projectGraphqlObject(
    source,
    field.selectionSet?.selections ?? [],
    getDocumentFragments('query X { __typename }'),
    {
      projectFieldValue({ field: child, fieldName }) {
        if (fieldName === 'subscriptionLineItem') {
          return { handled: true, value: lineItem ? serializeAppSubscriptionLineItem(runtime, lineItem, child) : null };
        }
        return { handled: false };
      },
    },
  );
}

export function serializeAppUsageRecordNodeById(
  runtime: ProxyRuntimeContext,
  id: string,
  selectedFields: readonly FieldNode[],
): Record<string, unknown> | null {
  return objectOrNull(
    serializeUsageRecord(runtime, runtime.store.getEffectiveAppUsageRecordById(id), nodeField(selectedFields)),
  );
}

function serializeSubscriptionConnection(
  runtime: ProxyRuntimeContext,
  subscriptions: AppSubscriptionRecord[],
  field: FieldNode,
): unknown {
  const window = paginateConnectionItems(subscriptions, field, {}, (subscription) => subscription.id);
  return serializeConnection(field, {
    ...window,
    getCursorValue: (subscription) => subscription.id,
    serializeNode: (subscription, nodeField) => serializeAppSubscription(runtime, subscription, nodeField),
  });
}

function serializeOneTimePurchaseConnection(purchases: AppOneTimePurchaseRecord[], field: FieldNode): unknown {
  const window = paginateConnectionItems(purchases, field, {}, (purchase) => purchase.id);
  return serializeConnection(field, {
    ...window,
    getCursorValue: (purchase) => purchase.id,
    serializeNode: (purchase, nodeField) => serializeOneTimePurchase(purchase, nodeField),
  });
}

function serializeUsageRecordConnection(
  runtime: ProxyRuntimeContext,
  records: AppUsageRecord[],
  field: FieldNode,
): unknown {
  const window = paginateConnectionItems(records, field, {}, (record) => record.id);
  return serializeConnection(field, {
    ...window,
    getCursorValue: (record) => record.id,
    serializeNode: (record, nodeField) => serializeUsageRecord(runtime, record, nodeField),
  });
}

function serializeAppInstallation(
  runtime: ProxyRuntimeContext,
  installation: AppInstallationRecord | null,
  field: FieldNode,
): unknown {
  if (!installation) {
    return null;
  }

  const result: Record<string, unknown> = {};
  for (const child of getSelectedChildFields(field, { includeInlineFragments: true })) {
    const key = getFieldResponseKey(child);
    switch (child.name.value) {
      case '__typename':
        result[key] = 'AppInstallation';
        break;
      case 'app':
        result[key] = serializeApp(getAppRecord(runtime, installation.appId), child);
        break;
      case 'accessScopes':
        result[key] = installation.accessScopes.map((scope) => serializeAccessScope(scope, child));
        break;
      case 'activeSubscriptions':
        result[key] = installation.activeSubscriptionIds
          .map((id) => runtime.store.getEffectiveAppSubscriptionById(id))
          .filter((subscription): subscription is AppSubscriptionRecord => subscription !== null)
          .filter((subscription) => subscription.status === 'ACTIVE')
          .map((subscription) => serializeAppSubscription(runtime, subscription, child));
        break;
      case 'allSubscriptions':
        result[key] = serializeSubscriptionConnection(
          runtime,
          installation.allSubscriptionIds
            .map((id) => runtime.store.getEffectiveAppSubscriptionById(id))
            .filter((subscription): subscription is AppSubscriptionRecord => subscription !== null),
          child,
        );
        break;
      case 'oneTimePurchases':
        result[key] = serializeOneTimePurchaseConnection(
          installation.oneTimePurchaseIds
            .map((id) => runtime.store.getEffectiveAppOneTimePurchaseById(id))
            .filter((purchase): purchase is AppOneTimePurchaseRecord => purchase !== null),
          child,
        );
        break;
      default:
        result[key] = (installation as unknown as Record<string, unknown>)[child.name.value] ?? null;
    }
  }
  return result;
}

export function serializeAppInstallationNodeById(
  runtime: ProxyRuntimeContext,
  id: string,
  selectedFields: readonly FieldNode[],
): Record<string, unknown> | null {
  return objectOrNull(
    serializeAppInstallation(runtime, runtime.store.getEffectiveAppInstallationById(id), nodeField(selectedFields)),
  );
}

function serializeAppInstallationConnection(
  runtime: ProxyRuntimeContext,
  installations: AppInstallationRecord[],
  field: FieldNode,
): unknown {
  const window = paginateConnectionItems(installations, field, {}, (installation) => installation.id);
  return serializeConnection(field, {
    ...window,
    getCursorValue: (installation) => installation.id,
    serializeNode: (installation, nodeField) => serializeAppInstallation(runtime, installation, nodeField),
  });
}

function rootFieldData(
  runtime: ProxyRuntimeContext,
  document: string,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  const data: Record<string, unknown> = {};
  for (const field of getRootFields(document)) {
    const key = getFieldResponseKey(field);
    const args = getFieldArguments(field, variables);
    switch (field.name.value) {
      case 'currentAppInstallation':
        data[key] = serializeAppInstallation(runtime, runtime.store.getCurrentAppInstallation(), field);
        break;
      case 'appInstallation':
        data[key] = serializeAppInstallation(
          runtime,
          asString(args['id']) ? runtime.store.getEffectiveAppInstallationById(String(args['id'])) : null,
          field,
        );
        break;
      case 'app': {
        const appId = asString(args['id']);
        data[key] = serializeApp(appId ? runtime.store.getEffectiveAppById(appId) : null, field);
        break;
      }
      case 'appByHandle': {
        const handle = asString(args['handle']);
        data[key] = serializeApp(handle ? runtime.store.findEffectiveAppByHandle(handle) : null, field);
        break;
      }
      case 'appByKey': {
        const apiKey = asString(args['apiKey']);
        data[key] = serializeApp(apiKey ? runtime.store.findEffectiveAppByApiKey(apiKey) : null, field);
        break;
      }
      case 'appInstallations': {
        const current = runtime.store.getCurrentAppInstallation();
        data[key] = serializeAppInstallationConnection(runtime, current ? [current] : [], field);
        break;
      }
    }
  }
  return data;
}

export function handleAppQuery(
  runtime: ProxyRuntimeContext,
  document: string,
  variables: Record<string, unknown>,
): Record<string, unknown> {
  return { data: rootFieldData(runtime, document, variables) };
}

function subscriptionPayload(
  runtime: ProxyRuntimeContext,
  subscription: AppSubscriptionRecord | null,
  field: FieldNode,
  errors: UserError[] = [],
): unknown {
  return serializeMutationPayload(runtime, { appSubscription: subscription, userErrors: errors }, field);
}

function serializeMutationPayload(
  runtime: ProxyRuntimeContext,
  source: Record<string, unknown>,
  field: FieldNode,
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const child of getSelectedChildFields(field, { includeInlineFragments: true })) {
    const key = getFieldResponseKey(child);
    switch (child.name.value) {
      case 'app':
        result[key] = serializeApp(source['app'] as AppRecord | null, child);
        break;
      case 'appPurchaseOneTime':
        result[key] = serializeOneTimePurchase(source['appPurchaseOneTime'] as AppOneTimePurchaseRecord | null, child);
        break;
      case 'appSubscription':
        result[key] = serializeAppSubscription(
          runtime,
          source['appSubscription'] as AppSubscriptionRecord | null,
          child,
        );
        break;
      case 'appUsageRecord':
        result[key] = serializeUsageRecord(runtime, source['appUsageRecord'] as AppUsageRecord | null, child);
        break;
      case 'delegateAccessToken':
        result[key] =
          source['delegateAccessToken'] && isPlainObject(source['delegateAccessToken'])
            ? serializePlainObject(source['delegateAccessToken'], child)
            : null;
        break;
      case 'revoked':
      case 'userErrors':
        result[key] =
          child.name.value === 'revoked'
            ? ((source['revoked'] as AccessScopeRecord[] | undefined) ?? []).map((scope) =>
                serializeAccessScope(scope, child),
              )
            : serializeUserErrors((source['userErrors'] as UserError[] | undefined) ?? [], child);
        break;
      default:
        result[key] = source[child.name.value] ?? null;
    }
  }
  return result;
}

function readLineItemPlan(
  runtime: ProxyRuntimeContext,
  lineItem: Record<string, unknown>,
  subscriptionId: string,
  index: number,
): AppSubscriptionLineItemRecord {
  const plan = isPlainObject(lineItem['plan']) ? lineItem['plan'] : {};
  const recurring = isPlainObject(plan['appRecurringPricingDetails']) ? plan['appRecurringPricingDetails'] : null;
  const usage = isPlainObject(plan['appUsagePricingDetails']) ? plan['appUsagePricingDetails'] : null;
  const pricingDetails = recurring
    ? {
        __typename: 'AppRecurringPricing',
        price: readMoneyInput(recurring['price']),
        interval: readStringValue(recurring['interval']) ?? 'EVERY_30_DAYS',
        planHandle: readStringValue(recurring['planHandle']),
      }
    : {
        __typename: 'AppUsagePricing',
        cappedAmount: readMoneyInput(usage?.['cappedAmount']),
        balanceUsed: { amount: '0.0', currencyCode: readMoneyInput(usage?.['cappedAmount']).currencyCode },
        interval: readStringValue(usage?.['interval']) ?? 'EVERY_30_DAYS',
        terms: readStringValue(usage?.['terms']),
      };

  return {
    id: `${runtime.syntheticIdentity.makeSyntheticGid('AppSubscriptionLineItem')}?v=1&index=${index}`,
    subscriptionId,
    plan: { pricingDetails },
  };
}

function handlePurchaseCreate(
  runtime: ProxyRuntimeContext,
  field: FieldNode,
  variables: Record<string, unknown>,
  origin: string,
): unknown {
  const args = getFieldArguments(field, variables);
  const installation = ensureCurrentInstallation(runtime, origin);
  const purchase: AppOneTimePurchaseRecord = {
    id: runtime.syntheticIdentity.makeSyntheticGid('AppPurchaseOneTime'),
    name: asString(args['name']) ?? '',
    status: 'PENDING',
    test: asBoolean(args['test']),
    createdAt: runtime.syntheticIdentity.makeSyntheticTimestamp(),
    price: readMoneyInput(args['price']),
  };
  runtime.store.stageAppOneTimePurchase(purchase);
  runtime.store.stageAppInstallation({
    ...installation,
    oneTimePurchaseIds: [...installation.oneTimePurchaseIds, purchase.id],
  });

  return serializeMutationPayload(
    runtime,
    {
      appPurchaseOneTime: purchase,
      confirmationUrl: confirmationUrl(origin, 'ApplicationCharge', purchase.id),
      userErrors: [],
    },
    field,
  );
}

function handleSubscriptionCreate(
  runtime: ProxyRuntimeContext,
  field: FieldNode,
  variables: Record<string, unknown>,
  origin: string,
): unknown {
  const args = getFieldArguments(field, variables);
  const installation = ensureCurrentInstallation(runtime, origin);
  const subscriptionId = runtime.syntheticIdentity.makeSyntheticGid('AppSubscription');
  const lineItems = asRecordArray(args['lineItems']).map((lineItem, index) =>
    readLineItemPlan(runtime, lineItem, subscriptionId, index + 1),
  );
  for (const lineItem of lineItems) {
    runtime.store.stageAppSubscriptionLineItem(lineItem);
  }
  const subscription: AppSubscriptionRecord = {
    id: subscriptionId,
    name: asString(args['name']) ?? '',
    status: 'PENDING',
    test: asBoolean(args['test']),
    trialDays: asNumber(args['trialDays']),
    currentPeriodEnd: null,
    createdAt: runtime.syntheticIdentity.makeSyntheticTimestamp(),
    lineItemIds: lineItems.map((lineItem) => lineItem.id),
  };
  runtime.store.stageAppSubscription(subscription);
  runtime.store.stageAppInstallation({
    ...installation,
    allSubscriptionIds: [...installation.allSubscriptionIds, subscription.id],
  });

  return serializeMutationPayload(
    runtime,
    {
      appSubscription: subscription,
      confirmationUrl: confirmationUrl(origin, 'RecurringApplicationCharge', subscription.id),
      userErrors: [],
    },
    field,
  );
}

function handleSubscriptionCancel(
  runtime: ProxyRuntimeContext,
  field: FieldNode,
  variables: Record<string, unknown>,
): unknown {
  const args = getFieldArguments(field, variables);
  const subscriptionId = asString(args['id']);
  const subscription = subscriptionId ? runtime.store.getEffectiveAppSubscriptionById(subscriptionId) : null;
  if (!subscription) {
    return subscriptionPayload(runtime, null, field, [userError(['id'], 'Subscription not found')]);
  }

  const cancelled = runtime.store.stageAppSubscription({ ...subscription, status: 'CANCELLED' });
  const installation = runtime.store.getCurrentAppInstallation();
  if (installation) {
    runtime.store.stageAppInstallation({
      ...installation,
      activeSubscriptionIds: installation.activeSubscriptionIds.filter((id) => id !== cancelled.id),
    });
  }
  return subscriptionPayload(runtime, cancelled, field);
}

function handleLineItemUpdate(
  runtime: ProxyRuntimeContext,
  field: FieldNode,
  variables: Record<string, unknown>,
  origin: string,
): unknown {
  const args = getFieldArguments(field, variables);
  const lineItemId = asString(args['id']);
  const lineItem = lineItemId ? runtime.store.getEffectiveAppSubscriptionLineItemById(lineItemId) : null;
  const subscription = lineItem ? runtime.store.getEffectiveAppSubscriptionById(lineItem.subscriptionId) : null;
  if (!lineItem || !subscription) {
    return serializeMutationPayload(
      runtime,
      {
        appSubscription: null,
        confirmationUrl: null,
        userErrors: [userError(['id'], 'Subscription line item not found')],
      },
      field,
    );
  }

  const pricingDetails = isPlainObject(lineItem.plan['pricingDetails']) ? lineItem.plan['pricingDetails'] : {};
  const cappedAmount = readMoneyInput(args['cappedAmount']);
  runtime.store.stageAppSubscriptionLineItem({
    ...lineItem,
    plan: {
      ...lineItem.plan,
      pricingDetails: {
        ...pricingDetails,
        cappedAmount,
      },
    },
  });

  return serializeMutationPayload(
    runtime,
    {
      appSubscription: subscription,
      confirmationUrl: confirmationUrl(origin, 'RecurringApplicationCharge', subscription.id),
      userErrors: [],
    },
    field,
  );
}

function handleTrialExtend(
  runtime: ProxyRuntimeContext,
  field: FieldNode,
  variables: Record<string, unknown>,
): unknown {
  const args = getFieldArguments(field, variables);
  const subscriptionId = asString(args['id']);
  const days = asNumber(args['days']) ?? 0;
  const subscription = subscriptionId ? runtime.store.getEffectiveAppSubscriptionById(subscriptionId) : null;
  if (!subscription) {
    return subscriptionPayload(runtime, null, field, [userError(['id'], 'Subscription not found')]);
  }

  const extended = runtime.store.stageAppSubscription({
    ...subscription,
    trialDays: (subscription.trialDays ?? 0) + days,
  });
  return subscriptionPayload(runtime, extended, field);
}

function handleUsageRecordCreate(
  runtime: ProxyRuntimeContext,
  field: FieldNode,
  variables: Record<string, unknown>,
): unknown {
  const args = getFieldArguments(field, variables);
  const lineItemId = asString(args['subscriptionLineItemId']);
  const lineItem = lineItemId ? runtime.store.getEffectiveAppSubscriptionLineItemById(lineItemId) : null;
  if (!lineItem) {
    return serializeMutationPayload(
      runtime,
      { appUsageRecord: null, userErrors: [userError(['subscriptionLineItemId'], 'Subscription line item not found')] },
      field,
    );
  }

  const record = runtime.store.stageAppUsageRecord({
    id: runtime.syntheticIdentity.makeSyntheticGid('AppUsageRecord'),
    subscriptionLineItemId: lineItem.id,
    description: asString(args['description']) ?? '',
    price: readMoneyInput(args['price']),
    createdAt: runtime.syntheticIdentity.makeSyntheticTimestamp(),
    idempotencyKey: asString(args['idempotencyKey']),
  });
  return serializeMutationPayload(runtime, { appUsageRecord: record, userErrors: [] }, field);
}

function handleRevokeAccessScopes(
  runtime: ProxyRuntimeContext,
  field: FieldNode,
  variables: Record<string, unknown>,
  origin: string,
): unknown {
  const args = getFieldArguments(field, variables);
  const installation = ensureCurrentInstallation(runtime, origin);
  const requestedScopes = Array.isArray(args['scopes'])
    ? args['scopes'].filter((value): value is string => typeof value === 'string')
    : [];
  const currentHandles = new Set(installation.accessScopes.map((scope) => scope.handle));
  const revoked = installation.accessScopes.filter((scope) => requestedScopes.includes(scope.handle));
  const errors = requestedScopes
    .filter((scope) => !currentHandles.has(scope))
    .map((scope) => userError(['scopes'], `Access scope '${scope}' is not granted.`, 'UNKNOWN_SCOPES'));
  runtime.store.stageAppInstallation({
    ...installation,
    accessScopes: installation.accessScopes.filter((scope) => !requestedScopes.includes(scope.handle)),
  });
  return serializeMutationPayload(runtime, { revoked, userErrors: errors }, field);
}

function handleAppUninstall(runtime: ProxyRuntimeContext, field: FieldNode, origin: string): unknown {
  const installation = ensureCurrentInstallation(runtime, origin);
  const app = runtime.store.getEffectiveAppById(installation.appId);
  runtime.store.stageAppInstallation({
    ...installation,
    uninstalledAt: runtime.syntheticIdentity.makeSyntheticTimestamp(),
  });
  return serializeMutationPayload(runtime, { app, userErrors: [] }, field);
}

function handleDelegateCreate(
  runtime: ProxyRuntimeContext,
  field: FieldNode,
  variables: Record<string, unknown>,
): unknown {
  const args = getFieldArguments(field, variables);
  const input = isPlainObject(args['input']) ? args['input'] : {};
  const delegateAccessScope = asString(input['delegateAccessScope']);
  const legacyAccessScopes = Array.isArray(input['accessScopes'])
    ? input['accessScopes'].filter((value): value is string => typeof value === 'string')
    : [];
  const accessScopes = delegateAccessScope ? [delegateAccessScope] : legacyAccessScopes;
  const rawToken = `shpat_delegate_proxy_${runtime.syntheticIdentity.makeSyntheticGid('DelegateAccessToken').split('/').at(-1)}`;
  const createdAt = runtime.syntheticIdentity.makeSyntheticTimestamp();
  runtime.store.stageDelegatedAccessToken({
    id: runtime.syntheticIdentity.makeSyntheticGid('DelegateAccessToken'),
    accessTokenSha256: tokenHash(rawToken),
    accessTokenPreview: tokenPreview(rawToken),
    accessScopes,
    createdAt,
    expiresIn: asNumber(input['expiresIn']),
    destroyedAt: null,
  });

  return serializeMutationPayload(
    runtime,
    {
      delegateAccessToken: {
        __typename: 'DelegateAccessToken',
        accessToken: rawToken,
        accessScopes,
        createdAt,
        expiresIn: asNumber(input['expiresIn']),
      },
      shop: null,
      userErrors: [],
    },
    field,
  );
}

function handleDelegateDestroy(
  runtime: ProxyRuntimeContext,
  field: FieldNode,
  variables: Record<string, unknown>,
): unknown {
  const args = getFieldArguments(field, variables);
  const accessToken = asString(args['accessToken']);
  const token = accessToken ? runtime.store.findDelegatedAccessTokenByHash(tokenHash(accessToken)) : null;
  if (!token) {
    return serializeMutationPayload(
      runtime,
      {
        status: false,
        shop: null,
        userErrors: [userError(['accessToken'], 'Access token not found.', 'ACCESS_TOKEN_NOT_FOUND')],
      },
      field,
    );
  }

  runtime.store.destroyDelegatedAccessToken(token.id, runtime.syntheticIdentity.makeSyntheticTimestamp());
  return serializeMutationPayload(runtime, { status: true, shop: null, userErrors: [] }, field);
}

export function handleAppMutation(
  runtime: ProxyRuntimeContext,
  document: string,
  variables: Record<string, unknown>,
  origin: string,
): Record<string, unknown> | null {
  const data: Record<string, unknown> = {};
  for (const field of getRootFields(document)) {
    if (!APP_MUTATION_ROOTS.has(field.name.value)) {
      return null;
    }

    const key = getFieldResponseKey(field);
    switch (field.name.value) {
      case 'appPurchaseOneTimeCreate':
        data[key] = handlePurchaseCreate(runtime, field, variables, origin);
        break;
      case 'appSubscriptionCreate':
        data[key] = handleSubscriptionCreate(runtime, field, variables, origin);
        break;
      case 'appSubscriptionCancel':
        data[key] = handleSubscriptionCancel(runtime, field, variables);
        break;
      case 'appSubscriptionLineItemUpdate':
        data[key] = handleLineItemUpdate(runtime, field, variables, origin);
        break;
      case 'appSubscriptionTrialExtend':
        data[key] = handleTrialExtend(runtime, field, variables);
        break;
      case 'appUsageRecordCreate':
        data[key] = handleUsageRecordCreate(runtime, field, variables);
        break;
      case 'appRevokeAccessScopes':
        data[key] = handleRevokeAccessScopes(runtime, field, variables, origin);
        break;
      case 'appUninstall':
        data[key] = handleAppUninstall(runtime, field, origin);
        break;
      case 'delegateAccessTokenCreate':
        data[key] = handleDelegateCreate(runtime, field, variables);
        break;
      case 'delegateAccessTokenDestroy':
        data[key] = handleDelegateDestroy(runtime, field, variables);
        break;
    }
  }

  return { data };
}

export function hydrateAppsFromUpstreamResponse(runtime: ProxyRuntimeContext, upstreamPayload: unknown): void {
  if (!isPlainObject(upstreamPayload) || !isPlainObject(upstreamPayload['data'])) {
    return;
  }

  for (const value of Object.values(upstreamPayload['data'])) {
    if (!isPlainObject(value) || typeof value['id'] !== 'string') {
      continue;
    }

    if (!String(value['id']).startsWith('gid://shopify/AppInstallation/')) {
      continue;
    }

    const appPayload = isPlainObject(value['app']) ? value['app'] : null;
    if (!appPayload || typeof appPayload['id'] !== 'string') {
      continue;
    }

    const app: AppRecord = {
      id: appPayload['id'],
      apiKey: readStringValue(appPayload['apiKey']),
      handle: readStringValue(appPayload['handle']),
      title: readStringValue(appPayload['title']),
      developerName: readStringValue(appPayload['developerName']),
      embedded: typeof appPayload['embedded'] === 'boolean' ? appPayload['embedded'] : null,
      previouslyInstalled:
        typeof appPayload['previouslyInstalled'] === 'boolean' ? appPayload['previouslyInstalled'] : null,
      requestedAccessScopes: asRecordArray(appPayload['requestedAccessScopes']).map((scope) => ({
        handle: readStringValue(scope['handle']) ?? '',
        description: readStringValue(scope['description']),
      })),
    };
    const installation: AppInstallationRecord = {
      id: value['id'],
      appId: app.id,
      launchUrl: readStringValue(value['launchUrl']),
      uninstallUrl: readStringValue(value['uninstallUrl']),
      accessScopes: asRecordArray(value['accessScopes']).map((scope) => ({
        handle: readStringValue(scope['handle']) ?? '',
        description: readStringValue(scope['description']),
      })),
      activeSubscriptionIds: [],
      allSubscriptionIds: [],
      oneTimePurchaseIds: [],
      uninstalledAt: null,
    };
    runtime.store.upsertBaseAppInstallation(installation, app);
  }
}
