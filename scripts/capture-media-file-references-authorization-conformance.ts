/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { setTimeout as delay } from 'node:timers/promises';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;
type UserError = { field?: string[] | null; message?: string | null; code?: string | null };
type ProductCreateData = {
  productCreate?: {
    product?: { id?: string | null } | null;
    userErrors?: UserError[] | null;
  } | null;
};
type ProductDeleteData = {
  productDelete?: {
    deletedProductId?: string | null;
    userErrors?: UserError[] | null;
  } | null;
};
type FileCreateData = {
  fileCreate?: {
    files?: Array<{ id?: string | null; fileStatus?: string | null } | null> | null;
    userErrors?: UserError[] | null;
  } | null;
};
type FileDeleteData = {
  fileDelete?: {
    deletedFileIds?: string[] | null;
    userErrors?: UserError[] | null;
  } | null;
};
type FileReadData = {
  node?: {
    id?: string | null;
    fileStatus?: string | null;
  } | null;
};

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const requestPath = path.join(
  'config',
  'parity-requests',
  'media',
  'media-file-create-references-authorization.graphql',
);
const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const fixturePath = path.join(
  'fixtures',
  'conformance',
  storeDomain,
  apiVersion,
  'media',
  'media-file-create-references-authorization.json',
);

const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });

function client(accessToken: string) {
  return createAdminGraphqlClient({
    adminOrigin,
    apiVersion,
    headers: buildAdminAuthHeaders(accessToken),
  });
}

const adminClient = client(adminAccessToken);

function absolutePath(relativePath: string): string {
  return path.join(repoRoot, relativePath);
}

const productCreateMutation = `#graphql
  mutation MediaReferencesAuthorizationProductSeed($product: ProductCreateInput!) {
    productCreate(product: $product) {
      product {
        id
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const productDeleteMutation = `#graphql
  mutation MediaReferencesAuthorizationProductCleanup($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors {
        field
        message
      }
    }
  }
`;

const fileCreateMutation = `#graphql
  mutation MediaReferencesAuthorizationFileSeed($files: [FileCreateInput!]!) {
    fileCreate(files: $files) {
      files {
        id
        fileStatus
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const fileReadQuery = `#graphql
  query MediaReferencesAuthorizationReadyPoll($id: ID!) {
    node(id: $id) {
      ... on MediaImage {
        id
        fileStatus
      }
    }
  }
`;

const fileDeleteMutation = `#graphql
  mutation MediaReferencesAuthorizationFileCleanup($fileIds: [ID!]!) {
    fileDelete(fileIds: $fileIds) {
      deletedFileIds
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const createDelegateTokenMutation = `#graphql
  mutation MediaReferencesAuthorizationDelegateSetup {
    delegateAccessTokenCreate(input: { delegateAccessScope: ["read_files", "write_files", "read_products"], expiresIn: 30 }) {
      delegateAccessToken {
        accessToken
        accessScopes
        createdAt
        expiresIn
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const destroyDelegateTokenMutation = `#graphql
  mutation MediaReferencesAuthorizationDelegateCleanup($token: String!) {
    delegateAccessTokenDestroy(accessToken: $token) {
      status
      userErrors {
        field
        message
        code
      }
    }
  }
`;

function expectNoUserErrors(label: string, errors: UserError[] | null | undefined): void {
  if (Array.isArray(errors) && errors.length === 0) return;
  throw new Error(`${label} returned userErrors: ${JSON.stringify(errors ?? null, null, 2)}`);
}

function requireId(label: string, id: unknown): string {
  if (typeof id === 'string' && id.length > 0) return id;
  throw new Error(`${label} did not return an id: ${JSON.stringify(id, null, 2)}`);
}

function readDelegateToken(capture: ConformanceGraphqlResult<JsonRecord>): string {
  const token = (capture.payload.data as JsonRecord | undefined)?.['delegateAccessTokenCreate'] as
    | JsonRecord
    | undefined;
  const delegate = token?.['delegateAccessToken'] as JsonRecord | undefined;
  const accessToken = delegate?.['accessToken'];
  if (typeof accessToken === 'string' && accessToken.length > 0) {
    return accessToken;
  }

  throw new Error(`delegateAccessTokenCreate did not return an access token: ${JSON.stringify(capture, null, 2)}`);
}

function redactDelegateToken(capture: ConformanceGraphqlResult<JsonRecord>): ConformanceGraphqlResult<JsonRecord> {
  const payload = JSON.parse(JSON.stringify(capture)) as ConformanceGraphqlResult<JsonRecord>;
  const data = payload.payload.data as JsonRecord | undefined;
  const root = data?.['delegateAccessTokenCreate'] as JsonRecord | undefined;
  const token = root?.['delegateAccessToken'] as JsonRecord | undefined;
  if (token) token['accessToken'] = '[redacted-live-delegate-token]';
  return payload;
}

function assertAccessDenied(response: ConformanceGraphqlResult<JsonRecord>): void {
  const payload = response.payload as JsonRecord;
  const errors = payload['errors'];
  const data = payload['data'] as JsonRecord | undefined;
  const firstError = Array.isArray(errors) ? (errors[0] as JsonRecord | undefined) : undefined;
  const extensions = firstError?.['extensions'] as JsonRecord | undefined;
  const path = firstError?.['path'];
  if (
    response.status !== 200 ||
    data?.['fileUpdate'] !== null ||
    !Array.isArray(path) ||
    path[0] !== 'fileUpdate' ||
    extensions?.['code'] !== 'ACCESS_DENIED'
  ) {
    throw new Error(`fileUpdate references did not return ACCESS_DENIED: ${JSON.stringify(response, null, 2)}`);
  }
}

async function waitForReadyFile(fileId: string): Promise<ConformanceGraphqlResult<FileReadData>> {
  let lastResponse: ConformanceGraphqlResult<FileReadData> | null = null;
  for (let attempt = 0; attempt < 30; attempt += 1) {
    lastResponse = await adminClient.runGraphqlRequest<FileReadData>(fileReadQuery, { id: fileId });
    if (lastResponse.payload.data?.node?.fileStatus === 'READY') {
      return lastResponse;
    }
    await delay(2000);
  }

  throw new Error(`Timed out waiting for ${fileId} to become READY: ${JSON.stringify(lastResponse, null, 2)}`);
}

const document = await readFile(absolutePath(requestPath), 'utf8');
const runId = `${Date.now()}`;
const productVariables = { product: { title: `Media references authorization ${runId}` } };
const fileVariables = {
  files: [
    {
      originalSource: 'https://placehold.co/600x400.jpg',
      contentType: 'IMAGE',
      filename: `media-references-authorization-${runId}.jpg`,
    },
  ],
};

let productId: string | undefined;
let fileId: string | undefined;
let delegateToken: string | undefined;
let fixture: JsonRecord | undefined;
let cleanup: JsonRecord = {};

try {
  const productCreate = await adminClient.runGraphql<ProductCreateData>(productCreateMutation, productVariables);
  expectNoUserErrors('productCreate', productCreate.data?.productCreate?.userErrors);
  productId = requireId('productCreate.product.id', productCreate.data?.productCreate?.product?.id);

  const fileCreate = await adminClient.runGraphql<FileCreateData>(fileCreateMutation, fileVariables);
  expectNoUserErrors('fileCreate', fileCreate.data?.fileCreate?.userErrors);
  fileId = requireId('fileCreate.files[0].id', fileCreate.data?.fileCreate?.files?.[0]?.id);
  const readyFile = await waitForReadyFile(fileId);

  const delegateSetup = await adminClient.runGraphqlRequest<JsonRecord>(createDelegateTokenMutation);
  delegateToken = readDelegateToken(delegateSetup);
  const delegateClient = client(delegateToken);
  const variables = { files: [{ id: fileId, referencesToAdd: [productId] }] };
  const accessDenied = await delegateClient.runGraphqlRequest<JsonRecord>(document, variables);
  assertAccessDenied(accessDenied);

  fixture = {
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    document,
    notes: [
      'Captured fileUpdate referencesToAdd authorization with a short-lived delegate token that has read_files/write_files/read_products but lacks product write permission.',
      'Public Admin GraphQL 2026-04 does not expose referencesToAdd on FileCreateInput, so fileCreate reference authorization remains covered by local runtime tests using the same manage-products affordance.',
    ],
    setup: {
      productCreate: { variables: productVariables, response: productCreate },
      fileCreate: { variables: fileVariables, response: fileCreate },
      readyFile,
      delegateSetup: redactDelegateToken(delegateSetup),
    },
    mutation: {
      variables,
      response: accessDenied,
    },
    cleanup: {},
    upstreamCalls: [],
  };

  process.stdout.write(`${JSON.stringify({ fixturePath, response: accessDenied }, null, 2)}\n`);
} finally {
  if (delegateToken) {
    cleanup = {
      ...cleanup,
      delegateDestroy: await adminClient.runGraphqlRequest<JsonRecord>(destroyDelegateTokenMutation, {
        token: delegateToken,
      }),
    };
  }
  if (fileId) {
    cleanup = {
      ...cleanup,
      fileDelete: await adminClient.runGraphqlRequest<FileDeleteData>(fileDeleteMutation, { fileIds: [fileId] }),
    };
  }
  if (productId) {
    cleanup = {
      ...cleanup,
      productDelete: await adminClient.runGraphqlRequest<ProductDeleteData>(productDeleteMutation, {
        input: { id: productId },
      }),
    };
  }
  if (fixture) {
    fixture['cleanup'] = cleanup;
    const absoluteFixturePath = absolutePath(fixturePath);
    await mkdir(path.dirname(absoluteFixturePath), { recursive: true });
    await writeFile(absoluteFixturePath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
  }
}
