/* oxlint-disable no-console -- CLI scripts intentionally write status output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type Capture = {
  name: string;
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
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'metaobjects');
const outputPath = path.join(outputDir, 'metaobject-definition-lifecycle-invariants.json');
const runId = Date.now().toString();

const requestPaths = {
  create: 'config/parity-requests/metaobjects/metaobject-definition-lifecycle-invariants-create.graphql',
};

const queries = Object.fromEntries(
  await Promise.all(
    Object.entries(requestPaths).map(async ([name, requestPath]) => [name, await readFile(requestPath, 'utf8')]),
  ),
) as Record<keyof typeof requestPaths, string>;

const deleteGuardCandidateDiscoveryQuery = `#graphql
  query MetaobjectDefinitionDeleteGuardCandidateDiscovery($first: Int!) {
    metaobjectDefinitionType: __type(name: "MetaobjectDefinition") {
      fields {
        name
      }
    }
    standardTemplateType: __type(name: "StandardMetaobjectDefinitionTemplate") {
      fields {
        name
      }
    }
    metaobjectDefinitions(first: $first) {
      nodes {
        id
        type
        name
        standardTemplate {
          type
          name
        }
        createdByApp {
          id
          apiKey
          handle
          title
        }
        metaobjectsCount
      }
      pageInfo {
        hasNextPage
        endCursor
      }
    }
  }
`;

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

function readUserErrors(payload: unknown, pathParts: string[]): unknown[] {
  const value = readPath(payload, pathParts);
  return Array.isArray(value) ? value : [];
}

function assertGraphqlOk(result: ConformanceGraphqlResult, label: string): void {
  if (result.status < 200 || result.status >= 300 || readPath(result.payload, ['errors'])) {
    throw new Error(`${label} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function assertHasUserErrorCode(payload: unknown, pathParts: string[], code: string, label: string): void {
  const userErrors = readUserErrors(payload, pathParts);
  if (!userErrors.some((error) => readPath(error, ['code']) === code)) {
    throw new Error(`${label} did not return ${code}: ${JSON.stringify(userErrors, null, 2)}`);
  }
}

function captureFromResult(
  name: string,
  query: string,
  variables: Record<string, unknown>,
  result: ConformanceGraphqlResult,
): Capture {
  return {
    name,
    request: {
      query,
      variables,
    },
    status: result.status,
    response: result.payload,
  };
}

const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

async function captureGraphql(name: string, query: string, variables: Record<string, unknown>): Promise<Capture> {
  const result = await runGraphqlRaw(query, variables);
  assertGraphqlOk(result, name);
  return captureFromResult(name, query, variables, result);
}

const standardTemplateReservedCreate = await captureGraphql('standard-template-reserved-create', queries.create, {
  definition: {
    type: 'shopify--qa-pair',
    name: 'Reserved Standard Template Probe',
    displayNameKey: 'title',
    fieldDefinitions: [{ key: 'title', name: 'Title', type: 'single_line_text_field', required: true }],
  },
});
assertHasUserErrorCode(
  standardTemplateReservedCreate.response,
  ['data', 'metaobjectDefinitionCreate', 'userErrors'],
  'NOT_AUTHORIZED',
  'standard-template-reserved-create',
);

const shopifyReservedPrefixCreate = await captureGraphql('shopify-reserved-prefix-create', queries.create, {
  definition: {
    type: `shopify--lifecycle-invariants-${runId}`,
    name: 'Reserved Prefix Probe',
    displayNameKey: 'title',
    fieldDefinitions: [{ key: 'title', name: 'Title', type: 'single_line_text_field', required: true }],
  },
});
assertHasUserErrorCode(
  shopifyReservedPrefixCreate.response,
  ['data', 'metaobjectDefinitionCreate', 'userErrors'],
  'NOT_AUTHORIZED',
  'shopify-reserved-prefix-create',
);

const deleteGuardCandidateDiscovery = await captureGraphql(
  'delete-guard-candidate-discovery',
  deleteGuardCandidateDiscoveryQuery,
  {
    first: 250,
  },
);

await mkdir(outputDir, { recursive: true });
await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      capturedAt: new Date().toISOString(),
      storeDomain,
      apiVersion,
      summary:
        'Metaobject definition lifecycle invariant probes for reserved create validation plus public-schema discovery for delete guard candidate availability.',
      standardTemplateReservedCreate,
      shopifyReservedPrefixCreate,
      deleteGuardCandidateDiscovery,
      unavailableDeleteGuardEvidence: [
        {
          code: 'APP_CONFIG_MANAGED',
          reason:
            'The current conformance shop exposes no visible metaobject definition created by another app, so an app-config-managed delete rejection cannot be safely recorded without a different app/shop setup.',
        },
        {
          code: 'STANDARD_METAOBJECT_DEFINITION_DEPENDENT_ON_APP',
          reason:
            'Public Admin GraphQL 2026-04 does not expose standard_template_id or dependent_on_app metadata on MetaobjectDefinition or StandardMetaobjectDefinitionTemplate, so a dependent-on-app standard definition cannot be identified safely from this credential.',
        },
      ],
      cleanup: [],
      upstreamCalls: [],
    },
    null,
    2,
  )}\n`,
);
console.log(`Wrote ${outputPath}`);
