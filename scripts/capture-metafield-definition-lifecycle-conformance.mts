/* oxlint-disable no-console -- CLI scripts intentionally write capture status output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'metafields');
const outputPath = path.join(outputDir, 'metafield-definition-lifecycle-mutations.json');
const { runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

type UpstreamCall = {
  method: 'POST';
  apiSurface: 'admin';
  path: string;
  operationName: string;
  variables: Record<string, unknown>;
  query: string;
  response: { status: number; body: unknown };
};

const suffix = Date.now().toString(36);
const namespace = `definition_lifecycle_${suffix}`;
const key = 'definition_lifecycle';
const looseNamespace = `definition_loose_${suffix}`;
const looseKey = 'loose_metafield';
const hydrateByIdentifierDocument = await readFile(
  'config/parity-requests/metafields/metafield-definition-hydrate-by-identifier.graphql',
  'utf8',
);
const hydrateResourceScopeDocument = await readFile(
  'config/parity-requests/metafields/metafield-definitions-hydrate-resource-scope.graphql',
  'utf8',
);
const hydrateWindowDocument = await readFile(
  'config/parity-requests/metafields/metafield-definitions-hydrate-window.graphql',
  'utf8',
);

const productCreateMutation = `#graphql
  mutation CreateProduct($product: ProductCreateInput!) {
    productCreate(product: $product) {
      product { id title }
      userErrors { field message }
    }
  }
`;

const productDeleteMutation = `#graphql
  mutation DeleteProduct($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors { field message }
    }
  }
`;

const createDefinitionMutation = `#graphql
  mutation CreateDefinition($definition: MetafieldDefinitionInput!) {
    metafieldDefinitionCreate(definition: $definition) {
      createdDefinition {
        id
        name
        namespace
        key
        ownerType
        type { name category }
        description
        validations { name value }
        pinnedPosition
        validationStatus
      }
      userErrors { field message code }
    }
  }
`;

const updateDefinitionMutation = `#graphql
  mutation UpdateDefinition($definition: MetafieldDefinitionUpdateInput!) {
    metafieldDefinitionUpdate(definition: $definition) {
      updatedDefinition {
        id
        name
        namespace
        key
        ownerType
        type { name category }
        description
        validations { name value }
        pinnedPosition
        validationStatus
      }
      userErrors { field message code }
      validationJob { id }
    }
  }
`;

const setMetafieldMutation = `#graphql
  mutation SetMetafield($metafields: [MetafieldsSetInput!]!) {
    metafieldsSet(metafields: $metafields) {
      metafields {
        id
        namespace
        key
        type
        value
        definition { id namespace key type { name } }
      }
      userErrors { field message code elementIndex }
    }
  }
`;

const deleteDefinitionMutation = `#graphql
  mutation DeleteDefinition($id: ID!, $deleteAllAssociatedMetafields: Boolean!) {
    metafieldDefinitionDelete(id: $id, deleteAllAssociatedMetafields: $deleteAllAssociatedMetafields) {
      deletedDefinitionId
      deletedDefinition { ownerType namespace key }
      userErrors { field message code }
    }
  }
`;

const downstreamReadQuery = `#graphql
  query DownstreamDefinitionRead($productId: ID!, $namespace: String!, $key: String!, $looseNamespace: String!, $looseKey: String!) {
    definition: metafieldDefinition(identifier: { ownerType: PRODUCT, namespace: $namespace, key: $key }) {
      id
      name
      namespace
      key
      ownerType
      validations { name value }
      metafieldsCount
    }
    definitions: metafieldDefinitions(ownerType: PRODUCT, namespace: $namespace, first: 5) {
      nodes { id namespace key name }
    }
    product(id: $productId) {
      id
      metafield(namespace: $namespace, key: $key) {
        id
        namespace
        key
        type
        value
        definition { id namespace key type { name } }
      }
      looseMetafield: metafield(namespace: $looseNamespace, key: $looseKey) {
        id
        namespace
        key
        type
        value
        definition { id }
      }
    }
  }
`;

const ownerMetafieldsHydrateQuery =
  'query OwnerMetafieldsHydrateNodes($ids: [ID!]!, $metafield0Namespace: String!, $metafield0Key: String!, $metafield1Namespace: String!, $metafield1Key: String!) { nodes(ids: $ids) { __typename id ... on Product { id title handle status totalInventory tracksInventory createdAt updatedAt metafield0: metafield(namespace: $metafield0Namespace, key: $metafield0Key) { id namespace key type value jsonValue compareDigest createdAt updatedAt ownerType } metafield1: metafield(namespace: $metafield1Namespace, key: $metafield1Key) { id namespace key type value jsonValue compareDigest createdAt updatedAt ownerType }  } ... on ProductVariant { id title sku barcode price compareAtPrice taxable inventoryPolicy inventoryQuantity selectedOptions { name value } inventoryItem { id tracked requiresShipping } product { id title handle status totalInventory tracksInventory createdAt updatedAt } metafield0: metafield(namespace: $metafield0Namespace, key: $metafield0Key) { id namespace key type value jsonValue compareDigest createdAt updatedAt ownerType } metafield1: metafield(namespace: $metafield1Namespace, key: $metafield1Key) { id namespace key type value jsonValue compareDigest createdAt updatedAt ownerType } } ... on Collection { id title handle metafield0: metafield(namespace: $metafield0Namespace, key: $metafield0Key) { id namespace key type value jsonValue compareDigest createdAt updatedAt ownerType } metafield1: metafield(namespace: $metafield1Namespace, key: $metafield1Key) { id namespace key type value jsonValue compareDigest createdAt updatedAt ownerType } } ... on Customer { id displayName email metafield0: metafield(namespace: $metafield0Namespace, key: $metafield0Key) { id namespace key type value jsonValue compareDigest createdAt updatedAt ownerType } metafield1: metafield(namespace: $metafield1Namespace, key: $metafield1Key) { id namespace key type value jsonValue compareDigest createdAt updatedAt ownerType } } ... on Order { id name metafield0: metafield(namespace: $metafield0Namespace, key: $metafield0Key) { id namespace key type value jsonValue compareDigest createdAt updatedAt ownerType } metafield1: metafield(namespace: $metafield1Namespace, key: $metafield1Key) { id namespace key type value jsonValue compareDigest createdAt updatedAt ownerType } } ... on Company { id name metafield0: metafield(namespace: $metafield0Namespace, key: $metafield0Key) { id namespace key type value jsonValue compareDigest createdAt updatedAt ownerType } metafield1: metafield(namespace: $metafield1Namespace, key: $metafield1Key) { id namespace key type value jsonValue compareDigest createdAt updatedAt ownerType } } ... on Shop { id metafield0: metafield(namespace: $metafield0Namespace, key: $metafield0Key) { id namespace key type value jsonValue compareDigest createdAt updatedAt ownerType } metafield1: metafield(namespace: $metafield1Namespace, key: $metafield1Key) { id namespace key type value jsonValue compareDigest createdAt updatedAt ownerType } } } }';

function assertHttpOk(result: ConformanceGraphqlResult, label: string): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${label} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function readObject(value: unknown): Record<string, unknown> | null {
  return value && typeof value === 'object' && !Array.isArray(value) ? (value as Record<string, unknown>) : null;
}

async function capture(label: string, query: string, variables: Record<string, unknown>) {
  const result = await runGraphqlRaw(query, variables);
  assertHttpOk(result, label);
  return {
    label,
    request: { query, variables },
    status: result.status,
    response: result.payload,
  };
}

async function recordUpstreamCall(
  operationName: string,
  query: string,
  variables: Record<string, unknown>,
): Promise<UpstreamCall> {
  const result = await runGraphqlRaw(query, variables);
  assertHttpOk(result, operationName);
  return {
    method: 'POST',
    apiSurface: 'admin',
    path: `/admin/api/${apiVersion}/graphql.json`,
    operationName,
    variables,
    query,
    response: { status: result.status, body: result.payload },
  };
}

async function recordResourceScope(): Promise<UpstreamCall[]> {
  const calls: UpstreamCall[] = [];
  let after: string | null = null;
  let observed = 0;
  for (let page = 0; page < 3; page += 1) {
    const variables = { ownerType: 'PRODUCT', query: '-namespace:app--*', first: 250, after };
    const call = await recordUpstreamCall(
      'MetafieldDefinitionsHydrateResourceScope',
      hydrateResourceScopeDocument,
      variables,
    );
    calls.push(call);
    const nodes = readObject(readObject(call.response.body)?.['data'])?.['metafieldDefinitions'];
    const nodeArray = readObject(nodes)?.['nodes'];
    if (!Array.isArray(nodeArray)) throw new Error(`resource-scope page ${page + 1} omitted nodes`);
    observed += nodeArray.filter((node) => readObject(node)?.['namespace'] !== 'shopify').length;
    const pageInfo = readObject(readObject(nodes)?.['pageInfo']);
    if (observed >= 256 || pageInfo?.['hasNextPage'] !== true) break;
    const endCursor = pageInfo?.['endCursor'];
    if (typeof endCursor !== 'string') throw new Error(`resource-scope page ${page + 1} omitted endCursor`);
    after = endCursor;
  }
  return calls;
}

function windowVariables(first: number) {
  return {
    ownerType: 'PRODUCT',
    key: null,
    namespace,
    pinnedStatus: 'ANY',
    constraintSubtype: null,
    constraintStatus: null,
    first,
    after: null,
    last: null,
    before: null,
    reverse: false,
    sortKey: 'ID',
    query: null,
  };
}

let productId: string | null = null;
let definitionId: string | null = null;
const captures = [];
const cleanup = [];
const upstreamCalls: UpstreamCall[] = [];

try {
  const productCreate = await capture('productCreate setup', productCreateMutation, {
    product: { title: `Metafield definition lifecycle ${suffix}` },
  });
  captures.push(productCreate);
  const createdProduct = readObject(readObject(readObject(productCreate.response)?.['data'])?.['productCreate'])?.[
    'product'
  ] as { id?: string } | null;
  productId = createdProduct?.id ?? null;
  if (!productId) {
    throw new Error('productCreate setup did not return a product id');
  }

  upstreamCalls.push(
    await recordUpstreamCall('MetafieldDefinitionHydrateByIdentifier', hydrateByIdentifierDocument, {
      identifier: { ownerType: 'PRODUCT', namespace, key },
    }),
  );
  upstreamCalls.push(...(await recordResourceScope()));

  const createDefinition = await capture('metafieldDefinitionCreate success', createDefinitionMutation, {
    definition: {
      name: 'Lifecycle definition',
      namespace,
      key,
      ownerType: 'PRODUCT',
      type: 'single_line_text_field',
      description: 'Temporary conformance definition for lifecycle capture',
      validations: [{ name: 'max', value: '8' }],
    },
  });
  captures.push(createDefinition);
  const createdDefinition = readObject(
    readObject(readObject(createDefinition.response)?.['data'])?.['metafieldDefinitionCreate'],
  )?.['createdDefinition'] as { id?: string } | null;
  definitionId = createdDefinition?.id ?? null;
  if (!definitionId) {
    throw new Error('metafieldDefinitionCreate did not return a definition id');
  }

  captures.push(
    await capture('metafieldsSet matching definition', setMetafieldMutation, {
      metafields: [
        {
          ownerId: productId,
          namespace,
          key,
          type: 'single_line_text_field',
          value: 'ABCDEFGH',
        },
        {
          ownerId: productId,
          namespace: looseNamespace,
          key: looseKey,
          type: 'single_line_text_field',
          value: 'loose',
        },
      ],
    }),
  );

  upstreamCalls.push(
    await recordUpstreamCall('OwnerMetafieldsHydrateNodes', ownerMetafieldsHydrateQuery, {
      ids: [productId],
      metafield0Namespace: namespace,
      metafield0Key: key,
      metafield1Namespace: looseNamespace,
      metafield1Key: looseKey,
    }),
    await recordUpstreamCall('MetafieldDefinitionsHydrateWindow', hydrateWindowDocument, windowVariables(6)),
  );

  captures.push(
    await capture('downstream read after create and metafieldsSet', downstreamReadQuery, {
      productId,
      namespace,
      key,
      looseNamespace,
      looseKey,
    }),
  );

  captures.push(
    await capture('metafieldDefinitionUpdate success', updateDefinitionMutation, {
      definition: {
        name: 'Lifecycle definition updated',
        namespace,
        key,
        ownerType: 'PRODUCT',
        description: 'Updated temporary conformance definition for lifecycle capture',
      },
    }),
  );

  captures.push(
    await capture('metafieldDefinitionDelete deleteAllAssociatedMetafields', deleteDefinitionMutation, {
      id: definitionId,
      deleteAllAssociatedMetafields: true,
    }),
  );
  definitionId = null;

  upstreamCalls.push(
    await recordUpstreamCall('MetafieldDefinitionsHydrateWindow', hydrateWindowDocument, windowVariables(56)),
  );

  captures.push(
    await capture('downstream read immediately after delete', downstreamReadQuery, {
      productId,
      namespace,
      key,
      looseNamespace,
      looseKey,
    }),
  );
} finally {
  if (definitionId) {
    cleanup.push(
      await capture('cleanup metafieldDefinitionDelete', deleteDefinitionMutation, {
        id: definitionId,
        deleteAllAssociatedMetafields: true,
      }).catch((error: unknown) => ({ label: 'cleanup metafieldDefinitionDelete', error: String(error) })),
    );
  }

  if (productId) {
    cleanup.push(
      await capture('cleanup productDelete', productDeleteMutation, {
        input: { id: productId },
      }).catch((error: unknown) => ({ label: 'cleanup productDelete', error: String(error) })),
    );
  }
}

const fixture = {
  capturedAt: new Date().toISOString(),
  storeDomain,
  apiVersion,
  namespace,
  key,
  looseNamespace,
  looseKey,
  captures,
  cleanup,
  upstreamCalls,
};

await mkdir(outputDir, { recursive: true });
await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');

console.log(`Wrote ${outputPath}`);
