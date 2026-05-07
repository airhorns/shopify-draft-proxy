/* oxlint-disable no-console -- CLI capture scripts intentionally write status output. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import {
  createAdminGraphqlClient,
  type AdminGraphqlClient,
  type ConformanceGraphqlPayload,
  type ConformanceGraphqlResult,
} from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;
type Capture = {
  request: {
    query: string;
    variables: JsonRecord;
  };
  response: ConformanceGraphqlPayload;
};

const legacyApiVersion = '2025-01';
const currentApiVersion = '2026-04';
const { storeDomain, adminOrigin } = readConformanceScriptConfig({
  defaultApiVersion: legacyApiVersion,
  exitOnMissing: true,
});

async function readText(filePath: string): Promise<string> {
  return await readFile(filePath, 'utf8');
}

async function readJson(filePath: string): Promise<JsonRecord> {
  return JSON.parse(await readText(filePath)) as JsonRecord;
}

function isRecord(value: unknown): value is JsonRecord {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function readPath(value: unknown, segments: string[]): unknown {
  let current = value;
  for (const segment of segments) {
    if (Array.isArray(current)) {
      const index = Number.parseInt(segment, 10);
      current = Number.isNaN(index) ? undefined : current[index];
    } else if (isRecord(current)) {
      current = current[segment];
    } else {
      return undefined;
    }
  }
  return current;
}

function readStringPath(value: unknown, segments: string[], context: string): string {
  const pathValue = readPath(value, segments);
  if (typeof pathValue !== 'string' || pathValue.length === 0) {
    throw new Error(`${context} did not return a string at ${segments.join('.')}: ${JSON.stringify(value, null, 2)}`);
  }
  return pathValue;
}

function readArrayPath(value: unknown, segments: string[], context: string): unknown[] {
  const pathValue = readPath(value, segments);
  if (!Array.isArray(pathValue)) {
    throw new Error(`${context} did not return an array at ${segments.join('.')}: ${JSON.stringify(value, null, 2)}`);
  }
  return pathValue;
}

function assertGraphqlOk(result: ConformanceGraphqlResult, context: string): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${context} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function assertNoUserErrors(payload: unknown, segments: string[], context: string): void {
  const userErrors = readPath(payload, segments);
  if (!Array.isArray(userErrors) || userErrors.length !== 0) {
    throw new Error(`${context} returned userErrors: ${JSON.stringify(userErrors, null, 2)}`);
  }
}

function assertHasUserErrors(payload: unknown, segments: string[], context: string): void {
  const userErrors = readPath(payload, segments);
  if (!Array.isArray(userErrors) || userErrors.length === 0) {
    throw new Error(`${context} did not return userErrors: ${JSON.stringify(payload, null, 2)}`);
  }
}

async function makeClient(apiVersion: string): Promise<AdminGraphqlClient> {
  const token = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
  return createAdminGraphqlClient({
    adminOrigin,
    apiVersion,
    headers: buildAdminAuthHeaders(token),
  });
}

async function captureGraphql(
  client: AdminGraphqlClient,
  query: string,
  variables: JsonRecord,
  context: string,
): Promise<Capture> {
  const response = await client.runGraphqlRequest(query, variables);
  assertGraphqlOk(response, context);
  return {
    request: { query, variables },
    response: response.payload,
  };
}

async function writeJson(outputPath: string, body: JsonRecord): Promise<void> {
  await mkdir(path.dirname(outputPath), { recursive: true });
  await writeFile(outputPath, `${JSON.stringify(body, null, 2)}\n`, 'utf8');
}

async function writeParityVariables(outputPath: string, body: JsonRecord): Promise<void> {
  await writeFile(outputPath, `${JSON.stringify(body, null, 2)}\n`, 'utf8');
}

function fixturePath(apiVersion: string, domain: string, filename: string): string {
  return path.join('fixtures', 'conformance', storeDomain, apiVersion, domain, filename);
}

function upstreamCall(
  operationName: string,
  query: string,
  variables: JsonRecord,
  response: ConformanceGraphqlPayload,
) {
  return {
    operationName,
    variables,
    query,
    response: {
      status: 200,
      body: response,
    },
  };
}

async function captureB2BMutationValidation(client: AdminGraphqlClient): Promise<string> {
  const query = await readText('config/parity-requests/b2b/b2b-company-mutation-validation.graphql');
  const variables = {
    companyId: 'gid://shopify/Company/999999999999',
    companyLocationId: 'gid://shopify/CompanyLocation/999999999999',
    companyContactId: 'gid://shopify/CompanyContact/999999999999',
  };
  const capture = await captureGraphql(client, query, variables, 'B2B mutation validation');
  assertHasUserErrors(capture.response, ['data', 'companyUpdate', 'userErrors'], 'companyUpdate not-found');
  assertHasUserErrors(
    capture.response,
    ['data', 'companyLocationUpdate', 'userErrors'],
    'companyLocationUpdate not-found',
  );
  assertHasUserErrors(
    capture.response,
    ['data', 'companyContactUpdate', 'userErrors'],
    'companyContactUpdate not-found',
  );

  const outputPath = fixturePath(legacyApiVersion, 'b2b', 'b2b-company-mutation-validation.json');
  await writeJson(outputPath, {
    scenarioId: 'b2b-company-mutation-validation',
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion: legacyApiVersion,
    mutationValidation: capture,
    upstreamCalls: [],
  });
  return outputPath;
}

async function captureB2BRootsRead(client: AdminGraphqlClient): Promise<string> {
  const setupQuery = `#graphql
    mutation B2BRootsReadSetup($input: CompanyCreateInput!) {
      companyCreate(input: $input) {
        company {
          id
          name
          mainContact { id }
          contactRoles(first: 5) { nodes { id name } }
          locations(first: 5) { nodes { id name } }
        }
        userErrors { field message code }
      }
    }
  `;
  const deleteQuery = `#graphql
    mutation B2BRootsReadCleanup($id: ID!) {
      companyDelete(id: $id) {
        deletedCompanyId
        userErrors { field message code }
      }
    }
  `;
  const readQuery = await readText('config/parity-requests/b2b/b2b-company-roots-read.graphql');
  const timestamp = Date.now();
  const setupVariables = {
    input: {
      company: {
        name: `Recorder B2B roots ${timestamp}`,
        note: 'B2B roots read recorder fixture',
        externalId: `recorder-b2b-roots-${timestamp}`,
      },
      companyContact: {
        firstName: 'Recorder',
        lastName: 'Contact',
        email: `recorder-b2b-roots-${timestamp}@example.com`,
        title: 'Buyer',
      },
      companyLocation: {
        name: `Recorder B2B roots ${timestamp} HQ`,
        phone: '+16135550101',
        billingAddress: {
          address1: '1 Capture Way',
          city: 'Ottawa',
          countryCode: 'CA',
        },
      },
    },
  };
  let companyId: string | null = null;
  try {
    const setup = await captureGraphql(client, setupQuery, setupVariables, 'B2B roots setup');
    assertNoUserErrors(setup.response, ['data', 'companyCreate', 'userErrors'], 'B2B roots setup');
    companyId = readStringPath(setup.response, ['data', 'companyCreate', 'company', 'id'], 'B2B roots setup');
    const contactId = readStringPath(
      setup.response,
      ['data', 'companyCreate', 'company', 'mainContact', 'id'],
      'B2B roots setup contact',
    );
    const roleId = readStringPath(
      setup.response,
      ['data', 'companyCreate', 'company', 'contactRoles', 'nodes', '0', 'id'],
      'B2B roots setup role',
    );
    const locationId = readStringPath(
      setup.response,
      ['data', 'companyCreate', 'company', 'locations', 'nodes', '0', 'id'],
      'B2B roots setup location',
    );
    const variables = {
      first: 5,
      companyId,
      contactId,
      roleId,
      locationId,
      unknownCompanyId: 'gid://shopify/Company/999999999999',
      unknownContactId: 'gid://shopify/CompanyContact/999999999999',
      unknownRoleId: 'gid://shopify/CompanyContactRole/999999999999',
      unknownLocationId: 'gid://shopify/CompanyLocation/999999999999',
      companyQuery: `name:"Recorder B2B roots ${timestamp}"`,
    };
    const readCapture = await captureGraphql(client, readQuery, variables, 'B2B roots read');
    await writeParityVariables('config/parity-requests/b2b/b2b-company-roots-read.variables.json', variables);
    const cleanup = await captureGraphql(client, deleteQuery, { id: companyId }, 'B2B roots cleanup');
    companyId = null;

    const outputPath = fixturePath(legacyApiVersion, 'b2b', 'b2b-company-roots-read.json');
    await writeJson(outputPath, {
      scenarioId: 'b2b-company-roots-read',
      capturedAt: new Date().toISOString(),
      storeDomain,
      apiVersion: legacyApiVersion,
      variables,
      data: readCapture.response.data,
      cleanup,
      upstreamCalls: [upstreamCall('B2BCompanyRootsRead', readQuery, variables, readCapture.response)],
    });
    return outputPath;
  } finally {
    if (companyId) {
      await client.runGraphqlRequest(deleteQuery, { id: companyId });
    }
  }
}

async function captureMetafieldDefinitionProductRead(client: AdminGraphqlClient): Promise<string> {
  const query = await readText('config/parity-requests/metafields/metafield-definitions-product-read.graphql');
  const variables = await readJson(
    'config/parity-requests/metafields/metafield-definitions-product-read.variables.json',
  );
  const capture = await captureGraphql(client, query, variables, 'metafield definitions product read');
  const outputPath = fixturePath(legacyApiVersion, 'metafields', 'metafield-definitions-product-read.json');
  await writeJson(outputPath, {
    scenarioId: 'metafield-definitions-product-read',
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion: legacyApiVersion,
    variables,
    response: capture.response,
    upstreamCalls: [upstreamCall('MetafieldDefinitionsProductRead', query, variables, capture.response)],
  });
  return outputPath;
}

async function captureSavedSearchUrlRedirects(client: AdminGraphqlClient): Promise<string> {
  const createQuery = await readText('config/parity-requests/saved-searches/saved-search-local-staging-create.graphql');
  const readQuery = await readText(
    'config/parity-requests/saved-searches/saved-search-local-staging-read-after-create.graphql',
  );
  const updateQuery = await readText(
    'config/parity-requests/saved-searches/saved-search-local-staging-update-too-long-name.graphql',
  );
  const missingQuery = await readText(
    'config/parity-requests/saved-searches/saved-search-local-staging-missing.graphql',
  );
  const deleteQuery = await readText(
    'config/parity-requests/saved-searches/saved-search-delete-shop-payload-delete.graphql',
  );
  const timestamp = Date.now();
  const createVariables = {
    input: {
      resourceType: 'PRODUCT',
      name: `Recorder Product Search ${timestamp}`,
      query: `title:Recorder ${timestamp}`,
    },
  };
  let savedSearchId: string | null = null;
  try {
    const savedSearchCreateProduct = await captureGraphql(client, createQuery, createVariables, 'saved search create');
    assertNoUserErrors(
      savedSearchCreateProduct.response,
      ['data', 'savedSearchCreate', 'userErrors'],
      'saved search create',
    );
    savedSearchId = readStringPath(
      savedSearchCreateProduct.response,
      ['data', 'savedSearchCreate', 'savedSearch', 'id'],
      'saved search create',
    );
    const readVariables = {};
    const productSavedSearchesAfterCreate = await captureGraphql(
      client,
      readQuery,
      readVariables,
      'saved search read after create',
    );
    const updateVariables = {
      input: {
        id: savedSearchId,
        name: 'Recorder Product Search Name That Is Too Long For Shopify',
        query: `status:ACTIVE ${timestamp}`,
      },
    };
    const savedSearchUpdateTooLongName = await captureGraphql(
      client,
      updateQuery,
      updateVariables,
      'saved search update too long name',
    );
    assertHasUserErrors(
      savedSearchUpdateTooLongName.response,
      ['data', 'savedSearchUpdate', 'userErrors'],
      'saved search update too long name',
    );
    const missingVariables = {
      input: { id: 'gid://shopify/SavedSearch/0', name: 'Missing' },
      deleteInput: { id: 'gid://shopify/SavedSearch/0' },
    };
    const missingSavedSearch = await captureGraphql(client, missingQuery, missingVariables, 'saved search missing');
    const cleanup = await captureGraphql(client, deleteQuery, { input: { id: savedSearchId } }, 'saved search cleanup');
    savedSearchId = null;

    const outputPath = fixturePath(legacyApiVersion, 'saved-searches', 'saved-search-url-redirects.json');
    await writeJson(outputPath, {
      scenarioId: 'saved-search-local-staging',
      capturedAt: new Date().toISOString(),
      storeDomain,
      apiVersion: legacyApiVersion,
      notes: [
        'SavedSearch create/read/update-validation/delete behavior was recorded from live Shopify.',
        'URL redirect saved-search roots remain documented as access-scope blockers for the current conformance token.',
      ],
      savedSearchCreateProduct: {
        variables: createVariables,
        payload: savedSearchCreateProduct.response,
      },
      productSavedSearchesAfterCreate: {
        request: { variables: readVariables },
        payload: productSavedSearchesAfterCreate.response,
      },
      savedSearchUpdateTooLongName: {
        request: { variables: updateVariables },
        payload: savedSearchUpdateTooLongName.response,
      },
      missingSavedSearch: {
        request: { variables: missingVariables },
        payload: missingSavedSearch.response,
      },
      cleanup,
      urlRedirectBlockers: {
        readOnlineStoreNavigation: [
          {
            root: 'urlRedirectSavedSearches',
            message:
              'Access denied for urlRedirectSavedSearches field. Required access: `read_online_store_navigation` access scope.',
          },
          {
            root: 'urlRedirectsCount',
            message:
              'Access denied for urlRedirectsCount field. Required access: `read_online_store_navigation` access scope.',
          },
          { root: 'urlRedirects', message: 'Access denied for urlRedirects field.' },
        ],
        writeOnlineStoreNavigation: [
          {
            root: 'urlRedirectCreate',
            message:
              'Access denied for urlRedirectCreate field. Required access: `write_online_store_navigation` access scope.',
          },
          {
            root: 'urlRedirectImportCreate',
            message:
              'Access denied for urlRedirectImportCreate field. Required access: `write_online_store_navigation` access scope.',
          },
        ],
      },
      upstreamCalls: [],
    });
    return outputPath;
  } finally {
    if (savedSearchId) {
      await client.runGraphqlRequest(deleteQuery, { input: { id: savedSearchId } });
    }
  }
}

async function captureSegmentLifecycleValidation(client: AdminGraphqlClient): Promise<string> {
  const createQuery = await readText('config/parity-requests/segments/segment-create-invalid-query-validation.graphql');
  const updateQuery = await readText('config/parity-requests/segments/segment-update-unknown-id-validation.graphql');
  const deleteQuery = await readText('config/parity-requests/segments/segment-delete-unknown-id-validation.graphql');
  const blankNameQuery = `#graphql
    mutation SegmentCreateBlankName($name: String!, $query: String!) {
      segmentCreate(name: $name, query: $query) {
        segment { id name query creationDate lastEditDate }
        userErrors { field message }
      }
    }
  `;
  const cases = [
    {
      name: 'segmentCreateBlankName',
      ...(await captureGraphql(
        client,
        blankNameQuery,
        { name: '', query: "email_subscription_status = 'SUBSCRIBED'" },
        'segment blank name',
      )),
    },
    {
      name: 'segmentCreateInvalidQuery',
      ...(await captureGraphql(
        client,
        createQuery,
        { name: 'Invalid query', query: 'not a valid segment query ???' },
        'segment invalid query',
      )),
    },
    {
      name: 'segmentUpdateUnknownId',
      ...(await captureGraphql(
        client,
        updateQuery,
        { id: 'gid://shopify/Segment/999999999999', name: 'Nope' },
        'segment update unknown id',
      )),
    },
    {
      name: 'segmentDeleteUnknownId',
      ...(await captureGraphql(
        client,
        deleteQuery,
        { id: 'gid://shopify/Segment/999999999999' },
        'segment delete unknown id',
      )),
    },
  ];
  for (const item of cases) {
    assertHasUserErrors(
      item.response,
      ['data', Object.keys((item.response.data as JsonRecord) ?? {})[0] ?? '', 'userErrors'],
      item.name,
    );
  }
  const outputPath = fixturePath(legacyApiVersion, 'segments', 'segment-lifecycle-validation.json');
  await writeJson(outputPath, {
    scenarioId: 'segment-lifecycle-validation',
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion: legacyApiVersion,
    cases: cases.map((item) => ({
      name: item.name,
      query: item.request.query,
      variables: item.request.variables,
      response: { status: 200, payload: item.response },
    })),
    upstreamCalls: [],
  });
  return outputPath;
}

function findProductTitleDigest(readCapturePayload: ConformanceGraphqlPayload): { resourceId: string; digest: string } {
  const nodes = readArrayPath(readCapturePayload, ['data', 'resources', 'nodes'], 'localization resources');
  for (const node of nodes) {
    if (!isRecord(node) || typeof node['resourceId'] !== 'string') continue;
    const content = Array.isArray(node['translatableContent']) ? node['translatableContent'] : [];
    for (const item of content) {
      if (isRecord(item) && item['key'] === 'title' && typeof item['digest'] === 'string') {
        return { resourceId: node['resourceId'], digest: item['digest'] };
      }
    }
  }
  throw new Error('Could not find a product title digest in localization read capture.');
}

async function disableFrenchLocaleIfEnabled(client: AdminGraphqlClient, disableQuery: string): Promise<void> {
  const response = await client.runGraphqlRequest(disableQuery, { locale: 'fr' });
  if (response.status < 200 || response.status >= 300 || response.payload.errors) {
    throw new Error(`French locale cleanup failed: ${JSON.stringify(response, null, 2)}`);
  }
}

async function captureLocalizationLocaleTranslation(client: AdminGraphqlClient): Promise<string> {
  const readQuery = await readText('config/parity-requests/localization/localization-locale-translation-read.graphql');
  const unknownResourceQuery = await readText(
    'config/parity-requests/localization/localization-unknown-resource-validation.graphql',
  );
  const enableQuery = await readText('config/parity-requests/localization/localization-shop-locale-enable.graphql');
  const updateQuery = await readText('config/parity-requests/localization/localization-shop-locale-update.graphql');
  const registerQuery = await readText(
    'config/parity-requests/localization/localization-translations-register.graphql',
  );
  const translationsReadQuery = await readText(
    'config/parity-requests/localization/localization-translations-read.graphql',
  );
  const removeQuery = await readText('config/parity-requests/localization/localization-translations-remove.graphql');
  const disableQuery = await readText('config/parity-requests/localization/localization-shop-locale-disable.graphql');
  await disableFrenchLocaleIfEnabled(client, disableQuery);

  const readVariables = {
    first: 3,
    resourceType: 'PRODUCT',
    ids: ['gid://shopify/Product/999999999999999'],
  };
  const readCapture = await captureGraphql(client, readQuery, readVariables, 'localization read');
  const { resourceId, digest } = findProductTitleDigest(readCapture.response);
  const unknownResourceValidation = await captureGraphql(
    client,
    unknownResourceQuery,
    {
      resourceId: 'gid://shopify/Product/999999999999999',
      translations: [
        {
          locale: 'fr',
          key: 'title',
          value: 'Missing product title',
          translatableContentDigest: 'missing-digest',
        },
      ],
      keys: ['title'],
      locales: ['fr'],
    },
    'localization unknown resource validation',
  );
  const translationValue = `Recorder title ${Date.now()}`;
  let disabled = false;
  try {
    const enable = await captureGraphql(client, enableQuery, { locale: 'fr' }, 'shop locale enable');
    const update = await captureGraphql(
      client,
      updateQuery,
      { locale: 'fr', shopLocale: { published: false } },
      'shop locale update',
    );
    const registerVariables = {
      resourceId,
      translations: [
        {
          locale: 'fr',
          key: 'title',
          value: translationValue,
          translatableContentDigest: digest,
        },
      ],
    };
    const register = await captureGraphql(client, registerQuery, registerVariables, 'translations register');
    const downstreamRegistered = await captureGraphql(
      client,
      translationsReadQuery,
      { resourceId },
      'translations downstream registered',
    );
    const removeVariables = { resourceId, keys: ['title'], locales: ['fr'] };
    const remove = await captureGraphql(client, removeQuery, removeVariables, 'translations remove');
    const downstreamRemoved = await captureGraphql(
      client,
      translationsReadQuery,
      { resourceId },
      'translations downstream removed',
    );
    const disable = await captureGraphql(client, disableQuery, { locale: 'fr' }, 'shop locale disable');
    disabled = true;
    const outputPath = fixturePath(currentApiVersion, 'localization', 'localization-locale-translation-fixture.json');
    await writeJson(outputPath, {
      scenarioId: 'localization-locale-translation-fixture',
      capturedAt: new Date().toISOString(),
      storeDomain,
      apiVersion: currentApiVersion,
      readCapture: {
        request: { variables: readVariables },
        response: readCapture.response,
      },
      unknownResourceValidation: {
        variables: unknownResourceValidation.request.variables,
        response: unknownResourceValidation.response,
      },
      shopLocaleLifecycle: {
        enable: readPath(enable.response, ['data', 'shopLocaleEnable']),
        update: readPath(update.response, ['data', 'shopLocaleUpdate']),
        disable: readPath(disable.response, ['data', 'shopLocaleDisable']),
      },
      translationLifecycle: {
        resourceId,
        titleDigest: digest,
        registerRequest: { variables: registerVariables },
        register: readPath(register.response, ['data', 'translationsRegister']),
        downstreamRegistered: readPath(downstreamRegistered.response, ['data', 'translatableResource']),
        removeRequest: { variables: removeVariables },
        remove: readPath(remove.response, ['data', 'translationsRemove']),
        downstreamRemoved: readPath(downstreamRemoved.response, ['data', 'translatableResource']),
      },
      upstreamCalls: [
        upstreamCall('LocalizationLocaleTranslationRead', readQuery, readVariables, readCapture.response),
      ],
    });
    return outputPath;
  } finally {
    if (!disabled) {
      await disableFrenchLocaleIfEnabled(client, disableQuery);
    }
  }
}

async function captureB2BCompanyCreateLifecycle(client: AdminGraphqlClient): Promise<string> {
  const createQuery = await readText('config/parity-requests/b2b/b2b-company-create-lifecycle.graphql');
  const readQuery = await readText('config/parity-requests/b2b/b2b-company-create-lifecycle-read.graphql');
  const deleteQuery = `#graphql
    mutation B2BCompanyCreateLifecycleCleanup($id: ID!) {
      companyDelete(id: $id) { deletedCompanyId userErrors { field message code } }
    }
  `;
  const timestamp = Date.now();
  const variables = {
    input: {
      company: {
        name: `Recorder B2B lifecycle ${timestamp}`,
        externalId: `recorder-b2b-lifecycle-${timestamp}`,
      },
    },
  };
  let companyId: string | null = null;
  try {
    const companyCreate = await captureGraphql(client, createQuery, variables, 'B2B companyCreate lifecycle');
    assertNoUserErrors(companyCreate.response, ['data', 'companyCreate', 'userErrors'], 'B2B companyCreate lifecycle');
    companyId = readStringPath(companyCreate.response, ['data', 'companyCreate', 'company', 'id'], 'companyCreate');
    const downstreamRead = await captureGraphql(client, readQuery, { companyId }, 'B2B companyCreate downstream read');
    const cleanup = await captureGraphql(client, deleteQuery, { id: companyId }, 'B2B companyCreate cleanup');
    companyId = null;
    const outputPath = fixturePath(currentApiVersion, 'b2b', 'b2b-company-create-lifecycle.json');
    await writeJson(outputPath, {
      scenarioId: 'b2b-company-create-lifecycle',
      capturedAt: new Date().toISOString(),
      apiVersion: currentApiVersion,
      storeDomain,
      setup: {
        plan: 'Create a disposable B2B company, record immediate downstream read, then delete the company.',
      },
      companyCreate,
      downstreamRead,
      cleanup,
      upstreamCalls: [],
    });
    return outputPath;
  } finally {
    if (companyId) {
      await client.runGraphqlRequest(deleteQuery, { id: companyId });
    }
  }
}

async function captureMetaobjectCreateColdHydration(client: AdminGraphqlClient): Promise<string> {
  const definitionCreateQuery = `#graphql
    mutation MetaobjectColdDefinitionCreate($definition: MetaobjectDefinitionCreateInput!) {
      metaobjectDefinitionCreate(definition: $definition) {
        metaobjectDefinition { id type }
        userErrors { field message code }
      }
    }
  `;
  const hydrateQuery =
    'query MetaobjectDefinitionHydrateByType($type: String!) { metaobjectDefinitionByType(type: $type) { id type name description displayNameKey access { admin storefront } capabilities { publishable { enabled } translatable { enabled } renderable { enabled } onlineStore { enabled } } fieldDefinitions { key name description required type { name category } capabilities { adminFilterable { enabled } } validations { name value } } hasThumbnailField metaobjectsCount standardTemplate { type name } createdAt updatedAt } }';
  const createQuery = await readText(
    'config/parity-requests/metaobjects/metaobject-create-cold-hydration-create.graphql',
  );
  const metaobjectDeleteQuery = `#graphql
    mutation MetaobjectColdDelete($id: ID!) {
      metaobjectDelete(id: $id) { deletedId userErrors { field message code elementKey elementIndex } }
    }
  `;
  const definitionDeleteQuery = `#graphql
    mutation MetaobjectColdDefinitionDelete($id: ID!) {
      metaobjectDefinitionDelete(id: $id) { deletedId userErrors { field message code elementKey elementIndex } }
    }
  `;
  const timestamp = Date.now();
  const type = `recorder_cold_${timestamp}`;
  let definitionId: string | null = null;
  let metaobjectId: string | null = null;
  try {
    const definitionCreate = await captureGraphql(
      client,
      definitionCreateQuery,
      {
        definition: {
          type,
          name: `Recorder cold ${timestamp}`,
          displayNameKey: 'title',
          capabilities: { publishable: { enabled: true } },
          fieldDefinitions: [
            {
              key: 'title',
              name: 'Title',
              type: 'single_line_text_field',
              required: true,
            },
            {
              key: 'body',
              name: 'Body',
              type: 'multi_line_text_field',
              required: false,
            },
          ],
        },
      },
      'metaobject cold definition create',
    );
    assertNoUserErrors(
      definitionCreate.response,
      ['data', 'metaobjectDefinitionCreate', 'userErrors'],
      'metaobject cold definition create',
    );
    definitionId = readStringPath(
      definitionCreate.response,
      ['data', 'metaobjectDefinitionCreate', 'metaobjectDefinition', 'id'],
      'metaobject cold definition create',
    );
    const createVariables = {
      metaobject: {
        type,
        handle: `recorder-cold-${timestamp}`,
        capabilities: { publishable: { status: 'ACTIVE' } },
        fields: [
          { key: 'title', value: `Recorder cold title ${timestamp}` },
          { key: 'body', value: `Recorder cold body ${timestamp}` },
        ],
      },
    };
    const create = await captureGraphql(client, createQuery, createVariables, 'metaobject cold create');
    assertNoUserErrors(create.response, ['data', 'metaobjectCreate', 'userErrors'], 'metaobject cold create');
    metaobjectId = readStringPath(
      create.response,
      ['data', 'metaobjectCreate', 'metaobject', 'id'],
      'metaobject cold create',
    );
    const hydrate = await captureGraphql(client, hydrateQuery, { type }, 'metaobject cold hydrate');
    const cleanupMetaobject = await captureGraphql(
      client,
      metaobjectDeleteQuery,
      { id: metaobjectId },
      'metaobject cold delete',
    );
    metaobjectId = null;
    const cleanupDefinition = await captureGraphql(
      client,
      definitionDeleteQuery,
      { id: definitionId },
      'metaobject cold definition delete',
    );
    definitionId = null;

    const outputPath = fixturePath(currentApiVersion, 'metaobjects', 'metaobject-create-cold-hydration.json');
    await writeJson(outputPath, {
      scenarioId: 'metaobject-create-cold-hydration',
      apiVersion: currentApiVersion,
      storeDomain,
      capturedAt: new Date().toISOString(),
      setup: [
        {
          name: 'create-metaobject-cold',
          request: create.request,
          response: create.response,
        },
      ],
      cleanup: { metaobject: cleanupMetaobject, definition: cleanupDefinition },
      upstreamCalls: [upstreamCall('MetaobjectDefinitionHydrateByType', hydrateQuery, { type }, hydrate.response)],
    });
    return outputPath;
  } finally {
    if (metaobjectId) {
      await client.runGraphqlRequest(metaobjectDeleteQuery, { id: metaobjectId });
    }
    if (definitionId) {
      await client.runGraphqlRequest(definitionDeleteQuery, { id: definitionId });
    }
  }
}

const legacyClient = await makeClient(legacyApiVersion);
const currentClient = await makeClient(currentApiVersion);
const outputPaths = [
  await captureB2BMutationValidation(legacyClient),
  await captureB2BRootsRead(legacyClient),
  await captureMetafieldDefinitionProductRead(legacyClient),
  await captureSavedSearchUrlRedirects(legacyClient),
  await captureSegmentLifecycleValidation(legacyClient),
  await captureB2BCompanyCreateLifecycle(currentClient),
  await captureLocalizationLocaleTranslation(currentClient),
  await captureMetaobjectCreateColdHydration(currentClient),
];

console.log(JSON.stringify({ ok: true, outputPaths }, null, 2));
