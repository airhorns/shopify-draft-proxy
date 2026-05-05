/* oxlint-disable no-console -- CLI scripts intentionally write capture status output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'metafields');
const outputPath = path.join(outputDir, 'metafield-definition-non-product-metafields.json');
const { runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const suffix = Date.now().toString(36);
const customerNamespace = `har691_value_customer_${suffix}`;
const orderNamespace = `har691_value_order_${suffix}`;
const companyNamespace = `har691_value_company_${suffix}`;

async function readText(filePath: string): Promise<string> {
  return readFile(filePath, 'utf8');
}

async function readJson(filePath: string): Promise<JsonRecord> {
  return JSON.parse(await readText(filePath)) as JsonRecord;
}

function readObject(value: unknown): JsonRecord | null {
  return value && typeof value === 'object' && !Array.isArray(value) ? (value as JsonRecord) : null;
}

function readStringAtPath(value: unknown, pathSegments: string[]): string {
  let current: unknown = value;
  for (const segment of pathSegments) {
    current = readObject(current)?.[segment];
  }
  if (typeof current !== 'string') {
    throw new Error(`Expected string at ${pathSegments.join('.')}: ${JSON.stringify(value, null, 2)}`);
  }
  return current;
}

function assertHttpOk(result: ConformanceGraphqlResult, label: string): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${label} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function assertNoUserErrors(payload: unknown, label: string): void {
  const userErrors = readObject(payload)?.['userErrors'];
  if (!Array.isArray(userErrors) || userErrors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(userErrors)}`);
  }
}

async function capture(label: string, query: string, variables: JsonRecord) {
  const result = await runGraphqlRaw(query, variables);
  assertHttpOk(result, label);
  return {
    label,
    request: { query, variables },
    status: result.status,
    response: result.payload,
  };
}

async function captureDefinitionCreate(
  label: string,
  query: string,
  definition: JsonRecord,
  activeDefinitionIds: Set<string>,
) {
  const create = await capture(label, query, { definition });
  const payload = readObject(readObject(create.response)?.['data'])?.['metafieldDefinitionCreate'];
  assertNoUserErrors(payload, label);
  const id = readStringAtPath(payload, ['createdDefinition', 'id']);
  activeDefinitionIds.add(id);
  return create;
}

async function captureMetafieldsSet(label: string, query: string, ownerId: string, namespace: string, key: string) {
  const set = await capture(label, query, {
    metafields: [
      {
        ownerId,
        namespace,
        key,
        type: 'single_line_text_field',
        value: `${label} value`,
      },
    ],
  });
  const payload = readObject(readObject(set.response)?.['data'])?.['metafieldsSet'];
  assertNoUserErrors(payload, label);
  return set;
}

const customerCreateDocument = await readText('config/parity-requests/customers/customerCreate-parity-plan.graphql');
const orderCreateDocument = await readText('config/parity-requests/orders/orderCreate-parity-plan.graphql');
const companyCreateDocument = await readText('config/parity-requests/b2b/b2b-company-create-lifecycle.graphql');
const definitionCreateDocument = await readText(
  'config/parity-requests/metafields/metafield-definition-non-product-create.graphql',
);
const metafieldsSetDocument = await readText(
  'config/parity-requests/metafields/metafield-definition-non-product-metafields-set.graphql',
);
const customerReadDocument = await readText(
  'config/parity-requests/metafields/metafield-definition-non-product-metafields-customer-read.graphql',
);
const orderReadDocument = await readText(
  'config/parity-requests/metafields/metafield-definition-non-product-metafields-order-read.graphql',
);
const companyReadDocument = await readText(
  'config/parity-requests/metafields/metafield-definition-non-product-metafields-company-read.graphql',
);

const customerCreateVariables = await readJson(
  'config/parity-requests/customers/customerCreate-parity-plan.variables.json',
);
const customerInput = readObject(customerCreateVariables['input']);
if (!customerInput) {
  throw new Error('customerCreate variables missing input');
}
customerInput['email'] = `har691-metafields-${suffix}@example.com`;
delete customerInput['phone'];

const orderCreateVariables = await readJson('config/parity-requests/orders/orderCreate-parity-plan.variables.json');
const orderInput = readObject(orderCreateVariables['order']);
if (!orderInput) {
  throw new Error('orderCreate variables missing order');
}
orderInput['email'] = `har691-order-metafields-${suffix}@example.com`;
orderInput['note'] = `HAR-691 non-product metafields ${suffix}`;
delete orderInput['presentmentCurrency'];
const lineItems = orderInput['lineItems'];
if (Array.isArray(lineItems)) {
  for (const lineItem of lineItems) {
    const priceSet = readObject(readObject(lineItem)?.['priceSet']);
    if (priceSet) {
      delete priceSet['presentmentMoney'];
    }
  }
}

const companyCreateVariables = await readJson('config/parity-requests/b2b/b2b-company-create-lifecycle.variables.json');
const companyInput = readObject(companyCreateVariables['input']);
const companyFields = readObject(companyInput?.['company']);
if (!companyFields) {
  throw new Error('companyCreate variables missing input.company');
}
companyFields['name'] = `HAR-691 metafields ${suffix}`;
companyFields['externalId'] = `har-691-metafields-${suffix}`;

const captures = [];
const cleanup = [];
const activeDefinitionIds = new Set<string>();
let customerId: string | null = null;
let companyId: string | null = null;

const customerDeleteMutation = `#graphql
  mutation HAR691MetafieldsCustomerDelete($input: CustomerDeleteInput!) {
    customerDelete(input: $input) {
      deletedCustomerId
      userErrors { field message }
    }
  }
`;
const companyDeleteMutation = `#graphql
  mutation HAR691MetafieldsCompanyDelete($id: ID!) {
    companyDelete(id: $id) {
      deletedCompanyId
      userErrors { field message code }
    }
  }
`;
const deleteDefinitionMutation = `#graphql
  mutation HAR691MetafieldsDefinitionDelete($id: ID!, $deleteAllAssociatedMetafields: Boolean!) {
    metafieldDefinitionDelete(id: $id, deleteAllAssociatedMetafields: $deleteAllAssociatedMetafields) {
      deletedDefinitionId
      userErrors { field message code }
    }
  }
`;

try {
  const customerCreate = await capture('customerCreate owner setup', customerCreateDocument, customerCreateVariables);
  assertNoUserErrors(
    readObject(readObject(customerCreate.response)?.['data'])?.['customerCreate'],
    'customerCreate owner setup',
  );
  customerId = readStringAtPath(customerCreate.response, ['data', 'customerCreate', 'customer', 'id']);
  captures.push(customerCreate);

  captures.push(
    await captureDefinitionCreate(
      'metafieldDefinitionCreate CUSTOMER for metafieldsSet',
      definitionCreateDocument,
      {
        name: 'HAR-691 Customer Value',
        namespace: customerNamespace,
        key: 'value',
        ownerType: 'CUSTOMER',
        type: 'single_line_text_field',
        description: 'Temporary HAR-691 customer value definition',
      },
      activeDefinitionIds,
    ),
  );
  captures.push(
    await captureMetafieldsSet('CUSTOMER metafieldsSet', metafieldsSetDocument, customerId, customerNamespace, 'value'),
  );
  captures.push(
    await capture('customer metafield read after set', customerReadDocument, {
      id: customerId,
      namespace: customerNamespace,
      key: 'value',
    }),
  );

  const orderCreate = await capture('orderCreate owner setup', orderCreateDocument, orderCreateVariables);
  assertNoUserErrors(
    readObject(readObject(orderCreate.response)?.['data'])?.['orderCreate'],
    'orderCreate owner setup',
  );
  const orderId = readStringAtPath(orderCreate.response, ['data', 'orderCreate', 'order', 'id']);
  captures.push(orderCreate);

  captures.push(
    await captureDefinitionCreate(
      'metafieldDefinitionCreate ORDER for metafieldsSet',
      definitionCreateDocument,
      {
        name: 'HAR-691 Order Value',
        namespace: orderNamespace,
        key: 'value',
        ownerType: 'ORDER',
        type: 'single_line_text_field',
        description: 'Temporary HAR-691 order value definition',
      },
      activeDefinitionIds,
    ),
  );
  captures.push(
    await captureMetafieldsSet('ORDER metafieldsSet', metafieldsSetDocument, orderId, orderNamespace, 'value'),
  );
  captures.push(
    await capture('order metafield read after set', orderReadDocument, {
      id: orderId,
      namespace: orderNamespace,
      key: 'value',
    }),
  );

  const companyCreate = await capture('companyCreate owner setup', companyCreateDocument, companyCreateVariables);
  assertNoUserErrors(
    readObject(readObject(companyCreate.response)?.['data'])?.['companyCreate'],
    'companyCreate owner setup',
  );
  companyId = readStringAtPath(companyCreate.response, ['data', 'companyCreate', 'company', 'id']);
  captures.push(companyCreate);

  captures.push(
    await captureDefinitionCreate(
      'metafieldDefinitionCreate COMPANY for metafieldsSet',
      definitionCreateDocument,
      {
        name: 'HAR-691 Company Value',
        namespace: companyNamespace,
        key: 'value',
        ownerType: 'COMPANY',
        type: 'single_line_text_field',
        description: 'Temporary HAR-691 company value definition',
      },
      activeDefinitionIds,
    ),
  );
  captures.push(
    await captureMetafieldsSet('COMPANY metafieldsSet', metafieldsSetDocument, companyId, companyNamespace, 'value'),
  );
  captures.push(
    await capture('company metafield read after set', companyReadDocument, {
      id: companyId,
      namespace: companyNamespace,
      key: 'value',
    }),
  );
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
  if (companyId) {
    cleanup.push(
      await capture('cleanup companyDelete', companyDeleteMutation, { id: companyId }).catch((error: unknown) => ({
        label: 'cleanup companyDelete',
        id: companyId,
        error: String(error),
      })),
    );
  }
  if (customerId) {
    cleanup.push(
      await capture('cleanup customerDelete', customerDeleteMutation, { input: { id: customerId } }).catch(
        (error: unknown) => ({
          label: 'cleanup customerDelete',
          id: customerId,
          error: String(error),
        }),
      ),
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
