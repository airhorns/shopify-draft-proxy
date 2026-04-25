/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion);
const { runGraphql } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const definitionSelection = `#graphql
  id
  name
  namespace
  key
  ownerType
  type {
    name
    category
  }
  description
  validations {
    name
    value
  }
  access {
    admin
    storefront
  }
  capabilities {
    adminFilterable {
      enabled
      eligible
      status
    }
    smartCollectionCondition {
      enabled
      eligible
    }
    uniqueValues {
      enabled
      eligible
    }
  }
  constraints {
    key
    values(first: 10) {
      nodes {
        value
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
        startCursor
        endCursor
      }
    }
  }
  pinnedPosition
  validationStatus
`;

const createDefinitionMutation = `#graphql
  mutation CreateDefinitionForPinning($definition: MetafieldDefinitionInput!) {
    metafieldDefinitionCreate(definition: $definition) {
      createdDefinition {
        ${definitionSelection}
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
  mutation DeleteDefinitionForPinning($id: ID!) {
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

const readDefinitionsQuery = `#graphql
  query MetafieldDefinitionPinningRead($namespace: String!) {
    byIdentifier: metafieldDefinition(identifier: { ownerType: PRODUCT, namespace: $namespace, key: "pin_a" }) {
      ${definitionSelection}
    }
    metafieldDefinitions(ownerType: PRODUCT, first: 10, namespace: $namespace, sortKey: PINNED_POSITION) {
      nodes {
        ${definitionSelection}
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
        startCursor
        endCursor
      }
    }
    pinned: metafieldDefinitions(
      ownerType: PRODUCT
      first: 10
      namespace: $namespace
      sortKey: PINNED_POSITION
      pinnedStatus: PINNED
    ) {
      nodes {
        id
        key
        pinnedPosition
      }
    }
    unpinned: metafieldDefinitions(
      ownerType: PRODUCT
      first: 10
      namespace: $namespace
      sortKey: PINNED_POSITION
      pinnedStatus: UNPINNED
    ) {
      nodes {
        id
        key
        pinnedPosition
      }
    }
  }
`;

const seedDefinitionsQuery = `#graphql
  query MetafieldDefinitionPinningSeedRead($namespace: String!) {
    byIdentifier: metafieldDefinition(identifier: { ownerType: PRODUCT, namespace: $namespace, key: "pin_a" }) {
      ${definitionSelection}
    }
    metafieldDefinitions(ownerType: PRODUCT, first: 10, namespace: $namespace, sortKey: PINNED_POSITION) {
      nodes {
        ${definitionSelection}
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
        startCursor
        endCursor
      }
    }
    seedCatalog: metafieldDefinitions(ownerType: PRODUCT, first: 50, sortKey: PINNED_POSITION) {
      nodes {
        ${definitionSelection}
      }
    }
  }
`;

const pinByIdentifierMutation = `#graphql
  mutation MetafieldDefinitionPinByIdentifier($identifier: MetafieldDefinitionIdentifierInput!) {
    metafieldDefinitionPin(identifier: $identifier) {
      pinnedDefinition {
        ${definitionSelection}
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const pinByIdMutation = `#graphql
  mutation MetafieldDefinitionPinById($definitionId: ID!) {
    metafieldDefinitionPin(definitionId: $definitionId) {
      pinnedDefinition {
        ${definitionSelection}
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const unpinByIdentifierMutation = `#graphql
  mutation MetafieldDefinitionUnpinByIdentifier($identifier: MetafieldDefinitionIdentifierInput!) {
    metafieldDefinitionUnpin(identifier: $identifier) {
      unpinnedDefinition {
        ${definitionSelection}
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
  mutation MetafieldDefinitionUnpinById($definitionId: ID!) {
    metafieldDefinitionUnpin(definitionId: $definitionId) {
      unpinnedDefinition {
        ${definitionSelection}
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const createdDefinitionIds: string[] = [];
const namespace = `har256_pin_${Date.now().toString(36)}`;
const downstreamReadVariables = { namespace };

async function createDefinition(key: string): Promise<string> {
  const response = await runGraphql(createDefinitionMutation, {
    definition: {
      ownerType: 'PRODUCT',
      namespace,
      key,
      name: `HAR 256 ${key}`,
      type: 'single_line_text_field',
      pin: false,
    },
  });
  const createdDefinition = response.data?.metafieldDefinitionCreate?.createdDefinition;
  const userErrors = response.data?.metafieldDefinitionCreate?.userErrors ?? [];
  if (typeof createdDefinition?.id !== 'string' || userErrors.length > 0) {
    throw new Error(`Failed to create ${key} definition: ${JSON.stringify(response)}`);
  }

  createdDefinitionIds.push(createdDefinition.id);
  return createdDefinition.id;
}

try {
  await mkdir(outputDir, { recursive: true });

  const definitionAId = await createDefinition('pin_a');
  const definitionBId = await createDefinition('pin_b');
  const baselineRead = await runGraphql(seedDefinitionsQuery, downstreamReadVariables);

  const pinByIdentifierVariables = {
    identifier: {
      ownerType: 'PRODUCT',
      namespace,
      key: 'pin_a',
    },
  };
  const pinByIdentifierResponse = await runGraphql(pinByIdentifierMutation, pinByIdentifierVariables);
  const afterPinByIdentifierRead = await runGraphql(readDefinitionsQuery, downstreamReadVariables);

  const pinByIdVariables = { definitionId: definitionBId };
  const pinByIdResponse = await runGraphql(pinByIdMutation, pinByIdVariables);
  const afterPinByIdRead = await runGraphql(readDefinitionsQuery, downstreamReadVariables);

  const unpinByIdentifierVariables = {
    identifier: {
      ownerType: 'PRODUCT',
      namespace,
      key: 'pin_a',
    },
  };
  const unpinByIdentifierResponse = await runGraphql(unpinByIdentifierMutation, unpinByIdentifierVariables);
  const afterUnpinByIdentifierRead = await runGraphql(readDefinitionsQuery, downstreamReadVariables);

  const unpinByIdVariables = { definitionId: definitionBId };
  const unpinByIdResponse = await runGraphql(unpinByIdMutation, unpinByIdVariables);
  const afterUnpinByIdRead = await runGraphql(readDefinitionsQuery, downstreamReadVariables);

  const captureFile = 'metafield-definition-pinning-parity.json';
  await writeFile(
    path.join(outputDir, captureFile),
    `${JSON.stringify(
      {
        response: baselineRead,
        downstreamReadVariables,
        createdDefinitionIds: {
          pinA: definitionAId,
          pinB: definitionBId,
        },
        pinByIdentifier: {
          variables: pinByIdentifierVariables,
          response: pinByIdentifierResponse,
        },
        afterPinByIdentifierRead,
        pinById: {
          variables: pinByIdVariables,
          response: pinByIdResponse,
        },
        afterPinByIdRead,
        unpinByIdentifier: {
          variables: unpinByIdentifierVariables,
          response: unpinByIdentifierResponse,
        },
        afterUnpinByIdentifierRead,
        unpinById: {
          variables: unpinByIdVariables,
          response: unpinByIdResponse,
        },
        afterUnpinByIdRead,
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
        outputDir,
        files: [captureFile],
        definitionIds: {
          pinA: definitionAId,
          pinB: definitionBId,
        },
      },
      null,
      2,
    ),
  );
} finally {
  for (const definitionId of createdDefinitionIds.reverse()) {
    try {
      await runGraphql(deleteDefinitionMutation, { id: definitionId });
    } catch (error) {
      console.warn(
        JSON.stringify(
          {
            ok: false,
            cleanup: 'metafieldDefinitionDelete',
            definitionId,
            error: error instanceof Error ? error.message : String(error),
          },
          null,
          2,
        ),
      );
    }
  }
}
