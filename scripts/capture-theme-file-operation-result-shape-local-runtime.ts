/* oxlint-disable no-console -- Capture scripts intentionally write status output to stdio. */
import { spawnSync } from 'node:child_process';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';
import { createDraftProxy, type DraftProxyHttpResponse } from '../js/src/index.js';

type JsonRecord = Record<string, unknown>;

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, '..');
const apiVersion = '2026-04';
const scenarioId = 'online-store/theme-file-operation-result-shape';
const requestPath = path.join(
  repoRoot,
  'config',
  'parity-requests',
  'online-store',
  'theme-file-operation-result-shape.graphql',
);
const fixturePath = path.join(
  repoRoot,
  'fixtures',
  'conformance',
  'local-runtime',
  apiVersion,
  'online-store',
  'theme-file-operation-result-shape.json',
);
const specPath = path.join(
  repoRoot,
  'config',
  'parity-specs',
  'online-store',
  'theme-file-operation-result-shape.json',
);

const generatedQuery = `mutation RustOnlineStoreThemeFileLocalRuntimeResultShape {
  themeCreate(source: "https://example.com/theme-file-result-shape.zip", name: "Theme file result shape") {
    theme {
      id
    }
    userErrors {
      field
      message
      code
    }
  }
  upsert: themeFilesUpsert(
    themeId: "gid://shopify/OnlineStoreTheme/1?shopify-draft-proxy=synthetic"
    files: [{ filename: "assets/app.js", body: { type: TEXT, value: "console.log(1)" } }]
  ) {
    upsertedThemeFiles {
      filename
      createdAt
      updatedAt
      size
      checksumMd5
      body {
        ... on OnlineStoreThemeFileBodyText {
          content
        }
      }
    }
    userErrors {
      field
      message
      code
    }
  }
  copy: themeFilesCopy(
    themeId: "gid://shopify/OnlineStoreTheme/1?shopify-draft-proxy=synthetic"
    files: [{ srcFilename: "assets/app.js", dstFilename: "assets/app-copy.js" }]
  ) {
    copiedThemeFiles {
      filename
      createdAt
      updatedAt
      size
      checksumMd5
      body {
        ... on OnlineStoreThemeFileBodyText {
          content
        }
      }
    }
    userErrors {
      field
      message
      code
    }
  }
  delete: themeFilesDelete(
    themeId: "gid://shopify/OnlineStoreTheme/1?shopify-draft-proxy=synthetic"
    files: ["assets/app-copy.js"]
  ) {
    deletedThemeFiles {
      filename
      createdAt
      updatedAt
      size
      checksumMd5
      body {
        ... on OnlineStoreThemeFileBodyText {
          content
        }
      }
    }
    userErrors {
      field
      message
      code
    }
  }
}`;

const typeShapeQuery = `#graphql
  query OnlineStoreThemeFileOperationResultShape {
    operationResult: __type(name: "OnlineStoreThemeFileOperationResult") {
      fields {
        name
        type {
          kind
          name
          ofType {
            kind
            name
          }
        }
      }
    }
  }
`;

function readObject(value: unknown, context: string): JsonRecord {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    throw new Error(`${context} was not an object: ${JSON.stringify(value)}`);
  }
  return value as JsonRecord;
}

function assertResponseOk(response: DraftProxyHttpResponse): JsonRecord {
  if (response.status !== 200) {
    throw new Error(`Proxy request returned HTTP ${response.status}: ${JSON.stringify(response.body)}`);
  }
  const body = readObject(response.body, 'proxy response body');
  if ('errors' in body) {
    throw new Error(`Proxy request returned top-level GraphQL errors: ${JSON.stringify(body['errors'])}`);
  }
  return body;
}

function formatGeneratedFiles(): void {
  const result = spawnSync('corepack', ['pnpm', 'exec', 'oxfmt', fixturePath, specPath], {
    cwd: repoRoot,
    stdio: 'inherit',
    shell: process.platform === 'win32',
  });

  if (result.status !== 0) {
    throw new Error(`Generated file formatting failed with status ${String(result.status)}`);
  }
}

async function captureLiveTypeShape(): Promise<JsonRecord | null> {
  if (!process.env['SHOPIFY_CONFORMANCE_STORE_DOMAIN']) {
    return null;
  }
  const { storeDomain, adminOrigin } = readConformanceScriptConfig({
    defaultApiVersion: apiVersion,
    exitOnMissing: false,
    requireAdminOrigin: false,
  });

  const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
  const { runGraphqlRaw } = createAdminGraphqlClient({
    adminOrigin,
    apiVersion,
    headers: buildAdminAuthHeaders(adminAccessToken),
  });
  const result = await runGraphqlRaw(typeShapeQuery, {});
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`Theme file operation result introspection failed: ${JSON.stringify(result.payload)}`);
  }
  return {
    storeDomain,
    apiVersion,
    capturedAt: new Date().toISOString(),
    response: result.payload,
  };
}

async function main(): Promise<void> {
  await mkdir(path.dirname(requestPath), { recursive: true });
  await writeFile(requestPath, `${generatedQuery}\n`, 'utf8');
  const query = await readFile(requestPath, 'utf8');
  const proxy = createDraftProxy({
    readMode: 'snapshot',
    unsupportedMutationMode: 'reject',
    port: 0,
    shopifyAdminOrigin: 'https://local-runtime.invalid',
  });
  try {
    const body = assertResponseOk(await proxy.processGraphQLRequest({ query, variables: {} }, { apiVersion }));
    const log = readObject(proxy.getLog(), 'proxy log');
    const liveTypeShape = await captureLiveTypeShape();

    const fixture = {
      fixtureKind: 'local-runtime-theme-file-operation-result-shape',
      apiVersion,
      capturedAt: '2026-06-15T00:00:00.000Z',
      storeDomain: 'local-runtime.myshopify.com',
      expected: {
        primary: {
          variables: {},
          data: body['data'],
        },
        mutationLog: log['entries'],
      },
      evidence: {
        source: 'local-runtime-backed with live no-side-effect schema introspection',
        liveTypeShape,
        notes: [
          'Live Shopify writes for theme file mutations require theme-write authorization and can mutate storefront theme assets, so this executable fixture proves proxy-local result payload shape without runtime Shopify writes.',
          'The live no-side-effect introspection records the public OnlineStoreThemeFileOperationResult field names exposed to the conformance credential when available.',
          'The request intentionally selects body on mutation result objects; the local response omits it because body belongs to read-side OnlineStoreThemeFile, not OnlineStoreThemeFileOperationResult.',
        ],
      },
      upstreamCalls: [],
    };

    const spec = {
      scenarioId,
      operationNames: ['themeCreate', 'themeFilesUpsert', 'themeFilesCopy', 'themeFilesDelete'],
      scenarioStatus: 'captured',
      assertionKinds: [
        'runtime-staging',
        'payload-shape',
        'downstream-read-parity',
        'side-effect-boundary',
        'local-runtime-backed',
        'schema-introspection',
      ],
      liveCaptureFiles: [
        'fixtures/conformance/local-runtime/2026-04/online-store/theme-file-operation-result-shape.json',
      ],
      runtimeTestFiles: ['tests/graphql_routes/marketing_inventory_online_store.rs'],
      proxyRequest: {
        documentPath: 'config/parity-requests/online-store/theme-file-operation-result-shape.graphql',
        variablesCapturePath: '$.expected.primary.variables',
        apiVersion,
      },
      comparisonMode: 'captured-vs-proxy-request',
      comparison: {
        mode: 'strict-json',
        expectedDifferences: [],
        targets: [
          {
            name: 'theme-file-operation-result-shape',
            capturePath: '$.expected.primary.data',
            proxyPath: '$.data',
          },
          {
            name: 'theme-file-mutations-stage-raw-log-entries',
            capturePath: '$.expected.mutationLog',
            proxyLogPath: '$.entries',
          },
        ],
      },
      notes:
        'Executable local-runtime parity for ThemeFileOperationResult payload shape: themeFilesUpsert, themeFilesCopy, and themeFilesDelete return filename, createdAt, updatedAt, size, and checksumMd5, omit non-schema body from mutation result objects, and keep supported mutations local-only with replayable raw mutation log entries. Live 2026-04 introspection is captured inside the fixture evidence because direct theme-file writes can mutate storefront theme assets and require theme-write authorization.',
    };

    await mkdir(path.dirname(fixturePath), { recursive: true });
    await mkdir(path.dirname(specPath), { recursive: true });
    await writeFile(fixturePath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
    await writeFile(specPath, `${JSON.stringify(spec, null, 2)}\n`, 'utf8');
    formatGeneratedFiles();
    console.log(`Wrote ${path.relative(repoRoot, requestPath)}`);
    console.log(`Wrote ${path.relative(repoRoot, fixturePath)}`);
    console.log(`Wrote ${path.relative(repoRoot, specPath)}`);
  } finally {
    proxy.dispose();
  }
}

await main();
