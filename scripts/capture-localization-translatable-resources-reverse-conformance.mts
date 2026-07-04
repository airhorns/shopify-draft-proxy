/* oxlint-disable no-console -- CLI capture script intentionally writes status output. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

type UserError = {
  field?: string[] | null;
  message?: string;
  code?: string | null;
};

type ProductNode = {
  id?: string | null;
  title?: string | null;
  handle?: string | null;
  status?: string | null;
};

type ProductCreateData = {
  productCreate?: {
    product?: ProductNode | null;
    userErrors?: UserError[] | null;
  } | null;
};

type ProductDeleteData = {
  productDelete?: {
    deletedProductId?: string | null;
    userErrors?: UserError[] | null;
  } | null;
};

type TranslatableConnection = {
  nodes?: Array<{ resourceId?: string | null }> | null;
  edges?: Array<{ cursor?: string | null; node?: { resourceId?: string | null } | null }> | null;
  pageInfo?: {
    hasNextPage?: boolean | null;
    hasPreviousPage?: boolean | null;
    startCursor?: string | null;
    endCursor?: string | null;
  } | null;
};

type ReverseReadData = {
  reversedFirst?: TranslatableConnection | null;
};

type CapturedCase<TData> = {
  name: string;
  query: string;
  variables: JsonRecord;
  response: ConformanceGraphqlResult<TData>;
};

const scenarioId = 'localization-translatable-resources-reverse';
const createRequestPath = path.join(
  'config',
  'parity-requests',
  'localization',
  'localization-translatable-resources-reverse-product-create.graphql',
);
const readRequestPath = path.join(
  'config',
  'parity-requests',
  'localization',
  'localization-translatable-resources-reverse-read.graphql',
);

const productDeleteMutation = `#graphql
mutation LocalizationTranslatableResourcesReverseProductDelete($input: ProductDeleteInput!) {
  productDelete(input: $input) {
    deletedProductId
    userErrors {
      field
      message
    }
  }
}
`;

function userErrors<TData extends JsonRecord>(result: ConformanceGraphqlResult<TData>, root: keyof TData): UserError[] {
  const data = result.payload.data;
  if (typeof data !== 'object' || data === null) return [];
  const payload = data[root];
  if (typeof payload !== 'object' || payload === null) return [];
  const maybeErrors = (payload as { userErrors?: UserError[] | null }).userErrors;
  return Array.isArray(maybeErrors) ? maybeErrors : [];
}

function assertNoUserErrors<TData extends JsonRecord>(
  result: ConformanceGraphqlResult<TData>,
  root: keyof TData,
  label: string,
): void {
  const errors = userErrors(result, root);
  if (result.status !== 200 || result.payload.errors || errors.length > 0) {
    throw new Error(
      `${label} failed: status=${result.status} userErrors=${JSON.stringify(errors)} errors=${JSON.stringify(
        result.payload.errors ?? null,
      )}`,
    );
  }
}

function createdProductId(result: ConformanceGraphqlResult<ProductCreateData>): string {
  const id = result.payload.data?.productCreate?.product?.id;
  if (typeof id !== 'string') {
    throw new Error(`productCreate did not return a product id: ${JSON.stringify(result.payload)}`);
  }
  return id;
}

function firstResourceId(connection: TranslatableConnection | null | undefined, label: string): string {
  const edge = connection?.edges?.[0];
  const resourceId = edge?.node?.resourceId;
  if (typeof resourceId !== 'string') {
    throw new Error(`${label} did not return one edge resourceId: ${JSON.stringify(connection)}`);
  }
  if (connection?.nodes?.[0]?.resourceId !== resourceId) {
    throw new Error(`${label} nodes/edges resourceId mismatch: ${JSON.stringify(connection)}`);
  }
  if (typeof edge.cursor !== 'string' || edge.cursor.length === 0) {
    throw new Error(`${label} expected a non-empty edge cursor: ${JSON.stringify(connection)}`);
  }
  return resourceId;
}

function assertPageInfo(
  connection: TranslatableConnection | null | undefined,
  expected: { hasNextPage: boolean; hasPreviousPage: boolean },
  label: string,
): void {
  const pageInfo = connection?.pageInfo;
  if (
    !pageInfo ||
    pageInfo.hasNextPage !== expected.hasNextPage ||
    pageInfo.hasPreviousPage !== expected.hasPreviousPage ||
    typeof pageInfo.startCursor !== 'string' ||
    typeof pageInfo.endCursor !== 'string'
  ) {
    throw new Error(`${label} unexpected pageInfo: ${JSON.stringify(pageInfo)}`);
  }
}

function assertReverseSemantics(read: ConformanceGraphqlResult<ReverseReadData>, expectedResourceId: string): void {
  const data = read.payload.data;
  if (read.status !== 200 || read.payload.errors || !data) {
    throw new Error(`translatableResources reverse read failed: ${JSON.stringify(read.payload)}`);
  }
  const reversedFirst = firstResourceId(data.reversedFirst, 'reversedFirst');
  if (reversedFirst !== expectedResourceId) {
    throw new Error(
      `Expected reverse:first to return newest disposable Product, got ${JSON.stringify({
        expectedResourceId,
        reversedFirst,
      })}`,
    );
  }
  assertPageInfo(data.reversedFirst, { hasNextPage: true, hasPreviousPage: false }, 'reversedFirst');
}

async function main(): Promise<void> {
  const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
  if (apiVersion !== '2026-04') {
    throw new Error(`Expected SHOPIFY_CONFORMANCE_API_VERSION=2026-04, got ${apiVersion}`);
  }
  const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
  const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'localization');
  const outputPath = path.join(outputDir, `${scenarioId}.json`);
  const productCreateMutation = await readFile(createRequestPath, 'utf8');
  const readQuery = await readFile(readRequestPath, 'utf8');

  const { runGraphqlRequest } = createAdminGraphqlClient({
    adminOrigin,
    apiVersion,
    headers: buildAdminAuthHeaders(adminAccessToken),
  });

  async function captureCase<TData>(name: string, query: string, variables: JsonRecord): Promise<CapturedCase<TData>> {
    return {
      name,
      query,
      variables,
      response: await runGraphqlRequest<TData>(query, variables),
    };
  }

  const unique = new Date().toISOString().replace(/\D/gu, '').slice(0, 14);
  const slug = unique.toLowerCase();
  const productInputs = [
    {
      title: `Localization reverse ${unique} Alpha`,
      handle: `localization-reverse-${slug}-alpha`,
      status: 'DRAFT',
    },
    {
      title: `Localization reverse ${unique} Beta`,
      handle: `localization-reverse-${slug}-beta`,
      status: 'DRAFT',
    },
  ];
  const cases: Array<CapturedCase<unknown>> = [];
  const createdProductIds: string[] = [];
  const cleanup: Array<CapturedCase<ProductDeleteData>> = [];

  try {
    for (const [index, product] of productInputs.entries()) {
      const create = await captureCase<ProductCreateData>(`productCreate ${index + 1}`, productCreateMutation, {
        product,
      });
      assertNoUserErrors(create.response, 'productCreate', `productCreate ${index + 1}`);
      createdProductIds.push(createdProductId(create.response));
      cases.push(create);
    }

    const read = await captureCase<ReverseReadData>('translatableResources reverse read', readQuery, {
      resourceType: 'PRODUCT',
    });
    assertReverseSemantics(read.response, createdProductIds[createdProductIds.length - 1] ?? '');
    cases.push(read);
  } finally {
    for (const id of [...createdProductIds].reverse()) {
      cleanup.push(
        await captureCase<ProductDeleteData>('productDelete cleanup', productDeleteMutation, {
          input: { id },
        }),
      );
    }
  }

  await mkdir(outputDir, { recursive: true });
  await writeFile(
    outputPath,
    `${JSON.stringify(
      {
        scenarioId,
        storeDomain,
        apiVersion,
        capturedAt: new Date().toISOString(),
        scope: 'translatableResources reverse connection argument',
        cases,
        cleanup,
        upstreamCalls: [],
      },
      null,
      2,
    )}\n`,
    'utf8',
  );

  console.log(
    JSON.stringify(
      {
        ok: true,
        outputPath,
        storeDomain,
        apiVersion,
        cases: cases.map((capture) => ({ name: capture.name, status: capture.response.status })),
        cleanup: cleanup.map((capture) => ({ name: capture.name, status: capture.response.status })),
      },
      null,
      2,
    ),
  );
}

await main();
