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

function definitionCount(definition: unknown): number {
  const count = (definition as { metaobjectsCount?: unknown } | null | undefined)?.metaobjectsCount;
  return typeof count === 'number' ? count : 0;
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

function gleamString(value: string): string {
  return JSON.stringify(value);
}

function gleamOptionString(value: unknown): string {
  return typeof value === 'string' ? `Some(${gleamString(value)})` : 'None';
}

function gleamOptionBool(value: unknown): string {
  return typeof value === 'boolean' ? `Some(${value ? 'True' : 'False'})` : 'None';
}

function gleamCapability(capability: unknown): string {
  const enabled = (capability as { enabled?: unknown } | null | undefined)?.enabled;
  return typeof enabled === 'boolean'
    ? `Some(MetaobjectDefinitionCapabilityRecord(${enabled ? 'True' : 'False'}))`
    : 'None';
}

function gleamValidation(validation: Record<string, unknown>): string {
  return `MetaobjectFieldDefinitionValidationRecord(${gleamString(String(validation['name']))}, ${gleamOptionString(validation['value'])})`;
}

function gleamType(type: Record<string, unknown>): string {
  return `MetaobjectDefinitionTypeRecord(${gleamString(String(type['name']))}, ${gleamOptionString(type['category'])})`;
}

function gleamField(field: Record<string, unknown>): string {
  const validations = Array.isArray(field['validations'])
    ? `[${field['validations'].map((validation) => gleamValidation(validation as Record<string, unknown>)).join(', ')}]`
    : '[]';
  return [
    'MetaobjectFieldDefinitionRecord(',
    gleamString(String(field['key'])),
    ', ',
    gleamOptionString(field['name']),
    ', ',
    gleamOptionString(field['description']),
    ', ',
    gleamOptionBool(field['required']),
    ', ',
    gleamType((field['type'] ?? {}) as Record<string, unknown>),
    ', ',
    validations,
    ')',
  ].join('');
}

function gleamTemplate(definition: Record<string, unknown>): string {
  const access = definition['access'] as Record<string, unknown>;
  const capabilities = definition['capabilities'] as Record<string, unknown>;
  const fields = Array.isArray(definition['fieldDefinitions'])
    ? definition['fieldDefinitions'].map((field) => gleamField(field as Record<string, unknown>)).join(',\n      ')
    : '';
  return `StandardMetaobjectTemplate(
    type_: ${gleamString(String(definition['type']))},
    name: ${gleamString(String(definition['name']))},
    description: ${gleamOptionString(definition['description'])},
    display_name_key: ${gleamString(String(definition['displayNameKey']))},
    access: dict.from_list([
      #("admin", ${gleamOptionString(access['admin'])}),
      #("storefront", ${gleamOptionString(access['storefront'])}),
    ]),
    capabilities: MetaobjectDefinitionCapabilitiesRecord(
      publishable: ${gleamCapability(capabilities['publishable'])},
      translatable: ${gleamCapability(capabilities['translatable'])},
      renderable: ${gleamCapability(capabilities['renderable'])},
      online_store: ${gleamCapability(capabilities['onlineStore'])},
    ),
    field_definitions: [
      ${fields}
    ],
    has_thumbnail_field: ${gleamOptionBool(definition['hasThumbnailField'])},
  )`;
}

function generatedGleamModule(templates: Record<string, unknown>[]): string {
  return `//// Generated by scripts/capture-standard-metaobject-template-catalog-conformance.ts.
//// Source fixture: fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/metaobjects/standard-metaobject-templates.json

import gleam/dict
import gleam/option.{type Option, None, Some}
import shopify_draft_proxy/state/types.{
  type MetaobjectDefinitionCapabilitiesRecord,
  type MetaobjectFieldDefinitionRecord,
  MetaobjectDefinitionCapabilitiesRecord,
  MetaobjectDefinitionCapabilityRecord,
  MetaobjectDefinitionTypeRecord,
  MetaobjectFieldDefinitionRecord,
  MetaobjectFieldDefinitionValidationRecord,
}

pub type StandardMetaobjectTemplate {
  StandardMetaobjectTemplate(
    type_: String,
    name: String,
    description: Option(String),
    display_name_key: String,
    access: dict.Dict(String, Option(String)),
    capabilities: MetaobjectDefinitionCapabilitiesRecord,
    field_definitions: List(MetaobjectFieldDefinitionRecord),
    has_thumbnail_field: Option(Bool),
  )
}

pub fn templates() -> List(StandardMetaobjectTemplate) {
  [
    ${templates.map(gleamTemplate).join(',\n    ')}
  ]
}
`;
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

  async function cleanupExisting(type: string) {
    const existing = await graphql(existingDefinitionQuery, { type });
    assertNoGraphqlErrors(existing.payload, `existing-${type}`);
    const definition = definitionFromExisting(existing.payload);
    const id = definitionId(definition);
    if (id === null) return;
    if (definitionCount(definition) > 0) {
      throw new Error(`Existing definition for ${type} has metaobjectsCount > 0; refusing to delete test data.`);
    }
    const deleted = await graphql(deleteMutation, { id });
    cleanupCaptures.push(capture(`preclean-${type}`, deleteMutation, { id }, deleted));
    assertNoGraphqlErrors(deleted.payload, `preclean-${type}`);
    assertNoUserErrors(deleted.payload, 'metaobjectDefinitionDelete', `preclean-${type}`);
  }

  async function enableTemplate(name: string, type: string) {
    await cleanupExisting(type);
    const enabled = await graphql(enableMutation, { type });
    assertNoGraphqlErrors(enabled.payload, name);
    return enabled;
  }

  async function deleteCreated(name: string, definition: unknown) {
    const id = definitionId(definition);
    if (id === null) return null;
    const deleted = await graphql(deleteMutation, { id });
    const captured = capture(name, deleteMutation, { id }, deleted);
    cleanupCaptures.push(captured);
    assertNoGraphqlErrors(deleted.payload, name);
    assertNoUserErrors(deleted.payload, 'metaobjectDefinitionDelete', name);
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
  writeFileSync(
    path.join('src', 'shopify_draft_proxy', 'proxy', 'metaobject_standard_templates_data.gleam'),
    generatedGleamModule(catalogTemplates),
  );

  const scenario = {
    capturedAt,
    storeDomain: config.storeDomain,
    apiVersion: config.apiVersion,
    scenarioId: 'standard-metaobject-definition-enable-catalog',
    issue: 'HAR-682',
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
