/* oxlint-disable no-console -- CLI scripts intentionally write capture status output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'metafields');
const outputPath = path.join(outputDir, 'metafield-definition-owner-scoped-duplicates.json');
const { runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const suffix = Date.now().toString(36);
const namespace = 'custom';
const key = `spec_${suffix}`;

const createDefinitionMutation = `#graphql
  mutation CreateDefinition($definition: MetafieldDefinitionInput!) {
    metafieldDefinitionCreate(definition: $definition) {
      createdDefinition {
        id
        namespace
        key
        ownerType
        name
        type { name category }
        pinnedPosition
      }
      userErrors { __typename field message code }
    }
  }
`;

const deleteDefinitionMutation = `#graphql
  mutation DeleteDefinition($id: ID!, $deleteAllAssociatedMetafields: Boolean!) {
    metafieldDefinitionDelete(id: $id, deleteAllAssociatedMetafields: $deleteAllAssociatedMetafields) {
      deletedDefinitionId
      deletedDefinition { ownerType namespace key }
      userErrors { field message code }
    }
  }
`;

const ownerScopedReadQuery = `#graphql
  query OwnerScopedDefinitionRead($namespace: String!, $key: String!) {
    productByIdentifier: metafieldDefinition(identifier: { ownerType: PRODUCT, namespace: $namespace, key: $key }) {
      id
      ownerType
      namespace
      key
      name
    }
    customerByIdentifier: metafieldDefinition(identifier: { ownerType: CUSTOMER, namespace: $namespace, key: $key }) {
      id
      ownerType
      namespace
      key
      name
    }
    productCatalog: metafieldDefinitions(ownerType: PRODUCT, namespace: $namespace, key: $key, first: 10) {
      nodes { id ownerType namespace key name }
    }
    customerCatalog: metafieldDefinitions(ownerType: CUSTOMER, namespace: $namespace, key: $key, first: 10) {
      nodes { id ownerType namespace key name }
    }
  }
`;

function assertHttpOk(result: ConformanceGraphqlResult, label: string): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${label} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function readObject(value: unknown): Record<string, unknown> | null {
  return value && typeof value === 'object' && !Array.isArray(value) ? (value as Record<string, unknown>) : null;
}

function payloadAt(response: unknown, root: string): Record<string, unknown> {
  const payload = readObject(readObject(readObject(response)?.['data'])?.[root]);
  if (!payload) throw new Error(`Missing ${root} payload: ${JSON.stringify(response)}`);
  return payload;
}

function definitionIdFromCreate(response: unknown): string {
  const created = readObject(payloadAt(response, 'metafieldDefinitionCreate')['createdDefinition']);
  const id = created?.['id'];
  if (typeof id !== 'string') throw new Error(`Create did not return a definition id: ${JSON.stringify(response)}`);
  return id;
}

function assertNoUserErrors(response: unknown, root: string, label: string): void {
  const userErrors = payloadAt(response, root)['userErrors'];
  if (Array.isArray(userErrors) && userErrors.length === 0) return;
  throw new Error(`${label} returned userErrors: ${JSON.stringify(userErrors)}`);
}

function assertTakenDuplicate(response: unknown): void {
  const payload = payloadAt(response, 'metafieldDefinitionCreate');
  const createdDefinition = payload['createdDefinition'];
  const userErrors = payload['userErrors'];
  const firstError = Array.isArray(userErrors) ? readObject(userErrors[0]) : null;
  if (
    createdDefinition === null &&
    Array.isArray(userErrors) &&
    userErrors.length === 1 &&
    firstError?.['code'] === 'TAKEN'
  ) {
    return;
  }
  throw new Error(`Duplicate create did not return TAKEN: ${JSON.stringify(payload)}`);
}

function assertOwnerScopedRead(response: unknown): void {
  const data = readObject(readObject(response)?.['data']);
  const product = readObject(data?.['productByIdentifier']);
  const customer = readObject(data?.['customerByIdentifier']);
  const productNodes = readObject(data?.['productCatalog'])?.['nodes'];
  const customerNodes = readObject(data?.['customerCatalog'])?.['nodes'];
  if (
    product?.['ownerType'] === 'PRODUCT' &&
    customer?.['ownerType'] === 'CUSTOMER' &&
    Array.isArray(productNodes) &&
    productNodes.length === 1 &&
    readObject(productNodes[0])?.['ownerType'] === 'PRODUCT' &&
    Array.isArray(customerNodes) &&
    customerNodes.length === 1 &&
    readObject(customerNodes[0])?.['ownerType'] === 'CUSTOMER'
  ) {
    return;
  }
  throw new Error(`Owner-scoped read did not return both definitions: ${JSON.stringify(response)}`);
}

async function capture(label: string, query: string, variables: Record<string, unknown>) {
  const result = await runGraphqlRaw(query, variables);
  assertHttpOk(result, label);
  return {
    label,
    request: { query, variables },
    status: result.status,
    response: result.payload,
  };
}

let productDefinitionId: string | null = null;
let customerDefinitionId: string | null = null;
const captures = [];
const cleanup = [];

try {
  const productCreate = await capture('metafieldDefinitionCreate PRODUCT success', createDefinitionMutation, {
    definition: {
      namespace,
      key,
      ownerType: 'PRODUCT',
      name: 'Owner scoped product spec',
      type: 'single_line_text_field',
    },
  });
  captures.push(productCreate);
  assertNoUserErrors(productCreate.response, 'metafieldDefinitionCreate', 'PRODUCT create');
  productDefinitionId = definitionIdFromCreate(productCreate.response);

  const duplicateProductCreate = await capture(
    'metafieldDefinitionCreate PRODUCT duplicate TAKEN',
    createDefinitionMutation,
    {
      definition: {
        namespace,
        key,
        ownerType: 'PRODUCT',
        name: 'Owner scoped product duplicate',
        type: 'single_line_text_field',
      },
    },
  );
  captures.push(duplicateProductCreate);
  assertTakenDuplicate(duplicateProductCreate.response);

  const customerCreate = await capture(
    'metafieldDefinitionCreate CUSTOMER same key success',
    createDefinitionMutation,
    {
      definition: {
        namespace,
        key,
        ownerType: 'CUSTOMER',
        name: 'Owner scoped customer spec',
        type: 'single_line_text_field',
      },
    },
  );
  captures.push(customerCreate);
  assertNoUserErrors(customerCreate.response, 'metafieldDefinitionCreate', 'CUSTOMER create');
  customerDefinitionId = definitionIdFromCreate(customerCreate.response);

  const ownerScopedRead = await capture('owner-type scoped definition readback', ownerScopedReadQuery, {
    namespace,
    key,
  });
  captures.push(ownerScopedRead);
  assertOwnerScopedRead(ownerScopedRead.response);
} finally {
  if (customerDefinitionId) {
    cleanup.push(
      await capture('cleanup CUSTOMER metafieldDefinitionDelete', deleteDefinitionMutation, {
        id: customerDefinitionId,
        deleteAllAssociatedMetafields: true,
      }).catch((error: unknown) => ({ label: 'cleanup CUSTOMER metafieldDefinitionDelete', error: String(error) })),
    );
  }
  if (productDefinitionId) {
    cleanup.push(
      await capture('cleanup PRODUCT metafieldDefinitionDelete', deleteDefinitionMutation, {
        id: productDefinitionId,
        deleteAllAssociatedMetafields: true,
      }).catch((error: unknown) => ({ label: 'cleanup PRODUCT metafieldDefinitionDelete', error: String(error) })),
    );
  }
}

const fixture = {
  capturedAt: new Date().toISOString(),
  storeDomain,
  apiVersion,
  namespace,
  key,
  captures,
  cleanup,
};

await mkdir(outputDir, { recursive: true });
await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');

console.log(`Wrote ${outputPath}`);
