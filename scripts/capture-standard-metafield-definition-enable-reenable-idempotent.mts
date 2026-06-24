/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'metafields');
const outputPath = path.join(outputDir, 'standard-metafield-definition-enable-reenable-idempotent.json');
const primaryDocumentPath =
  'config/parity-requests/metafields/standard-metafield-definition-enable-reenable-idempotent.graphql';
const readAfterDocumentPath =
  'config/parity-requests/metafields/standard-metafield-definition-enable-reenable-read.graphql';

const { runGraphql } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const readPinnedDefinitionsQuery = `#graphql
  query ExistingPinnedMetafieldDefinitions {
    metafieldDefinitions(ownerType: PRODUCT, first: 50, pinnedStatus: PINNED, sortKey: PINNED_POSITION) {
      nodes {
        id
        namespace
        key
        pinnedPosition
      }
    }
  }
`;

const readNamespaceDefinitionsQuery = `#graphql
  query TemporaryNamespaceDefinitions($namespace: String!) {
    metafieldDefinitions(ownerType: PRODUCT, first: 100, namespace: $namespace) {
      nodes {
        id
        namespace
        key
        pinnedPosition
      }
    }
  }
`;

const readStandardDefinitionQuery = `#graphql
  query ExistingStandardSubtitleDefinition {
    metafieldDefinition(identifier: { ownerType: PRODUCT, namespace: "descriptors", key: "subtitle" }) {
      id
      namespace
      key
      pinnedPosition
    }
  }
`;

const pinByIdMutation = `#graphql
  mutation RestorePinnedMetafieldDefinition($definitionId: ID!) {
    metafieldDefinitionPin(definitionId: $definitionId) {
      pinnedDefinition {
        id
        pinnedPosition
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const unpinByIdMutation = `#graphql
  mutation TemporarilyUnpinMetafieldDefinition($definitionId: ID!) {
    metafieldDefinitionUnpin(definitionId: $definitionId) {
      unpinnedDefinition {
        id
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const deleteDefinitionMutation = `#graphql
  mutation DeleteTemporaryMetafieldDefinition($id: ID!) {
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

type DefinitionNode = {
  id: string;
  namespace?: string;
  key?: string;
  pinnedPosition?: number | null;
};

type GraphqlResponse = {
  data?: Record<string, unknown>;
};

const namespace = `standard_reenable_${Date.now().toString(36)}`;
const variables = {
  namespace,
  standardTemplateId: 'gid://shopify/StandardMetafieldDefinitionTemplate/1',
};

const primaryDocument = await readFile(primaryDocumentPath, 'utf8');
const readAfterDocument = await readFile(readAfterDocumentPath, 'utf8');
const baselinePinned =
  ((await runGraphql(readPinnedDefinitionsQuery)).data?.metafieldDefinitions?.nodes as DefinitionNode[] | undefined) ??
  [];
const baselineStandardDefinition =
  ((await runGraphql(readStandardDefinitionQuery)).data?.metafieldDefinition as DefinitionNode | null | undefined) ??
  null;

function readStandardInitialId(response: GraphqlResponse): string {
  const data = response.data;
  const standardInitial = data?.['standardInitial'];
  if (typeof standardInitial !== 'object' || standardInitial === null || Array.isArray(standardInitial)) {
    throw new Error(`standardInitial payload missing from response: ${JSON.stringify(response, null, 2)}`);
  }
  const standardInitialRecord = standardInitial as Record<string, unknown>;
  const createdDefinition = standardInitialRecord['createdDefinition'];
  if (typeof createdDefinition !== 'object' || createdDefinition === null || Array.isArray(createdDefinition)) {
    throw new Error(`standardInitial.createdDefinition missing from response: ${JSON.stringify(response, null, 2)}`);
  }
  const createdDefinitionRecord = createdDefinition as Record<string, unknown>;
  const id = createdDefinitionRecord['id'];
  if (typeof id !== 'string') {
    throw new Error(`standardInitial.createdDefinition.id missing from response: ${JSON.stringify(response, null, 2)}`);
  }
  return id;
}

async function deleteNamespaceDefinitions(): Promise<DefinitionNode[]> {
  const read = await runGraphql(readNamespaceDefinitionsQuery, { namespace });
  const definitions = (read.data?.metafieldDefinitions?.nodes as DefinitionNode[] | undefined) ?? [];
  for (const definition of definitions) {
    try {
      await runGraphql(deleteDefinitionMutation, { id: definition.id });
    } catch (error) {
      console.warn(`Failed to delete temporary metafield definition ${definition.id}: ${String(error)}`);
    }
  }
  return definitions;
}

async function deleteCreatedStandardDefinition(): Promise<DefinitionNode | null> {
  if (baselineStandardDefinition) {
    return null;
  }
  const read = await runGraphql(readStandardDefinitionQuery);
  const definition = (read.data?.metafieldDefinition as DefinitionNode | null | undefined) ?? null;
  if (!definition) {
    return null;
  }
  try {
    await runGraphql(deleteDefinitionMutation, { id: definition.id });
  } catch (error) {
    console.warn(`Failed to delete created standard metafield definition ${definition.id}: ${String(error)}`);
  }
  return definition;
}

async function restoreBaselinePins(): Promise<void> {
  const ascending = [...baselinePinned].sort((left, right) => (left.pinnedPosition ?? 0) - (right.pinnedPosition ?? 0));
  for (const definition of ascending) {
    try {
      await runGraphql(pinByIdMutation, { definitionId: definition.id });
    } catch (error) {
      console.warn(`Failed to restore pinned metafield definition ${definition.id}: ${String(error)}`);
    }
  }
}

let primaryResponse: GraphqlResponse | null = null;
let readAfterResponse: GraphqlResponse | null = null;
let readAfterVariables: { definitionId: string } | null = null;
let deletedDefinitions: DefinitionNode[] = [];
let deletedStandardDefinition: DefinitionNode | null = null;

try {
  await mkdir(outputDir, { recursive: true });

  for (const definition of baselinePinned) {
    await runGraphql(unpinByIdMutation, { definitionId: definition.id });
  }

  primaryResponse = (await runGraphql(primaryDocument, variables)) as GraphqlResponse;
  readAfterVariables = { definitionId: readStandardInitialId(primaryResponse) };
  readAfterResponse = (await runGraphql(readAfterDocument, readAfterVariables)) as GraphqlResponse;
} finally {
  deletedDefinitions = await deleteNamespaceDefinitions();
  deletedStandardDefinition = await deleteCreatedStandardDefinition();
  await restoreBaselinePins();
}

await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      storeDomain,
      apiVersion,
      variables,
      baselinePinnedDefinitions: baselinePinned,
      baselineStandardDefinition,
      primary: {
        request: {
          documentPath: primaryDocumentPath,
          variables,
        },
        response: primaryResponse,
      },
      readAfter: {
        request: {
          documentPath: readAfterDocumentPath,
          variables: readAfterVariables,
        },
        response: readAfterResponse,
      },
      cleanup: {
        deletedDefinitions,
        deletedStandardDefinition,
        restoredPinnedDefinitions: baselinePinned,
      },
      upstreamCalls: [],
    },
    null,
    2,
  )}\n`,
  'utf8',
);

console.log(
  JSON.stringify(
    {
      ok: true,
      outputPath,
      namespace,
      baselinePinnedCount: baselinePinned.length,
      deletedDefinitionCount: deletedDefinitions.length,
      deletedStandardDefinitionId: deletedStandardDefinition?.id ?? null,
    },
    null,
    2,
  ),
);
