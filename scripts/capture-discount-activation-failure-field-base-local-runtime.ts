/* oxlint-disable no-console -- Capture scripts intentionally write status output to stdio. */
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { createServer, type Server } from 'node:http';
import type { AddressInfo } from 'node:net';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { createDraftProxy } from '../js/src/index.js';

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

async function startRecordedUpstreamServer(
  calls: RecordedCall[],
): Promise<{ origin: string; close: () => Promise<void> }> {
  const server: Server = createServer((request, response) => {
    let body = '';
    request.setEncoding('utf8');
    request.on('data', (chunk: string) => {
      body += chunk;
    });
    request.on('end', () => {
      try {
        const parsed = JSON.parse(body || '{}') as GraphqlBody;
        const operationName = parsed.operationName ?? extractOperationName(parsed.query);
        const variables = parsed.variables ?? {};
        const call = calls.find(
          (candidate) => candidate.operationName === operationName && sameJson(candidate.variables, variables),
        );
        if (!call) {
          response.writeHead(500, { 'content-type': 'application/json' });
          response.end(
            JSON.stringify({
              errors: [
                {
                  message: `cassette miss: operation=${operationName} variables=${JSON.stringify(variables)}`,
                },
              ],
            }),
          );
          return;
        }
        response.writeHead(call.response.status, { 'content-type': 'application/json' });
        response.end(JSON.stringify(call.response.body));
      } catch (error) {
        response.writeHead(500, { 'content-type': 'application/json' });
        response.end(JSON.stringify({ errors: [{ message: (error as Error).message }] }));
      }
    });
  });

  await new Promise<void>((resolve) => server.listen(0, '127.0.0.1', resolve));
  const address = server.address() as AddressInfo;
  return {
    origin: `http://127.0.0.1:${address.port}`,
    close: () =>
      new Promise<void>((resolve, reject) => {
        server.close((error) => (error ? reject(error) : resolve()));
      }),
  };
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

const upstreamCalls = [makeFunctionHydrationCall(), makeFunctionUnavailableCall()];
const upstreamServer = await startRecordedUpstreamServer(upstreamCalls);
const proxy = createDraftProxy({
  readMode: 'live-hybrid',
  port: 0,
  shopifyAdminOrigin: upstreamServer.origin,
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
  await proxy.processGraphQLRequest(
    {
      query: createQuery,
      variables: createVariables,
      operationName: 'DiscountActivationFailureFieldBaseCreate',
    },
    { apiVersion },
  ),
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
  await proxy.processGraphQLRequest(
    {
      query: codeActivateQuery,
      variables: { id: codeId },
      operationName: 'DiscountActivationFailureFieldBaseCodeActivate',
    },
    { apiVersion },
  ),
  'code activation failure',
);
const automaticActivationFailure = assertResponseOk(
  await proxy.processGraphQLRequest(
    {
      query: automaticActivateQuery,
      variables: { id: automaticId },
      operationName: 'DiscountActivationFailureFieldBaseAutomaticActivate',
    },
    { apiVersion },
  ),
  'automatic activation failure',
);
const unknown = assertResponseOk(
  await proxy.processGraphQLRequest(
    {
      query: unknownQuery,
      variables: {},
      operationName: 'DiscountActivationFailureFieldBaseUnknown',
    },
    { apiVersion },
  ),
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
proxy.dispose();
await upstreamServer.close();
