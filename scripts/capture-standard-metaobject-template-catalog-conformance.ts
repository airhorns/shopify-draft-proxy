/* oxlint-disable no-console -- CLI capture script intentionally reports progress. */

import 'dotenv/config';

import { mkdirSync, writeFileSync } from 'node:fs';
import path from 'node:path';

import { runAdminGraphqlRequest } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const TEMPLATE_TYPES = [
  'shopify--qa-pair',
  'shopify--accessory-size',
  'shopify--age-group',
  'shopify--battery-type',
  'shopify--color-pattern',
  'shopify--fabric',
  'shopify--finish',
  'shopify--flavor',
  'shopify--jewelry-material',
  'shopify--material',
  'shopify--neckline',
  'shopify--occasion',
  'shopify--one-piece-style',
  'shopify--ring-size',
  'shopify--shoe-size',
  'shopify--size',
  'shopify--sleeve-length-type',
  'shopify--style',
  'shopify--target-gender',
  'shopify--theme',
  'shopify--toe-style',
  'shopify--top-length-type',
  'shopify--waist-rise',
  'shopify--watch-band-material',
  'shopify--pants-length-type',
  'shopify--closure-type',
  'shopify--fit-type',
] as const;

const PARITY_TYPES = ['shopify--qa-pair', 'shopify--color-pattern', 'shopify--material'] as const;
const UNKNOWN_TYPE = 'shopify--unknown-template';

const definitionFields = `#graphql
  fragment StandardMetaobjectDefinitionEnableFields on MetaobjectDefinition {
    id
    type
    name
    description
    displayNameKey
    access {
      admin
      storefront
    }
    capabilities {
      publishable {
        enabled
      }
      translatable {
        enabled
      }
      renderable {
        enabled
      }
      onlineStore {
        enabled
      }
    }
    fieldDefinitions {
      key
      name
      description
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
    hasThumbnailField
    metaobjectsCount
    standardTemplate {
      type
      name
    }
  }
`;

const enableMutation = `#graphql
  ${definitionFields}
  mutation StandardMetaobjectDefinitionEnableCatalog($type: String!) {
    standardMetaobjectDefinitionEnable(type: $type) {
      metaobjectDefinition {
        ...StandardMetaobjectDefinitionEnableFields
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

const existingDefinitionQuery = `#graphql
  ${definitionFields}
  query StandardMetaobjectDefinitionExisting($type: String!) {
    metaobjectDefinitionByType(type: $type) {
      ...StandardMetaobjectDefinitionEnableFields
    }
  }
`;

const readDefinitionQuery = `#graphql
  ${definitionFields}
  query StandardMetaobjectDefinitionEnableRead($id: ID!, $type: String!) {
    byId: metaobjectDefinition(id: $id) {
      ...StandardMetaobjectDefinitionEnableFields
    }
    byType: metaobjectDefinitionByType(type: $type) {
      ...StandardMetaobjectDefinitionEnableFields
    }
  }
`;

const deleteMutation = `#graphql
  mutation StandardMetaobjectDefinitionCleanup($id: ID!) {
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

function responsePayload(result: { status: number; payload: unknown }) {
  return result.payload;
}

function readUserErrors(payload: unknown, root: string): unknown[] {
  return (payload as { data?: Record<string, { userErrors?: unknown[] }> }).data?.[root]?.userErrors ?? [];
}

function assertNoGraphqlErrors(payload: unknown, label: string): void {
  const errors = (payload as { errors?: unknown }).errors;
  if (errors !== undefined) {
    throw new Error(`${label} returned top-level GraphQL errors: ${JSON.stringify(errors)}`);
  }
}

function assertNoUserErrors(payload: unknown, root: string, label: string): void {
  const userErrors = readUserErrors(payload, root);
  if (userErrors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(userErrors)}`);
  }
}

function capture(
  name: string,
  query: string,
  variables: Record<string, unknown>,
  result: { status: number; payload: unknown },
) {
  return {
    name,
    request: { query, variables },
    status: result.status,
    response: responsePayload(result),
  };
}

function definitionFromEnable(payload: unknown) {
  return (payload as { data?: { standardMetaobjectDefinitionEnable?: { metaobjectDefinition?: unknown } } }).data
    ?.standardMetaobjectDefinitionEnable?.metaobjectDefinition;
}

function definitionFromExisting(payload: unknown) {
  return (payload as { data?: { metaobjectDefinitionByType?: unknown } }).data?.metaobjectDefinitionByType;
}

function definitionId(definition: unknown): string | null {
  const id = (definition as { id?: unknown } | null | undefined)?.id;
  return typeof id === 'string' ? id : null;
}

function stripInstanceFields(definition: unknown) {
  const { id: _id, createdAt: _createdAt, updatedAt: _updatedAt, ...rest } = definition as Record<string, unknown>;
  const fieldDefinitions = Array.isArray(rest['fieldDefinitions'])
    ? rest['fieldDefinitions'].map((field) => {
        const { id: _fieldId, ...fieldRest } = field as Record<string, unknown>;
        return fieldRest;
      })
    : [];
  return {
    ...rest,
    fieldDefinitions,
  };
}

async function main() {
  const config = readConformanceScriptConfig({
    defaultApiVersion: '2026-04',
    requireAdminOrigin: false,
  });
  const accessToken = await getValidConformanceAccessToken({
    adminOrigin: config.adminOrigin,
    apiVersion: config.apiVersion,
  });
  const headers = buildAdminAuthHeaders(accessToken);
  const graphql = (query: string, variables: Record<string, unknown> = {}) =>
    runAdminGraphqlRequest(
      { adminOrigin: config.adminOrigin, apiVersion: config.apiVersion, headers },
      query,
      variables,
    );

  const catalogTemplates: Record<string, unknown>[] = [];
  const unavailableCandidates: unknown[] = [];
  const catalogCaptures: unknown[] = [];
  const cleanupCaptures: unknown[] = [];
  const definitionsCreatedByCapture = new Set<string>();

  async function enableTemplate(name: string, type: string) {
    const existing = await graphql(existingDefinitionQuery, { type });
    assertNoGraphqlErrors(existing.payload, `existing-${type}`);
    const existingId = definitionId(definitionFromExisting(existing.payload));
    const enabled = await graphql(enableMutation, { type });
    assertNoGraphqlErrors(enabled.payload, name);
    if (existingId === null) {
      const createdId = definitionId(definitionFromEnable(enabled.payload));
      if (createdId !== null) definitionsCreatedByCapture.add(createdId);
    }
    return enabled;
  }

  async function deleteCreated(name: string, definition: unknown) {
    const id = definitionId(definition);
    if (id === null || !definitionsCreatedByCapture.has(id)) return null;
    const deleted = await graphql(deleteMutation, { id });
    const captured = capture(name, deleteMutation, { id }, deleted);
    cleanupCaptures.push(captured);
    assertNoGraphqlErrors(deleted.payload, name);
    assertNoUserErrors(deleted.payload, 'metaobjectDefinitionDelete', name);
    definitionsCreatedByCapture.delete(id);
    return captured;
  }

  for (const type of TEMPLATE_TYPES) {
    const enabled = await enableTemplate(`catalog-enable-${type}`, type);
    const captured = capture(`catalog-enable-${type}`, enableMutation, { type }, enabled);
    catalogCaptures.push(captured);
    const definition = definitionFromEnable(enabled.payload);
    if (definition === undefined || definition === null) {
      unavailableCandidates.push({
        type,
        userErrors: readUserErrors(enabled.payload, 'standardMetaobjectDefinitionEnable'),
      });
      continue;
    }
    assertNoUserErrors(enabled.payload, 'standardMetaobjectDefinitionEnable', `catalog-enable-${type}`);
    catalogTemplates.push(stripInstanceFields(definition));
    await deleteCreated(`catalog-cleanup-${type}`, definition);
  }

  const [qaPairType, colorPatternType, materialType] = PARITY_TYPES;
  const qaPair = await enableTemplate('qaPair', qaPairType);
  const qaPairDefinition = definitionFromEnable(qaPair.payload);
  const qaPairRead = await graphql(readDefinitionQuery, { id: definitionId(qaPairDefinition), type: qaPairType });
  await deleteCreated('qaPairCleanup', qaPairDefinition);

  const colorPattern = await enableTemplate('colorPattern', colorPatternType);
  const colorPatternDefinition = definitionFromEnable(colorPattern.payload);
  const colorPatternRead = await graphql(readDefinitionQuery, {
    id: definitionId(colorPatternDefinition),
    type: colorPatternType,
  });
  await deleteCreated('colorPatternCleanup', colorPatternDefinition);

  const material = await enableTemplate('material', materialType);
  const materialDefinition = definitionFromEnable(material.payload);
  const materialRead = await graphql(readDefinitionQuery, { id: definitionId(materialDefinition), type: materialType });
  await deleteCreated('materialCleanup', materialDefinition);

  const unknown = await graphql(enableMutation, { type: UNKNOWN_TYPE });
  assertNoGraphqlErrors(unknown.payload, 'unknown');

  const duplicateFirst = await enableTemplate('duplicateFirst', qaPairType);
  const duplicateDefinition = definitionFromEnable(duplicateFirst.payload);
  const duplicateSecond = await graphql(enableMutation, { type: qaPairType });
  await deleteCreated('duplicateCleanup', duplicateDefinition);

  const capturedAt = new Date().toISOString();
  const outputDir = path.join('fixtures', 'conformance', config.storeDomain, config.apiVersion, 'metaobjects');
  mkdirSync(outputDir, { recursive: true });

  const catalog = {
    kind: 'shopify-standard-metaobject-template-catalog',
    capturedAt,
    storeDomain: config.storeDomain,
    apiVersion: config.apiVersion,
    source:
      'Derived from standardMetaobjectDefinitionEnable round-trip captures against the disposable conformance shop; live definition instance IDs and timestamps are stripped.',
    templateTypes: catalogTemplates.map((template) => template['type']),
    templates: catalogTemplates,
    unavailableCandidates,
    captures: catalogCaptures,
    cleanup: cleanupCaptures,
  };

  writeFileSync(path.join(outputDir, 'standard-metaobject-templates.json'), `${JSON.stringify(catalog, null, 2)}\n`);

  const scenario = {
    capturedAt,
    storeDomain: config.storeDomain,
    apiVersion: config.apiVersion,
    scenarioId: 'standard-metaobject-definition-enable-catalog',
    summary:
      'standardMetaobjectDefinitionEnable catalog, unknown-template RECORD_NOT_FOUND, idempotent duplicate enable, and read-after-enable behavior.',
    qaPair: capture('qaPair', enableMutation, { type: qaPairType }, qaPair),
    qaPairRead: capture(
      'qaPairRead',
      readDefinitionQuery,
      { id: definitionId(qaPairDefinition), type: qaPairType },
      qaPairRead,
    ),
    colorPattern: capture('colorPattern', enableMutation, { type: colorPatternType }, colorPattern),
    colorPatternRead: capture(
      'colorPatternRead',
      readDefinitionQuery,
      {
        id: definitionId(colorPatternDefinition),
        type: colorPatternType,
      },
      colorPatternRead,
    ),
    material: capture('material', enableMutation, { type: materialType }, material),
    materialRead: capture(
      'materialRead',
      readDefinitionQuery,
      { id: definitionId(materialDefinition), type: materialType },
      materialRead,
    ),
    unknown: capture('unknown', enableMutation, { type: UNKNOWN_TYPE }, unknown),
    duplicateFirst: capture('duplicateFirst', enableMutation, { type: qaPairType }, duplicateFirst),
    duplicateSecond: capture('duplicateSecond', enableMutation, { type: qaPairType }, duplicateSecond),
    cleanup: cleanupCaptures,
    upstreamCalls: [],
  };

  writeFileSync(
    path.join(outputDir, 'standard-metaobject-definition-enable-catalog.json'),
    `${JSON.stringify(scenario, null, 2)}\n`,
  );

  console.log(`Wrote standard metaobject template catalog and parity capture to ${outputDir}`);
}

main().catch((error: unknown) => {
  console.error(error instanceof Error ? (error.stack ?? error.message) : error);
  process.exit(1);
});
