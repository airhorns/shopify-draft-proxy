import type { AppConfig } from '../config.js';
import { makeSyntheticGid, makeSyntheticTimestamp } from '../state/synthetic-identity.js';
import { store } from '../state/store.js';
import type { MutationLogEntry, ProductRecord, WebhookSubscriptionRecord } from '../state/types.js';

const SUPPORTED_PRODUCT_CREATE_TOPIC = 'PRODUCTS_CREATE';

interface EnqueueProductCreateWebhookOutboxOptions {
  product: ProductRecord;
  mutationLogEntry: MutationLogEntry;
  mutationLogIndex: number;
  config: AppConfig;
  path: string;
}

function readShopDomain(shopifyAdminOrigin: string): string | null {
  try {
    return new URL(shopifyAdminOrigin).hostname;
  } catch {
    return null;
  }
}

function readAdminApiVersion(path: string): string | null {
  return path.match(/\/admin\/api\/([^/]+)\/graphql\.json/u)?.[1] ?? null;
}

function readLegacyNumericId(gid: string, legacyResourceId: string | null): number | null {
  const rawId = legacyResourceId ?? gid.match(/^gid:\/\/shopify\/[^/]+\/([^?]+)/u)?.[1] ?? null;
  if (!rawId || !/^\d+$/u.test(rawId)) {
    return null;
  }
  return Number(rawId);
}

function isSupportedProductCreateSubscription(subscription: WebhookSubscriptionRecord): boolean {
  return (
    subscription.topic === SUPPORTED_PRODUCT_CREATE_TOPIC &&
    (subscription.format === null || subscription.format === 'JSON') &&
    subscription.includeFields.length === 0 &&
    subscription.metafieldNamespaces.length === 0 &&
    (subscription.filter === null || subscription.filter === '')
  );
}

function buildProductCreatePayload(product: ProductRecord): Record<string, string | number | null> {
  return {
    id: readLegacyNumericId(product.id, product.legacyResourceId),
    admin_graphql_api_id: product.id,
    title: product.title,
    handle: product.handle,
    status: product.status.toLowerCase(),
    vendor: product.vendor,
    product_type: product.productType,
    tags: product.tags.join(', '),
    body_html: product.descriptionHtml,
    created_at: product.createdAt,
    updated_at: product.updatedAt,
    published_at: product.publishedAt ?? null,
    template_suffix: product.templateSuffix,
  };
}

export function enqueueProductCreateWebhookOutboxRecords(options: EnqueueProductCreateWebhookOutboxOptions): void {
  const subscriptions = store.listEffectiveWebhookSubscriptions().filter(isSupportedProductCreateSubscription);
  if (subscriptions.length === 0) {
    return;
  }

  const shopDomain = readShopDomain(options.config.shopifyAdminOrigin);
  const apiVersion = readAdminApiVersion(options.path);

  for (const subscription of subscriptions) {
    const recordedAt = makeSyntheticTimestamp();
    const deliveryId = makeSyntheticGid('WebhookDelivery');

    store.appendWebhookOutboxRecord({
      id: deliveryId,
      recordedAt,
      topic: SUPPORTED_PRODUCT_CREATE_TOPIC,
      subscriptionId: subscription.id,
      endpoint: subscription.endpoint,
      format: subscription.format,
      includeFields: subscription.includeFields,
      metafieldNamespaces: subscription.metafieldNamespaces,
      filter: subscription.filter,
      sourceMutationLogEntryId: options.mutationLogEntry.id,
      sourceMutationLogIndex: options.mutationLogIndex,
      sourceMutationLog: {
        id: options.mutationLogEntry.id,
        index: options.mutationLogIndex,
      },
      resourceGid: options.product.id,
      payload: buildProductCreatePayload(options.product),
      headers: {
        'x-shopify-topic': 'products/create',
        'x-shopify-shop-domain': shopDomain,
        'x-shopify-api-version': apiVersion,
        'x-shopify-webhook-id': deliveryId,
        'x-shopify-triggered-at': recordedAt,
        'x-shopify-hmac-sha256': null,
      },
      delivery: {
        mode: 'recorded',
        status: 'recorded',
        attempts: [],
      },
    });
  }
}
