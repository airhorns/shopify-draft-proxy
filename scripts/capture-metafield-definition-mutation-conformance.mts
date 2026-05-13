/* oxlint-disable no-console -- CLI scripts intentionally write status output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type CaptureCase = {
  name: string;
  variables: Record<string, unknown>;
};

type GraphqlCapture = {
  request: {
    query: string;
    variables: Record<string, unknown>;
  };
  status: number;
  response: ConformanceGraphqlResult['payload'];
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'metafields');
const outputPath = path.join(outputDir, 'standard-metafield-definition-enable-validation.json');
const regularUserErrorTypenamesDocumentPath =
  'config/parity-requests/metafields/metafield-definition-user-error-typenames.graphql';
const { runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const schemaQuery = `#graphql
  query StandardMetafieldDefinitionEnableSchema {
    mutationRoot: __type(name: "Mutation") {
      fields {
        name
        args {
          name
          type {
            kind
            name
            ofType {
              kind
              name
              ofType {
                kind
                name
                ofType {
                  kind
                  name
                }
              }
            }
          }
        }
        type {
          kind
          name
          ofType {
            kind
            name
          }
        }
      }
    }
    payload: __type(name: "StandardMetafieldDefinitionEnablePayload") {
      fields {
        name
        type {
          kind
          name
          ofType {
            kind
            name
            ofType {
              kind
              name
            }
          }
        }
      }
    }
    userError: __type(name: "StandardMetafieldDefinitionEnableUserError") {
      fields {
        name
        type {
          kind
          name
          ofType {
            kind
            name
            ofType {
              kind
              name
            }
          }
        }
      }
    }
  }
`;

const templateSampleQuery = `#graphql
  query StandardMetafieldDefinitionTemplatesSample {
    standardMetafieldDefinitionTemplates(first: 3, excludeActivated: false) {
      nodes {
        id
        namespace
        key
        name
        description
        ownerTypes
        type {
          name
          category
        }
        validations {
          name
          value
        }
        visibleToStorefrontApi
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
        startCursor
        endCursor
      }
    }
  }
`;

const validationMutation = `#graphql
  mutation StandardMetafieldDefinitionEnableValidation(
    $ownerType: MetafieldOwnerType!
    $id: ID
    $namespace: String
    $key: String
  ) {
    standardMetafieldDefinitionEnable(ownerType: $ownerType, id: $id, namespace: $namespace, key: $key) {
      createdDefinition {
        id
        namespace
        key
        ownerType
        name
        type {
          name
          category
        }
      }
      userErrors {
        __typename
        code
        field
        message
      }
    }
  }
`;

const productCreateMutation = `#graphql
  mutation RegularUserErrorTypenameProductCreate($product: ProductCreateInput!) {
    productCreate(product: $product) {
      product {
        id
        title
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const productDeleteMutation = `#graphql
  mutation RegularUserErrorTypenameProductDelete($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors {
        field
        message
      }
    }
  }
`;

const cleanupRegularUserErrorTypenameDefinitionsMutation = `#graphql
  mutation CleanupRegularUserErrorTypenameDefinitions($namespace: String!) {
    deleteReferenceTarget: metafieldDefinitionDelete(
      identifier: { ownerType: PRODUCT, namespace: $namespace, key: "delete_error" }
      deleteAllAssociatedMetafields: true
    ) {
      deletedDefinitionId
      userErrors {
        field
        message
        code
      }
    }
    deletePinTarget: metafieldDefinitionDelete(
      identifier: { ownerType: PRODUCT, namespace: $namespace, key: "pin_error" }
      deleteAllAssociatedMetafields: true
    ) {
      deletedDefinitionId
      userErrors {
        field
        message
        code
      }
    }
    deleteUnpinTarget: metafieldDefinitionDelete(
      identifier: { ownerType: PRODUCT, namespace: $namespace, key: "unpin_error" }
      deleteAllAssociatedMetafields: true
    ) {
      deletedDefinitionId
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const regularUserErrorTypenamesMutation = await readFile(regularUserErrorTypenamesDocumentPath, 'utf8');

const validationCases: CaptureCase[] = [
  {
    name: 'missing-selector',
    variables: {
      ownerType: 'PRODUCT',
    },
  },
  {
    name: 'unknown-id',
    variables: {
      ownerType: 'PRODUCT',
      id: 'gid://shopify/StandardMetafieldDefinitionTemplate/999999999',
    },
  },
  {
    name: 'unknown-namespace-key',
    variables: {
      ownerType: 'PRODUCT',
      namespace: 'codex_missing_standard',
      key: 'codex_missing_key',
    },
  },
  {
    name: 'incompatible-owner-type',
    variables: {
      ownerType: 'CUSTOMER',
      id: 'gid://shopify/StandardMetafieldDefinitionTemplate/1',
    },
  },
];

function assertGraphqlOk(result: ConformanceGraphqlResult, label: string): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${label} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function readObject(value: unknown): Record<string, unknown> | null {
  return value && typeof value === 'object' && !Array.isArray(value) ? (value as Record<string, unknown>) : null;
}

function readPath(value: unknown, pathParts: string[]): unknown {
  let current: unknown = value;
  for (const part of pathParts) {
    const object = readObject(current);
    if (object === null) {
      return undefined;
    }
    current = object[part];
  }
  return current;
}

function readRequiredStringPath(value: unknown, pathParts: string[], label: string): string {
  const found = readPath(value, pathParts);
  if (typeof found !== 'string' || found.length === 0) {
    throw new Error(`${label} did not return a string at ${pathParts.join('.')}: ${JSON.stringify(value, null, 2)}`);
  }

  return found;
}

function captureFromResult(
  query: string,
  variables: Record<string, unknown>,
  result: ConformanceGraphqlResult,
): GraphqlCapture {
  return {
    request: {
      query,
      variables,
    },
    status: result.status,
    response: result.payload,
  };
}

async function captureSchema() {
  const result = await runGraphqlRaw(schemaQuery, {});
  assertGraphqlOk(result, 'standardMetafieldDefinitionEnable schema introspection');

  const payload = readObject(result.payload);
  const data = readObject(payload?.['data']);
  const mutationRoot = readObject(data?.['mutationRoot']);
  const fields = mutationRoot?.['fields'];
  const standardMetafieldDefinitionEnable =
    Array.isArray(fields) && fields.every((field) => readObject(field) !== null)
      ? fields.find((field) => readObject(field)?.['name'] === 'standardMetafieldDefinitionEnable')
      : null;

  return {
    request: {
      query: schemaQuery,
      variables: {},
    },
    status: result.status,
    response: {
      ...result.payload,
      data: {
        ...data,
        mutationRoot: {
          ...mutationRoot,
          fields: standardMetafieldDefinitionEnable ? [standardMetafieldDefinitionEnable] : [],
        },
      },
    },
  };
}

async function captureGraphqlRawRecord(
  query: string,
  variables: Record<string, unknown> = {},
): Promise<GraphqlCapture> {
  const result = await runGraphqlRaw(query, variables);
  return captureFromResult(query, variables, result);
}

async function captureGraphql(
  label: string,
  query: string,
  variables: Record<string, unknown> = {},
): Promise<GraphqlCapture> {
  const result = await runGraphqlRaw(query, variables);
  assertGraphqlOk(result, label);
  return captureFromResult(query, variables, result);
}

async function captureRegularUserErrorTypenames() {
  const runId = Date.now().toString(36);
  const namespace = `har1213_typename_${runId}`;
  const setup: GraphqlCapture[] = [];
  const cleanup: GraphqlCapture[] = [];
  let ownerProductId: string | undefined;
  let referenceProductId: string | undefined;
  let regularUserErrorTypenames: GraphqlCapture | undefined;

  try {
    const ownerProduct = await captureGraphql('regular typename owner productCreate', productCreateMutation, {
      product: { title: `HAR-1213 typename owner ${runId}` },
    });
    setup.push(ownerProduct);
    ownerProductId = readRequiredStringPath(
      ownerProduct.response,
      ['data', 'productCreate', 'product', 'id'],
      'regular typename owner productCreate',
    );

    const referenceProduct = await captureGraphql('regular typename reference productCreate', productCreateMutation, {
      product: { title: `HAR-1213 typename reference ${runId}` },
    });
    setup.push(referenceProduct);
    referenceProductId = readRequiredStringPath(
      referenceProduct.response,
      ['data', 'productCreate', 'product', 'id'],
      'regular typename reference productCreate',
    );

    const variables = {
      namespace,
      ownerProductId,
      referenceProductId,
      categoryId: 'gid://shopify/TaxonomyCategory/ap-2',
    };
    const result = await runGraphqlRaw(regularUserErrorTypenamesMutation, variables);
    regularUserErrorTypenames = captureFromResult(regularUserErrorTypenamesMutation, variables, result);
    assertGraphqlOk(result, 'metafieldDefinition regular mutation userError typenames');
  } finally {
    cleanup.push(
      await captureGraphqlRawRecord(cleanupRegularUserErrorTypenameDefinitionsMutation, { namespace }).catch(
        (error: unknown) => ({
          request: { query: cleanupRegularUserErrorTypenameDefinitionsMutation, variables: { namespace } },
          status: 0,
          response: { error: String(error) },
        }),
      ),
    );
    if (ownerProductId) {
      cleanup.push(
        await captureGraphqlRawRecord(productDeleteMutation, { input: { id: ownerProductId } }).catch(
          (error: unknown) => ({
            request: { query: productDeleteMutation, variables: { input: { id: ownerProductId } } },
            status: 0,
            response: { error: String(error) },
          }),
        ),
      );
    }
    if (referenceProductId) {
      cleanup.push(
        await captureGraphqlRawRecord(productDeleteMutation, { input: { id: referenceProductId } }).catch(
          (error: unknown) => ({
            request: { query: productDeleteMutation, variables: { input: { id: referenceProductId } } },
            status: 0,
            response: { error: String(error) },
          }),
        ),
      );
    }
  }

  if (!regularUserErrorTypenames) {
    throw new Error('metafieldDefinition regular mutation userError typenames did not complete');
  }

  return {
    variables: {
      namespace,
      ownerProductId,
      referenceProductId,
      categoryId: 'gid://shopify/TaxonomyCategory/ap-2',
    },
    setup,
    capture: regularUserErrorTypenames,
    cleanup,
  };
}

const schema = await captureSchema();
const templateSample = await captureGraphql('standard metafield definition template sample', templateSampleQuery);
const validation = [];

for (const validationCase of validationCases) {
  validation.push({
    name: validationCase.name,
    ...(await captureGraphql(
      `standardMetafieldDefinitionEnable ${validationCase.name}`,
      validationMutation,
      validationCase.variables,
    )),
  });
}
const regularUserErrorTypenamesFlow = await captureRegularUserErrorTypenames();

const fixture = {
  capturedAt: new Date().toISOString(),
  storeDomain,
  apiVersion,
  upstreamCalls: [],
  safety: {
    successfulEnablementNotCaptured:
      'This fixture records standardMetafieldDefinitionEnable validation branches plus a disposable regular metafieldDefinition user-error typename setup with explicit cleanup evidence.',
  },
  schema,
  templateSample,
  validation,
  regularUserErrorTypenamesSetup: regularUserErrorTypenamesFlow.setup,
  regularUserErrorTypenamesVariables: regularUserErrorTypenamesFlow.variables,
  regularUserErrorTypenames: regularUserErrorTypenamesFlow.capture,
  regularUserErrorTypenamesCleanup: regularUserErrorTypenamesFlow.cleanup,
};

await mkdir(outputDir, { recursive: true });
await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`);
console.log(`Wrote ${outputPath}`);
