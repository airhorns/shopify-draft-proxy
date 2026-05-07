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

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, '..');
const apiVersion = '2026-04';
const outputPath = path.join(
  repoRoot,
  'fixtures',
  'conformance',
  'local-runtime',
  apiVersion,
  'discounts',
  'discount-activation-failure-field-base.json',
);

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

async function readRequest(relativePath: string): Promise<string> {
  return readFile(path.join(repoRoot, relativePath), 'utf8');
}

function sameJson(left: unknown, right: unknown): boolean {
  return JSON.stringify(left) === JSON.stringify(right);
}

function readObject(value: unknown, context: string): JsonRecord {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    throw new Error(`${context} was not an object: ${JSON.stringify(value)}`);
  }

  return value as JsonRecord;
}

function readPath(value: unknown, pathSegments: string[]): unknown {
  let current = value;
  for (const segment of pathSegments) {
    if (!current || typeof current !== 'object' || Array.isArray(current)) {
      return undefined;
    }
    current = (current as JsonRecord)[segment];
  }

  return current;
}

function readRequiredString(value: unknown, pathSegments: string[]): string {
  const found = readPath(value, pathSegments);
  if (typeof found !== 'string' || found.length === 0) {
    throw new Error(`Missing required string at ${pathSegments.join('.')}: ${JSON.stringify(value)}`);
  }

  return found;
}

function assertResponseOk(response: { status: number; body: unknown }, context: string): JsonRecord {
  if (response.status !== 200) {
    throw new Error(`${context} returned HTTP ${response.status}: ${JSON.stringify(response.body)}`);
  }

  const body = readObject(response.body, `${context} response body`);
  if ('errors' in body) {
    throw new Error(`${context} returned top-level GraphQL errors: ${JSON.stringify(body['errors'])}`);
  }

  return body;
}

function extractOperationName(query: string): string {
  return query.match(/\b(?:query|mutation|subscription)\s+([A-Za-z_][A-Za-z0-9_]*)/)?.[1] ?? '';
}

function makeFunctionHydrationCall(): RecordedCall {
  return {
    operationName: 'ShopifyFunctionByHandle',
    variables: { handle: 'discount-local' },
    query:
      'query ShopifyFunctionByHandle($handle: String!) { shopifyFunctions(first: 1, handle: $handle) { nodes { id title handle apiType description appKey app { id title handle apiKey } } } }',
    response: {
      status: 200,
      body: {
        data: {
          shopifyFunctions: {
            nodes: [
              {
                id: 'gid://shopify/ShopifyFunction/discount-local',
                title: 'Local volume discount',
                handle: 'discount-local',
                apiType: 'DISCOUNT',
                description: 'Captured local Function metadata',
                appKey: 'app-key-787',
                app: null,
              },
            ],
          },
        },
      },
    },
  };
}

function makeFunctionUnavailableCall(): RecordedCall {
  return {
    operationName: 'ShopifyFunctionAvailabilityForDiscountActivation',
    variables: { handle: 'discount-local' },
    query:
      'query ShopifyFunctionAvailabilityForDiscountActivation($handle: String!) { shopifyFunctions(first: 1, handle: $handle) { nodes { id title handle apiType description appKey app { id title handle apiKey } } } }',
    response: {
      status: 200,
      body: {
        data: {
          shopifyFunctions: {
            nodes: [],
          },
        },
      },
    },
  };
}

ensureGleamJsBuild();

const [{ createDraftProxy }, { Ok, Error, toList }, { HttpOutcome, CommitTransportError }] = await Promise.all([
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

const upstreamCalls = [makeFunctionHydrationCall(), makeFunctionUnavailableCall()];
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
    const call = upstreamCalls.find(
      (call) => call.operationName === operationName && sameJson(call.variables, variables),
    );

    if (!call) {
      return new Error(
        new CommitTransportError(`cassette miss: operation=${operationName} variables=${JSON.stringify(variables)}`),
      );
    }

    return new Ok(new HttpOutcome(call.response.status, JSON.stringify(call.response.body), toList([])));
  } catch (error) {
    return new Error(new CommitTransportError((error as Error).message));
  }
});

const createQuery = await readRequest(
  'config/parity-requests/discounts/discount-activation-failure-field-base-create.graphql',
);
const codeActivateQuery = await readRequest(
  'config/parity-requests/discounts/discount-activation-failure-field-base-code-activate.graphql',
);
const automaticActivateQuery = await readRequest(
  'config/parity-requests/discounts/discount-activation-failure-field-base-automatic-activate.graphql',
);
const unknownQuery = await readRequest(
  'config/parity-requests/discounts/discount-activation-failure-field-base-unknown.graphql',
);

const createVariables = {
  codeInput: {
    title: 'Activation failure code app',
    code: 'ACTIVATEFAILBASE',
    startsAt: '2024-02-01T00:00:00.000Z',
    functionHandle: 'discount-local',
  },
  automaticInput: {
    title: 'Activation failure automatic app',
    startsAt: '2024-02-01T00:00:00.000Z',
    functionHandle: 'discount-local',
  },
};

const create = assertResponseOk(
  await proxy.processGraphQLRequest({
    query: createQuery,
    variables: createVariables,
    operationName: 'DiscountActivationFailureFieldBaseCreate',
  }),
  'create',
);
const codeId = readRequiredString(create, ['data', 'discountCodeAppCreate', 'codeAppDiscount', 'discountId']);
const automaticId = readRequiredString(create, [
  'data',
  'discountAutomaticAppCreate',
  'automaticAppDiscount',
  'discountId',
]);

const codeActivationFailure = assertResponseOk(
  await proxy.processGraphQLRequest({
    query: codeActivateQuery,
    variables: { id: codeId },
    operationName: 'DiscountActivationFailureFieldBaseCodeActivate',
  }),
  'code activation failure',
);
const automaticActivationFailure = assertResponseOk(
  await proxy.processGraphQLRequest({
    query: automaticActivateQuery,
    variables: { id: automaticId },
    operationName: 'DiscountActivationFailureFieldBaseAutomaticActivate',
  }),
  'automatic activation failure',
);
const unknown = assertResponseOk(
  await proxy.processGraphQLRequest({
    query: unknownQuery,
    variables: {},
    operationName: 'DiscountActivationFailureFieldBaseUnknown',
  }),
  'unknown activate/deactivate',
);

const fixture = {
  fixtureKind: 'local-runtime-discount-activation-failure-field-base',
  apiVersion,
  capturedAt: new Date().toISOString(),
  summary:
    'Executable local-runtime fixture for app-discount activation failure after the backing Function is unavailable. Setup creates scheduled staged app code and automatic discounts through normal GraphQL mutations and cassette-backed Function hydration; activation failure is earned through a second Function availability read returning no nodes.',
  create: {
    variables: createVariables,
    response: create,
  },
  codeActivationFailure: {
    response: codeActivationFailure,
  },
  automaticActivationFailure: {
    response: automaticActivationFailure,
  },
  unknown: {
    response: unknown,
  },
  upstreamCalls,
};

await mkdir(path.dirname(outputPath), { recursive: true });
await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
console.log(`Wrote ${path.relative(repoRoot, outputPath)}`);
