/* oxlint-disable no-console -- Capture scripts intentionally write status output to stdio. */
import { spawnSync } from 'node:child_process';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { createDraftProxy, type DraftProxyHttpResponse } from '../js/src/index.js';

type JsonRecord = Record<string, unknown>;

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, '..');
const apiVersion = '2026-04';
const requestPath = path.join(
  repoRoot,
  'config',
  'parity-requests',
  'online-store',
  'mobile_platform_application_create_model_validation.graphql',
);
const fixturePath = path.join(
  repoRoot,
  'fixtures',
  'conformance',
  'local-runtime',
  apiVersion,
  'online-store',
  'mobile_platform_application_create_model_validation.json',
);
const specPath = path.join(
  repoRoot,
  'config',
  'parity-specs',
  'online-store',
  'mobile_platform_application_create_model_validation.json',
);

const variables = {
  longApplicationId: 'a'.repeat(101),
  longAppClipApplicationId: 'c'.repeat(256),
};

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

function formatGeneratedJson(): void {
  const result = spawnSync('corepack', ['pnpm', 'exec', 'oxfmt', fixturePath, specPath], {
    cwd: repoRoot,
    stdio: 'inherit',
    shell: process.platform === 'win32',
  });

  if (result.status !== 0) {
    throw new Error(`Generated JSON formatting failed with status ${String(result.status)}`);
  }
}

async function main(): Promise<void> {
  const query = await readFile(requestPath, 'utf8');
  const proxy = createDraftProxy({
    readMode: 'snapshot',
    unsupportedMutationMode: 'reject',
    port: 0,
    shopifyAdminOrigin: 'https://local-runtime.invalid',
  });
  try {
    const body = assertResponseOk(await proxy.processGraphQLRequest({ query, variables }, { apiVersion }));
    const log = readObject(proxy.getLog(), 'proxy log');
    const fixture = {
      fixtureKind: 'local-runtime-mobile-platform-application-create-model-validation',
      apiVersion,
      capturedAt: '2026-06-12T00:00:00.000Z',
      storeDomain: 'local-runtime.myshopify.com',
      summary: 'Executable local-runtime fixture for mobilePlatformApplicationCreate Core model-level validation.',
      expected: {
        primary: {
          data: body['data'],
        },
        emptyMutationLog: log['entries'],
      },
      evidence: {
        source: 'Core mobile platform application model validations plus local-runtime recorder',
        notes: [
          'The active live conformance credential lacks mobile-platform write/read scopes for public Admin GraphQL capture; this fixture records the Core model-level validation branches as executable local-runtime parity evidence.',
          'The supported proxy mutation must reject these invalid inputs locally without staging mobile platform application records or writing upstream.',
        ],
      },
      upstreamCalls: [],
    };

    const spec = {
      scenarioId: 'mobile-platform-application-create-model-validation',
      operationNames: ['mobilePlatformApplicationCreate'],
      scenarioStatus: 'captured',
      assertionKinds: [
        'runtime-staging',
        'user-errors-parity',
        'payload-shape',
        'side-effect-boundary',
        'local-runtime-backed',
      ],
      liveCaptureFiles: [
        'fixtures/conformance/local-runtime/2026-04/online-store/mobile_platform_application_create_model_validation.json',
      ],
      runtimeTestFiles: ['tests/graphql_routes/marketing_inventory_online_store.rs'],
      proxyRequest: {
        documentPath: 'config/parity-requests/online-store/mobile_platform_application_create_model_validation.graphql',
        apiVersion,
        variables,
      },
      comparisonMode: 'captured-vs-proxy-request',
      comparison: {
        mode: 'strict-json',
        expectedDifferences: [],
        targets: [
          {
            name: 'mobile-platform-application-create-model-validation-user-errors',
            capturePath: '$.expected.primary.data',
            proxyPath: '$.data',
          },
          {
            name: 'rejected-requests-do-not-stage',
            capturePath: '$.expected.emptyMutationLog',
            proxyLogPath: '$.entries',
          },
        ],
      },
      notes:
        'Executable local-runtime parity for Core mobile platform application create model validation: Android/Apple application identifiers over 100 characters return TOO_LONG, Android sha256CertFingerprints must be present and non-empty, and Apple appClipApplicationId is required when appClipsEnabled is true and capped at 255 characters. The scenario uses an empty upstream cassette because supported mobile platform application mutations stage locally and must not write upstream at runtime.',
    };

    await mkdir(path.dirname(fixturePath), { recursive: true });
    await writeFile(fixturePath, `${JSON.stringify(fixture, null, 2)}\n`);
    await writeFile(specPath, `${JSON.stringify(spec, null, 2)}\n`);
    formatGeneratedJson();
    console.log(`Wrote ${path.relative(repoRoot, fixturePath)}`);
    console.log(`Wrote ${path.relative(repoRoot, specPath)}`);
  } finally {
    proxy.dispose();
  }
}

await main();
