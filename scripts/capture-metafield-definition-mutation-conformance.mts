/* oxlint-disable no-console -- CLI scripts intentionally write status output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type CaptureCase = {
  name: string;
  variables: Record<string, unknown>;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'metafields');
const outputPath = path.join(outputDir, 'standard-metafield-definition-enable-validation.json');
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
        field
        message
        code
      }
    }
  }
`;

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

async function captureGraphql(label: string, query: string, variables: Record<string, unknown> = {}) {
  const result = await runGraphqlRaw(query, variables);
  assertGraphqlOk(result, label);

  return {
    request: {
      query,
      variables,
    },
    status: result.status,
    response: result.payload,
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

const fixture = {
  capturedAt: new Date().toISOString(),
  storeDomain,
  apiVersion,
  safety: {
    successfulEnablementNotCaptured:
      'This fixture records validation branches only. Successful standardMetafieldDefinitionEnable calls create real metafield definitions and may be captured in a disposable test-shop setup with explicit cleanup evidence.',
  },
  schema,
  templateSample,
  validation,
};

await mkdir(outputDir, { recursive: true });
await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`);
console.log(`Wrote ${outputPath}`);
