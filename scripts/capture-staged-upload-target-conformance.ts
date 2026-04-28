/* oxlint-disable no-console -- CLI scripts intentionally write status output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type GraphqlVariables = Record<string, unknown>;
type UserError = { field?: string[] | null; message?: string | null };
type StagedUploadsCreateData = {
  stagedUploadsCreate?: {
    stagedTargets?: Array<{
      url?: string | null;
      resourceUrl?: string | null;
      parameters?: Array<{ name?: string | null; value?: string | null } | null> | null;
    } | null> | null;
    userErrors?: UserError[] | null;
  } | null;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'media');
const { runGraphql } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
}) as {
  runGraphql: <TData>(query: string, variables?: GraphqlVariables) => Promise<{ data?: TData; errors?: unknown }>;
};

const schemaIntrospectionQuery = `#graphql
  query StagedUploadTargetSchema {
    stagedUploadInput: __type(name: "StagedUploadInput") {
      inputFields {
        name
        type {
          kind
          name
          ofType {
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
    stagedUploadResource: __type(name: "StagedUploadTargetGenerateUploadResource") {
      enumValues {
        name
      }
    }
    stagedUploadHttpMethod: __type(name: "StagedUploadHttpMethodType") {
      enumValues {
        name
      }
    }
  }
`;

const stagedUploadsCreateMutation = `#graphql
  mutation StagedUploadTargetsParity($input: [StagedUploadInput!]!) {
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

const stagedUploadsCreateVariables = {
  input: [
    {
      filename: 'har-405-image.png',
      mimeType: 'image/png',
      resource: 'IMAGE',
      httpMethod: 'POST',
    },
    {
      filename: 'har-405-file.txt',
      mimeType: 'text/plain',
      resource: 'FILE',
      httpMethod: 'POST',
    },
    {
      filename: 'har-405-video.mp4',
      mimeType: 'video/mp4',
      resource: 'VIDEO',
      httpMethod: 'POST',
      fileSize: '4096',
    },
    {
      filename: 'har-405-model.glb',
      mimeType: 'model/gltf-binary',
      resource: 'MODEL_3D',
      httpMethod: 'POST',
      fileSize: '4096',
    },
  ],
};

function expectNoUserErrors(userErrors: UserError[] | null | undefined): void {
  if (Array.isArray(userErrors) && userErrors.length === 0) {
    return;
  }

  throw new Error(`stagedUploadsCreate returned userErrors: ${JSON.stringify(userErrors ?? null, null, 2)}`);
}

await mkdir(outputDir, { recursive: true });

const schema = await runGraphql(schemaIntrospectionQuery);
const response = await runGraphql<StagedUploadsCreateData>(stagedUploadsCreateMutation, stagedUploadsCreateVariables);
expectNoUserErrors(response.data?.stagedUploadsCreate?.userErrors);

const outputPath = path.join(outputDir, 'staged-upload-targets-parity.json');
const payload = {
  notes:
    'HAR-405 captures live stagedUploadsCreate target metadata for representative IMAGE, FILE, VIDEO, and MODEL_3D inputs. No upload bytes are sent; this fixture records signed target metadata only.',
  schema,
  stagedUploadsCreate: {
    variables: stagedUploadsCreateVariables,
    response,
  },
};

await writeFile(outputPath, `${JSON.stringify(payload, null, 2)}\n`, 'utf8');

console.log(
  JSON.stringify(
    {
      ok: true,
      outputPath,
      apiVersion,
      storeDomain,
      targetCount: response.data?.stagedUploadsCreate?.stagedTargets?.length ?? 0,
    },
    null,
    2,
  ),
);
