/* oxlint-disable no-console -- CLI scripts intentionally write status output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type GraphqlVariables = Record<string, unknown>;
type StagedUploadsCreateData = {
  stagedUploadsCreate?: {
    stagedTargets?: Array<{
      url?: string | null;
      resourceUrl?: string | null;
      parameters?: Array<{ name?: string | null; value?: string | null } | null> | null;
    } | null> | null;
    userErrors?: Array<{ field?: string[] | null; message?: string | null } | null> | null;
  } | null;
};

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

const stagedUploadsCreateMutation = `#graphql
  mutation StagedUploadDefaultHttpMethod($input: [StagedUploadInput!]!) {
    stagedUploadsCreate(input: $input) {
      stagedTargets {
        url
        resourceUrl
        parameters {
          name
          value
        }
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const variables = {
  input: [
    {
      filename: 'default-method-image.png',
      mimeType: 'image/png',
      resource: 'IMAGE',
    },
    {
      filename: 'default-method-file.txt',
      mimeType: 'text/plain',
      resource: 'FILE',
    },
  ],
};

await mkdir(outputDir, { recursive: true });

const response = await runGraphqlRequest<StagedUploadsCreateData>(stagedUploadsCreateMutation, variables);
const stagedUploadsCreate = response.payload.data?.stagedUploadsCreate;
if (stagedUploadsCreate?.userErrors?.length) {
  throw new Error(
    `stagedUploadsCreate returned userErrors: ${JSON.stringify(stagedUploadsCreate.userErrors, null, 2)}`,
  );
}

const cases: CaptureCase[] = [
  {
    name: 'imageAndFileOmittedHttpMethod',
    request: {
      document: stagedUploadsCreateMutation,
      variables,
    },
    response,
  },
];

const outputPath = path.join(outputDir, 'media-staged-uploads-create-default-http-method.json');
const payload = {
  notes:
    'Captures stagedUploadsCreate target metadata when IMAGE and FILE inputs omit httpMethod. No upload bytes are sent; this fixture records Shopify default-method target parameter shape only.',
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
      targetCount: stagedUploadsCreate?.stagedTargets?.length ?? 0,
    },
    null,
    2,
  ),
);
