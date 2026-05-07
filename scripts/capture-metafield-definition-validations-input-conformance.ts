/* oxlint-disable no-console -- CLI scripts intentionally write status output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
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
  defaultApiVersion: '2025-01',
  exitOnMissing: true,
});
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'metafields');
const outputPath = path.join(outputDir, 'metafield-definition-validations-input.json');
const paritySpecPath = path.join(
  'config',
  'parity-specs',
  'metafield-definitions',
  'metafield-definition-validations-input.json',
);
const requestPath = path.join(
  'config',
  'parity-requests',
  'metafield-definitions',
  'metafield-definition-validations-input.graphql',
);

const requestDocument = await readFile(requestPath, 'utf8');
const runId = Date.now().toString(36);
const namespace = `validations_input_${runId}`;
const targetType = `validations_input_target_${runId}`;
const replacementType = `validations_input_replacement_${runId}`;

const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

async function capture(query: string, variables: Record<string, unknown>): Promise<GraphqlCapture> {
  const result = await runGraphqlRaw(query, variables);
  return {
    request: { query, variables },
    status: result.status,
    response: result.payload,
  };
}

const createMetaobjectDefinitionMutation = `#graphql
  mutation CreateMetaobjectDefinitionForMetafieldValidation($definition: MetaobjectDefinitionCreateInput!) {
    metaobjectDefinitionCreate(definition: $definition) {
      metaobjectDefinition {
        id
        type
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const deleteMetaobjectDefinitionMutation = `#graphql
  mutation DeleteMetaobjectDefinitionForMetafieldValidation($id: ID!) {
    metaobjectDefinitionDelete(id: $id) {
      deletedId
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const deleteMetafieldDefinitionMutation = `#graphql
  mutation DeleteMetafieldDefinitionForValidationInput($id: ID!) {
    metafieldDefinitionDelete(id: $id, deleteAllAssociatedMetafields: true) {
      deletedDefinitionId
      userErrors {
        field
        message
        code
      }
    }
  }
`;

function readMetaobjectDefinitionId(captureResult: GraphqlCapture): string {
  const payload = captureResult.response as {
    data?: {
      metaobjectDefinitionCreate?: {
        metaobjectDefinition?: {
          id?: unknown;
        } | null;
      } | null;
    };
  };
  const id = payload.data?.metaobjectDefinitionCreate?.metaobjectDefinition?.id;
  if (typeof id !== 'string') {
    throw new Error(`Metaobject definition setup failed: ${JSON.stringify(captureResult.response)}`);
  }
  return id;
}

function readCreatedMetafieldDefinitionId(captureResult: GraphqlCapture): string | undefined {
  const payload = captureResult.response as {
    data?: {
      createMetaobjectReference?: {
        createdDefinition?: {
          id?: unknown;
        } | null;
      } | null;
    };
  };
  const id = payload.data?.createMetaobjectReference?.createdDefinition?.id;
  return typeof id === 'string' ? id : undefined;
}

async function createMetaobjectDefinition(type: string, name: string): Promise<GraphqlCapture> {
  return capture(createMetaobjectDefinitionMutation, {
    definition: {
      type,
      name,
      fieldDefinitions: [{ key: 'title', name: 'Title', type: 'single_line_text_field' }],
    },
  });
}

const setupTarget = await createMetaobjectDefinition(targetType, `Validation Target ${runId}`);
const setupReplacement = await createMetaobjectDefinition(replacementType, `Validation Replacement ${runId}`);
const targetDefinitionId = readMetaobjectDefinitionId(setupTarget);
const replacementDefinitionId = readMetaobjectDefinitionId(setupReplacement);
const variables = {
  namespace,
  targetDefinitionId,
  replacementDefinitionId,
};

const primary = await capture(requestDocument, variables);
const createdMetafieldDefinitionId = readCreatedMetafieldDefinitionId(primary);
const cleanup: GraphqlCapture[] = [];
if (createdMetafieldDefinitionId) {
  cleanup.push(await capture(deleteMetafieldDefinitionMutation, { id: createdMetafieldDefinitionId }));
}
cleanup.push(await capture(deleteMetaobjectDefinitionMutation, { id: targetDefinitionId }));
cleanup.push(await capture(deleteMetaobjectDefinitionMutation, { id: replacementDefinitionId }));

await mkdir(outputDir, { recursive: true });
await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      storeDomain,
      apiVersion,
      capturedAt: new Date().toISOString(),
      variables,
      setup: {
        target: setupTarget,
        replacement: setupReplacement,
      },
      primary,
      cleanup,
      upstreamCalls: [],
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
      scenarioId: 'metafield-definition-validations-input',
      operationNames: ['metafieldDefinitionCreate', 'metafieldDefinitionUpdate'],
      scenarioStatus: 'captured',
      assertionKinds: ['user-errors-parity', 'input-validation', 'no-local-staging-on-validation-error'],
      liveCaptureFiles: [outputPath],
      proxyRequest: {
        documentPath: requestPath,
        variablesCapturePath: '$.variables',
        apiVersion,
      },
      comparisonMode: 'captured-vs-proxy-request',
      comparison: {
        mode: 'strict-json',
        expectedDifferences: [],
        targets: [
          {
            name: 'unknown-option-name',
            capturePath: '$.primary.response.data.unknown',
            proxyPath: '$.data.unknown',
          },
          {
            name: 'duplicate-option-name',
            capturePath: '$.primary.response.data.duplicate',
            proxyPath: '$.data.duplicate',
          },
          {
            name: 'invalid-integer-coercion',
            capturePath: '$.primary.response.data.badNumber',
            proxyPath: '$.data.badNumber',
          },
          {
            name: 'min-greater-than-max',
            capturePath: '$.primary.response.data.minMax',
            proxyPath: '$.data.minMax',
          },
          {
            name: 'metaobject-reference-required-option',
            capturePath: '$.primary.response.data.metaobjectRequired',
            proxyPath: '$.data.metaobjectRequired',
          },
          {
            name: 'rating-required-options',
            capturePath: '$.primary.response.data.ratingRequired',
            proxyPath: '$.data.ratingRequired',
          },
          {
            name: 'metaobject-reference-create',
            capturePath: '$.primary.response.data.createMetaobjectReference',
            proxyPath: '$.data.createMetaobjectReference',
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
            name: 'metaobject-definition-id-immutability',
            capturePath: '$.primary.response.data.updateMetaobjectReference',
            proxyPath: '$.data.updateMetaobjectReference',
          },
        ],
      },
      notes:
        'Captured against Shopify Admin API validation branches for metafield definition validations[]. The scenario verifies create-time unknown/duplicate/required/coercion/min-max errors and update-time metaobject_definition_id immutability. Validation failures return null definitions and do not allocate local staged definitions.',
    },
    null,
    2,
  )}\n`,
);

console.log(
  JSON.stringify(
    {
      outputPath,
      paritySpecPath,
      namespace,
      apiVersion,
      status: primary.status,
    },
    null,
    2,
  ),
);
