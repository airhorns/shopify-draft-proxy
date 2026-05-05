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
const outputPath = path.join(outputDir, 'metafield-definition-pin-limit-and-constraint-guard.json');
const primaryDocumentPath =
  'config/parity-requests/metafields/metafield-definition-pin-limit-and-constraint-guard.graphql';
const listingDocumentPath = 'config/parity-requests/metafields/metafield-definition-pin-limit-listing.graphql';
const unpinDocumentPath = 'config/parity-requests/metafields/metafield-definition-pin-limit-unpin.graphql';

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
        key
        namespace
        pinnedPosition
      }
    }
  }
`;

const readNamespaceDefinitionsQuery = `#graphql
  query HAR699NamespaceDefinitions($namespace: String!) {
    metafieldDefinitions(ownerType: PRODUCT, first: 50, namespace: $namespace) {
      nodes {
        id
        key
      }
    }
  }
`;

const pinByIdMutation = `#graphql
  mutation HAR699RestorePin($definitionId: ID!) {
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
  mutation HAR699TemporaryUnpin($definitionId: ID!) {
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
  mutation HAR699DeleteDefinition($id: ID!) {
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
  key?: string;
  namespace?: string;
  pinnedPosition?: number | null;
};

const namespace = `har699_pin_guard_${Date.now().toString(36)}`;
const variables = {
  namespace,
  categoryId: 'gid://shopify/TaxonomyCategory/ap-2',
};

const primaryDocument = await readFile(primaryDocumentPath, 'utf8');
const listingDocument = await readFile(listingDocumentPath, 'utf8');
const unpinDocument = await readFile(unpinDocumentPath, 'utf8');
const baselinePinned =
  ((await runGraphql(readPinnedDefinitionsQuery)).data?.metafieldDefinitions?.nodes as DefinitionNode[] | undefined) ??
  [];

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

let primaryResponse: {
  data?: {
    pin01?: {
      pinnedDefinition?: {
        id?: unknown;
      } | null;
    } | null;
  };
} | null = null;
let pinnedDefinitionsListing: unknown = null;
let unpinFirst: { variables: { definitionId: string }; response: unknown } | null = null;
let deletedDefinitions: DefinitionNode[] = [];

try {
  await mkdir(outputDir, { recursive: true });

  for (const definition of baselinePinned) {
    await runGraphql(unpinByIdMutation, { definitionId: definition.id });
  }

  primaryResponse = await runGraphql(primaryDocument, variables);
  pinnedDefinitionsListing = await runGraphql(listingDocument, { namespace });

  const firstPinnedId = primaryResponse.data?.pin01?.pinnedDefinition?.id;
  if (typeof firstPinnedId !== 'string') {
    throw new Error(`Primary capture did not return pin01.pinnedDefinition.id: ${JSON.stringify(primaryResponse)}`);
  }
  const unpinVariables = { definitionId: firstPinnedId };
  unpinFirst = {
    variables: unpinVariables,
    response: await runGraphql(unpinDocument, unpinVariables),
  };
} finally {
  deletedDefinitions = await deleteNamespaceDefinitions();
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
      primary: {
        request: {
          documentPath: primaryDocumentPath,
          variables,
        },
        response: primaryResponse,
      },
      pinnedDefinitionsListing,
      unpinFirst,
      cleanup: {
        deletedDefinitions,
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
    },
    null,
    2,
  ),
);
