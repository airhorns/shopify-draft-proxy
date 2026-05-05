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

type ValidationCase = {
  name: string;
  key: string;
  type: string;
  value: string;
  definitionValidations?: Array<{ name: string; value: string | null }>;
  note?: string;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});

const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'metaobjects');
const outputPath = path.join(outputDir, 'metaobject-field-validation-matrix.json');
const specPath = path.join('config', 'parity-specs', 'metaobjects', 'metaobject-field-validation-matrix.json');
const runId = Date.now().toString();
const targetType = `codex_har685_validation_target_${runId}`;
const matrixType = `codex_har685_validation_${runId}`;
const matrixHandle = `codex-har685-validation-${runId}`;

const requestPaths = {
  definitionCreate: 'config/parity-requests/metaobjects/metaobject-field-validation-matrix-definition-create.graphql',
  create: 'config/parity-requests/metaobjects/metaobject-field-validation-matrix-create.graphql',
  update: 'config/parity-requests/metaobjects/metaobject-field-validation-matrix-update.graphql',
};

const queries = Object.fromEntries(
  await Promise.all(
    Object.entries(requestPaths).map(async ([name, requestPath]) => [name, await readFile(requestPath, 'utf8')]),
  ),
) as Record<keyof typeof requestPaths, string>;

const metaobjectDeleteMutation = `#graphql
  mutation MetaobjectFieldValidationMatrixDelete($id: ID!) {
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
  mutation MetaobjectFieldValidationMatrixDefinitionDelete($id: ID!) {
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
  if (result.status < 200 || result.status >= 300 || readPath(result.payload, ['errors'])) {
    throw new Error(`${label} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function assertNoUserErrors(payload: unknown, pathParts: string[], label: string): void {
  const userErrors = readUserErrors(payload, pathParts);
  if (userErrors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(userErrors, null, 2)}`);
  }
}

function extractString(payload: unknown, pathParts: string[], label: string): string {
  const value = readPath(payload, pathParts);
  if (typeof value !== 'string' || value.length === 0) {
    throw new Error(`${label} did not return a string at ${pathParts.join('.')}: ${JSON.stringify(payload, null, 2)}`);
  }
  return value;
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

function measurementBadValue(): string {
  return jsonString({ value: 'not-a-number' });
}

function validationCases(metaobjectDefinitionId: string): ValidationCase[] {
  const scalarCases: ValidationCase[] = [
    {
      name: 'number-integer-max',
      key: 'number_integer',
      type: 'number_integer',
      value: '10',
      definitionValidations: [{ name: 'max', value: '5' }],
    },
    { name: 'number-decimal-invalid', key: 'number_decimal', type: 'number_decimal', value: 'hello' },
    {
      name: 'boolean-coercion',
      key: 'boolean',
      type: 'boolean',
      value: 'hello',
      note: 'Shopify 2026-04 coerces this scalar value instead of returning INVALID_VALUE.',
    },
    { name: 'date-invalid', key: 'date', type: 'date', value: '2024-99-01' },
    { name: 'date-time-invalid', key: 'date_time', type: 'date_time', value: 'not-a-date-time' },
    { name: 'dimension-invalid', key: 'dimension', type: 'dimension', value: measurementBadValue() },
    { name: 'volume-invalid', key: 'volume', type: 'volume', value: measurementBadValue() },
    { name: 'weight-invalid', key: 'weight', type: 'weight', value: measurementBadValue() },
    {
      name: 'rating-scale-max',
      key: 'rating',
      type: 'rating',
      value: jsonString({ value: '10', scale_min: '1.0', scale_max: '5.0' }),
      definitionValidations: [
        { name: 'scale_min', value: '1.0' },
        { name: 'scale_max', value: '5.0' },
      ],
    },
    { name: 'color-invalid', key: 'color', type: 'color', value: 'red' },
    { name: 'url-invalid', key: 'url', type: 'url', value: 'hello' },
    {
      name: 'product-reference-invalid',
      key: 'product_reference',
      type: 'product_reference',
      value: 'gid://shopify/Metaobject/1',
    },
    {
      name: 'variant-reference-invalid',
      key: 'variant_reference',
      type: 'variant_reference',
      value: 'gid://shopify/Product/1',
    },
    {
      name: 'collection-reference-invalid',
      key: 'collection_reference',
      type: 'collection_reference',
      value: 'gid://shopify/Product/1',
    },
    {
      name: 'customer-reference-invalid',
      key: 'customer_reference',
      type: 'customer_reference',
      value: 'gid://shopify/Product/1',
    },
    {
      name: 'company-reference-invalid',
      key: 'company_reference',
      type: 'company_reference',
      value: 'gid://shopify/Product/1',
    },
    {
      name: 'metaobject-reference-invalid',
      key: 'metaobject_reference',
      type: 'metaobject_reference',
      value: 'gid://shopify/Product/1',
      definitionValidations: [{ name: 'metaobject_definition_id', value: metaobjectDefinitionId }],
    },
    {
      name: 'single-line-text-max',
      key: 'single_line_text_field',
      type: 'single_line_text_field',
      value: 'abcd',
      definitionValidations: [{ name: 'max', value: '3' }],
    },
  ];

  const listCases = scalarCases
    .filter((typeCase) => typeCase.type !== 'boolean' && typeCase.type !== 'single_line_text_field')
    .map((typeCase) => {
      const listItemValue = typeCase.type === 'number_integer' ? 'hello' : typeCase.value;
      const definitionValidations = caseListDefinitionValidations(typeCase);
      return {
        name: `list-${typeCase.name}`,
        key: `list_${typeCase.key}`,
        type: `list.${typeCase.type}`,
        value: listValue(listItemValue),
        ...(typeCase.note ? { note: typeCase.note } : {}),
        ...(definitionValidations ? { definitionValidations } : {}),
      };
    });

  return [...scalarCases, ...listCases];
}

function caseListDefinitionValidations(
  typeCase: ValidationCase,
): Array<{ name: string; value: string | null }> | undefined {
  if (typeCase.type === 'metaobject_reference') {
    return typeCase.definitionValidations;
  }
  if (typeCase.type === 'number_integer' || typeCase.type.endsWith('_reference')) {
    return undefined;
  }
  return typeCase.definitionValidations;
}

function toFieldDefinitions(cases: ValidationCase[]): Record<string, unknown>[] {
  return [
    { key: 'title', name: 'Title', type: 'single_line_text_field', required: false },
    ...cases.map((typeCase) => ({
      key: typeCase.key,
      name: typeCase.type,
      type: typeCase.type,
      required: false,
      ...(typeCase.definitionValidations ? { validations: typeCase.definitionValidations } : {}),
    })),
  ];
}

function createVariablesFor(typeCase: ValidationCase, suffix: string): Record<string, unknown> {
  return {
    metaobject: {
      type: matrixType,
      handle: `${matrixHandle}-${suffix}`,
      fields: [{ key: typeCase.key, value: typeCase.value }],
    },
  };
}

function updateVariablesFor(typeCase: ValidationCase): Record<string, unknown> {
  return {
    id: '',
    metaobject: {
      fields: [{ key: typeCase.key, value: typeCase.value }],
    },
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

async function cleanup(
  createdMetaobjectIds: string[],
  definitionIds: string[],
  cleanupCaptures: Capture[],
): Promise<void> {
  for (const id of createdMetaobjectIds) {
    cleanupCaptures.push(await captureGraphql('cleanup-metaobject-delete', metaobjectDeleteMutation, { id }));
  }
  for (const id of definitionIds) {
    cleanupCaptures.push(
      await captureGraphql('cleanup-metaobject-definition-delete', metaobjectDefinitionDeleteMutation, { id }),
    );
  }
}

function userErrorsPath(root: 'metaobjectCreate' | 'metaobjectUpdate' | 'metaobjectDefinitionCreate'): string[] {
  return ['data', root, 'userErrors'];
}

function buildSpec(caseNames: string[]): Record<string, unknown> {
  return {
    scenarioId: 'metaobject-field-validation-matrix',
    operationNames: ['metaobjectDefinitionCreate', 'metaobjectCreate', 'metaobjectUpdate', 'metaobject'],
    scenarioStatus: 'captured',
    assertionKinds: ['user-errors-parity', 'validation-semantics'],
    liveCaptureFiles: [outputPath],
    runtimeTestFiles: ['test/parity_test.gleam', 'test/shopify_draft_proxy/proxy/metaobject_definitions_test.gleam'],
    proxyRequest: {
      documentPath: requestPaths.definitionCreate,
      variablesCapturePath: '$.definitionCreate.request.variables',
      apiVersion,
    },
    comparisonMode: 'captured-vs-proxy-request',
    notes:
      'HAR-685 strict metaobject field value validation parity. Captured create/update branches compare only userErrors so failing branches do not depend on live IDs. Shopify 2026-04 coerces scalar boolean input like "hello"; that branch is retained as captured behavior while list/scalar validation errors cover the strict custom-data types.',
    comparison: {
      mode: 'strict-json',
      expectedDifferences: [],
      targets: [
        {
          name: 'definition-create-setup',
          capturePath: '$.definitionCreate.response.data.metaobjectDefinitionCreate.userErrors',
          proxyPath: '$.data.metaobjectDefinitionCreate.userErrors',
        },
        {
          name: 'setup-metaobject-create',
          capturePath: '$.setupMetaobjectCreate.response.data.metaobjectCreate.userErrors',
          proxyPath: '$.data.metaobjectCreate.userErrors',
          proxyRequest: {
            documentPath: requestPaths.create,
            variablesCapturePath: '$.setupMetaobjectCreate.request.variables',
            apiVersion,
          },
        },
        ...caseNames.flatMap((caseName) => [
          {
            name: `create-${caseName}`,
            capturePath: `$.invalidCreateCases.${caseName}.response.data.metaobjectCreate.userErrors`,
            proxyPath: '$.data.metaobjectCreate.userErrors',
            proxyRequest: {
              documentPath: requestPaths.create,
              variablesCapturePath: `$.invalidCreateCases.${caseName}.request.variables`,
              apiVersion,
            },
          },
          {
            name: `update-${caseName}`,
            capturePath: `$.invalidUpdateCases.${caseName}.response.data.metaobjectUpdate.userErrors`,
            proxyPath: '$.data.metaobjectUpdate.userErrors',
            proxyRequest: {
              documentPath: requestPaths.update,
              variables: {
                id: {
                  fromProxyResponse: 'setup-metaobject-create',
                  path: '$.data.metaobjectCreate.metaobject.id',
                },
                metaobject: {
                  fromCapturePath: `$.invalidUpdateCases.${caseName}.request.variables.metaobject`,
                },
              },
              apiVersion,
            },
          },
        ]),
      ],
    },
  };
}

const cleanupCaptures: Capture[] = [];
const createdMetaobjectIds: string[] = [];
const definitionIds: string[] = [];
let capturedDefinitionId: string | null = null;
let targetDefinitionCreate: Capture | null = null;

try {
  targetDefinitionCreate = await captureGraphql('target-definition-create', queries.definitionCreate, {
    definition: {
      type: targetType,
      name: `HAR-685 Field Validation Target ${runId}`,
      displayNameKey: 'title',
      fieldDefinitions: [{ key: 'title', name: 'Title', type: 'single_line_text_field', required: false }],
    },
  });
  assertNoUserErrors(
    targetDefinitionCreate.response,
    userErrorsPath('metaobjectDefinitionCreate'),
    'target-definition-create',
  );
  const targetDefinitionId = extractString(
    targetDefinitionCreate.response,
    ['data', 'metaobjectDefinitionCreate', 'metaobjectDefinition', 'id'],
    'target-definition-create',
  );
  definitionIds.push(targetDefinitionId);

  const cases = validationCases(targetDefinitionId);
  const definitionCreate = await captureGraphql('definition-create', queries.definitionCreate, {
    definition: {
      type: matrixType,
      name: `HAR-685 Field Validation ${runId}`,
      displayNameKey: 'title',
      fieldDefinitions: toFieldDefinitions(cases),
    },
  });
  assertNoUserErrors(definitionCreate.response, userErrorsPath('metaobjectDefinitionCreate'), 'definition-create');
  const matrixDefinitionId = extractString(
    definitionCreate.response,
    ['data', 'metaobjectDefinitionCreate', 'metaobjectDefinition', 'id'],
    'definition-create',
  );
  definitionIds.unshift(matrixDefinitionId);
  capturedDefinitionId = matrixDefinitionId;

  const setupMetaobjectCreate = await captureGraphql('setup-metaobject-create', queries.create, {
    metaobject: {
      type: matrixType,
      handle: matrixHandle,
      fields: [{ key: 'title', value: `HAR-685 ${runId}` }],
    },
  });
  assertNoUserErrors(setupMetaobjectCreate.response, userErrorsPath('metaobjectCreate'), 'setup-metaobject-create');
  const setupMetaobjectId = extractString(
    setupMetaobjectCreate.response,
    ['data', 'metaobjectCreate', 'metaobject', 'id'],
    'setup-metaobject-create',
  );
  createdMetaobjectIds.push(setupMetaobjectId);

  const invalidCreateCases: Record<string, Capture> = {};
  const invalidUpdateCases: Record<string, Capture> = {};

  for (const typeCase of cases) {
    const createCapture = await captureGraphql(
      `create-${typeCase.name}`,
      queries.create,
      createVariablesFor(typeCase, typeCase.name),
    );
    const createdId = readPath(createCapture.response, ['data', 'metaobjectCreate', 'metaobject', 'id']);
    if (typeof createdId === 'string' && createdId.length > 0) {
      createdMetaobjectIds.push(createdId);
    }
    invalidCreateCases[typeCase.name] = createCapture;

    const updateVariables = updateVariablesFor(typeCase);
    updateVariables['id'] = setupMetaobjectId;
    invalidUpdateCases[typeCase.name] = await captureGraphql(
      `update-${typeCase.name}`,
      queries.update,
      updateVariables,
    );
  }

  await cleanup(createdMetaobjectIds, definitionIds, cleanupCaptures);
  definitionIds.splice(0, definitionIds.length);

  const caseNames = cases.map((typeCase) => typeCase.name);

  await mkdir(outputDir, { recursive: true });
  await writeFile(
    outputPath,
    `${JSON.stringify(
      {
        capturedAt: new Date().toISOString(),
        storeDomain,
        apiVersion,
        summary:
          'HAR-685 metaobjectCreate/metaobjectUpdate field value validation matrix for scalar, measurement, reference, rating, URL/color/date/time, text max, and list field types.',
        seed: {
          runId,
          targetType,
          matrixType,
          matrixHandle,
          definitionId: capturedDefinitionId,
        },
        coveredTypes: cases.map((typeCase) => ({
          name: typeCase.name,
          key: typeCase.key,
          type: typeCase.type,
          note: typeCase.note,
        })),
        targetDefinitionCreate,
        definitionCreate,
        setupMetaobjectCreate,
        invalidCreateCases,
        invalidUpdateCases,
        cleanup: cleanupCaptures,
        upstreamCalls: [
          {
            operationName: 'MetaobjectDefinitionHydrateByType',
            variables: { type: matrixType },
            query: 'sha:hand-synthesized-from-capture',
            response: {
              status: 200,
              body: {
                data: {
                  metaobjectDefinitionByType: null,
                },
              },
            },
          },
        ],
      },
      null,
      2,
    )}\n`,
  );
  await writeFile(specPath, `${JSON.stringify(buildSpec(caseNames), null, 2)}\n`);
  console.log(`Wrote ${outputPath}`);
  console.log(`Wrote ${specPath}`);
} catch (error) {
  try {
    await cleanup(createdMetaobjectIds, definitionIds, cleanupCaptures);
  } catch (cleanupError) {
    cleanupCaptures.push({
      name: 'cleanup-failure',
      request: { query: '', variables: {} },
      status: 0,
      response: cleanupError instanceof Error ? cleanupError.message : String(cleanupError),
    });
  }
  await mkdir(outputDir, { recursive: true });
  const blockerPath = path.join(outputDir, `metaobject-field-validation-matrix-blocker-${runId}.json`);
  await writeFile(
    blockerPath,
    `${JSON.stringify(
      {
        capturedAt: new Date().toISOString(),
        storeDomain,
        apiVersion,
        seed: {
          runId,
          targetType,
          matrixType,
          matrixHandle,
          definitionIds,
        },
        blocker: error instanceof Error ? error.message : String(error),
        cleanup: cleanupCaptures,
      },
      null,
      2,
    )}\n`,
  );
  console.error(`Wrote blocker evidence to ${blockerPath}`);
  throw error;
}
