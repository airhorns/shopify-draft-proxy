/* oxlint-disable no-console -- Capture scripts intentionally write status output to stdio. */
import { spawnSync } from 'node:child_process';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

type JsonRecord = Record<string, unknown>;
type GraphqlBody = {
  query: string;
  variables?: JsonRecord;
  operationName?: string;
};
type RecordedCall = {
  operationName: string;
  variables: JsonRecord;
  query: string;
  response: {
    status: number;
    body: JsonRecord;
  };
};
type ProxyResponse = {
  status: number;
  body: unknown;
};

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, '..');
const apiVersion = '2025-01';
const outputPath = path.join(
  repoRoot,
  'fixtures',
  'conformance',
  'local-runtime',
  apiVersion,
  'shipping-fulfillments',
  'shipping-package-default-lifecycle-local-runtime.json',
);

const seedShippingPackages = [
  {
    id: 'gid://shopify/ShippingPackage/1',
    name: 'Starter box',
    type: 'BOX',
    default: true,
    weight: {
      value: 1,
      unit: 'KILOGRAMS',
    },
    dimensions: {
      length: 10,
      width: 8,
      height: 4,
      unit: 'CENTIMETERS',
    },
    createdAt: '2026-04-27T00:00:00.000Z',
    updatedAt: '2026-04-27T00:00:00.000Z',
  },
  {
    id: 'gid://shopify/ShippingPackage/2',
    name: 'Backup mailer',
    type: 'ENVELOPE',
    default: false,
    weight: {
      value: 0.5,
      unit: 'KILOGRAMS',
    },
    dimensions: {
      length: 8,
      width: 6,
      height: 1,
      unit: 'CENTIMETERS',
    },
    createdAt: '2026-04-27T00:00:00.000Z',
    updatedAt: '2026-04-27T00:00:00.000Z',
  },
] satisfies JsonRecord[];

function ensureGleamJsBuild(): void {
  const result = spawnSync('corepack', ['pnpm', 'gleam:build:js'], {
    cwd: repoRoot,
    stdio: 'inherit',
    shell: process.platform === 'win32',
  });

  if (result.status !== 0) {
    throw new Error(`Gleam JS build failed with status ${String(result.status)}`);
  }
}

async function readRequest(name: string): Promise<string> {
  return readFile(path.join(repoRoot, 'config', 'parity-requests', 'shipping-fulfillments', name), 'utf8');
}

function readObject(value: unknown, context: string): JsonRecord {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    throw new Error(`${context} was not an object: ${JSON.stringify(value)}`);
  }

  return value as JsonRecord;
}

function readPath(value: unknown, segments: string[]): unknown {
  let current = value;
  for (const segment of segments) {
    if (!current || typeof current !== 'object' || Array.isArray(current)) {
      return undefined;
    }
    current = (current as JsonRecord)[segment];
  }

  return current;
}

function selectedShippingPackageState(proxyState: JsonRecord, packageIds: string[]): JsonRecord {
  const stagedPackages = readObject(
    readPath(proxyState, ['stagedState', 'shippingPackages']),
    'proxyState.stagedState.shippingPackages',
  );
  const selectedPackages = Object.fromEntries(
    packageIds
      .map((id) => [id, stagedPackages[id]])
      .filter((entry): entry is [string, unknown] => entry[1] !== undefined),
  );

  const stagedState: JsonRecord = {
    shippingPackages: selectedPackages,
  };
  const deletedIds = readPath(proxyState, ['stagedState', 'deletedShippingPackageIds']);
  if (deletedIds && typeof deletedIds === 'object' && !Array.isArray(deletedIds)) {
    const deletedEntries = Object.entries(deletedIds);
    if (deletedEntries.length > 0) {
      stagedState['deletedShippingPackageIds'] = Object.fromEntries(deletedEntries);
    }
  }

  return { stagedState };
}

function selectedMutationLog(proxyLog: JsonRecord): JsonRecord {
  const entries = readPath(proxyLog, ['entries']);
  if (!Array.isArray(entries)) {
    throw new Error(`proxy log did not contain entries: ${JSON.stringify(proxyLog)}`);
  }

  return {
    entries: entries.map((entry) => {
      const record = readObject(entry, 'mutation log entry');
      return {
        operationName: record['operationName'],
        status: record['status'],
      };
    }),
  };
}

function extractOperationName(query: string): string {
  return query.match(/\b(?:query|mutation|subscription)\s+([A-Za-z_][A-Za-z0-9_]*)/)?.[1] ?? '';
}

function makeHydrationResponse(packageId: unknown): JsonRecord | null {
  if (typeof packageId !== 'string') return null;
  const shippingPackage = seedShippingPackages.find((candidate) => candidate.id === packageId);
  if (!shippingPackage) return null;
  return {
    data: {
      shippingPackage,
    },
  };
}

function assertResponseOk(response: ProxyResponse, context: string): JsonRecord {
  if (response.status !== 200) {
    throw new Error(`${context} returned HTTP ${response.status}: ${JSON.stringify(response.body)}`);
  }

  const body = readObject(response.body, `${context} response body`);
  if ('errors' in body) {
    throw new Error(`${context} returned top-level GraphQL errors: ${JSON.stringify(body['errors'])}`);
  }

  return body;
}

async function runProxyRequest(
  proxy: { processGraphQLRequest: (input: GraphqlBody) => Promise<ProxyResponse> },
  query: string,
  variables: JsonRecord,
  operationName: string,
): Promise<JsonRecord> {
  return assertResponseOk(
    await proxy.processGraphQLRequest({
      query,
      variables,
      operationName,
    }),
    operationName,
  );
}

ensureGleamJsBuild();

const [{ createDraftProxy }, { Ok, Error: GleamError, toList }, { HttpOutcome, CommitTransportError }] =
  await Promise.all([
    import('../js/src/index.js'),
    import(pathToFileURL(path.join(repoRoot, 'build', 'dev', 'javascript', 'prelude.mjs')).href),
    import(
      pathToFileURL(
        path.join(
          repoRoot,
          'build',
          'dev',
          'javascript',
          'shopify_draft_proxy',
          'shopify_draft_proxy',
          'proxy',
          'commit.mjs',
        ),
      ).href
    ),
  ]);

const upstreamCalls: RecordedCall[] = [];
const proxy = createDraftProxy({
  readMode: 'live-hybrid',
  port: 0,
  shopifyAdminOrigin: 'https://local-runtime.invalid',
});

proxy.installSyncTransport((request: JsonRecord) => {
  try {
    const body = JSON.parse(String(request['body'] ?? '{}')) as GraphqlBody;
    const operationName = body.operationName ?? extractOperationName(body.query);
    const variables = body.variables ?? {};
    const responseBody = makeHydrationResponse(variables['id']);
    if (!responseBody) {
      return new GleamError(
        new CommitTransportError(`cassette miss: operation=${operationName} variables=${JSON.stringify(variables)}`),
      );
    }

    upstreamCalls.push({
      operationName,
      variables,
      query: body.query,
      response: {
        status: 200,
        body: responseBody,
      },
    });

    return new Ok(new HttpOutcome(200, JSON.stringify(responseBody), toList([])));
  } catch (error) {
    return new GleamError(new CommitTransportError((error as Error).message));
  }
});

const updateQuery = await readRequest('shipping-package-update-local-runtime.graphql');
const makeDefaultQuery = await readRequest('shipping-package-make-default-local-runtime.graphql');
const deleteQuery = await readRequest('shipping-package-delete-local-runtime.graphql');

const updateVariables = {
  id: 'gid://shopify/ShippingPackage/1',
  shippingPackage: {
    name: 'Updated box',
    type: 'BOX',
    default: true,
    weight: {
      value: 2.5,
      unit: 'POUNDS',
    },
    dimensions: {
      length: 12,
      width: 9,
      height: 5,
      unit: 'INCHES',
    },
  },
};
const update = await runProxyRequest(proxy, updateQuery, updateVariables, 'ShippingPackageUpdateLocalRuntime');
const updateState = selectedShippingPackageState(readObject(proxy.getState(), 'proxy state after update'), [
  'gid://shopify/ShippingPackage/1',
]);

const makeDefaultVariables = {
  id: 'gid://shopify/ShippingPackage/2',
};
const makeDefault = await runProxyRequest(
  proxy,
  makeDefaultQuery,
  makeDefaultVariables,
  'ShippingPackageMakeDefaultLocalRuntime',
);
const makeDefaultState = selectedShippingPackageState(readObject(proxy.getState(), 'proxy state after make default'), [
  'gid://shopify/ShippingPackage/1',
  'gid://shopify/ShippingPackage/2',
]);

const restoreDefaultVariables = {
  id: 'gid://shopify/ShippingPackage/1',
  shippingPackage: {
    default: true,
  },
};
const restoreDefault = await runProxyRequest(
  proxy,
  updateQuery,
  restoreDefaultVariables,
  'ShippingPackageUpdateLocalRuntime',
);
const restoreDefaultState = selectedShippingPackageState(
  readObject(proxy.getState(), 'proxy state after restore default'),
  ['gid://shopify/ShippingPackage/1', 'gid://shopify/ShippingPackage/2'],
);

const deleteVariables = {
  id: 'gid://shopify/ShippingPackage/1',
};
const deleteResponse = await runProxyRequest(proxy, deleteQuery, deleteVariables, 'ShippingPackageDeleteLocalRuntime');
const deleteState = selectedShippingPackageState(readObject(proxy.getState(), 'proxy state after delete'), [
  'gid://shopify/ShippingPackage/2',
]);
const deleteLog = selectedMutationLog(readObject(proxy.getLog(), 'proxy log after delete'));

const fixture = {
  fixtureKind: 'local-runtime-shipping-package-default-lifecycle',
  apiVersion,
  capturedAt: new Date().toISOString(),
  source: 'local-runtime-capture-script',
  notes:
    'Executable local-runtime parity fixture for shipping package update, make-default, restore-default, and delete staging. Public Admin GraphQL 2025-01 rejects selecting shippingPackage on ShippingPackageMakeDefaultPayload in the configured conformance store, so this fixture records the supported local proxy contract from actual proxy execution with deterministic cassette-backed package hydration.',
  seed: {
    shippingPackages: seedShippingPackages,
  },
  update: {
    variables: updateVariables,
    response: update,
    state: updateState,
  },
  makeDefault: {
    variables: makeDefaultVariables,
    response: makeDefault,
    state: makeDefaultState,
  },
  restoreDefault: {
    variables: restoreDefaultVariables,
    response: restoreDefault,
    state: restoreDefaultState,
  },
  delete: {
    variables: deleteVariables,
    response: deleteResponse,
    state: deleteState,
    log: deleteLog,
  },
  upstreamCalls,
};

await mkdir(path.dirname(outputPath), { recursive: true });
await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
console.log(`Wrote ${path.relative(repoRoot, outputPath)}`);
