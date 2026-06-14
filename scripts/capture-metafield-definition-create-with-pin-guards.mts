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
const outputPath = path.join(outputDir, 'metafield-definition-create-with-pin-guards.json');
const primaryDocumentPath = 'config/parity-requests/metafields/metafield-definition-create-with-pin-guards.graphql';
const readAfterDocumentPath =
  'config/parity-requests/metafields/metafield-definition-create-with-pin-guards-read.graphql';

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
  query TemporaryNamespaceDefinitions($namespace: String!) {
    metafieldDefinitions(ownerType: PRODUCT, first: 100, namespace: $namespace) {
      nodes {
        id
        key
      }
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
  key?: string;
  namespace?: string;
  pinnedPosition?: number | null;
};

const namespace = `create_pin_guard_${Date.now().toString(36)}`;
const variables = {
  namespace,
  categoryId: 'gid://shopify/TaxonomyCategory/ap-2',
  standardTemplateId: 'gid://shopify/StandardMetafieldDefinitionTemplate/10004',
};

const primaryDocument = await readFile(primaryDocumentPath, 'utf8');
const readAfterDocument = await readFile(readAfterDocumentPath, 'utf8');
const baselinePinned =
  ((await runGraphql(readPinnedDefinitionsQuery)).data?.metafieldDefinitions?.nodes as DefinitionNode[] | undefined) ??
  [];

function createPinnedBatchDocument(start: number, end: number): string {
  const fields: string[] = [];
  for (let index = start; index <= end; index++) {
    const suffix = String(index).padStart(2, '0');
    const createdDefinitionSelection =
      index === 50
        ? `
        createdDefinition {
          id
          key
          pinnedPosition
        }`
        : '';
    fields.push(`
      create${suffix}: metafieldDefinitionCreate(
        definition: {
          ownerType: PRODUCT
          namespace: $namespace
          key: "pin_${suffix}"
          name: "Create pin guard ${suffix}"
          type: "single_line_text_field"
          pin: true
        }
      ) {
        ${createdDefinitionSelection}
        userErrors {
          field
          message
          code
        }
      }`);
  }
  return `#graphql
    mutation MetafieldDefinitionCreateWithPinGuardsBatch($namespace: String!) {
      ${fields.join('\n')}
    }
  `;
}

const createGuardFinalDocument = `#graphql
  mutation MetafieldDefinitionCreateWithPinGuardsFinal(
    $namespace: String!
    $categoryId: String!
    $standardTemplateId: ID!
  ) {
    overCapCreate: metafieldDefinitionCreate(
      definition: {
        ownerType: PRODUCT
        namespace: $namespace
        key: "over_cap"
        name: "Create over cap"
        type: "single_line_text_field"
        pin: true
      }
    ) {
      createdDefinition {
        id
        key
        pinnedPosition
      }
      userErrors {
        field
        message
        code
      }
    }
    constrainedCreate: metafieldDefinitionCreate(
      definition: {
        ownerType: PRODUCT
        namespace: $namespace
        key: "constrained"
        name: "Create constrained pin"
        type: "single_line_text_field"
        constraints: { key: "category", values: [$categoryId] }
        pin: true
      }
    ) {
      createdDefinition {
        id
        key
        pinnedPosition
        constraints {
          key
        }
      }
      userErrors {
        field
        message
        code
      }
    }
    standardConstrainedEnable: standardMetafieldDefinitionEnable(ownerType: PRODUCT, id: $standardTemplateId, pin: true) {
      createdDefinition {
        id
        namespace
        key
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

async function capturePrimaryResponse(): Promise<unknown> {
  void primaryDocument;
  const responses = [
    await runGraphql(createPinnedBatchDocument(1, 10), { namespace }),
    await runGraphql(createPinnedBatchDocument(11, 20), { namespace }),
    await runGraphql(createPinnedBatchDocument(21, 30), { namespace }),
    await runGraphql(createPinnedBatchDocument(31, 40), { namespace }),
    await runGraphql(createPinnedBatchDocument(41, 50), { namespace }),
    await runGraphql(createGuardFinalDocument, variables),
  ];
  return {
    data: Object.assign({}, ...responses.map((response) => response.data ?? {})),
  };
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

let primaryResponse: unknown = null;
let readAfterResponse: unknown = null;
let deletedDefinitions: DefinitionNode[] = [];

try {
  await mkdir(outputDir, { recursive: true });

  for (const definition of baselinePinned) {
    await runGraphql(unpinByIdMutation, { definitionId: definition.id });
  }

  primaryResponse = await capturePrimaryResponse();
  readAfterResponse = await runGraphql(readAfterDocument, variables);
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
      readAfter: {
        request: {
          documentPath: readAfterDocumentPath,
          variables,
        },
        response: readAfterResponse,
      },
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
