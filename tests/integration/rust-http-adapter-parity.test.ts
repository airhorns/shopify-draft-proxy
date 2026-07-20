import { createServer, type Server } from 'node:http';
import { spawn, type ChildProcessWithoutNullStreams } from 'node:child_process';
import { once } from 'node:events';
import type { AddressInfo } from 'node:net';
import { setTimeout as delay } from 'node:timers/promises';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const repoRoot = new URL('../..', import.meta.url);
const integrationCargoTargetDir = fileURLToPath(new URL('../../target/integration-rust-server', import.meta.url));
const pnpmCommand = 'corepack';
const serverStartupTimeoutMs = 90_000;
const adapterTestTimeoutMs = serverStartupTimeoutMs + 30_000;

function pnpmArgs(args: string[]): string[] {
  return ['pnpm', ...args];
}

function collectOutput(child: ChildProcessWithoutNullStreams): { getOutput: () => string } {
  let output = '';
  child.stdout.on('data', (chunk: Buffer) => {
    output += chunk.toString();
  });
  child.stderr.on('data', (chunk: Buffer) => {
    output += chunk.toString();
  });
  return { getOutput: () => output };
}

async function waitForRustServer(child: ChildProcessWithoutNullStreams, getOutput: () => string): Promise<void> {
  const deadline = Date.now() + serverStartupTimeoutMs;
  while (Date.now() < deadline) {
    if (getOutput().includes('shopify-draft-proxy rust runtime listening')) return;
    if (child.exitCode !== null) {
      throw new Error(`server process exited before listening:\n${getOutput()}`);
    }
    await delay(100);
  }
  throw new Error(`server did not start before timeout:\n${getOutput()}`);
}

async function stopServer(child: ChildProcessWithoutNullStreams): Promise<void> {
  if (child.exitCode !== null) return;
  killServerProcess(child, 'SIGTERM');
  for (let attempt = 0; attempt < 20; attempt += 1) {
    if (child.exitCode !== null) return;
    await delay(100);
  }
  killServerProcess(child, 'SIGKILL');
}

function killServerProcess(child: ChildProcessWithoutNullStreams, signal: NodeJS.Signals): void {
  try {
    if (child.pid) {
      process.kill(-child.pid, signal);
      return;
    }
    child.kill(signal);
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code !== 'ESRCH') throw error;
  }
}

async function withRustServer<T>(
  port: number,
  run: (origin: string) => Promise<T>,
  options: { shopifyAdminOrigin?: string; readMode?: string } = {},
): Promise<T> {
  const child = spawn(pnpmCommand, pnpmArgs(['dev']), {
    cwd: repoRoot,
    detached: true,
    env: {
      ...process.env,
      PORT: String(port),
      SHOPIFY_ADMIN_ORIGIN: options.shopifyAdminOrigin ?? 'https://shopify.com',
      READ_MODE: options.readMode,
      CARGO_TARGET_DIR: integrationCargoTargetDir,
    },
  });
  const { getOutput } = collectOutput(child);
  try {
    await waitForRustServer(child, getOutput);
    return await run(`http://127.0.0.1:${port}`);
  } finally {
    await stopServer(child);
  }
}

async function getJson(origin: string, path: string, init: RequestInit = {}) {
  const response = await fetch(`${origin}${path}`, init);
  return { status: response.status, body: await response.json() };
}

async function getText(origin: string, path: string, init: RequestInit = {}) {
  const response = await fetch(`${origin}${path}`, init);
  return { status: response.status, body: await response.text() };
}

async function closeServer(server: Server): Promise<void> {
  await new Promise<void>((resolve, reject) => server.close((error) => (error ? reject(error) : resolve())));
}

async function unusedLocalPort(): Promise<number> {
  const server = createServer();
  server.listen(0, '127.0.0.1');
  await once(server, 'listening');
  const address = server.address() as AddressInfo;
  const { port } = address;
  await closeServer(server);
  return port;
}

async function withChunkedUpstream<T>(run: (origin: string) => Promise<T>): Promise<T> {
  const server = createServer((request, response) => {
    request.resume();
    response.statusCode = 500;
    response.setHeader('content-type', 'application/json');
    response.end(JSON.stringify({ errors: [{ message: 'unexpected upstream' }] }));
  });
  server.listen(0, '127.0.0.1');
  await once(server, 'listening');
  const address = server.address() as AddressInfo;
  try {
    return await run(`http://127.0.0.1:${address.port}`);
  } finally {
    await closeServer(server);
  }
}

describe('Rust HTTP adapter route surface', { timeout: adapterTestTimeoutMs }, () => {
  it('serves the required meta route response shapes through the Rust HTTP adapter', async () => {
    const port = await unusedLocalPort();
    await withRustServer(port, async (origin) => {
      expect(await getJson(origin, '/__meta/health')).toEqual({
        status: 200,
        body: {
          ok: true,
          message: 'shopify-draft-proxy is running',
        },
      });

      expect(await getJson(origin, '/__meta/config')).toEqual({
        status: 200,
        body: {
          runtime: {
            readMode: 'snapshot',
            unsupportedMutationMode: 'passthrough',
            bulkOperationRunMutationMaxInputFileSizeBytes: 104857600,
          },
          proxy: { port, shopifyAdminOrigin: 'https://shopify.com' },
          snapshot: { enabled: false, path: null },
        },
      });

      expect(await getJson(origin, '/__meta/log')).toEqual({
        status: 200,
        body: { entries: [] },
      });

      expect(await getJson(origin, '/__meta/state')).toMatchObject({
        status: 200,
        body: {
          baseState: {
            products: {},
            productOrder: [],
            productVariants: {},
            productVariantOrder: [],
            giftCards: {},
            giftCardCompleteQueries: [],
            giftCardConfiguration: null,
            orders: {},
            orderOrder: [],
            orderCountBaselines: {},
            draftOrders: {},
            draftOrderOrder: [],
            draftOrderCountBaselines: {},
            savedSearches: {},
            savedSearchOrder: [],
            segments: {},
            segmentOrder: [],
            shopPolicies: {},
            shopPolicyOrder: [],
            deliveryProfiles: {},
            deliveryProfileOrder: [],
            deliveryPromiseProviders: {},
            deliveryPromiseProviderOrder: [],
            deliveryPromiseParticipants: {},
            deliveryPromiseParticipantOrder: [],
            bulkOperations: {},
            bulkOperationOrder: [],
            bulkOperationsObserved: false,
            discounts: {},
            discountOrder: [],
            discountCountBaselines: {},
            shop: null,
            publicationIds: [],
            publicationCount: null,
            availableLocales: expect.objectContaining({
              en: 'English',
              fr: 'French',
            }),
            shopLocales: expect.objectContaining({
              en: expect.objectContaining({
                locale: 'en',
                name: 'English',
                primary: true,
                published: true,
              }),
            }),
            localizationProductIds: [],
            metafieldDefinitions: {},
            metafieldDefinitionOwnerCatalogs: [],
            metafieldDefinitionNamespaces: [],
            storefrontShop: null,
            storefrontLocalizations: {},
            storefrontProductTags: null,
            storefrontProductTypes: null,
            storefrontPaymentSettings: null,
            storefrontLocations: {},
            storefrontLocationOrder: [],
            storefrontLocationCursors: {},
            storefrontMenus: {},
            storefrontMenuOrder: [],
            storefrontPublicApiVersions: [],
            adminPublicApiVersions: [],
            adminPublicApiVersionsObserved: false,
            taxonomyCategories: {},
            taxonomyCategoryOrder: [],
            taxonomyConnectionWindows: {},
            taxonomyCompleteScopes: {},
            taxonomyMissingCategoryIds: [],
          },
          // This mirrors the authoritative empty staged-state serialization in
          // src/proxy/core.rs (the `/__meta/state` snapshot, which is also the
          // dump/restore payload the parity runner round-trips between targets).
          // b2b/inventory/metaobject fields are emitted only when non-empty
          // (`has_staged_b2b_state()` etc.), so they are absent from the empty state.
          stagedState: {
            products: {},
            productOrder: [],
            deletedProductIds: [],
            productVariants: {},
            productVariantOrder: [],
            deletedProductVariantIds: [],
            productFeeds: {},
            productFeedOrder: [],
            deletedProductFeedIds: [],
            collections: {},
            deletedCollectionIds: [],
            deletedCollectionHandles: [],
            collectionJobs: {},
            savedSearches: {},
            savedSearchOrder: [],
            deletedSavedSearchIds: [],
            segments: {},
            segmentOrder: [],
            deletedSegmentIds: [],
            shopPolicies: {},
            shopPolicyOrder: [],
            deletedShopPolicyIds: [],
            shippingPackages: {},
            deletedShippingPackageIds: {},
            installedApps: {},
            revokedAppAccessScopes: {},
            uninstalledAppIds: [],
            delegatedAccessTokens: {},
            customers: {},
            deletedCustomerIds: [],
            customerAddresses: {},
            customerAddressOrder: {},
            customerAddressOwners: {},
            customerOrders: {},
            mergedCustomerIds: {},
            customerMergeRequests: {},
            customerDataErasureRequests: {},
            storefrontCustomerEmailIndex: {},
            storefrontCustomerAccessTokens: {},
            nextStorefrontCustomerAccessTokenId: 1,
            nextStorefrontCustomerResetTokenId: 1,
            storefrontCarts: {},
            storefrontCartOrder: [],
            storefrontCartLines: {},
            storefrontCartLineOrder: {},
            nextStorefrontCartId: 1,
            nextStorefrontCartLineId: 1,
            nextStorefrontCartAppliedGiftCardId: 1,
            nextStorefrontCartMetafieldId: 1,
            customersCountBase: null,
            storeCreditAccounts: {},
            storeCreditAccountOrder: [],
            storeCreditTransactions: {},
            storeCreditTransactionOrder: [],
            nextStoreCreditAccountId: 1,
            nextStoreCreditTransactionId: 1,
            giftCards: {},
            locallyCreatedCustomerIds: [],
            taggableResources: {},
            abandonments: {},
            orders: {},
            deletedOrderIds: [],
            nextDraftOrderId: 1,
            draftOrderTags: {},
            returns: {},
            returnsByOrder: {},
            reverseDeliveries: {},
            reverseFulfillmentOrders: {},
            observedShippingLocations: {},
            observedShippingLocationOrder: [],
            locations: {},
            locationOrder: [],
            deletedLocationIds: [],
            deliveryProfiles: {},
            deliveryProfileOrder: [],
            deletedDeliveryProfileIds: [],
            deliveryPromiseProviders: {},
            deliveryPromiseProviderOrder: [],
            deletedDeliveryPromiseProviderIds: [],
            deliveryPromiseParticipants: {},
            deliveryPromiseParticipantOrder: [],
            deletedDeliveryPromiseParticipantIds: [],
            deliveryCustomizations: {},
            deliveryCustomizationOrder: [],
            deletedDeliveryCustomizationIds: [],
            publicationIds: [],
            createdPublicationIds: [],
            currentChannelPublicationId: null,
            currentChannelPublicationResolved: false,
            publications: {},
            resourcePublications: {},
            locationLimitReached: false,
            discounts: {},
            discountCodeIndex: {},
            deletedDiscountIds: [],
            discountRedeemCodeBulkCreations: {},
            ownerMetafields: {},
            deletedOwnerMetafields: [],
            deletedMetafieldDefinitions: [],
          },
        },
      });

      expect(await getJson(origin, '/__meta/reset', { method: 'POST' })).toEqual({
        status: 200,
        body: { ok: true, message: 'state reset' },
      });
    });
  });

  it('serves Admin GraphQL, staged upload, and error envelopes through Rust HTTP', async () => {
    const graphQLBody = {
      query:
        'mutation { savedSearchCreate(input: { name: "Promo products", query: "tag:promo", resourceType: PRODUCT }) { savedSearch { id name query resourceType filters { key value } } userErrors { field message } } }',
    };

    await withRustServer(await unusedLocalPort(), async (origin) => {
      const rustCreate = await getJson(origin, '/admin/api/2026-04/graphql.json', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify(graphQLBody),
      });
      expect(rustCreate).toMatchObject({
        status: 200,
        body: {
          data: {
            savedSearchCreate: {
              savedSearch: {
                id: 'gid://shopify/SavedSearch/1?shopify-draft-proxy=synthetic',
                name: 'Promo products',
                query: 'tag:promo',
                resourceType: 'PRODUCT',
              },
              userErrors: [],
            },
          },
        },
      });

      const stagedUpload = await getJson(origin, '/staged-uploads/gid%3A%2F%2Fshopify%2FProduct%2F1/import.jsonl', {
        method: 'PUT',
        body: '{"id":"gid://shopify/Product/1"}\n',
      });
      expect(stagedUpload).toEqual({
        status: 201,
        body: {
          ok: true,
          key: 'shopify-draft-proxy/gid://shopify/Product/1/import.jsonl',
        },
      });

      const rustMissing = await getJson(origin, '/missing');
      expect(rustMissing).toEqual({
        status: 404,
        body: { errors: [{ message: 'Not found' }] },
      });

      const rustMethod = await getJson(origin, '/__meta/health', { method: 'POST' });
      expect(rustMethod).toEqual({
        status: 405,
        body: { errors: [{ message: 'Method not allowed' }] },
      });
    });
  });

  it('captures staged upload bytes for local bulk mutation imports through Rust HTTP', async () => {
    await withRustServer(await unusedLocalPort(), async (origin) => {
      const stagedUpload = await getJson(origin, '/admin/api/2026-04/graphql.json', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({
          query: `mutation CreateBulkUpload($input: [StagedUploadInput!]!) {
            stagedUploadsCreate(input: $input) {
              stagedTargets { resourceUrl parameters { name value } }
              userErrors { field message }
            }
          }`,
          variables: {
            input: [
              {
                resource: 'BULK_MUTATION_VARIABLES',
                filename: 'http-bulk-product-create.jsonl',
                mimeType: 'text/jsonl',
                httpMethod: 'POST',
              },
            ],
          },
        }),
      });
      expect(stagedUpload.status).toBe(200);
      const target = (
        stagedUpload.body as {
          data: {
            stagedUploadsCreate: {
              stagedTargets: Array<{ resourceUrl: string; parameters: Array<{ name: string; value: string }> }>;
              userErrors: unknown[];
            };
          };
        }
      ).data.stagedUploadsCreate.stagedTargets[0];
      if (target === undefined) {
        throw new Error(`staged upload target was missing: ${JSON.stringify(stagedUpload.body)}`);
      }
      const key = target.parameters.find((parameter) => parameter.name === 'key')?.value;
      if (!key) {
        throw new Error(`staged upload target did not include a key parameter: ${JSON.stringify(target)}`);
      }

      const jsonl = `${JSON.stringify({ product: { title: 'HTTP bulk import product' } })}\n`;
      const uploadPath = new URL(target.resourceUrl).pathname;
      const upload = await getJson(origin, uploadPath, {
        method: 'POST',
        headers: { 'content-type': 'text/jsonl' },
        body: jsonl,
      });
      expect(upload).toEqual({
        status: 201,
        body: {
          ok: true,
          key: expect.stringContaining('http-bulk-product-create.jsonl'),
        },
      });

      const run = await getJson(origin, '/admin/api/2026-04/graphql.json', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({
          query: `mutation RunBulkImport($mutation: String!, $path: String!) {
            bulkOperationRunMutation(mutation: $mutation, stagedUploadPath: $path) {
              bulkOperation { id status objectCount fileSize url }
              userErrors { field message code }
            }
          }`,
          variables: {
            mutation:
              'mutation ProductCreate($product: ProductCreateInput!) { productCreate(product: $product) { product { id title } userErrors { field message } } }',
            path: key,
          },
        }),
      });
      expect(run).toMatchObject({
        status: 200,
        body: {
          data: {
            bulkOperationRunMutation: {
              bulkOperation: {
                status: 'CREATED',
                objectCount: '0',
                fileSize: null,
                url: null,
              },
              userErrors: [],
            },
          },
        },
      });

      const current = await getJson(origin, '/admin/api/2026-04/graphql.json', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({
          query: `query CurrentBulkMutation {
            currentBulkOperation(type: MUTATION) {
              status
              objectCount
              rootObjectCount
              fileSize
              url
            }
          }`,
        }),
      });
      const currentOperation = (
        current.body as {
          data: {
            currentBulkOperation: {
              status: string;
              objectCount: string;
              rootObjectCount: string;
              fileSize: string;
              url: string;
            };
          };
        }
      ).data.currentBulkOperation;
      expect(currentOperation).toMatchObject({
        status: 'COMPLETED',
        objectCount: '1',
        rootObjectCount: '1',
      });
      const artifactPath = new URL(currentOperation.url).pathname;
      const artifact = await getText(origin, artifactPath);
      expect(artifact.status).toBe(200);
      expect(JSON.parse(artifact.body.trim())).toMatchObject({
        data: {
          productCreate: {
            product: {
              title: 'HTTP bulk import product',
            },
            userErrors: [],
          },
        },
        __lineNumber: 0,
      });
      expect(currentOperation.fileSize).toBe(String(artifact.body.length));
    });
  });

  it('forwards chunked upstream passthrough responses without producing duplicate hop-by-hop headers', async () => {
    await withChunkedUpstream(async (upstreamOrigin) => {
      await withRustServer(
        await unusedLocalPort(),
        async (origin) => {
          const response = await getJson(origin, '/admin/api/2026-04/graphql.json', {
            method: 'POST',
            headers: { 'content-type': 'application/json' },
            body: JSON.stringify({
              query: '{ currentStaffMember { id } }',
            }),
          });
          expect(response).toEqual({
            status: 500,
            body: { errors: [{ message: 'unexpected upstream' }] },
          });
        },
        { readMode: 'live-hybrid', shopifyAdminOrigin: upstreamOrigin },
      );
    });
  });
});
