/* oxlint-disable no-console -- CLI capture script intentionally reports progress. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type GraphqlResult = Awaited<ReturnType<ReturnType<typeof createAdminGraphqlClient>['runGraphqlRequest']>>;

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'metafields');
const outputPath = path.join(outputDir, 'standard-metafield-definition-enable-material.json');
const requestPaths = {
  setup: 'config/parity-requests/metafields/standard-metafield-definition-enable-material.graphql',
  productSetup: 'config/parity-requests/metafields/standard-metafield-definition-linked-product-create.graphql',
  definitionRead: 'config/parity-requests/metafields/standard-metafield-definition-linked-read.graphql',
  valuesCreate: 'config/parity-requests/metafields/standard-metafield-definition-linked-values-create.graphql',
  attach: 'config/parity-requests/metafields/standard-metafield-definition-linked-attach.graphql',
  readback: 'config/parity-requests/metafields/standard-metafield-definition-linked-readback.graphql',
} as const;

const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const documents = Object.fromEntries(
  await Promise.all(Object.entries(requestPaths).map(async ([name, file]) => [name, await readFile(file, 'utf8')])),
) as Record<keyof typeof requestPaths, string>;

const taxonomyDocument = `#graphql
  query StandardLinkedMetafieldTaxonomyValues {
    materialCategories: taxonomy {
      categories(first: 10, search: "chair") {
        nodes {
          id
          name
          attributes(first: 50) {
            nodes {
              __typename
              ... on TaxonomyChoiceListAttribute {
                id
                name
                values(first: 5) { nodes { id name } }
              }
            }
          }
        }
      }
    }
    colorCategories: taxonomy {
      categories(first: 10, search: "shirt") {
        nodes {
          id
          name
          attributes(first: 50) {
            nodes {
              __typename
              ... on TaxonomyChoiceListAttribute {
                id
                name
                values(first: 5) { nodes { id name } }
              }
            }
          }
        }
      }
    }
  }
`;
const constraintDocument = `#graphql
  query StandardLinkedMetafieldConstraintIntersection {
    material: metafieldDefinition(
      identifier: { ownerType: PRODUCT, namespace: "shopify", key: "material" }
    ) {
      constraints { values(first: 250) { nodes { value } } }
    }
    colorPattern: metafieldDefinition(
      identifier: { ownerType: PRODUCT, namespace: "shopify", key: "color-pattern" }
    ) {
      constraints { values(first: 250) { nodes { value } } }
    }
  }
`;
const cleanupMetaobjectDocument = `#graphql
  mutation StandardLinkedMetafieldValueCleanup($id: ID!) {
    metaobjectDelete(id: $id) {
      deletedId
      userErrors { field message code elementKey elementIndex }
    }
  }
`;
const cleanupProductDocument = `#graphql
  mutation StandardLinkedMetafieldProductCleanup($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors { field message }
    }
  }
`;
const metaobjectDefinitionHydrateQuery =
  'query MetaobjectDefinitionHydrateByType($type: String!) { metaobjectDefinitionByType(type: $type) { id type name description displayNameKey access { admin storefront } capabilities { publishable { enabled } translatable { enabled } renderable { enabled } onlineStore { enabled } } fieldDefinitions { key name description required type { name category } capabilities { adminFilterable { enabled } } validations { name value } } hasThumbnailField metaobjectsCount standardTemplate { type name } createdAt updatedAt } }';
const taxonomyCategoryHydrateQuery =
  'query ProductTaxonomyCategoryHydrate($id: ID!) { node(id: $id) { __typename id ... on TaxonomyCategory { name fullName isLeaf level parentId } } }';

function capture(query: string, variables: Record<string, unknown>, result: GraphqlResult) {
  return {
    request: { query, variables },
    status: result.status,
    response: result.payload,
  };
}

function upstreamCall(operationName: string, query: string, variables: Record<string, unknown>, result: GraphqlResult) {
  return {
    operationName,
    variables,
    query,
    response: { status: result.status, body: result.payload },
  };
}

function at(value: unknown, ...keys: string[]): unknown {
  return keys.reduce<unknown>((current, key) => {
    if (Array.isArray(current)) return current[Number(key)];
    if (typeof current !== 'object' || current === null) return undefined;
    return (current as Record<string, unknown>)[key];
  }, value);
}

function stringAt(value: unknown, ...keys: string[]): string {
  const selected = at(value, ...keys);
  if (typeof selected !== 'string' || selected.length === 0) {
    throw new Error(`Expected string at ${keys.join('.')}`);
  }
  return selected;
}

function assertIdAt(value: unknown, expected: string, label: string, ...keys: string[]): void {
  const actual = stringAt(value, ...keys);
  if (actual !== expected) {
    throw new Error(`${label} returned ${actual}; expected ${expected}`);
  }
}

function assertSuccessful(result: GraphqlResult, label: string, roots: string[]): void {
  if (result.status < 200 || result.status >= 300 || at(result.payload, 'errors') !== undefined) {
    throw new Error(`${label} failed: ${JSON.stringify({ status: result.status, payload: result.payload }, null, 2)}`);
  }
  for (const root of roots) {
    const userErrors = at(result.payload, 'data', root, 'userErrors');
    if (Array.isArray(userErrors) && userErrors.length > 0) {
      throw new Error(`${label}.${root} returned userErrors: ${JSON.stringify(userErrors, null, 2)}`);
    }
  }
}

function taxonomyChoice(
  payload: unknown,
  taxonomyAlias: 'materialCategories' | 'colorCategories',
  attributeName: string,
): { categoryId: string; valueId: string } {
  const categories = at(payload, 'data', taxonomyAlias, 'categories', 'nodes');
  if (!Array.isArray(categories)) throw new Error(`No taxonomy categories returned for ${taxonomyAlias}`);
  for (const category of categories) {
    const attributes = at(category, 'attributes', 'nodes');
    if (!Array.isArray(attributes)) continue;
    for (const attribute of attributes) {
      if (at(attribute, 'name') !== attributeName) continue;
      const values = at(attribute, 'values', 'nodes');
      if (!Array.isArray(values) || values.length === 0) continue;
      return {
        categoryId: stringAt(category, 'id'),
        valueId: stringAt(values[0], 'id'),
      };
    }
  }
  throw new Error(`No ${attributeName} taxonomy value found for ${taxonomyAlias}`);
}

const capturedAt = new Date().toISOString();
const suffix = Date.now().toString(36);
const cleanup: unknown[] = [];
let productId: string | undefined;
const metaobjectIds: string[] = [];

try {
  const taxonomy = await runGraphqlRequest(taxonomyDocument);
  assertSuccessful(taxonomy, 'taxonomy', []);
  const materialTaxonomy = taxonomyChoice(taxonomy.payload, 'materialCategories', 'Material');
  const colorTaxonomy = taxonomyChoice(taxonomy.payload, 'colorCategories', 'Color');
  const patternTaxonomy = taxonomyChoice(taxonomy.payload, 'colorCategories', 'Pattern');

  const setupVariables = {};
  const setup = await runGraphqlRequest(documents.setup, setupVariables);
  assertSuccessful(setup, 'setup', ['material', 'colorPattern']);
  const materialDefinitionId = stringAt(
    setup.payload,
    'data',
    'material',
    'createdDefinition',
    'validations',
    '0',
    'value',
  );
  const colorPatternDefinitionId = stringAt(
    setup.payload,
    'data',
    'colorPattern',
    'createdDefinition',
    'validations',
    '0',
    'value',
  );

  const constraints = await runGraphqlRequest(constraintDocument);
  assertSuccessful(constraints, 'constraints', []);
  const materialConstraints = at(constraints.payload, 'data', 'material', 'constraints', 'values', 'nodes');
  const colorConstraints = at(constraints.payload, 'data', 'colorPattern', 'constraints', 'values', 'nodes');
  if (!Array.isArray(materialConstraints) || !Array.isArray(colorConstraints)) {
    throw new Error('Standard linked metafield constraint catalogs were unavailable.');
  }
  const colorConstraintValues = new Set(colorConstraints.map((node) => stringAt(node, 'value')));
  const sharedCategory = materialConstraints
    .map((node) => stringAt(node, 'value'))
    .find((value) => colorConstraintValues.has(value));
  if (sharedCategory === undefined) {
    throw new Error('Material and color-pattern definitions returned no shared product category constraint.');
  }

  const materialDefinitionHydrateVariables = { type: 'shopify--material' };
  const materialDefinitionHydrate = await runGraphqlRequest(
    metaobjectDefinitionHydrateQuery,
    materialDefinitionHydrateVariables,
  );
  assertSuccessful(materialDefinitionHydrate, 'materialDefinitionHydrate', []);
  assertIdAt(
    materialDefinitionHydrate.payload,
    materialDefinitionId,
    'materialDefinitionHydrate',
    'data',
    'metaobjectDefinitionByType',
    'id',
  );
  const colorPatternDefinitionHydrateVariables = { type: 'shopify--color-pattern' };
  const colorPatternDefinitionHydrate = await runGraphqlRequest(
    metaobjectDefinitionHydrateQuery,
    colorPatternDefinitionHydrateVariables,
  );
  assertSuccessful(colorPatternDefinitionHydrate, 'colorPatternDefinitionHydrate', []);
  assertIdAt(
    colorPatternDefinitionHydrate.payload,
    colorPatternDefinitionId,
    'colorPatternDefinitionHydrate',
    'data',
    'metaobjectDefinitionByType',
    'id',
  );
  const categoryHydrateVariables = {
    id: `gid://shopify/TaxonomyCategory/${sharedCategory}`,
  };
  const categoryHydrate = await runGraphqlRequest(taxonomyCategoryHydrateQuery, categoryHydrateVariables);
  assertSuccessful(categoryHydrate, 'categoryHydrate', []);

  const productSetupVariables = {
    product: {
      title: `Standard linked metafield lifecycle ${suffix}`,
      status: 'DRAFT',
      category: `gid://shopify/TaxonomyCategory/${sharedCategory}`,
    },
  };
  const productSetup = await runGraphqlRequest(documents.productSetup, productSetupVariables);
  assertSuccessful(productSetup, 'productSetup', ['productCreate']);
  productId = stringAt(productSetup.payload, 'data', 'productCreate', 'product', 'id');

  const definitionReadVariables = {
    materialId: materialDefinitionId,
    colorPatternId: colorPatternDefinitionId,
  };
  const definitionRead = await runGraphqlRequest(documents.definitionRead, definitionReadVariables);
  assertSuccessful(definitionRead, 'definitionRead', []);
  for (const alias of ['materialById', 'materialByType', 'materialNode']) {
    assertIdAt(definitionRead.payload, materialDefinitionId, `definitionRead.${alias}`, 'data', alias, 'id');
  }
  for (const alias of ['colorPatternById', 'colorPatternByType', 'colorPatternNode']) {
    assertIdAt(definitionRead.payload, colorPatternDefinitionId, `definitionRead.${alias}`, 'data', alias, 'id');
  }

  const valuesCreateVariables = {
    material: {
      type: 'shopify--material',
      handle: `standard-linked-material-${suffix}`,
      fields: [
        { key: 'label', value: 'Acrylic' },
        { key: 'taxonomy_reference', value: materialTaxonomy.valueId },
      ],
    },
    colorPattern: {
      type: 'shopify--color-pattern',
      handle: `standard-linked-color-${suffix}`,
      fields: [
        { key: 'label', value: 'Blue abstract' },
        { key: 'color', value: '#0000FF' },
        { key: 'color_taxonomy_reference', value: JSON.stringify([colorTaxonomy.valueId]) },
        { key: 'pattern_taxonomy_reference', value: patternTaxonomy.valueId },
      ],
    },
  };
  const valuesCreate = await runGraphqlRequest(documents.valuesCreate, valuesCreateVariables);
  assertSuccessful(valuesCreate, 'valuesCreate', ['material', 'colorPattern']);
  const materialId = stringAt(valuesCreate.payload, 'data', 'material', 'metaobject', 'id');
  const colorPatternId = stringAt(valuesCreate.payload, 'data', 'colorPattern', 'metaobject', 'id');
  assertIdAt(
    valuesCreate.payload,
    materialDefinitionId,
    'valuesCreate.material.definition',
    'data',
    'material',
    'metaobject',
    'definition',
    'id',
  );
  assertIdAt(
    valuesCreate.payload,
    colorPatternDefinitionId,
    'valuesCreate.colorPattern.definition',
    'data',
    'colorPattern',
    'metaobject',
    'definition',
    'id',
  );
  metaobjectIds.push(materialId, colorPatternId);

  const attachVariables = {
    productId,
    metafields: [
      {
        ownerId: productId,
        namespace: 'shopify',
        key: 'material',
        type: 'list.metaobject_reference',
        value: JSON.stringify([materialId]),
      },
      {
        ownerId: productId,
        namespace: 'shopify',
        key: 'color-pattern',
        type: 'list.metaobject_reference',
        value: JSON.stringify([colorPatternId]),
      },
    ],
    options: [
      { name: 'Material', linkedMetafield: { namespace: 'shopify', key: 'material', values: [materialId] } },
      {
        name: 'Color',
        linkedMetafield: { namespace: 'shopify', key: 'color-pattern', values: [colorPatternId] },
      },
    ],
  };
  const attach = await runGraphqlRequest(documents.attach, attachVariables);
  assertSuccessful(attach, 'attach', ['metafieldsSet', 'productOptionsCreate']);

  const readbackVariables = { productId, materialId, colorPatternId };
  const readback = await runGraphqlRequest(documents.readback, readbackVariables);
  assertSuccessful(readback, 'readback', []);
  assertIdAt(
    readback.payload,
    materialDefinitionId,
    'readback.material.definition',
    'data',
    'material',
    'definition',
    'id',
  );
  assertIdAt(
    readback.payload,
    colorPatternDefinitionId,
    'readback.colorPattern.definition',
    'data',
    'colorPattern',
    'definition',
    'id',
  );
  assertIdAt(
    readback.payload,
    materialDefinitionId,
    'readback.product.material.reference.definition',
    'data',
    'product',
    'material',
    'references',
    'nodes',
    '0',
    'definition',
    'id',
  );
  assertIdAt(
    readback.payload,
    colorPatternDefinitionId,
    'readback.product.colorPattern.reference.definition',
    'data',
    'product',
    'colorPattern',
    'references',
    'nodes',
    '0',
    'definition',
    'id',
  );

  const payload = {
    storeDomain,
    apiVersion,
    capturedAt,
    taxonomy: capture(taxonomyDocument, {}, taxonomy),
    constraints: capture(constraintDocument, {}, constraints),
    setup: capture(documents.setup, setupVariables, setup),
    productSetup: capture(documents.productSetup, productSetupVariables, productSetup),
    definitionRead: capture(documents.definitionRead, definitionReadVariables, definitionRead),
    valuesCreate: capture(documents.valuesCreate, valuesCreateVariables, valuesCreate),
    attach: capture(documents.attach, attachVariables, attach),
    readback: capture(documents.readback, readbackVariables, readback),
    cleanup,
    upstreamCalls: [
      upstreamCall(
        'MetaobjectDefinitionHydrateByType',
        metaobjectDefinitionHydrateQuery,
        materialDefinitionHydrateVariables,
        materialDefinitionHydrate,
      ),
      upstreamCall(
        'MetaobjectDefinitionHydrateByType',
        metaobjectDefinitionHydrateQuery,
        colorPatternDefinitionHydrateVariables,
        colorPatternDefinitionHydrate,
      ),
      upstreamCall(
        'ProductTaxonomyCategoryHydrate',
        taxonomyCategoryHydrateQuery,
        categoryHydrateVariables,
        categoryHydrate,
      ),
    ],
  };

  await mkdir(outputDir, { recursive: true });
  await writeFile(outputPath, `${JSON.stringify(payload, null, 2)}\n`, 'utf8');
  console.log(`Wrote ${outputPath}`);
} finally {
  if (productId !== undefined) {
    const variables = { input: { id: productId } };
    const result = await runGraphqlRequest(cleanupProductDocument, variables);
    cleanup.push(capture(cleanupProductDocument, variables, result));
  }
  for (const id of metaobjectIds.reverse()) {
    const variables = { id };
    const result = await runGraphqlRequest(cleanupMetaobjectDocument, variables);
    cleanup.push(capture(cleanupMetaobjectDocument, variables, result));
  }
  if (cleanup.length > 0) {
    try {
      const existing = JSON.parse(await readFile(outputPath, 'utf8')) as Record<string, unknown>;
      existing['cleanup'] = cleanup;
      await writeFile(outputPath, `${JSON.stringify(existing, null, 2)}\n`, 'utf8');
    } catch {
      console.warn('Capture failed before the fixture was written; cleanup requests still ran.');
    }
  }
}
