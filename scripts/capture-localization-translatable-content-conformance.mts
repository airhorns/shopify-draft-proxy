/* oxlint-disable no-console -- CLI capture script intentionally writes status output. */
import 'dotenv/config';

import { createHash } from 'node:crypto';
import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlPayload } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

const scenarioId = 'localization-translatable-content-product';

const productCreateMutation = `#graphql
  mutation LocalizationTranslationsKnownResourceProductCreate($product: ProductCreateInput!) {
    productCreate(product: $product) {
      product {
        id
        title
        handle
        status
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const productDeleteMutation = `#graphql
  mutation LocalizationTranslatableContentProductDelete($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors {
        field
        message
      }
    }
  }
`;

const shopLocalesQuery = `#graphql
  query LocalizationTranslatableContentShopLocales {
    shopLocales {
      locale
      name
      primary
      published
    }
  }
`;

const readQuery = `#graphql
  query LocalizationTranslatableContentRead($resourceId: ID!) {
    translatableResource(resourceId: $resourceId) {
      resourceId
      translatableContent {
        key
        value
        digest
        locale
        type
      }
    }
  }
`;

function isRecord(value: unknown): value is JsonRecord {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function dataObject(payload: ConformanceGraphqlPayload<unknown>): JsonRecord {
  if (!isRecord(payload.data)) {
    throw new Error(`Expected GraphQL data object, got ${JSON.stringify(payload)}`);
  }
  return payload.data;
}

function payloadField(payload: ConformanceGraphqlPayload<unknown>, fieldName: string): JsonRecord {
  const field = dataObject(payload)[fieldName];
  if (!isRecord(field)) {
    throw new Error(`Expected data.${fieldName} object, got ${JSON.stringify(payload)}`);
  }
  return field;
}

function userErrors(payload: JsonRecord): JsonRecord[] {
  const errors = payload['userErrors'];
  return Array.isArray(errors) ? errors.filter(isRecord) : [];
}

function assertNoUserErrors(payload: JsonRecord, context: string): void {
  const errors = userErrors(payload);
  if (errors.length > 0) {
    throw new Error(`${context} returned userErrors: ${JSON.stringify(errors)}`);
  }
}

function primaryLocale(payload: ConformanceGraphqlPayload<unknown>): string {
  const locales = dataObject(payload)['shopLocales'];
  if (!Array.isArray(locales)) {
    throw new Error(`Expected shopLocales array, got ${JSON.stringify(payload)}`);
  }
  const primary = locales.find((entry) => isRecord(entry) && entry['primary'] === true);
  if (!isRecord(primary) || typeof primary['locale'] !== 'string') {
    throw new Error(`Expected a primary shop locale, got ${JSON.stringify(payload)}`);
  }
  return primary['locale'];
}

function sha256(value: string): string {
  return createHash('sha256').update(value).digest('hex');
}

function randomSuffix(): string {
  return `${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
}

function assertTranslatableContent(
  payload: ConformanceGraphqlPayload<unknown>,
  expected: Record<string, { value: string; type: string }>,
  expectedLocale: string,
): void {
  const resource = dataObject(payload)['translatableResource'];
  if (!isRecord(resource) || !Array.isArray(resource['translatableContent'])) {
    throw new Error(`Expected translatableResource.translatableContent array, got ${JSON.stringify(payload)}`);
  }
  const content = resource['translatableContent'].filter(isRecord);
  const keys = content.map((entry) => entry['key']);
  const expectedKeys = Object.keys(expected);
  if (JSON.stringify(keys) !== JSON.stringify(expectedKeys)) {
    throw new Error(`Expected translatableContent keys ${JSON.stringify(expectedKeys)}, got ${JSON.stringify(keys)}`);
  }
  for (const entry of content) {
    const key = entry['key'];
    if (typeof key !== 'string' || !(key in expected)) {
      throw new Error(`Unexpected translatableContent entry: ${JSON.stringify(entry)}`);
    }
    const expectedEntry = expected[key];
    if (expectedEntry === undefined) {
      throw new Error(`Missing expected translatableContent entry for ${key}`);
    }
    if (
      entry['value'] !== expectedEntry.value ||
      entry['type'] !== expectedEntry.type ||
      entry['locale'] !== expectedLocale ||
      entry['digest'] !== sha256(expectedEntry.value)
    ) {
      throw new Error(
        `Unexpected translatableContent entry for ${key}: ${JSON.stringify({
          actual: entry,
          expected: { ...expectedEntry, locale: expectedLocale, digest: sha256(expectedEntry.value) },
        })}`,
      );
    }
  }
}

async function bestEffortCleanup(options: {
  runGraphql: (query: string, variables?: JsonRecord) => Promise<ConformanceGraphqlPayload<unknown>>;
  productId: string | null;
}): Promise<JsonRecord> {
  const cleanup: JsonRecord = {};
  if (options.productId !== null) {
    try {
      cleanup['productDelete'] = await options.runGraphql(productDeleteMutation, {
        input: { id: options.productId },
      });
    } catch (error: unknown) {
      cleanup['productDeleteError'] = String(error);
    }
  }
  return cleanup;
}

async function main(): Promise<void> {
  const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
  if (apiVersion !== '2026-04') {
    throw new Error(`Expected SHOPIFY_CONFORMANCE_API_VERSION=2026-04, got ${apiVersion}`);
  }
  const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
  const { runGraphql } = createAdminGraphqlClient({
    adminOrigin,
    apiVersion,
    headers: buildAdminAuthHeaders(adminAccessToken),
  });
  const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'localization');
  const outputPath = path.join(outputDir, `${scenarioId}.json`);
  const captureToken = randomSuffix();
  const productInput = {
    title: `Localization translatable content ${captureToken}`,
    handle: `localization-translatable-content-${captureToken}`.replace(/[^a-z0-9-]/gu, '-'),
    descriptionHtml: `<p>Localization source body ${captureToken}</p>`,
    productType: `Localization Type ${captureToken}`,
    seo: {
      title: `Localization SEO title ${captureToken}`,
      description: `Localization SEO description ${captureToken}`,
    },
    status: 'DRAFT',
  };
  const expectedContent = {
    title: { value: productInput.title, type: 'SINGLE_LINE_TEXT_FIELD' },
    body_html: { value: productInput.descriptionHtml, type: 'HTML' },
    handle: { value: productInput.handle, type: 'URI' },
    product_type: { value: productInput.productType, type: 'SINGLE_LINE_TEXT_FIELD' },
    meta_title: { value: productInput.seo.title, type: 'MULTI_LINE_TEXT_FIELD' },
    meta_description: { value: productInput.seo.description, type: 'MULTI_LINE_TEXT_FIELD' },
  };

  let productId: string | null = null;
  let cleanup: JsonRecord = {};

  try {
    const shopLocales = await runGraphql(shopLocalesQuery);
    const locale = primaryLocale(shopLocales);
    const productCreateVariables = { product: productInput };
    const productCreate = await runGraphql(productCreateMutation, productCreateVariables);
    const productCreatePayload = payloadField(productCreate, 'productCreate');
    assertNoUserErrors(productCreatePayload, 'productCreate');
    const product = productCreatePayload['product'];
    if (!isRecord(product) || typeof product['id'] !== 'string') {
      throw new Error(`Product setup did not return a Product id: ${JSON.stringify(productCreate)}`);
    }
    productId = product['id'];

    const readVariables = { resourceId: productId };
    const read = await runGraphql(readQuery, readVariables);
    assertTranslatableContent(read, expectedContent, locale);

    cleanup = await bestEffortCleanup({ runGraphql, productId });
    productId = null;

    const capture = {
      scenarioId,
      storeDomain,
      apiVersion,
      capturedAt: new Date().toISOString(),
      setup: {
        shopLocales: {
          response: shopLocales,
        },
        productCreate: {
          variables: productCreateVariables,
          response: productCreate,
        },
      },
      read: {
        request: { variables: readVariables },
        response: read,
      },
      cleanup,
      upstreamCalls: [
        {
          operationName: 'LocalizationTranslatableContentRead',
          variables: readVariables,
          query: readQuery,
          response: {
            status: 200,
            body: read,
          },
        },
      ],
    };

    await mkdir(outputDir, { recursive: true });
    await writeFile(outputPath, `${JSON.stringify(capture, null, 2)}\n`, 'utf8');
    console.log(
      JSON.stringify({ ok: true, outputPath, storeDomain, apiVersion, productId: readVariables.resourceId }, null, 2),
    );
  } finally {
    if (productId !== null) {
      cleanup = await bestEffortCleanup({ runGraphql, productId });
      console.log(`Cleanup after failure: ${JSON.stringify(cleanup)}`);
    }
  }
}

await main();
