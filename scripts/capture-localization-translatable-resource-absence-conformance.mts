/* oxlint-disable no-console -- CLI capture script intentionally writes status output. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlPayload } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

const scenarioId = 'localization-translatable-resource-absence';
const requestPath = 'config/parity-requests/localization/localization-translatable-resource-absence.graphql';

function isRecord(value: unknown): value is JsonRecord {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function dataObject(payload: ConformanceGraphqlPayload<unknown>): JsonRecord {
  if (!isRecord(payload.data)) {
    throw new Error(`Expected GraphQL data object, got ${JSON.stringify(payload)}`);
  }
  return payload.data;
}

function connectionObject(data: JsonRecord, fieldName: string): JsonRecord {
  const connection = data[fieldName];
  if (!isRecord(connection)) {
    throw new Error(`Expected data.${fieldName} connection object, got ${JSON.stringify(data)}`);
  }
  return connection;
}

function assertEmptyConnection(connection: JsonRecord, context: string): void {
  if (!Array.isArray(connection['nodes']) || connection['nodes'].length !== 0) {
    throw new Error(`${context} expected empty nodes, got ${JSON.stringify(connection)}`);
  }
  if (!Array.isArray(connection['edges']) || connection['edges'].length !== 0) {
    throw new Error(`${context} expected empty edges, got ${JSON.stringify(connection)}`);
  }
  const pageInfo = connection['pageInfo'];
  if (
    !isRecord(pageInfo) ||
    pageInfo['hasNextPage'] !== false ||
    pageInfo['hasPreviousPage'] !== false ||
    pageInfo['startCursor'] !== null ||
    pageInfo['endCursor'] !== null
  ) {
    throw new Error(`${context} expected empty pageInfo, got ${JSON.stringify(connection)}`);
  }
}

async function main(): Promise<void> {
  const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
  if (apiVersion !== '2026-04') {
    throw new Error(`Expected SHOPIFY_CONFORMANCE_API_VERSION=2026-04, got ${apiVersion}`);
  }
  const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
  const { runGraphql } = createAdminGraphqlClient({
    adminOrigin,
    apiVersion,
    headers: buildAdminAuthHeaders(adminAccessToken),
  });
  const query = await readFile(requestPath, 'utf8');
  const variables = {
    first: 5,
    resourceType: 'ARTICLE',
    resourceIds: ['gid://shopify/Product/1'],
    missingResourceId: 'gid://shopify/Product/1',
  };
  const response = await runGraphql(query, variables);
  const data = dataObject(response);

  assertEmptyConnection(connectionObject(data, 'resources'), 'translatableResources ARTICLE');
  assertEmptyConnection(connectionObject(data, 'byIds'), 'translatableResourcesByIds missing product');
  if (data['missing'] !== null) {
    throw new Error(`Expected missing translatableResource to be null, got ${JSON.stringify(data['missing'])}`);
  }

  const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'localization');
  const outputPath = path.join(outputDir, `${scenarioId}.json`);
  const capture = {
    scenarioId,
    storeDomain,
    apiVersion,
    capturedAt: new Date().toISOString(),
    read: {
      request: { variables },
      response,
    },
    upstreamCalls: [
      {
        operationName: 'LocalizationTranslatableResourceAbsence',
        variables,
        query,
        response: {
          status: 200,
          body: response,
        },
      },
    ],
  };

  await mkdir(outputDir, { recursive: true });
  await writeFile(outputPath, `${JSON.stringify(capture, null, 2)}\n`, 'utf8');
  console.log(JSON.stringify({ ok: true, outputPath, storeDomain, apiVersion, variables }, null, 2));
}

await main();
