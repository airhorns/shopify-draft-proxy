import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type CaptureStep = {
  variables: Record<string, unknown>;
  response: ConformanceGraphqlResult;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'products');
const fixturePath = path.join(outputDir, 'collection-update-missing-id.json');
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const updateDocument = `#graphql
  mutation CollectionUpdateMissingId($input: CollectionInput!) {
    collectionUpdate(input: $input) {
      collection {
        id
        title
        handle
      }
      userErrors {
        field
        message
      }
    }
  }
`;

async function captureUpdate(variables: Record<string, unknown>): Promise<CaptureStep> {
  return {
    variables,
    response: await runGraphqlRequest(updateDocument, variables),
  };
}

const runId = `${Date.now()}`;

const missingId = await captureUpdate({
  input: {
    title: `Hermes Missing Id ${runId}`,
  },
});

const unknownId = await captureUpdate({
  input: {
    id: 'gid://shopify/Collection/999999999999999',
    title: `Hermes Unknown Id ${runId}`,
  },
});

await mkdir(outputDir, { recursive: true });
await writeFile(
  fixturePath,
  `${JSON.stringify(
    {
      summary:
        'Live 2026-04 collectionUpdate missing-id BadRequest shape plus present-but-unknown id userError branch.',
      storeDomain,
      apiVersion,
      missingId,
      unknownId,
      cleanup: {},
    },
    null,
    2,
  )}\n`,
  'utf8',
);

// oxlint-disable-next-line no-console -- CLI capture result is intentionally written to stdout.
console.log(JSON.stringify({ ok: true, fixturePath }, null, 2));
