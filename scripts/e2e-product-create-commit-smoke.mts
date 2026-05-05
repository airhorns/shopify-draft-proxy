// @ts-nocheck
/* oxlint-disable no-console -- CLI smoke script intentionally writes status output to stdout. */
/**
 * Live end-to-end smoke for the JavaScript/Node path.
 *
 * Boots an in-process Node HTTP app (`createApp`) backed by the JS-target
 * Gleam proxy in live-hybrid mode, stages 3 productCreate mutations
 * through it, asserts the staged IDs are synthetic and not yet visible
 * upstream, runs `/__meta/commit` to replay through real Shopify, then
 * cleans up the committed products. Mirrors
 * `elixir_smoke/test/live_hybrid_e2e_test.exs` — the two together
 * are the cross-target proof that committable mutations behave the
 * same on Erlang and JavaScript.
 *
 * Run via:  `pnpm e2e:product-create-commit-smoke`
 */
import 'dotenv/config';

import type { AddressInfo } from 'node:net';
import type { Server } from 'node:http';

import { createApp, type AppConfig } from '../js/src/index.js';

import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const SAMPLE_COUNT = 3;

function fail(message: string, extra?: unknown): never {
  console.error(`FAIL: ${message}`);
  if (extra !== undefined) console.error(JSON.stringify(extra, null, 2));
  process.exit(1);
}

function assertEqual<T>(actual: T, expected: T, message: string): void {
  if (actual !== expected) {
    fail(`${message} (expected ${JSON.stringify(expected)}, got ${JSON.stringify(actual)})`);
  }
}

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const authHeaders = buildAdminAuthHeaders(adminAccessToken);

const config: AppConfig = {
  port: 0,
  shopifyAdminOrigin: adminOrigin,
  readMode: 'live-hybrid',
};

const app = createApp(config);
const server: Server = await new Promise((resolve) => {
  const listener = app.listen(0, () => resolve(listener));
});
const address = server.address() as AddressInfo;
const proxyOrigin = `http://127.0.0.1:${address.port}`;
const graphqlPath = `/admin/api/${apiVersion}/graphql.json`;

console.log(`proxy listening on ${proxyOrigin} -> ${adminOrigin} (api ${apiVersion}, store ${storeDomain})`);

async function postJson(url: string, body: unknown, extraHeaders: Record<string, string> = {}) {
  const isString = typeof body === 'string';
  const response = await fetch(url, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json', ...authHeaders, ...extraHeaders },
    body: isString ? (body as string) : JSON.stringify(body),
  });
  const text = await response.text();
  let parsed: unknown;
  try {
    parsed = text ? JSON.parse(text) : null;
  } catch {
    parsed = text;
  }
  return { status: response.status, body: parsed as any };
}

async function getJson(url: string) {
  const response = await fetch(url);
  const text = await response.text();
  let parsed: unknown;
  try {
    parsed = text ? JSON.parse(text) : null;
  } catch {
    parsed = text;
  }
  return { status: response.status, body: parsed as any };
}

async function runProxyGraphql(query: string, variables: Record<string, unknown> = {}) {
  const result = await postJson(`${proxyOrigin}${graphqlPath}`, { query, variables });
  if (result.status !== 200) fail(`proxy GraphQL HTTP ${result.status}`, result.body);
  if (result.body?.errors) fail('proxy GraphQL errors', result.body.errors);
  return result.body;
}

async function runShopifyDirectGraphql(query: string, variables: Record<string, unknown> = {}) {
  const result = await postJson(`${adminOrigin}${graphqlPath}`, { query, variables });
  if (result.status !== 200) fail(`shopify direct GraphQL HTTP ${result.status}`, result.body);
  if (result.body?.errors) fail('shopify direct GraphQL errors', result.body.errors);
  return result.body;
}

const productCreateMutation = `
  mutation Create($product: ProductCreateInput!) {
    productCreate(product: $product) {
      product { id title handle status createdAt updatedAt }
      userErrors { field message }
    }
  }
`;

const productReadQuery = `
  query Read($id: ID!) {
    product(id: $id) { id title status handle }
  }
`;

const productDeleteMutation = `
  mutation Del($input: ProductDeleteInput!) {
    productDelete(input: $input) { deletedProductId userErrors { field message } }
  }
`;

let committedIds: string[] = [];

try {
  const reset = await postJson(`${proxyOrigin}/__meta/reset`, '');
  assertEqual(reset.status, 200, '__meta/reset status');
  console.log('proxy state reset');

  const stamp = Date.now();
  const titles = Array.from({ length: SAMPLE_COUNT }, (_, i) => `Draft Proxy E2E ${stamp} #${i + 1}`);

  const stagedIds: string[] = [];
  for (const title of titles) {
    const data = await runProxyGraphql(productCreateMutation, { product: { title, status: 'DRAFT' } });
    const payload = data?.data?.productCreate;
    if (!payload) fail('missing productCreate payload', data);
    const { product, userErrors } = payload;
    if (userErrors.length > 0) fail(`userErrors creating "${title}"`, userErrors);
    if (!product) fail(`null product for "${title}"`, data);
    if (typeof product.id !== 'string' || !product.id.startsWith('gid://shopify/Product/')) {
      fail('bad product id', product);
    }
    if (!product.id.includes('shopify-draft-proxy=synthetic')) {
      fail('expected synthetic id marker on staged product id', product);
    }
    assertEqual(product.title, title, `staged product title for "${title}"`);
    assertEqual(product.status, 'DRAFT', `staged product status for "${title}"`);
    if (!product.createdAt) fail('missing createdAt', product);
    if (!product.updatedAt) fail('missing updatedAt', product);
    if (!product.handle) fail('missing handle', product);
    console.log(`staged product ${product.id}  title="${product.title}"  handle=${product.handle}`);
    stagedIds.push(product.id);
  }

  for (const [i, id] of stagedIds.entries()) {
    const data = await runProxyGraphql(productReadQuery, { id });
    const product = data?.data?.product;
    if (!product) fail(`proxy could not read staged product ${id}`, data);
    assertEqual(product.id, id, `read-back id for staged product ${i}`);
    assertEqual(product.title, titles[i], `read-back title for staged product ${i}`);
  }
  console.log(`read-after-write OK for ${stagedIds.length} staged products`);

  for (const id of stagedIds) {
    const data = await runShopifyDirectGraphql(productReadQuery, { id });
    if (data?.data?.product !== null) {
      fail(`Shopify unexpectedly returned a product for staged synthetic id ${id}`, data);
    }
  }
  console.log('confirmed staged products are NOT in Shopify yet');

  const log = await getJson(`${proxyOrigin}/__meta/log`);
  if (log.status !== 200) fail(`__meta/log HTTP ${log.status}`, log.body);
  const stagedEntries = (log.body.entries ?? []).filter(
    (e: any) => e.status === 'staged' && e.operationName === 'productCreate',
  );
  assertEqual(stagedEntries.length, SAMPLE_COUNT, '__meta/log staged productCreate count');
  console.log(`__meta/log shows ${stagedEntries.length} staged productCreate entries`);

  const commit = await postJson(`${proxyOrigin}/__meta/commit`, '');
  if (commit.status !== 200) fail(`__meta/commit HTTP ${commit.status}`, commit.body);
  const commitBody = commit.body;
  console.log(
    `commit response: ok=${commitBody.ok} stopIndex=${commitBody.stopIndex} attempts=${commitBody.attempts.length}`,
  );
  if (!commitBody.ok) fail('commit not ok', commitBody);
  assertEqual(commitBody.stopIndex, null, 'commit stopIndex should be null');
  assertEqual(commitBody.attempts.length, SAMPLE_COUNT, 'commit attempts count');

  for (const [i, attempt] of commitBody.attempts.entries()) {
    if (!attempt.success) fail(`commit attempt ${i} failed`, attempt);
    assertEqual(attempt.operationName, 'productCreate', `attempt ${i} operationName`);
    assertEqual(attempt.status, 'committed', `attempt ${i} status`);
    if (typeof attempt.upstreamStatus !== 'number' || attempt.upstreamStatus < 200 || attempt.upstreamStatus >= 300) {
      fail(`attempt ${i} bad upstream status`, attempt);
    }
    if (attempt.upstreamError !== null) fail(`attempt ${i} unexpected upstreamError`, attempt);
    const upstreamProduct = attempt.upstreamBody?.data?.productCreate?.product;
    if (!upstreamProduct) fail(`attempt ${i} missing upstream product`, attempt);
    if (typeof upstreamProduct.id !== 'string' || !upstreamProduct.id.startsWith('gid://shopify/Product/')) {
      fail(`attempt ${i} bad real id`, upstreamProduct);
    }
    if (upstreamProduct.id.includes('shopify-draft-proxy=synthetic')) {
      fail(`attempt ${i} got a synthetic id back from Shopify`, upstreamProduct);
    }
    if (upstreamProduct.title !== titles[i]) fail(`attempt ${i} upstream title mismatch`, upstreamProduct);
    const ue = attempt.upstreamBody?.data?.productCreate?.userErrors ?? [];
    if (ue.length > 0) fail(`attempt ${i} upstream userErrors`, ue);
    committedIds.push(upstreamProduct.id);
    console.log(
      `committed[${i}]  staged=${attempt.upstreamBody?.data?.productCreate?.product?.id ? 'replaced' : '?'}  real=${upstreamProduct.id}  title="${upstreamProduct.title}"`,
    );
  }

  const postCommitLog = await getJson(`${proxyOrigin}/__meta/log`);
  const postCommitStaged = (postCommitLog.body.entries ?? []).filter((e: any) => e.status === 'staged');
  assertEqual(postCommitStaged.length, 0, 'no staged entries should remain after commit');
  const postCommitCommitted = (postCommitLog.body.entries ?? []).filter((e: any) => e.status === 'committed');
  assertEqual(postCommitCommitted.length, SAMPLE_COUNT, 'committed entry count after commit');

  for (const [i, realId] of committedIds.entries()) {
    const data = await runShopifyDirectGraphql(productReadQuery, { id: realId });
    const product = data?.data?.product;
    if (!product) fail(`Shopify did not return committed product ${realId}`, data);
    assertEqual(product.id, realId, `shopify product id for committed ${i}`);
    assertEqual(product.title, titles[i], `shopify product title for committed ${i}`);
  }
  console.log(`verified all ${committedIds.length} committed products exist in Shopify`);

  console.log(`\nE2E SUCCESS: created ${SAMPLE_COUNT} products via proxy and committed them to ${storeDomain}`);
} finally {
  if (committedIds.length > 0) {
    console.log(`\ncleaning up ${committedIds.length} committed product(s) from ${storeDomain}...`);
    for (const realId of committedIds) {
      try {
        const data = await runShopifyDirectGraphql(productDeleteMutation, { input: { id: realId } });
        const ue = data?.data?.productDelete?.userErrors ?? [];
        if (ue.length > 0) {
          console.warn(`cleanup userErrors for ${realId}: ${JSON.stringify(ue)}`);
        } else {
          console.log(`deleted ${data?.data?.productDelete?.deletedProductId ?? realId}`);
        }
      } catch (error) {
        console.warn(`cleanup failed for ${realId}: ${(error as Error).message}`);
      }
    }
  }
  server.close();
}
