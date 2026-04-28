import { createHash } from 'node:crypto';
import type { FieldNode } from 'graphql';

import { getFieldArguments, getRootFields } from '../graphql/root-field.js';
import { store } from '../state/store.js';
import { makeSyntheticGid, makeSyntheticTimestamp } from '../state/synthetic-identity.js';
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

function defaultApp(): AppRecord {
  return {
    id: makeSyntheticGid('App'),
    apiKey: 'shopify-draft-proxy-local-app',
    handle: 'shopify-draft-proxy',
    title: 'shopify-draft-proxy',
    developerName: 'shopify-draft-proxy',
    embedded: true,
    previouslyInstalled: false,
    requestedAccessScopes: [accessScope('read_products'), accessScope('write_products')],
  };
}

function ensureCurrentInstallation(origin: string): AppInstallationRecord {
  const existing = store.getCurrentAppInstallation();
  if (existing) {
    return existing;
  }

  const app = defaultApp();
  store.stageApp(app);
  return store.stageAppInstallation({
    id: makeSyntheticGid('AppInstallation'),
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

function getAppRecord(appId: string): AppRecord | null {
  return store.getEffectiveAppById(appId);
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

function serializeAppSubscriptionLineItem(lineItem: AppSubscriptionLineItemRecord, field: FieldNode): unknown {
  const source = {
    id: lineItem.id,
    plan: lineItem.plan,
    usageRecords: {
      nodes: store.listEffectiveAppUsageRecordsForLineItem(lineItem.id),
    },
  };
  const result: Record<string, unknown> = {};
  for (const child of getSelectedChildFields(field, { includeInlineFragments: true })) {
    const key = getFieldResponseKey(child);
    if (child.name.value === '__typename') {
      result[key] = 'AppSubscriptionLineItem';
    } else if (child.name.value === 'usageRecords') {
      result[key] = serializeUsageRecordConnection(store.listEffectiveAppUsageRecordsForLineItem(lineItem.id), child);
    } else {
      result[key] = child.selectionSet
        ? projectGraphqlObject(source, [child], getDocumentFragments('query X { __typename }'))[key]
        : ((source as Record<string, unknown>)[child.name.value] ?? null);
    }
  }
  return result;
}

function serializeAppSubscription(subscription: AppSubscriptionRecord | null, field: FieldNode): unknown {
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
          .map((lineItemId) => store.getEffectiveAppSubscriptionLineItemById(lineItemId))
          .filter((lineItem): lineItem is AppSubscriptionLineItemRecord => lineItem !== null)
          .map((lineItem) => serializeAppSubscriptionLineItem(lineItem, child));
        break;
      default:
        result[key] = (subscription as unknown as Record<string, unknown>)[child.name.value] ?? null;
    }
  }
  return result;
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

function serializeUsageRecord(record: AppUsageRecord | null, field: FieldNode): unknown {
  if (!record) {
    return null;
  }

  const lineItem = store.getEffectiveAppSubscriptionLineItemById(record.subscriptionLineItemId);
  const source = { __typename: 'AppUsageRecord', ...record, subscriptionLineItem: lineItem };
  return projectGraphqlObject(
    source,
    field.selectionSet?.selections ?? [],
    getDocumentFragments('query X { __typename }'),
    {
      projectFieldValue({ field: child, fieldName }) {
        if (fieldName === 'subscriptionLineItem') {
          return { handled: true, value: lineItem ? serializeAppSubscriptionLineItem(lineItem, child) : null };
        }
        return { handled: false };
      },
    },
  );
}

function serializeSubscriptionConnection(subscriptions: AppSubscriptionRecord[], field: FieldNode): unknown {
  const window = paginateConnectionItems(subscriptions, field, {}, (subscription) => subscription.id);
  return serializeConnection(field, {
    ...window,
    getCursorValue: (subscription) => subscription.id,
    serializeNode: (subscription, nodeField) => serializeAppSubscription(subscription, nodeField),
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

function serializeUsageRecordConnection(records: AppUsageRecord[], field: FieldNode): unknown {
  const window = paginateConnectionItems(records, field, {}, (record) => record.id);
  return serializeConnection(field, {
    ...window,
    getCursorValue: (record) => record.id,
    serializeNode: (record, nodeField) => serializeUsageRecord(record, nodeField),
  });
}

function serializeAppInstallation(installation: AppInstallationRecord | null, field: FieldNode): unknown {
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
        result[key] = serializeApp(getAppRecord(installation.appId), child);
        break;
      case 'accessScopes':
        result[key] = installation.accessScopes.map((scope) => serializeAccessScope(scope, child));
        break;
      case 'activeSubscriptions':
        result[key] = installation.activeSubscriptionIds
          .map((id) => store.getEffectiveAppSubscriptionById(id))
          .filter((subscription): subscription is AppSubscriptionRecord => subscription !== null)
          .filter((subscription) => subscription.status === 'ACTIVE')
          .map((subscription) => serializeAppSubscription(subscription, child));
        break;
      case 'allSubscriptions':
        result[key] = serializeSubscriptionConnection(
          installation.allSubscriptionIds
            .map((id) => store.getEffectiveAppSubscriptionById(id))
            .filter((subscription): subscription is AppSubscriptionRecord => subscription !== null),
          child,
        );
        break;
      case 'oneTimePurchases':
        result[key] = serializeOneTimePurchaseConnection(
          installation.oneTimePurchaseIds
            .map((id) => store.getEffectiveAppOneTimePurchaseById(id))
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

function serializeAppInstallationConnection(installations: AppInstallationRecord[], field: FieldNode): unknown {
  const window = paginateConnectionItems(installations, field, {}, (installation) => installation.id);
  return serializeConnection(field, {
    ...window,
    getCursorValue: (installation) => installation.id,
    serializeNode: (installation, nodeField) => serializeAppInstallation(installation, nodeField),
  });
}

function rootFieldData(document: string, variables: Record<string, unknown>): Record<string, unknown> {
  const data: Record<string, unknown> = {};
  for (const field of getRootFields(document)) {
    const key = getFieldResponseKey(field);
    const args = getFieldArguments(field, variables);
    switch (field.name.value) {
      case 'currentAppInstallation':
        data[key] = serializeAppInstallation(store.getCurrentAppInstallation(), field);
        break;
      case 'appInstallation':
        data[key] = serializeAppInstallation(
          asString(args['id']) ? store.getEffectiveAppInstallationById(String(args['id'])) : null,
          field,
        );
        break;
      case 'app': {
        const appId = asString(args['id']);
        data[key] = serializeApp(appId ? store.getEffectiveAppById(appId) : null, field);
        break;
      }
      case 'appByHandle': {
        const handle = asString(args['handle']);
        data[key] = serializeApp(handle ? store.findEffectiveAppByHandle(handle) : null, field);
        break;
      }
      case 'appByKey': {
        const apiKey = asString(args['apiKey']);
        data[key] = serializeApp(apiKey ? store.findEffectiveAppByApiKey(apiKey) : null, field);
        break;
      }
      case 'appInstallations': {
        const current = store.getCurrentAppInstallation();
        data[key] = serializeAppInstallationConnection(current ? [current] : [], field);
        break;
      }
    }
  }
  return data;
}

export function handleAppQuery(document: string, variables: Record<string, unknown>): Record<string, unknown> {
  return { data: rootFieldData(document, variables) };
}

function subscriptionPayload(
  subscription: AppSubscriptionRecord | null,
  field: FieldNode,
  errors: UserError[] = [],
): unknown {
  return serializeMutationPayload({ appSubscription: subscription, userErrors: errors }, field);
}

function serializeMutationPayload(source: Record<string, unknown>, field: FieldNode): Record<string, unknown> {
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
        result[key] = serializeAppSubscription(source['appSubscription'] as AppSubscriptionRecord | null, child);
        break;
      case 'appUsageRecord':
        result[key] = serializeUsageRecord(source['appUsageRecord'] as AppUsageRecord | null, child);
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
    id: `${makeSyntheticGid('AppSubscriptionLineItem')}?v=1&index=${index}`,
    subscriptionId,
    plan: { pricingDetails },
  };
}

function handlePurchaseCreate(field: FieldNode, variables: Record<string, unknown>, origin: string): unknown {
  const args = getFieldArguments(field, variables);
  const installation = ensureCurrentInstallation(origin);
  const purchase: AppOneTimePurchaseRecord = {
    id: makeSyntheticGid('AppPurchaseOneTime'),
    name: asString(args['name']) ?? '',
    status: 'PENDING',
    test: asBoolean(args['test']),
    createdAt: makeSyntheticTimestamp(),
    price: readMoneyInput(args['price']),
  };
  store.stageAppOneTimePurchase(purchase);
  store.stageAppInstallation({
    ...installation,
    oneTimePurchaseIds: [...installation.oneTimePurchaseIds, purchase.id],
  });

  return serializeMutationPayload(
    {
      appPurchaseOneTime: purchase,
      confirmationUrl: confirmationUrl(origin, 'ApplicationCharge', purchase.id),
      userErrors: [],
    },
    field,
  );
}

function handleSubscriptionCreate(field: FieldNode, variables: Record<string, unknown>, origin: string): unknown {
  const args = getFieldArguments(field, variables);
  const installation = ensureCurrentInstallation(origin);
  const subscriptionId = makeSyntheticGid('AppSubscription');
  const lineItems = asRecordArray(args['lineItems']).map((lineItem, index) =>
    readLineItemPlan(lineItem, subscriptionId, index + 1),
  );
  for (const lineItem of lineItems) {
    store.stageAppSubscriptionLineItem(lineItem);
  }
  const subscription: AppSubscriptionRecord = {
    id: subscriptionId,
    name: asString(args['name']) ?? '',
    status: 'PENDING',
    test: asBoolean(args['test']),
    trialDays: asNumber(args['trialDays']),
    currentPeriodEnd: null,
    createdAt: makeSyntheticTimestamp(),
    lineItemIds: lineItems.map((lineItem) => lineItem.id),
  };
  store.stageAppSubscription(subscription);
  store.stageAppInstallation({
    ...installation,
    allSubscriptionIds: [...installation.allSubscriptionIds, subscription.id],
  });

  return serializeMutationPayload(
    {
      appSubscription: subscription,
      confirmationUrl: confirmationUrl(origin, 'RecurringApplicationCharge', subscription.id),
      userErrors: [],
    },
    field,
  );
}

function handleSubscriptionCancel(field: FieldNode, variables: Record<string, unknown>): unknown {
  const args = getFieldArguments(field, variables);
  const subscriptionId = asString(args['id']);
  const subscription = subscriptionId ? store.getEffectiveAppSubscriptionById(subscriptionId) : null;
  if (!subscription) {
    return subscriptionPayload(null, field, [userError(['id'], 'Subscription not found')]);
  }

  const cancelled = store.stageAppSubscription({ ...subscription, status: 'CANCELLED' });
  const installation = store.getCurrentAppInstallation();
  if (installation) {
    store.stageAppInstallation({
      ...installation,
      activeSubscriptionIds: installation.activeSubscriptionIds.filter((id) => id !== cancelled.id),
    });
  }
  return subscriptionPayload(cancelled, field);
}

function handleLineItemUpdate(field: FieldNode, variables: Record<string, unknown>, origin: string): unknown {
  const args = getFieldArguments(field, variables);
  const lineItemId = asString(args['id']);
  const lineItem = lineItemId ? store.getEffectiveAppSubscriptionLineItemById(lineItemId) : null;
  const subscription = lineItem ? store.getEffectiveAppSubscriptionById(lineItem.subscriptionId) : null;
  if (!lineItem || !subscription) {
    return serializeMutationPayload(
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
  store.stageAppSubscriptionLineItem({
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
    {
      appSubscription: subscription,
      confirmationUrl: confirmationUrl(origin, 'RecurringApplicationCharge', subscription.id),
      userErrors: [],
    },
    field,
  );
}

function handleTrialExtend(field: FieldNode, variables: Record<string, unknown>): unknown {
  const args = getFieldArguments(field, variables);
  const subscriptionId = asString(args['id']);
  const days = asNumber(args['days']) ?? 0;
  const subscription = subscriptionId ? store.getEffectiveAppSubscriptionById(subscriptionId) : null;
  if (!subscription) {
    return subscriptionPayload(null, field, [userError(['id'], 'Subscription not found')]);
  }

  const extended = store.stageAppSubscription({
    ...subscription,
    trialDays: (subscription.trialDays ?? 0) + days,
  });
  return subscriptionPayload(extended, field);
}

function handleUsageRecordCreate(field: FieldNode, variables: Record<string, unknown>): unknown {
  const args = getFieldArguments(field, variables);
  const lineItemId = asString(args['subscriptionLineItemId']);
  const lineItem = lineItemId ? store.getEffectiveAppSubscriptionLineItemById(lineItemId) : null;
  if (!lineItem) {
    return serializeMutationPayload(
      { appUsageRecord: null, userErrors: [userError(['subscriptionLineItemId'], 'Subscription line item not found')] },
      field,
    );
  }

  const record = store.stageAppUsageRecord({
    id: makeSyntheticGid('AppUsageRecord'),
    subscriptionLineItemId: lineItem.id,
    description: asString(args['description']) ?? '',
    price: readMoneyInput(args['price']),
    createdAt: makeSyntheticTimestamp(),
    idempotencyKey: asString(args['idempotencyKey']),
  });
  return serializeMutationPayload({ appUsageRecord: record, userErrors: [] }, field);
}

function handleRevokeAccessScopes(field: FieldNode, variables: Record<string, unknown>, origin: string): unknown {
  const args = getFieldArguments(field, variables);
  const installation = ensureCurrentInstallation(origin);
  const requestedScopes = Array.isArray(args['scopes'])
    ? args['scopes'].filter((value): value is string => typeof value === 'string')
    : [];
  const currentHandles = new Set(installation.accessScopes.map((scope) => scope.handle));
  const revoked = installation.accessScopes.filter((scope) => requestedScopes.includes(scope.handle));
  const errors = requestedScopes
    .filter((scope) => !currentHandles.has(scope))
    .map((scope) => userError(['scopes'], `Access scope '${scope}' is not granted.`, 'UNKNOWN_SCOPES'));
  store.stageAppInstallation({
    ...installation,
    accessScopes: installation.accessScopes.filter((scope) => !requestedScopes.includes(scope.handle)),
  });
  return serializeMutationPayload({ revoked, userErrors: errors }, field);
}

function handleAppUninstall(field: FieldNode, origin: string): unknown {
  const installation = ensureCurrentInstallation(origin);
  const app = store.getEffectiveAppById(installation.appId);
  store.stageAppInstallation({ ...installation, uninstalledAt: makeSyntheticTimestamp() });
  return serializeMutationPayload({ app, userErrors: [] }, field);
}

function handleDelegateCreate(field: FieldNode, variables: Record<string, unknown>): unknown {
  const args = getFieldArguments(field, variables);
  const input = isPlainObject(args['input']) ? args['input'] : {};
  const accessScopes = Array.isArray(input['accessScopes'])
    ? input['accessScopes'].filter((value): value is string => typeof value === 'string')
    : [];
  const rawToken = `shpat_delegate_proxy_${makeSyntheticGid('DelegateAccessToken').split('/').at(-1)}`;
  const createdAt = makeSyntheticTimestamp();
  store.stageDelegatedAccessToken({
    id: makeSyntheticGid('DelegateAccessToken'),
    accessTokenSha256: tokenHash(rawToken),
    accessTokenPreview: tokenPreview(rawToken),
    accessScopes,
    createdAt,
    expiresIn: asNumber(input['expiresIn']),
    destroyedAt: null,
  });

  return serializeMutationPayload(
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

function handleDelegateDestroy(field: FieldNode, variables: Record<string, unknown>): unknown {
  const args = getFieldArguments(field, variables);
  const accessToken = asString(args['accessToken']);
  const token = accessToken ? store.findDelegatedAccessTokenByHash(tokenHash(accessToken)) : null;
  if (!token) {
    return serializeMutationPayload(
      {
        status: false,
        shop: null,
        userErrors: [userError(['accessToken'], 'Access token not found.', 'ACCESS_TOKEN_NOT_FOUND')],
      },
      field,
    );
  }

  store.destroyDelegatedAccessToken(token.id, makeSyntheticTimestamp());
  return serializeMutationPayload({ status: true, shop: null, userErrors: [] }, field);
}

export function handleAppMutation(
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
        data[key] = handlePurchaseCreate(field, variables, origin);
        break;
      case 'appSubscriptionCreate':
        data[key] = handleSubscriptionCreate(field, variables, origin);
        break;
      case 'appSubscriptionCancel':
        data[key] = handleSubscriptionCancel(field, variables);
        break;
      case 'appSubscriptionLineItemUpdate':
        data[key] = handleLineItemUpdate(field, variables, origin);
        break;
      case 'appSubscriptionTrialExtend':
        data[key] = handleTrialExtend(field, variables);
        break;
      case 'appUsageRecordCreate':
        data[key] = handleUsageRecordCreate(field, variables);
        break;
      case 'appRevokeAccessScopes':
        data[key] = handleRevokeAccessScopes(field, variables, origin);
        break;
      case 'appUninstall':
        data[key] = handleAppUninstall(field, origin);
        break;
      case 'delegateAccessTokenCreate':
        data[key] = handleDelegateCreate(field, variables);
        break;
      case 'delegateAccessTokenDestroy':
        data[key] = handleDelegateDestroy(field, variables);
        break;
    }
  }

  return { data };
}

export function hydrateAppsFromUpstreamResponse(upstreamPayload: unknown): void {
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
    store.upsertBaseAppInstallation(installation, app);
  }
}
