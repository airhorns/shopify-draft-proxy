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
const outputPath = path.join(outputDir, 'metafield-definition-non-product-owner-types.json');
const { runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const suffix = Date.now().toString(36);
const customerNamespace = `har691_customer_${suffix}`;
const orderNamespace = `har691_order_${suffix}`;
const companyNamespace = `har691_company_${suffix}`;

const createDefinitionMutation = `#graphql
  mutation MetafieldDefinitionNonProductCreate($definition: MetafieldDefinitionInput!) {
    metafieldDefinitionCreate(definition: $definition) {
      createdDefinition {
        id
        name
        namespace
        key
        ownerType
        type { name category }
        description
        validations { name value }
        pinnedPosition
        validationStatus
      }
      userErrors { field message code }
    }
  }
`;

const updateDefinitionMutation = `#graphql
  mutation MetafieldDefinitionNonProductUpdate($definition: MetafieldDefinitionUpdateInput!) {
    metafieldDefinitionUpdate(definition: $definition) {
      updatedDefinition {
        id
        name
        namespace
        key
        ownerType
        type { name category }
        description
        validations { name value }
        pinnedPosition
        validationStatus
      }
      userErrors { field message code }
    }
  }
`;

const readDefinitionQuery = `#graphql
  query MetafieldDefinitionNonProductRead($id: ID!) {
    metafieldDefinition(id: $id) {
      id
      name
      namespace
      key
      ownerType
      type { name category }
      description
      validations { name value }
      pinnedPosition
      validationStatus
    }
  }
`;

const deleteDefinitionMutation = `#graphql
  mutation MetafieldDefinitionNonProductDelete($id: ID!, $deleteAllAssociatedMetafields: Boolean!) {
    metafieldDefinitionDelete(id: $id, deleteAllAssociatedMetafields: $deleteAllAssociatedMetafields) {
      deletedDefinitionId
      deletedDefinition { ownerType namespace key }
      userErrors { field message code }
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

function readPayloadObject(result: unknown, path: string[]): Record<string, unknown> | null {
  let current: unknown = result;
  for (const segment of path) {
    current = readObject(current)?.[segment];
  }
  return readObject(current);
}

function assertNoUserErrors(payload: Record<string, unknown> | null, label: string): void {
  const userErrors = payload?.['userErrors'];
  if (!Array.isArray(userErrors) || userErrors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(userErrors)}`);
  }
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

async function captureCreate(
  label: string,
  definition: Record<string, unknown>,
  activeDefinitionIds: Set<string>,
): Promise<{ capture: Awaited<ReturnType<typeof capture>>; id: string }> {
  const create = await capture(label, createDefinitionMutation, { definition });
  const payload = readPayloadObject(create.response, ['data', 'metafieldDefinitionCreate']);
  assertNoUserErrors(payload, label);
  const createdDefinition = readObject(payload?.['createdDefinition']);
  const id = typeof createdDefinition?.['id'] === 'string' ? createdDefinition['id'] : null;
  if (!id) {
    throw new Error(`${label} did not return a created definition id`);
  }
  activeDefinitionIds.add(id);
  return { capture: create, id };
}

async function captureUpdate(label: string, definition: Record<string, unknown>) {
  const update = await capture(label, updateDefinitionMutation, { definition });
  const payload = readPayloadObject(update.response, ['data', 'metafieldDefinitionUpdate']);
  assertNoUserErrors(payload, label);
  const updatedDefinition = readObject(payload?.['updatedDefinition']);
  if (typeof updatedDefinition?.['id'] !== 'string') {
    throw new Error(`${label} did not return an updated definition id`);
  }
  return update;
}

async function captureDelete(label: string, id: string, activeDefinitionIds: Set<string>) {
  const deleted = await capture(label, deleteDefinitionMutation, {
    id,
    deleteAllAssociatedMetafields: true,
  });
  const payload = readPayloadObject(deleted.response, ['data', 'metafieldDefinitionDelete']);
  assertNoUserErrors(payload, label);
  activeDefinitionIds.delete(id);
  return deleted;
}

const captures = [];
const cleanup = [];
const activeDefinitionIds = new Set<string>();

try {
  const customer = await captureCreate(
    'metafieldDefinitionCreate CUSTOMER',
    {
      name: 'HAR-691 Loyalty Tier',
      namespace: customerNamespace,
      key: 'tier',
      ownerType: 'CUSTOMER',
      type: 'single_line_text_field',
      description: 'Temporary HAR-691 customer definition',
      validations: [{ name: 'max', value: '32' }],
    },
    activeDefinitionIds,
  );
  captures.push(customer.capture);

  const order = await captureCreate(
    'metafieldDefinitionCreate ORDER',
    {
      name: 'HAR-691 Order Channel',
      namespace: orderNamespace,
      key: 'channel',
      ownerType: 'ORDER',
      type: 'single_line_text_field',
      description: 'Temporary HAR-691 order definition',
      validations: [{ name: 'max', value: '24' }],
    },
    activeDefinitionIds,
  );
  captures.push(order.capture);

  const company = await captureCreate(
    'metafieldDefinitionCreate COMPANY',
    {
      name: 'HAR-691 Company Segment',
      namespace: companyNamespace,
      key: 'segment',
      ownerType: 'COMPANY',
      type: 'single_line_text_field',
      description: 'Temporary HAR-691 company definition',
      validations: [{ name: 'max', value: '40' }],
    },
    activeDefinitionIds,
  );
  captures.push(company.capture);

  captures.push(
    await captureUpdate('metafieldDefinitionUpdate CUSTOMER', {
      name: 'HAR-691 Loyalty Tier Updated',
      namespace: customerNamespace,
      key: 'tier',
      ownerType: 'CUSTOMER',
      description: 'Updated temporary HAR-691 customer definition',
      validations: [{ name: 'max', value: '64' }],
    }),
  );

  captures.push(
    await capture('metafieldDefinition read CUSTOMER by id', readDefinitionQuery, {
      id: customer.id,
    }),
  );

  captures.push(await captureDelete('metafieldDefinitionDelete CUSTOMER', customer.id, activeDefinitionIds));
} finally {
  for (const id of activeDefinitionIds) {
    cleanup.push(
      await capture('cleanup metafieldDefinitionDelete', deleteDefinitionMutation, {
        id,
        deleteAllAssociatedMetafields: true,
      }).catch((error: unknown) => ({
        label: 'cleanup metafieldDefinitionDelete',
        id,
        error: String(error),
      })),
    );
  }
}

const fixture = {
  capturedAt: new Date().toISOString(),
  storeDomain,
  apiVersion,
  namespaces: {
    customer: customerNamespace,
    order: orderNamespace,
    company: companyNamespace,
  },
  captures,
  cleanup,
  upstreamCalls: [],
};

await mkdir(outputDir, { recursive: true });
await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');

console.log(`Wrote ${outputPath}`);
