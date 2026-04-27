import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import request from 'supertest';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { createApp } from '../../src/app.js';
import type { AppConfig } from '../../src/config.js';
import { store } from '../../src/state/store.js';
import { resetSyntheticIdentity } from '../../src/state/synthetic-identity.js';
import {
  buildPerformanceSmokeSnapshot,
  makePerformanceSmokeOrder,
  makePerformanceSmokeProduct,
  performanceSmokeCounts,
  performanceSmokeTargets,
} from '../fixtures/performance-smoke-snapshot.js';

const config: AppConfig = {
  port: 3000,
  shopifyAdminOrigin: 'https://example.myshopify.com',
  readMode: 'snapshot',
};

const maxTotalRuntimeMs = Number(process.env['SHOPIFY_DRAFT_PROXY_PERF_SMOKE_MAX_MS'] ?? '10000');

type TimedMetrics = {
  name: string;
  maxMs: number;
  runs: number;
};

function writeSnapshotFile(): { path: string; directory: string } {
  const directory = mkdtempSync(join(tmpdir(), 'shopify-draft-proxy-performance-smoke-'));
  const path = join(directory, 'large-normalized-snapshot.json');
  writeFileSync(path, `${JSON.stringify(buildPerformanceSmokeSnapshot())}\n`, 'utf8');
  return { path, directory };
}

async function timedRequest<T>(name: string, metrics: TimedMetrics[], action: () => Promise<T>): Promise<T> {
  const startedAt = performance.now();
  const result = await action();
  const elapsedMs = performance.now() - startedAt;
  const existing = metrics.find((metric) => metric.name === name);
  if (existing) {
    existing.maxMs = Math.max(existing.maxMs, elapsedMs);
    existing.runs += 1;
  } else {
    metrics.push({ name, maxMs: elapsedMs, runs: 1 });
  }
  return result;
}

describe('performance smoke coverage', () => {
  beforeEach(() => {
    store.reset();
    resetSyntheticIdentity();
    vi.restoreAllMocks();
  });

  it('bounds large snapshot and hot overlay read behavior without live Shopify access', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockRejectedValue(new Error('performance smoke must stay local'));
    const snapshotFile = writeSnapshotFile();

    try {
      const app = createApp({ ...config, snapshotPath: snapshotFile.path }).callback();
      const stagedProduct = {
        ...makePerformanceSmokeProduct(1_234),
        title: 'Hot Overlay Product',
        tags: ['hot-smoke', 'baseline-smoke'],
        updatedAt: '2026-04-27T00:00:00.000Z',
      };
      const stagedCustomer = store.getEffectiveCustomerById(performanceSmokeTargets.customerId);
      const stagedCatalog = store.getEffectiveCatalogRecordById(performanceSmokeTargets.catalogId);

      store.stageUpdateProduct(stagedProduct);
      if (!stagedCustomer) {
        throw new Error('Missing performance smoke customer fixture target');
      }
      store.stageUpdateCustomer({
        ...stagedCustomer,
        note: 'hot overlay customer',
        tags: ['hot-smoke', 'baseline-smoke'],
        updatedAt: '2026-04-27T00:00:00.000Z',
      });
      if (!stagedCatalog) {
        throw new Error('Missing performance smoke catalog fixture target');
      }
      store.stageUpdateCatalog({
        ...stagedCatalog,
        data: {
          ...stagedCatalog.data,
          title: 'hot-smoke-catalog',
          status: 'ACTIVE',
        },
      });
      for (let index = 1; index <= performanceSmokeCounts.orders; index += 1) {
        store.stageCreateOrder(
          makePerformanceSmokeOrder(index, {
            tags: index === 404 || index % 50 === 0 ? ['hot-smoke'] : [],
            displayFinancialStatus: index % 2 === 0 ? 'PAID' : 'PENDING',
            note: index === 404 ? 'hot overlay order' : null,
          }),
        );
      }

      const metrics: TimedMetrics[] = [];
      const startedAt = performance.now();
      let productResponse: request.Response | null = null;
      let customerResponse: request.Response | null = null;
      let orderResponse: request.Response | null = null;
      let catalogResponse: request.Response | null = null;

      for (let run = 0; run < performanceSmokeCounts.repeats; run += 1) {
        productResponse = await timedRequest('product overlay read', metrics, () =>
          request(app)
            .post('/admin/api/2026-04/graphql.json')
            .send({
              query: `#graphql
                query ProductSmoke($id: ID!, $query: String!) {
                  product(id: $id) {
                    id
                    title
                    tags
                    variants(first: 2) {
                      nodes { id sku inventoryQuantity }
                      pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
                    }
                    collections(first: 2) {
                      nodes { id title handle }
                      pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
                    }
                  }
                  catalog: products(first: 5, query: $query, sortKey: UPDATED_AT, reverse: true) {
                    nodes { id title tags updatedAt }
                    pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
                  }
                  total: productsCount(query: $query) { count precision }
                }
              `,
              variables: {
                id: performanceSmokeTargets.productId,
                query: 'tag:hot-smoke',
              },
            }),
        );

        customerResponse = await timedRequest('customer overlay read', metrics, () =>
          request(app)
            .post('/admin/api/2026-04/graphql.json')
            .send({
              query: `#graphql
                query CustomerSmoke($id: ID!, $query: String!) {
                  customer(id: $id) {
                    id
                    email
                    note
                    tags
                    orders(first: 1) {
                      nodes { id }
                      pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
                    }
                  }
                  catalog: customers(first: 5, query: $query, sortKey: UPDATED_AT, reverse: true) {
                    nodes { id email note tags updatedAt }
                    pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
                  }
                  total: customersCount(query: $query) { count precision }
                }
              `,
              variables: {
                id: performanceSmokeTargets.customerId,
                query: 'tag:hot-smoke',
              },
            }),
        );

        orderResponse = await timedRequest('order overlay read', metrics, () =>
          request(app)
            .post('/admin/api/2026-04/graphql.json')
            .send({
              query: `#graphql
                query OrderSmoke($id: ID!, $query: String!) {
                  order(id: $id) { id name note tags displayFinancialStatus }
                  catalog: orders(first: 3, query: $query, sortKey: CREATED_AT, reverse: true) {
                    nodes { id name tags displayFinancialStatus }
                    pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
                  }
                  total: ordersCount(query: $query, limit: 2) { count precision }
                }
              `,
              variables: {
                id: performanceSmokeTargets.orderId,
                query: 'tag:hot-smoke financial_status:paid',
              },
            }),
        );

        catalogResponse = await timedRequest('catalog overlay read', metrics, () =>
          request(app)
            .post('/admin/api/2026-04/graphql.json')
            .send({
              query: `#graphql
                query CatalogSmoke($id: ID!, $query: String!) {
                  catalog(id: $id) { id title status }
                  catalogList: catalogs(first: 4, query: $query, sortKey: TITLE) {
                    nodes { id title status }
                    pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
                  }
                  total: catalogsCount(query: $query, limit: 3) { count precision }
                }
              `,
              variables: {
                id: performanceSmokeTargets.catalogId,
                query: 'title:hot-smoke-catalog',
              },
            }),
        );
      }

      const totalElapsedMs = performance.now() - startedAt;
      const summary = {
        fixtureCounts: performanceSmokeCounts,
        maxTotalRuntimeMs,
        totalElapsedMs: Number(totalElapsedMs.toFixed(2)),
        requests: metrics.map((metric) => ({
          ...metric,
          maxMs: Number(metric.maxMs.toFixed(2)),
        })),
        responseWindows: {
          products: productResponse?.body.data.catalog.nodes.length,
          customers: customerResponse?.body.data.catalog.nodes.length,
          orders: orderResponse?.body.data.catalog.nodes.length,
          catalogs: catalogResponse?.body.data.catalogList.nodes.length,
        },
      };
      process.stdout.write(`performance-smoke ${JSON.stringify(summary)}\n`);

      expect(productResponse?.status).toBe(200);
      expect(productResponse?.body.data.product).toMatchObject({
        id: performanceSmokeTargets.productId,
        title: 'Hot Overlay Product',
        tags: ['hot-smoke', 'baseline-smoke'],
      });
      expect(productResponse?.body.data.catalog.nodes.length).toBeGreaterThanOrEqual(1);
      expect(productResponse?.body.data.catalog.nodes.length).toBeLessThanOrEqual(5);
      expect(productResponse?.body.data.total.count).toBeGreaterThanOrEqual(1);

      expect(customerResponse?.status).toBe(200);
      expect(customerResponse?.body.data.customer).toMatchObject({
        id: performanceSmokeTargets.customerId,
        note: 'hot overlay customer',
        tags: ['hot-smoke', 'baseline-smoke'],
      });
      expect(customerResponse?.body.data.catalog.nodes.length).toBeLessThanOrEqual(5);
      expect(customerResponse?.body.data.total.count).toBeGreaterThanOrEqual(1);

      expect(orderResponse?.status).toBe(200);
      expect(orderResponse?.body.data.order).toMatchObject({
        id: performanceSmokeTargets.orderId,
        note: 'hot overlay order',
        tags: ['hot-smoke'],
      });
      expect(orderResponse?.body.data.catalog.nodes).toHaveLength(3);
      expect(orderResponse?.body.data.total).toEqual({ count: 2, precision: 'AT_LEAST' });

      expect(catalogResponse?.status).toBe(200);
      expect(catalogResponse?.body.data.catalog).toEqual({
        id: performanceSmokeTargets.catalogId,
        title: 'hot-smoke-catalog',
        status: 'ACTIVE',
      });
      expect(catalogResponse?.body.data.catalogList.nodes).toHaveLength(1);
      expect(catalogResponse?.body.data.total).toEqual({ count: 1, precision: 'EXACT' });

      expect(totalElapsedMs).toBeLessThan(maxTotalRuntimeMs);
      expect(fetchSpy).not.toHaveBeenCalled();
    } finally {
      rmSync(snapshotFile.directory, { recursive: true, force: true });
    }
  });
});
