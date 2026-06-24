/* oxlint-disable no-console -- CLI scripts intentionally write status output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type GraphqlVariables = Record<string, unknown>;

type CaptureCase = {
  name: string;
  request: {
    document: string;
    variables: GraphqlVariables;
  };
  response: {
    status: number;
    payload: unknown;
  };
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'media');
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const missingFilenameDocument = `#graphql
  mutation StagedUploadRequiredArgsMissingFilename {
    stagedUploadsCreate(input: [{ resource: FILE, mimeType: "text/plain" }]) {
      stagedTargets {
        url
        resourceUrl
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const missingMimeTypeDocument = `#graphql
  mutation StagedUploadRequiredArgsMissingMimeType {
    stagedUploadsCreate(input: [{ resource: FILE, filename: "required-args.txt" }]) {
      stagedTargets {
        url
        resourceUrl
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const requests: Array<{ name: string; document: string; variables: GraphqlVariables }> = [
  {
    name: 'missingFilename',
    document: missingFilenameDocument,
    variables: {},
  },
  {
    name: 'missingMimeType',
    document: missingMimeTypeDocument,
    variables: {},
  },
];

await mkdir(outputDir, { recursive: true });

const cases: CaptureCase[] = [];
for (const request of requests) {
  const response = await runGraphqlRequest(request.document, request.variables);
  cases.push({
    name: request.name,
    request: {
      document: request.document,
      variables: request.variables,
    },
    response,
  });
}

const outputPath = path.join(outputDir, 'staged_uploads_create_required_args.json');
const payload = {
  notes:
    'Captures Shopify Admin GraphQL stagedUploadsCreate schema coercion when required StagedUploadInput filename or mimeType is omitted. No upload target is allocated and no upload bytes are sent.',
  storeDomain,
  apiVersion,
  cases,
  upstreamCalls: [],
};

await writeFile(outputPath, `${JSON.stringify(payload, null, 2)}\n`, 'utf8');

console.log(
  JSON.stringify(
    {
      ok: true,
      outputPath,
      apiVersion,
      storeDomain,
      caseCount: cases.length,
    },
    null,
    2,
  ),
);
