/* oxlint-disable no-console -- CLI scripts intentionally write capture status output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type GraphqlCapture = {
  request: {
    query: string;
    variables: Record<string, unknown>;
  };
  status: number;
  response: unknown;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'metafields');
const outputPath = path.join(outputDir, 'metafield-definition-access-validation.json');
const paritySpecPath = path.join('config', 'parity-specs', 'metafield-definitions', 'access-validation.json');
const requestPaths = {
  create: path.join('config', 'parity-requests', 'metafield-definitions', 'access-validation-create.graphql'),
  update: path.join('config', 'parity-requests', 'metafield-definitions', 'access-validation-update.graphql'),
  standardEnable: path.join(
    'config',
    'parity-requests',
    'metafield-definitions',
    'access-validation-standard-enable.graphql',
  ),
  inlineGrants: path.join(
    'config',
    'parity-requests',
    'metafield-definitions',
    'access-validation-inline-grants.graphql',
  ),
};

const [createDocument, updateDocument, standardEnableDocument, inlineGrantsDocument] = await Promise.all([
  readFile(requestPaths.create, 'utf8'),
  readFile(requestPaths.update, 'utf8'),
  readFile(requestPaths.standardEnable, 'utf8'),
  readFile(requestPaths.inlineGrants, 'utf8'),
]);

const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

function readObject(value: unknown): Record<string, unknown> | null {
  return value && typeof value === 'object' && !Array.isArray(value) ? (value as Record<string, unknown>) : null;
}

function readPath(value: unknown, pathParts: string[]): unknown {
  let current = value;
  for (const part of pathParts) {
    const object = readObject(current);
    if (object === null) {
      return undefined;
    }
    current = object[part];
  }

  return current;
}

async function capture(query: string, variables: Record<string, unknown> = {}): Promise<GraphqlCapture> {
  const result: ConformanceGraphqlResult = await runGraphqlRaw(query, variables);
  return {
    request: { query, variables },
    status: result.status,
    response: result.payload,
  };
}

function readCreatedDefinitionId(captureResult: GraphqlCapture): string | undefined {
  const id = readPath(captureResult.response, ['data', 'metafieldDefinitionCreate', 'createdDefinition', 'id']);
  return typeof id === 'string' ? id : undefined;
}

function readMetafieldDefinitionId(captureResult: GraphqlCapture): string | undefined {
  const id = readPath(captureResult.response, ['data', 'metafieldDefinition', 'id']);
  return typeof id === 'string' ? id : undefined;
}

function readStandardEnableDefinitionId(captureResult: GraphqlCapture): string | undefined {
  const id = readPath(captureResult.response, ['data', 'standardMetafieldDefinitionEnable', 'createdDefinition', 'id']);
  return typeof id === 'string' ? id : undefined;
}

const runId = Date.now().toString(36);
const setupNamespace = `har1050_access_${runId}`;
const updateNamespace = `${setupNamespace}_update`;

const inlineGrants = await capture(inlineGrantsDocument);
const createMerchantAdminRead = await capture(createDocument, {
  definition: {
    name: 'Admin Read',
    namespace: `${setupNamespace}_admin`,
    key: 'flag',
    ownerType: 'PRODUCT',
    type: 'single_line_text_field',
    access: { admin: 'MERCHANT_READ' },
  },
});
const createShopifyStorefrontPublic = await capture(createDocument, {
  definition: {
    name: 'Shopify Public',
    namespace: 'shopify',
    key: setupNamespace,
    ownerType: 'PRODUCT',
    type: 'single_line_text_field',
    access: { storefront: 'PUBLIC_READ' },
  },
});
const setupDefinition = await capture(createDocument, {
  definition: {
    name: 'Access Validation Setup',
    namespace: updateNamespace,
    key: 'flag',
    ownerType: 'PRODUCT',
    type: 'single_line_text_field',
  },
});
const updateMerchantAdminRead = await capture(updateDocument, {
  definition: {
    namespace: updateNamespace,
    key: 'flag',
    ownerType: 'PRODUCT',
    access: { admin: 'MERCHANT_READ' },
  },
});
const standardAdminRead = await capture(standardEnableDocument, {
  ownerType: 'PRODUCT',
  id: 'gid://shopify/StandardMetafieldDefinitionTemplate/1',
  access: { admin: 'MERCHANT_READ' },
});
const standardReservedStorefrontPublic = await capture(standardEnableDocument, {
  ownerType: 'PRODUCT',
  namespace: 'shopify',
  key: 'color-pattern',
  access: { storefront: 'PUBLIC_READ' },
});
const reservedBeforeSetup = await capture(
  `#graphql
    query ReadReservedMetafieldDefinition($ownerType: MetafieldOwnerType!, $namespace: String!, $key: String!) {
      metafieldDefinition(identifier: { ownerType: $ownerType, namespace: $namespace, key: $key }) {
        id
        namespace
        key
      }
    }
  `,
  { ownerType: 'PRODUCT', namespace: 'shopify', key: 'color-pattern' },
);
const standardReservedSetup = await capture(standardEnableDocument, {
  ownerType: 'PRODUCT',
  namespace: 'shopify',
  key: 'color-pattern',
});
const updateReservedStorefrontPublic = await capture(updateDocument, {
  definition: {
    namespace: 'shopify',
    key: 'color-pattern',
    ownerType: 'PRODUCT',
    access: { storefront: 'PUBLIC_READ' },
  },
});

const cleanup: GraphqlCapture[] = [];
const setupDefinitionId = readCreatedDefinitionId(setupDefinition);
if (setupDefinitionId) {
  cleanup.push(
    await capture(
      `#graphql
        mutation DeleteMetafieldDefinitionAccessValidationSetup($id: ID!) {
          metafieldDefinitionDelete(id: $id, deleteAllAssociatedMetafields: true) {
            deletedDefinitionId
            userErrors {
              field
              message
              code
            }
          }
        }
      `,
      { id: setupDefinitionId },
    ),
  );
}
const reservedBeforeSetupId = readMetafieldDefinitionId(reservedBeforeSetup);
const standardReservedSetupId = readStandardEnableDefinitionId(standardReservedSetup);
if (!reservedBeforeSetupId && standardReservedSetupId) {
  cleanup.push(
    await capture(
      `#graphql
        mutation DeleteReservedMetafieldDefinitionAccessValidationSetup($id: ID!) {
          metafieldDefinitionDelete(id: $id, deleteAllAssociatedMetafields: true) {
            deletedDefinitionId
            userErrors {
              field
              message
              code
            }
          }
        }
      `,
      { id: standardReservedSetupId },
    ),
  );
}

await mkdir(outputDir, { recursive: true });
await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      storeDomain,
      apiVersion,
      capturedAt: new Date().toISOString(),
      variables: {
        runId,
        setupNamespace,
        updateNamespace,
      },
      inlineGrants,
      createMerchantAdminRead,
      createShopifyStorefrontPublic,
      setupDefinition,
      updateMerchantAdminRead,
      standardAdminRead,
      standardReservedStorefrontPublic,
      reservedBeforeSetup,
      standardReservedSetup,
      updateReservedStorefrontPublic,
      cleanup,
      upstreamCalls: [],
      notes:
        'Captured Shopify Admin API 2026-04 public access validation behavior. On this public schema, explicit access.grants is rejected as an unknown MetafieldAccessInput field before resolver execution; merchant admin access and reserved standard storefront access return resolver userErrors. Direct create/update under the shopify namespace are rejected by Shopify namespace access checks before resolver access validation.',
    },
    null,
    2,
  )}\n`,
);

await mkdir(path.dirname(paritySpecPath), { recursive: true });
await writeFile(
  paritySpecPath,
  `${JSON.stringify(
    {
      scenarioId: 'metafield-definitions-access-validation',
      operationNames: ['metafieldDefinitionCreate', 'metafieldDefinitionUpdate', 'standardMetafieldDefinitionEnable'],
      scenarioStatus: 'captured',
      assertionKinds: ['input-validation', 'user-errors-parity', 'no-local-staging-on-validation-error'],
      liveCaptureFiles: [outputPath],
      proxyRequest: {
        documentPath: requestPaths.inlineGrants,
        apiVersion,
      },
      comparisonMode: 'captured-vs-proxy-request',
      comparison: {
        mode: 'strict-json',
        expectedDifferences: [],
        targets: [
          {
            name: 'inline-grants-schema-rejection',
            capturePath: '$.inlineGrants.response',
            proxyPath: '$',
          },
          {
            name: 'create-merchant-admin-read-user-error',
            capturePath: '$.createMerchantAdminRead.response.data.metafieldDefinitionCreate',
            proxyPath: '$.data.metafieldDefinitionCreate',
            proxyRequest: {
              documentPath: requestPaths.create,
              apiVersion,
              variablesCapturePath: '$.createMerchantAdminRead.request.variables',
            },
          },
          {
            name: 'create-shopify-namespace-storefront-public-access-denied',
            capturePath: '$.createShopifyStorefrontPublic.response',
            proxyPath: '$',
            proxyRequest: {
              documentPath: requestPaths.create,
              apiVersion,
              variablesCapturePath: '$.createShopifyStorefrontPublic.request.variables',
            },
            excludedPaths: ['$.extensions', '$.errors[0].locations'],
          },
          {
            name: 'update-setup-definition',
            capturePath: '$.setupDefinition.response.data.metafieldDefinitionCreate',
            proxyPath: '$.data.metafieldDefinitionCreate',
            proxyRequest: {
              documentPath: requestPaths.create,
              apiVersion,
              variablesCapturePath: '$.setupDefinition.request.variables',
            },
            expectedDifferences: [
              {
                path: '$.createdDefinition.id',
                matcher: 'shopify-gid:MetafieldDefinition',
                reason:
                  'The proxy stages a deterministic synthetic definition ID while Shopify returned the live-store definition ID.',
              },
            ],
          },
          {
            name: 'update-merchant-admin-read-user-error',
            capturePath: '$.updateMerchantAdminRead.response.data.metafieldDefinitionUpdate',
            proxyPath: '$.data.metafieldDefinitionUpdate',
            proxyRequest: {
              documentPath: requestPaths.update,
              apiVersion,
              variablesCapturePath: '$.updateMerchantAdminRead.request.variables',
            },
          },
          {
            name: 'standard-admin-read-user-error',
            capturePath: '$.standardAdminRead.response.data.standardMetafieldDefinitionEnable',
            proxyPath: '$.data.standardMetafieldDefinitionEnable',
            proxyRequest: {
              documentPath: requestPaths.standardEnable,
              apiVersion,
              variablesCapturePath: '$.standardAdminRead.request.variables',
            },
          },
          {
            name: 'standard-reserved-storefront-public-user-error',
            capturePath: '$.standardReservedStorefrontPublic.response.data.standardMetafieldDefinitionEnable',
            proxyPath: '$.data.standardMetafieldDefinitionEnable',
            proxyRequest: {
              documentPath: requestPaths.standardEnable,
              apiVersion,
              variablesCapturePath: '$.standardReservedStorefrontPublic.request.variables',
            },
          },
          {
            name: 'standard-reserved-setup-definition',
            capturePath: '$.standardReservedSetup.response.data.standardMetafieldDefinitionEnable',
            proxyPath: '$.data.standardMetafieldDefinitionEnable',
            proxyRequest: {
              documentPath: requestPaths.standardEnable,
              apiVersion,
              variablesCapturePath: '$.standardReservedSetup.request.variables',
            },
            expectedDifferences: [
              {
                path: '$.createdDefinition.id',
                matcher: 'shopify-gid:MetafieldDefinition',
                reason:
                  'The proxy stages a deterministic synthetic definition ID while Shopify returned the live-store definition ID.',
              },
            ],
          },
          {
            name: 'update-shopify-namespace-storefront-public-access-denied',
            capturePath: '$.updateReservedStorefrontPublic.response',
            proxyPath: '$',
            proxyRequest: {
              documentPath: requestPaths.update,
              apiVersion,
              variablesCapturePath: '$.updateReservedStorefrontPublic.request.variables',
            },
            excludedPaths: ['$.extensions', '$.errors[0].locations'],
          },
        ],
      },
      notes:
        'Captured Shopify Admin API 2026-04 public access validation behavior for metafieldDefinitionCreate, metafieldDefinitionUpdate, and standardMetafieldDefinitionEnable. The public schema rejects access.grants as an argumentNotAccepted GraphQL error; merchant admin access must remain public_read_write, access controls cannot be set on the reserved shopify standard namespace, and direct create/update under shopify are blocked by namespace access checks.',
    },
    null,
    2,
  )}\n`,
);

console.log(`Wrote ${outputPath}`);
console.log(`Wrote ${paritySpecPath}`);
