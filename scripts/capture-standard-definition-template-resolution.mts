/* oxlint-disable no-console -- capture scripts intentionally report progress. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlPayload } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonObject = Record<string, unknown>;

type GraphqlCapture = {
  request: {
    query: string;
    variables: JsonObject;
  };
  status: number;
  response: ConformanceGraphqlPayload;
};

type CatalogEdge = {
  cursor: string;
  node: JsonObject;
};

type CatalogPage = {
  edges: CatalogEdge[];
  pageInfo: {
    hasNextPage: boolean;
    endCursor: string | null;
  };
};

const catalogPageQuery = `#graphql
  query StandardDefinitionTemplateCatalogPage($first: Int!, $after: String) {
    standardMetafieldDefinitionTemplates(first: $first, after: $after, excludeActivated: false) {
      edges {
        cursor
        node {
          id
          namespace
          key
          name
          description
          ownerTypes
          type {
            name
            category
          }
          validations {
            name
            value
          }
          visibleToStorefrontApi
        }
      }
      pageInfo {
        hasNextPage
        endCursor
      }
    }
  }
`;

const catalogIdsPageQuery = `#graphql
  query StandardDefinitionTemplateCatalogIdsPage(
    $first: Int!
    $after: String
    $excludeActivated: Boolean!
    $constraintStatus: MetafieldDefinitionConstraintStatus
    $constraintSubtype: MetafieldDefinitionConstraintSubtypeIdentifier
  ) {
    standardMetafieldDefinitionTemplates(
      first: $first
      after: $after
      excludeActivated: $excludeActivated
      constraintStatus: $constraintStatus
      constraintSubtype: $constraintSubtype
    ) {
      edges {
        cursor
        node {
          id
        }
      }
      pageInfo {
        hasNextPage
        endCursor
      }
    }
  }
`;

const catalogScenarioQuery = `#graphql
  query StandardDefinitionTemplateResolution(
    $first: Int!
    $after: String!
    $last: Int!
    $before: String!
    $constraintSubtype: MetafieldDefinitionConstraintSubtypeIdentifier!
  ) {
    firstPage: standardMetafieldDefinitionTemplates(first: $first, excludeActivated: false) {
      edges {
        cursor
        node {
          id
          namespace
          key
          name
          description
          ownerTypes
          type {
            name
            category
          }
          validations {
            name
            value
          }
          visibleToStorefrontApi
        }
      }
      nodes {
        id
        namespace
        key
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
        startCursor
        endCursor
      }
    }
    afterPage: standardMetafieldDefinitionTemplates(first: 3, after: $after, excludeActivated: false) {
      edges {
        cursor
        node {
          id
          namespace
          key
        }
      }
      nodes {
        id
        namespace
        key
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
        startCursor
        endCursor
      }
    }
    backwardPage: standardMetafieldDefinitionTemplates(last: $last, before: $before, excludeActivated: false) {
      edges {
        cursor
        node {
          id
          namespace
          key
        }
      }
      nodes {
        id
        namespace
        key
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
        startCursor
        endCursor
      }
    }
    reversed: standardMetafieldDefinitionTemplates(first: $first, reverse: true, excludeActivated: false) {
      nodes {
        id
        namespace
        key
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
        startCursor
        endCursor
      }
    }
    constrained: standardMetafieldDefinitionTemplates(
      first: 3
      constraintStatus: CONSTRAINED_ONLY
      excludeActivated: false
    ) {
      nodes {
        id
        namespace
        key
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
        startCursor
        endCursor
      }
    }
    constrainedAvailable: standardMetafieldDefinitionTemplates(
      first: 3
      constraintStatus: CONSTRAINED_ONLY
      excludeActivated: true
    ) {
      nodes {
        id
        namespace
        key
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
        startCursor
        endCursor
      }
    }
    subtype: standardMetafieldDefinitionTemplates(
      first: 3
      constraintStatus: CONSTRAINED_ONLY
      constraintSubtype: $constraintSubtype
      excludeActivated: false
    ) {
      nodes {
        id
        namespace
        key
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
        startCursor
        endCursor
      }
    }
  }
`;

const invalidBackwardQuery = `#graphql
  query StandardDefinitionTemplateInvalidBackward($last: Int!) {
    standardMetafieldDefinitionTemplates(last: $last, excludeActivated: false) {
      nodes {
        id
      }
    }
  }
`;

const materialEnableMutation = `#graphql
  mutation StandardDefinitionTemplateMaterialEnable {
    standardMetafieldDefinitionEnable(ownerType: PRODUCT, namespace: "shopify", key: "material") {
      createdDefinition {
        id
        namespace
        key
        ownerType
        name
        description
        type {
          name
          category
        }
        validations {
          name
          value
        }
        constraints {
          key
          values(first: 5) {
            nodes {
              value
            }
          }
        }
        access {
          admin
          storefront
          customerAccount
        }
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const materialReadQuery = `#graphql
  query StandardDefinitionTemplateMaterialRead($id: ID!) {
    metafieldDefinition(id: $id) {
      id
      namespace
      key
      ownerType
      name
      description
      type {
        name
        category
      }
      validations {
        name
        value
      }
      constraints {
        key
        values(first: 5) {
          nodes {
            value
          }
        }
      }
      access {
        admin
        storefront
        customerAccount
      }
      standardTemplate {
        id
        namespace
        key
      }
    }
  }
`;

const unknownEnableMutation = `#graphql
  mutation StandardDefinitionTemplateUnknownEnable {
    standardMetafieldDefinitionEnable(
      ownerType: PRODUCT
      namespace: "missing_standard_context"
      key: "missing_template"
    ) {
      createdDefinition {
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

const definitionByIdentifierQuery = `#graphql
  query StandardDefinitionTemplateDefinitionByIdentifier(
    $ownerType: MetafieldOwnerType!
    $namespace: String!
    $key: String!
  ) {
    metafieldDefinition(identifier: { ownerType: $ownerType, namespace: $namespace, key: $key }) {
      id
    }
  }
`;

const definitionDeleteMutation = `#graphql
  mutation StandardDefinitionTemplateDefinitionDelete($id: ID!) {
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

function asObject(value: unknown, label: string): JsonObject {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) {
    throw new Error(`${label} must be an object`);
  }
  return value as JsonObject;
}

function asArray(value: unknown, label: string): unknown[] {
  if (!Array.isArray(value)) {
    throw new Error(`${label} must be an array`);
  }
  return value;
}

function asString(value: unknown, label: string): string {
  if (typeof value !== 'string') {
    throw new Error(`${label} must be a string`);
  }
  return value;
}

function optionalString(value: unknown): string | null {
  return typeof value === 'string' ? value : null;
}

function readCatalogPage(payload: ConformanceGraphqlPayload): CatalogPage {
  if (payload.errors !== undefined) {
    throw new Error(`catalog page returned GraphQL errors: ${JSON.stringify(payload.errors)}`);
  }
  const data = asObject(payload.data, 'catalog page data');
  const connection = asObject(data['standardMetafieldDefinitionTemplates'], 'catalog connection');
  const pageInfo = asObject(connection['pageInfo'], 'catalog pageInfo');
  const edges = asArray(connection['edges'], 'catalog edges').map((value, index) => {
    const edge = asObject(value, `catalog edge ${index}`);
    return {
      cursor: asString(edge['cursor'], `catalog edge ${index} cursor`),
      node: asObject(edge['node'], `catalog edge ${index} node`),
    };
  });
  return {
    edges,
    pageInfo: {
      hasNextPage: pageInfo['hasNextPage'] === true,
      endCursor: typeof pageInfo['endCursor'] === 'string' ? pageInfo['endCursor'] : null,
    },
  };
}

function capture(
  query: string,
  variables: JsonObject,
  result: { status: number; payload: ConformanceGraphqlPayload },
): GraphqlCapture {
  return {
    request: { query, variables },
    status: result.status,
    response: result.payload,
  };
}

function assertSuccessfulCapture(captured: GraphqlCapture, label: string): void {
  if (captured.status < 200 || captured.status >= 300 || captured.response.errors !== undefined) {
    throw new Error(`${label} failed: ${JSON.stringify(captured)}`);
  }
}

async function main(): Promise<void> {
  const config = readConformanceScriptConfig({ exitOnMissing: true });
  if (!['2025-01', '2026-04'].includes(config.apiVersion)) {
    throw new Error(`This recorder only supports 2025-01 and 2026-04, received ${config.apiVersion}`);
  }
  const token = await getValidConformanceAccessToken({
    adminOrigin: config.adminOrigin,
    apiVersion: config.apiVersion,
  });
  const { runGraphqlRaw } = createAdminGraphqlClient({
    adminOrigin: config.adminOrigin,
    apiVersion: config.apiVersion,
    headers: buildAdminAuthHeaders(token),
  });
  const [colorPatternEnableMutation, colorPatternReadQuery] = await Promise.all([
    readFile('config/parity-requests/metafields/standard-definition-template-color-pattern-enable.graphql', 'utf8'),
    readFile('config/parity-requests/metafields/standard-definition-template-color-pattern-read.graphql', 'utf8'),
  ]);

  async function captureCatalog(
    query: string,
    baseVariables: JsonObject,
  ): Promise<{ edges: CatalogEdge[]; requests: Array<{ query: string; variables: JsonObject }> }> {
    const edges: CatalogEdge[] = [];
    const requests: Array<{ query: string; variables: JsonObject }> = [];
    let after: string | null = null;
    for (let pageNumber = 0; pageNumber < 100; pageNumber += 1) {
      const variables = { ...baseVariables, first: 250, after };
      requests.push({ query, variables });
      const result = await runGraphqlRaw(query, variables);
      if (result.status < 200 || result.status >= 300) {
        throw new Error(`catalog page ${pageNumber} returned HTTP ${result.status}`);
      }
      const page = readCatalogPage(result.payload);
      edges.push(...page.edges);
      if (!page.pageInfo.hasNextPage) {
        return { edges, requests };
      }
      if (page.pageInfo.endCursor === null || page.pageInfo.endCursor === after) {
        throw new Error(`catalog page ${pageNumber} did not advance its cursor`);
      }
      after = page.pageInfo.endCursor;
    }
    throw new Error('catalog pagination exceeded 100 pages');
  }

  const allCatalog = await captureCatalog(catalogPageQuery, {});
  const availableCatalog = await captureCatalog(catalogIdsPageQuery, {
    excludeActivated: true,
    constraintStatus: 'CONSTRAINED_AND_UNCONSTRAINED',
    constraintSubtype: null,
  });
  const constrainedCatalog = await captureCatalog(catalogIdsPageQuery, {
    excludeActivated: false,
    constraintStatus: 'CONSTRAINED_ONLY',
    constraintSubtype: null,
  });
  const subtypeCatalog = await captureCatalog(catalogIdsPageQuery, {
    excludeActivated: false,
    constraintStatus: 'CONSTRAINED_ONLY',
    constraintSubtype: { key: 'category', value: 'aa-2' },
  });

  const scenarioVariables = {
    first: 2,
    after: 'eyJsYXN0X2lkIjoyLCJsYXN0X3ZhbHVlIjoyfQ==',
    last: 1,
    before: 'eyJsYXN0X2lkIjozLCJsYXN0X3ZhbHVlIjozfQ==',
    constraintSubtype: { key: 'category', value: 'aa-2' },
  };
  const scenarioResult = await runGraphqlRaw(catalogScenarioQuery, scenarioVariables);
  const scenario = capture(catalogScenarioQuery, scenarioVariables, scenarioResult);
  assertSuccessfulCapture(scenario, 'catalog scenario');

  const invalidBackwardVariables = { last: 2 };
  const invalidBackwardResult = await runGraphqlRaw(invalidBackwardQuery, invalidBackwardVariables);
  const invalidBackward = capture(invalidBackwardQuery, invalidBackwardVariables, invalidBackwardResult);
  if (invalidBackward.status !== 200 || invalidBackward.response.errors === undefined) {
    throw new Error(`invalid backward pagination did not return GraphQL errors: ${JSON.stringify(invalidBackward)}`);
  }

  const materialEnableResult = await runGraphqlRaw(materialEnableMutation, {});
  const materialEnable = capture(materialEnableMutation, {}, materialEnableResult);
  assertSuccessfulCapture(materialEnable, 'material enable');
  const materialPayload = asObject(
    asObject(materialEnable.response.data, 'material enable data')['standardMetafieldDefinitionEnable'],
    'material enable payload',
  );
  if (asArray(materialPayload['userErrors'], 'material enable userErrors').length !== 0) {
    throw new Error(`material enable returned userErrors: ${JSON.stringify(materialPayload['userErrors'])}`);
  }
  const materialDefinition = asObject(materialPayload['createdDefinition'], 'material createdDefinition');
  const materialId = asString(materialDefinition['id'], 'material definition id');

  const materialReadVariables = { id: materialId };
  const materialReadResult = await runGraphqlRaw(materialReadQuery, materialReadVariables);
  const materialRead = capture(materialReadQuery, materialReadVariables, materialReadResult);
  assertSuccessfulCapture(materialRead, 'material read');

  const colorPatternIdentifier = {
    ownerType: 'PRODUCT',
    namespace: 'shopify',
    key: 'color-pattern',
  };
  const colorPatternBeforeResult = await runGraphqlRaw(definitionByIdentifierQuery, colorPatternIdentifier);
  const colorPatternBefore = capture(definitionByIdentifierQuery, colorPatternIdentifier, colorPatternBeforeResult);
  assertSuccessfulCapture(colorPatternBefore, 'color-pattern definition before enable');
  const colorPatternBeforeData = asObject(colorPatternBefore.response.data, 'color-pattern before data');
  const colorPatternBeforeDefinition = colorPatternBeforeData['metafieldDefinition'];
  const colorPatternBeforeId =
    colorPatternBeforeDefinition === null
      ? null
      : optionalString(asObject(colorPatternBeforeDefinition, 'color-pattern before definition')['id']);

  let colorPatternCreatedId: string | null = null;
  let colorPatternCleanup: GraphqlCapture | null = null;
  try {
    const colorPatternEnableResult = await runGraphqlRaw(colorPatternEnableMutation, {});
    const colorPatternEnable = capture(colorPatternEnableMutation, {}, colorPatternEnableResult);
    assertSuccessfulCapture(colorPatternEnable, 'color-pattern enable');
    const colorPatternPayload = asObject(
      asObject(colorPatternEnable.response.data, 'color-pattern enable data')['standardMetafieldDefinitionEnable'],
      'color-pattern enable payload',
    );
    if (asArray(colorPatternPayload['userErrors'], 'color-pattern enable userErrors').length !== 0) {
      throw new Error(`color-pattern enable returned userErrors: ${JSON.stringify(colorPatternPayload['userErrors'])}`);
    }
    const colorPatternDefinition = asObject(
      colorPatternPayload['createdDefinition'],
      'color-pattern createdDefinition',
    );
    const colorPatternId = asString(colorPatternDefinition['id'], 'color-pattern definition id');
    if (colorPatternBeforeId === null) {
      colorPatternCreatedId = colorPatternId;
    }

    const colorPatternReadVariables = { id: colorPatternId };
    const colorPatternReadResult = await runGraphqlRaw(colorPatternReadQuery, colorPatternReadVariables);
    const colorPatternRead = capture(colorPatternReadQuery, colorPatternReadVariables, colorPatternReadResult);
    assertSuccessfulCapture(colorPatternRead, 'color-pattern read');

    if (colorPatternCreatedId !== null) {
      const cleanupVariables = { id: colorPatternCreatedId };
      const cleanupResult = await runGraphqlRaw(definitionDeleteMutation, cleanupVariables);
      colorPatternCleanup = capture(definitionDeleteMutation, cleanupVariables, cleanupResult);
      assertSuccessfulCapture(colorPatternCleanup, 'color-pattern cleanup');
      const cleanupPayload = asObject(
        asObject(colorPatternCleanup.response.data, 'color-pattern cleanup data')['metafieldDefinitionDelete'],
        'color-pattern cleanup payload',
      );
      if (asArray(cleanupPayload['userErrors'], 'color-pattern cleanup userErrors').length !== 0) {
        throw new Error(`color-pattern cleanup returned userErrors: ${JSON.stringify(cleanupPayload['userErrors'])}`);
      }
      colorPatternCreatedId = null;
    }

    const unknownEnableResult = await runGraphqlRaw(unknownEnableMutation, {});
    const unknownEnable = capture(unknownEnableMutation, {}, unknownEnableResult);
    assertSuccessfulCapture(unknownEnable, 'unknown enable');
    const unknownPayload = asObject(
      asObject(unknownEnable.response.data, 'unknown enable data')['standardMetafieldDefinitionEnable'],
      'unknown enable payload',
    );
    if (asArray(unknownPayload['userErrors'], 'unknown enable userErrors').length === 0) {
      throw new Error('unknown enable unexpectedly returned no userErrors');
    }

    const fixture = {
      kind: 'shopify-standard-definition-template-resolution',
      capturedAt: new Date().toISOString(),
      storeDomain: config.storeDomain,
      apiVersion: config.apiVersion,
      context: {
        eligibility: 'public-admin-context-for-captured-shop',
        secondShopOrFlagAvailable: false,
        limitation:
          'Only the configured public conformance shop was available. No second shop or beta-flag context was synthesized.',
      },
      catalogs: {
        all: allCatalog,
        available: availableCatalog,
        constrained: constrainedCatalog,
        provenConstraintSubtypes: [
          {
            key: 'category',
            value: 'aa-2',
            ...subtypeCatalog,
          },
        ],
      },
      captures: {
        scenario,
        invalidBackward,
        materialEnable,
        materialRead,
        colorPatternBefore,
        colorPatternEnable,
        colorPatternRead,
        colorPatternCleanup,
        unknownEnable,
      },
      upstreamCalls: [],
    };

    const outputDirectory = path.join('fixtures', 'conformance', config.storeDomain, config.apiVersion, 'metafields');
    await mkdir(outputDirectory, { recursive: true });
    const outputPath = path.join(outputDirectory, 'standard-definition-template-resolution.json');
    await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
    console.log(`Wrote ${outputPath} (${allCatalog.edges.length} templates)`);
  } finally {
    if (colorPatternCreatedId !== null) {
      const cleanupResult = await runGraphqlRaw(definitionDeleteMutation, { id: colorPatternCreatedId });
      if (cleanupResult.status < 200 || cleanupResult.status >= 300 || cleanupResult.payload.errors !== undefined) {
        console.warn(`Failed to clean up color-pattern definition ${colorPatternCreatedId}`);
      }
    }
  }
}

await main();
