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

type TypeCase = {
  key: string;
  type: string;
  value: string;
  definitionValidations?: Array<{ name: string; value: string | null }>;
};

type SeedState = {
  namespace: string;
  productId?: string;
  variantId?: string;
  collectionId?: string;
  targetDefinitionId?: string;
  targetMetaobjectId?: string;
  matrixDefinitions: Array<{ id: string; type: string; handle: string; metaobjectId?: string }>;
  matrixTypePrefix: string;
  matrixHandlePrefix: string;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'metafields');
const outputPath = path.join(outputDir, 'custom-data-field-type-matrix.json');
const runId = Date.now().toString();
const seed: SeedState = {
  namespace: `har294_${runId}`,
  matrixDefinitions: [],
  matrixTypePrefix: `codex_har294_type_matrix_${runId}`,
  matrixHandlePrefix: `codex-har294-type-matrix-${runId}`,
};

const metafieldSetDocument = await readFile(
  'config/parity-requests/metafields/custom-data-metafield-type-matrix-set.graphql',
  'utf8',
);
const metafieldReadDocument = await readFile(
  'config/parity-requests/metafields/custom-data-metafield-type-matrix-read.graphql',
  'utf8',
);
const metaobjectCreateDocument = await readFile(
  'config/parity-requests/metaobjects/custom-data-metaobject-field-type-matrix-create.graphql',
  'utf8',
);
const metaobjectReadDocument = await readFile(
  'config/parity-requests/metaobjects/custom-data-metaobject-field-type-matrix-read.graphql',
  'utf8',
);

const productCreateMutation = `#graphql
  mutation CustomDataTypeMatrixProductCreate($product: ProductCreateInput!) {
    productCreate(product: $product) {
      product {
        id
        title
        variants(first: 1) {
          nodes {
            id
            title
          }
        }
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const productDeleteMutation = `#graphql
  mutation CustomDataTypeMatrixProductDelete($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors {
        field
        message
      }
    }
  }
`;

const collectionCreateMutation = `#graphql
  mutation CustomDataTypeMatrixCollectionCreate($input: CollectionInput!) {
    collectionCreate(input: $input) {
      collection {
        id
        title
        handle
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const collectionDeleteMutation = `#graphql
  mutation CustomDataTypeMatrixCollectionDelete($input: CollectionDeleteInput!) {
    collectionDelete(input: $input) {
      deletedCollectionId
      userErrors {
        field
        message
      }
    }
  }
`;

const definitionFields = `#graphql
  fragment CustomDataTypeMatrixDefinitionFields on MetaobjectDefinition {
    id
    type
    name
    description
    displayNameKey
    fieldDefinitions {
      key
      name
      required
      type {
        name
        category
      }
      validations {
        name
        value
      }
    }
    metaobjectsCount
  }
`;

const entryFields = `#graphql
  fragment CustomDataTypeMatrixEntryFields on Metaobject {
    id
    handle
    type
    displayName
    updatedAt
    fields {
      key
      type
      value
      jsonValue
      definition {
        key
        name
        required
        type {
          name
          category
        }
      }
    }
  }
`;

const metaobjectDefinitionCreateMutation = `#graphql
  ${definitionFields}
  mutation CustomDataTypeMatrixDefinitionCreate($definition: MetaobjectDefinitionCreateInput!) {
    metaobjectDefinitionCreate(definition: $definition) {
      metaobjectDefinition {
        ...CustomDataTypeMatrixDefinitionFields
      }
      userErrors {
        field
        message
        code
        elementKey
        elementIndex
      }
    }
  }
`;

const metaobjectDeleteMutation = `#graphql
  mutation CustomDataTypeMatrixMetaobjectDelete($id: ID!) {
    metaobjectDelete(id: $id) {
      deletedId
      userErrors {
        field
        message
        code
        elementKey
        elementIndex
      }
    }
  }
`;

const metaobjectDefinitionDeleteMutation = `#graphql
  mutation CustomDataTypeMatrixDefinitionDelete($id: ID!) {
    metaobjectDefinitionDelete(id: $id) {
      deletedId
      userErrors {
        field
        message
        code
        elementKey
        elementIndex
      }
    }
  }
`;

const metaobjectSeedReadQuery = `#graphql
  ${definitionFields}
  ${entryFields}
  query CustomDataTypeMatrixMetaobjectSeedRead($targetDefinitionId: ID!, $targetMetaobjectId: ID!, $matrixDefinitionId: ID!) {
    targetDefinition: metaobjectDefinition(id: $targetDefinitionId) {
      ...CustomDataTypeMatrixDefinitionFields
    }
    targetMetaobject: metaobject(id: $targetMetaobjectId) {
      ...CustomDataTypeMatrixEntryFields
    }
    matrixDefinition: metaobjectDefinition(id: $matrixDefinitionId) {
      ...CustomDataTypeMatrixDefinitionFields
    }
  }
`;

const shopCurrencyQuery = `#graphql
  query CustomDataTypeMatrixShopCurrency {
    shop {
      currencyCode
    }
  }
`;

function readObject(value: unknown): Record<string, unknown> | null {
  return value && typeof value === 'object' && !Array.isArray(value) ? (value as Record<string, unknown>) : null;
}

function readPath(value: unknown, pathParts: string[]): unknown {
  let current = value;
  for (const part of pathParts) {
    if (Array.isArray(current)) {
      const index = Number.parseInt(part, 10);
      current = Number.isInteger(index) ? current[index] : undefined;
      continue;
    }
    const object = readObject(current);
    if (!object) {
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
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${label} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function assertNoUserErrors(payload: unknown, pathParts: string[], label: string): void {
  const userErrors = readUserErrors(payload, pathParts);
  if (userErrors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(userErrors, null, 2)}`);
  }
}

function extractId(payload: unknown, pathParts: string[], label: string): string {
  const id = readPath(payload, pathParts);
  if (typeof id !== 'string' || id.length === 0) {
    throw new Error(`${label} did not return an id: ${JSON.stringify(payload, null, 2)}`);
  }
  return id;
}

function captureFromResult(
  name: string,
  query: string,
  variables: Record<string, unknown>,
  result: ConformanceGraphqlResult,
): Capture {
  return {
    name,
    request: { query, variables },
    status: result.status,
    response: result.payload,
  };
}

function jsonString(value: unknown): string {
  return JSON.stringify(value);
}

function listValue(value: string): string {
  try {
    return jsonString([JSON.parse(value) as unknown]);
  } catch {
    return jsonString([value]);
  }
}

function keyForType(prefix: string, type: string): string {
  return `${prefix}_${type.replace(/[^a-z0-9]+/gu, '_')}`.replace(/_+$/u, '').slice(0, 64);
}

function scalarTypeCases(currencyCode: string): TypeCase[] {
  return [
    { key: 'antenna_gain', type: 'antenna_gain', value: jsonString({ value: 5, unit: 'decibels_isotropic' }) },
    { key: 'area', type: 'area', value: jsonString({ value: 100, unit: 'square_meters' }) },
    {
      key: 'battery_charge_capacity',
      type: 'battery_charge_capacity',
      value: jsonString({ value: 3000, unit: 'milliamp_hours' }),
    },
    {
      key: 'battery_energy_capacity',
      type: 'battery_energy_capacity',
      value: jsonString({ value: 50, unit: 'watt_hours' }),
    },
    { key: 'boolean', type: 'boolean', value: 'true' },
    { key: 'capacitance', type: 'capacitance', value: jsonString({ value: 100, unit: 'microfarads' }) },
    { key: 'color', type: 'color', value: '#fff123' },
    { key: 'concentration', type: 'concentration', value: jsonString({ value: 5, unit: 'milligrams_per_milliliter' }) },
    {
      key: 'data_storage_capacity',
      type: 'data_storage_capacity',
      value: jsonString({ value: 256, unit: 'gigabytes' }),
    },
    {
      key: 'data_transfer_rate',
      type: 'data_transfer_rate',
      value: jsonString({ value: 100, unit: 'megabits_per_second' }),
    },
    { key: 'date', type: 'date', value: '2022-02-02' },
    { key: 'date_time', type: 'date_time', value: '2024-01-01T12:30:00' },
    { key: 'dimension', type: 'dimension', value: jsonString({ value: 25, unit: 'centimeters' }) },
    { key: 'display_density', type: 'display_density', value: jsonString({ value: 326, unit: 'pixels_per_inch' }) },
    { key: 'distance', type: 'distance', value: jsonString({ value: 42, unit: 'kilometers' }) },
    { key: 'duration', type: 'duration', value: jsonString({ value: 30, unit: 'seconds' }) },
    { key: 'electric_current', type: 'electric_current', value: jsonString({ value: 2.5, unit: 'amperes' }) },
    { key: 'electrical_resistance', type: 'electrical_resistance', value: jsonString({ value: 100, unit: 'ohms' }) },
    { key: 'energy', type: 'energy', value: jsonString({ value: 250, unit: 'kilocalories' }) },
    { key: 'frequency', type: 'frequency', value: jsonString({ value: 2.4, unit: 'gigahertz' }) },
    { key: 'illuminance', type: 'illuminance', value: jsonString({ value: 500, unit: 'lux' }) },
    { key: 'inductance', type: 'inductance', value: jsonString({ value: 10, unit: 'millihenries' }) },
    { key: 'json', type: 'json', value: jsonString({ ingredient: 'flour', amount: 0.3 }) },
    { key: 'language', type: 'language', value: 'en' },
    { key: 'link', type: 'link', value: jsonString({ text: 'Learn more', url: 'https://shopify.com' }) },
    { key: 'luminous_flux', type: 'luminous_flux', value: jsonString({ value: 800, unit: 'lumens' }) },
    { key: 'mass_flow_rate', type: 'mass_flow_rate', value: jsonString({ value: 5, unit: 'kilograms_per_hour' }) },
    { key: 'money', type: 'money', value: jsonString({ amount: '5.99', currency_code: currencyCode }) },
    { key: 'multi_line_text_field', type: 'multi_line_text_field', value: 'Ingredients\nFlour\nWater' },
    { key: 'number_decimal', type: 'number_decimal', value: '10.4' },
    { key: 'number_integer', type: 'number_integer', value: '10' },
    { key: 'power', type: 'power', value: jsonString({ value: 100, unit: 'watts' }) },
    { key: 'pressure', type: 'pressure', value: jsonString({ value: 14.7, unit: 'pounds_per_square_inch' }) },
    {
      key: 'rating',
      type: 'rating',
      value: jsonString({ value: '3.5', scale_min: '1.0', scale_max: '5.0' }),
      definitionValidations: [
        { name: 'scale_min', value: '1.0' },
        { name: 'scale_max', value: '5.0' },
      ],
    },
    { key: 'resolution', type: 'resolution', value: jsonString({ value: 12, unit: 'megapixels' }) },
    {
      key: 'rich_text_field',
      type: 'rich_text_field',
      value: jsonString({
        type: 'root',
        children: [{ type: 'paragraph', children: [{ type: 'text', value: 'Bold text.', bold: true }] }],
      }),
    },
    {
      key: 'rotational_speed',
      type: 'rotational_speed',
      value: jsonString({ value: 3000, unit: 'revolutions_per_minute' }),
    },
    { key: 'single_line_text_field', type: 'single_line_text_field', value: 'VIP shipping method' },
    { key: 'sound_level', type: 'sound_level', value: jsonString({ value: 85, unit: 'decibels' }) },
    { key: 'speed', type: 'speed', value: jsonString({ value: 60, unit: 'kilometers_per_hour' }) },
    { key: 'temperature', type: 'temperature', value: jsonString({ value: 22.5, unit: 'celsius' }) },
    {
      key: 'thermal_power',
      type: 'thermal_power',
      value: jsonString({ value: 12000, unit: 'british_thermal_units_per_hour' }),
    },
    { key: 'url', type: 'url', value: 'https://www.shopify.com' },
    { key: 'voltage', type: 'voltage', value: jsonString({ value: 120, unit: 'volts' }) },
    { key: 'volume', type: 'volume', value: jsonString({ value: 20, unit: 'milliliters' }) },
    {
      key: 'volumetric_flow_rate',
      type: 'volumetric_flow_rate',
      value: jsonString({ value: 5, unit: 'liters_per_minute' }),
    },
    { key: 'weight', type: 'weight', value: jsonString({ value: 2.5, unit: 'kilograms' }) },
  ];
}

function listTypeCases(baseCases: TypeCase[]): TypeCase[] {
  const listableTypes = new Set([
    'antenna_gain',
    'area',
    'battery_charge_capacity',
    'battery_energy_capacity',
    'boolean',
    'capacitance',
    'color',
    'concentration',
    'data_storage_capacity',
    'data_transfer_rate',
    'date',
    'date_time',
    'dimension',
    'display_density',
    'distance',
    'duration',
    'electric_current',
    'electrical_resistance',
    'energy',
    'frequency',
    'illuminance',
    'inductance',
    'link',
    'luminous_flux',
    'mass_flow_rate',
    'multi_line_text_field',
    'number_decimal',
    'number_integer',
    'power',
    'pressure',
    'rating',
    'resolution',
    'rotational_speed',
    'single_line_text_field',
    'sound_level',
    'speed',
    'temperature',
    'thermal_power',
    'url',
    'voltage',
    'volume',
    'volumetric_flow_rate',
    'weight',
  ]);

  return baseCases
    .filter((candidate) => listableTypes.has(candidate.type))
    .map((candidate) => ({
      key: keyForType('list', candidate.type),
      type: `list.${candidate.type}`,
      value: listValue(candidate.value),
      ...(candidate.definitionValidations ? { definitionValidations: candidate.definitionValidations } : {}),
    }));
}

function metaobjectListTypeCases(baseCases: TypeCase[]): TypeCase[] {
  const invalidMetaobjectListTypes = new Set(['list.boolean', 'list.multi_line_text_field']);
  return listTypeCases(baseCases).filter((typeCase) => !invalidMetaobjectListTypes.has(typeCase.type));
}

function referenceTypeCases(
  seedState: Required<Pick<SeedState, 'productId' | 'variantId' | 'collectionId' | 'targetMetaobjectId'>>,
): TypeCase[] {
  const references = [
    { type: 'product_reference', value: seedState.productId },
    { type: 'variant_reference', value: seedState.variantId },
    { type: 'collection_reference', value: seedState.collectionId },
    {
      type: 'metaobject_reference',
      value: seedState.targetMetaobjectId,
      definitionValidations: [{ name: 'metaobject_definition_id', value: seed.targetDefinitionId ?? null }],
    },
    {
      type: 'mixed_reference',
      value: seedState.targetMetaobjectId,
      definitionValidations: [{ name: 'metaobject_definition_ids', value: jsonString([seed.targetDefinitionId]) }],
    },
  ];

  return references.flatMap((reference) => [
    { key: reference.type, ...reference },
    {
      key: keyForType('list', reference.type),
      type: `list.${reference.type}`,
      value: listValue(reference.value),
      ...(reference.definitionValidations ? { definitionValidations: reference.definitionValidations } : {}),
    },
  ]);
}

function toMetafieldsSetInput(ownerId: string, namespace: string, cases: TypeCase[]): Record<string, unknown>[] {
  return cases.map((typeCase) => ({
    ownerId,
    namespace,
    key: typeCase.key,
    type: typeCase.type,
    value: typeCase.value,
  }));
}

function toMetaobjectFieldDefinitions(cases: TypeCase[]): Record<string, unknown>[] {
  return cases.map((typeCase) => ({
    key: typeCase.key,
    name: typeCase.type,
    type: typeCase.type,
    required: false,
    ...(typeCase.definitionValidations ? { validations: typeCase.definitionValidations } : {}),
  }));
}

function toMetaobjectFields(cases: TypeCase[]): Record<string, unknown>[] {
  return cases.map((typeCase) => ({ key: typeCase.key, value: typeCase.value }));
}

function chunk<T>(items: T[], size: number): T[][] {
  const chunks: T[][] = [];
  for (let index = 0; index < items.length; index += size) {
    chunks.push(items.slice(index, index + size));
  }
  return chunks;
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

async function runSuccessMutation(
  name: string,
  query: string,
  variables: Record<string, unknown>,
  userErrorPath: string[],
): Promise<Capture> {
  const capture = await captureGraphql(name, query, variables);
  assertNoUserErrors(capture.response, userErrorPath, name);
  return capture;
}

async function cleanup(captures: Capture[]): Promise<void> {
  for (const id of [...seed.matrixDefinitions.map((definition) => definition.metaobjectId), seed.targetMetaobjectId]) {
    if (id) {
      captures.push(await captureGraphql('cleanup-metaobject-delete', metaobjectDeleteMutation, { id }));
    }
  }
  for (const id of [...seed.matrixDefinitions.map((definition) => definition.id), seed.targetDefinitionId]) {
    if (id) {
      captures.push(
        await captureGraphql('cleanup-metaobject-definition-delete', metaobjectDefinitionDeleteMutation, { id }),
      );
    }
  }
  if (seed.collectionId) {
    captures.push(
      await captureGraphql('cleanup-collection-delete', collectionDeleteMutation, { input: { id: seed.collectionId } }),
    );
  }
  if (seed.productId) {
    captures.push(
      await captureGraphql('cleanup-product-delete', productDeleteMutation, { input: { id: seed.productId } }),
    );
  }
}

const setupCaptures: Capture[] = [];
const metafieldBatches: Array<{
  name: string;
  mutation: Capture;
  downstreamRead: Capture;
}> = [];
const seededReads: Capture[] = [];
const metaobjectMatrices: Array<{
  name: string;
  coveredTypes: string[];
  create: Capture;
  downstreamRead: Capture;
}> = [];
const cleanupCaptures: Capture[] = [];

try {
  const currencyCapture = await captureGraphql('shop-currency-read', shopCurrencyQuery, {});
  setupCaptures.push(currencyCapture);
  const currencyCode = String(readPath(currencyCapture.response, ['data', 'shop', 'currencyCode']) ?? 'CAD');

  const productCreate = await runSuccessMutation(
    'setup-product-create',
    productCreateMutation,
    { product: { title: `HAR-294 Custom Data Type Matrix ${runId}`, status: 'DRAFT' } },
    ['data', 'productCreate', 'userErrors'],
  );
  seed.productId = extractId(productCreate.response, ['data', 'productCreate', 'product', 'id'], 'productCreate');
  seed.variantId = extractId(
    productCreate.response,
    ['data', 'productCreate', 'product', 'variants', 'nodes', '0', 'id'],
    'productCreate variant',
  );
  setupCaptures.push(productCreate);

  const collectionCreate = await runSuccessMutation(
    'setup-collection-create',
    collectionCreateMutation,
    { input: { title: `HAR-294 Custom Data Type Matrix ${runId}` } },
    ['data', 'collectionCreate', 'userErrors'],
  );
  seed.collectionId = extractId(
    collectionCreate.response,
    ['data', 'collectionCreate', 'collection', 'id'],
    'collectionCreate',
  );
  setupCaptures.push(collectionCreate);

  const targetDefinition = await runSuccessMutation(
    'setup-target-metaobject-definition-create',
    metaobjectDefinitionCreateMutation,
    {
      definition: {
        type: `codex_har294_type_target_${runId}`,
        name: `HAR-294 Target ${runId}`,
        displayNameKey: 'title',
        fieldDefinitions: [{ key: 'title', name: 'Title', type: 'single_line_text_field', required: true }],
      },
    },
    ['data', 'metaobjectDefinitionCreate', 'userErrors'],
  );
  seed.targetDefinitionId = extractId(
    targetDefinition.response,
    ['data', 'metaobjectDefinitionCreate', 'metaobjectDefinition', 'id'],
    'target metaobjectDefinitionCreate',
  );
  setupCaptures.push(targetDefinition);

  const targetCreate = await runSuccessMutation(
    'setup-target-metaobject-create',
    metaobjectCreateDocument,
    {
      metaobject: {
        type: `codex_har294_type_target_${runId}`,
        handle: `codex-har294-type-target-${runId}`,
        fields: [{ key: 'title', value: `HAR-294 target ${runId}` }],
      },
    },
    ['data', 'metaobjectCreate', 'userErrors'],
  );
  seed.targetMetaobjectId = extractId(
    targetCreate.response,
    ['data', 'metaobjectCreate', 'metaobject', 'id'],
    'target metaobjectCreate',
  );
  setupCaptures.push(targetCreate);

  const references = referenceTypeCases({
    productId: seed.productId,
    variantId: seed.variantId,
    collectionId: seed.collectionId,
    targetMetaobjectId: seed.targetMetaobjectId,
  });
  const scalarCases = scalarTypeCases(currencyCode);
  const listCases = listTypeCases(scalarCases);
  const metaobjectCoveredCases = [
    { key: 'custom_id', type: 'id', value: `har294-${runId}` },
    ...scalarCases,
    ...metaobjectListTypeCases(scalarCases),
    ...references,
  ];
  const metafieldCoveredCases = [
    ...scalarCases,
    ...listCases,
    ...references.filter(
      (typeCase) => !typeCase.type.includes('metaobject_reference') && !typeCase.type.includes('mixed_reference'),
    ),
  ];

  const preconditionRead = await captureGraphql('metafield-precondition-read', metafieldReadDocument, {
    id: seed.productId,
    namespace: seed.namespace,
  });

  for (const [index, metafields] of chunk(
    toMetafieldsSetInput(seed.productId, seed.namespace, metafieldCoveredCases),
    25,
  ).entries()) {
    const variables = { metafields };
    const mutation = await runSuccessMutation(
      `metafields-set-type-matrix-batch-${index + 1}`,
      metafieldSetDocument,
      variables,
      ['data', 'metafieldsSet', 'userErrors'],
    );
    const downstreamRead = await captureGraphql(`metafield-downstream-read-batch-${index + 1}`, metafieldReadDocument, {
      id: seed.productId,
      namespace: seed.namespace,
    });
    metafieldBatches.push({ name: `batch-${index + 1}`, mutation, downstreamRead });
  }

  for (const [index, cases] of chunk(metaobjectCoveredCases, 40).entries()) {
    const matrixType = `${seed.matrixTypePrefix}_${index + 1}`;
    const matrixHandle = `${seed.matrixHandlePrefix}-${index + 1}`;
    const matrixDefinition = await runSuccessMutation(
      `setup-matrix-metaobject-definition-create-${index + 1}`,
      metaobjectDefinitionCreateMutation,
      {
        definition: {
          type: matrixType,
          name: `HAR-294 Type Matrix ${runId} ${index + 1}`,
          displayNameKey: cases[0]?.key ?? 'single_line_text_field',
          fieldDefinitions: toMetaobjectFieldDefinitions(cases),
        },
      },
      ['data', 'metaobjectDefinitionCreate', 'userErrors'],
    );
    const matrixDefinitionId = extractId(
      matrixDefinition.response,
      ['data', 'metaobjectDefinitionCreate', 'metaobjectDefinition', 'id'],
      `matrix metaobjectDefinitionCreate ${index + 1}`,
    );
    seed.matrixDefinitions.push({ id: matrixDefinitionId, type: matrixType, handle: matrixHandle });
    setupCaptures.push(matrixDefinition);

    const metaobjectSeedRead = await captureGraphql(`metaobject-seed-read-${index + 1}`, metaobjectSeedReadQuery, {
      targetDefinitionId: seed.targetDefinitionId,
      targetMetaobjectId: seed.targetMetaobjectId,
      matrixDefinitionId,
    });
    seededReads.push(metaobjectSeedRead);

    const matrixCreate = await runSuccessMutation(
      `metaobject-create-type-matrix-${index + 1}`,
      metaobjectCreateDocument,
      {
        metaobject: {
          type: matrixType,
          handle: matrixHandle,
          fields: toMetaobjectFields(cases),
        },
      },
      ['data', 'metaobjectCreate', 'userErrors'],
    );
    const matrixMetaobjectId = extractId(
      matrixCreate.response,
      ['data', 'metaobjectCreate', 'metaobject', 'id'],
      `matrix metaobjectCreate ${index + 1}`,
    );
    seed.matrixDefinitions[index] = {
      id: matrixDefinitionId,
      type: matrixType,
      handle: matrixHandle,
      metaobjectId: matrixMetaobjectId,
    };

    const matrixRead = await captureGraphql(`metaobject-read-type-matrix-${index + 1}`, metaobjectReadDocument, {
      id: matrixMetaobjectId,
      handle: { type: matrixType, handle: matrixHandle },
      type: matrixType,
    });
    metaobjectMatrices.push({
      name: `matrix-${index + 1}`,
      coveredTypes: cases.map((typeCase) => typeCase.type),
      create: matrixCreate,
      downstreamRead: matrixRead,
    });
  }

  await cleanup(cleanupCaptures);

  await mkdir(outputDir, { recursive: true });
  await writeFile(
    outputPath,
    `${JSON.stringify(
      {
        capturedAt: new Date().toISOString(),
        storeDomain,
        apiVersion,
        seed,
        metafieldCoveredTypes: metafieldCoveredCases.map((typeCase) => typeCase.type),
        metaobjectCoveredTypes: metaobjectCoveredCases.map((typeCase) => typeCase.type),
        excludedTypes: [
          {
            types: [
              'id',
              'list.id',
              'metaobject_reference',
              'list.metaobject_reference',
              'mixed_reference',
              'list.mixed_reference',
              'company_reference',
              'list.company_reference',
              'customer_reference',
              'list.customer_reference',
              'file_reference',
              'list.file_reference',
              'page_reference',
              'list.page_reference',
              'article_reference',
              'list.article_reference',
              'order_reference',
              'list.order_reference',
              'product_taxonomy_value_reference',
              'list.product_taxonomy_value_reference',
            ],
            reason:
              'These require separate definition-backed metafield or resource-specific setup not covered by the product-owned metafieldsSet portion of this disposable matrix. Metaobject-owned id/metaobject/mixed reference fields are covered by metaobjectCoveredTypes.',
          },
        ],
        setup: setupCaptures,
        seededReads,
        preconditionRead,
        metafieldBatches,
        metaobjectMatrices,
        cleanup: cleanupCaptures,
      },
      null,
      2,
    )}\n`,
  );
  console.log(`Wrote ${outputPath}`);
} catch (error) {
  try {
    await cleanup(cleanupCaptures);
  } catch (cleanupError) {
    cleanupCaptures.push({
      name: 'cleanup-failure',
      request: { query: '', variables: {} },
      status: 0,
      response: cleanupError instanceof Error ? cleanupError.message : String(cleanupError),
    });
  }
  await mkdir(outputDir, { recursive: true });
  const blockerPath = path.join(outputDir, `custom-data-field-type-matrix-blocker-${runId}.json`);
  await writeFile(
    blockerPath,
    `${JSON.stringify(
      {
        capturedAt: new Date().toISOString(),
        storeDomain,
        apiVersion,
        seed,
        blocker: error instanceof Error ? error.message : String(error),
        setup: setupCaptures,
        seededReads,
        metafieldBatches,
        metaobjectMatrices,
        cleanup: cleanupCaptures,
      },
      null,
      2,
    )}\n`,
  );
  console.error(`Wrote blocker evidence to ${blockerPath}`);
  throw error;
}
