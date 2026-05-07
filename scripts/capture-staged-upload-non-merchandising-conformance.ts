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

const schemaIntrospectionQuery = `#graphql
  query StagedUploadNonMerchandisingSchema {
    stagedUploadResource: __type(name: "StagedUploadTargetGenerateUploadResource") {
      enumValues(includeDeprecated: true) {
        name
        isDeprecated
        deprecationReason
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
  mutation StagedUploadNonMerchandising($input: [StagedUploadInput!]!) {
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

const requests: Array<{ name: string; variables: GraphqlVariables }> = [
  {
    name: 'bulkMutationVariablesPost',
    variables: {
      input: [
        {
          resource: 'BULK_MUTATION_VARIABLES',
          filename: 'staged-upload-vars.jsonl',
          mimeType: 'text/jsonl',
          httpMethod: 'POST',
        },
      ],
    },
  },
  {
    name: 'bulkMutationVariablesPut',
    variables: {
      input: [
        {
          resource: 'BULK_MUTATION_VARIABLES',
          filename: 'staged-upload-vars-put.jsonl',
          mimeType: 'text/jsonl',
          httpMethod: 'PUT',
        },
      ],
    },
  },
  {
    name: 'urlRedirectImportPost',
    variables: {
      input: [
        {
          resource: 'URL_REDIRECT_IMPORT',
          filename: 'staged-upload-url-redirects.csv',
          mimeType: 'text/csv',
          httpMethod: 'POST',
        },
      ],
    },
  },
  {
    name: 'urlRedirectImportPut',
    variables: {
      input: [
        {
          resource: 'URL_REDIRECT_IMPORT',
          filename: 'staged-upload-url-redirects-put.csv',
          mimeType: 'text/csv',
          httpMethod: 'PUT',
        },
      ],
    },
  },
  {
    name: 'returnLabelPost',
    variables: {
      input: [
        {
          resource: 'RETURN_LABEL',
          filename: 'staged-upload-return-label.pdf',
          mimeType: 'application/pdf',
          httpMethod: 'POST',
        },
      ],
    },
  },
  {
    name: 'returnLabelPut',
    variables: {
      input: [
        {
          resource: 'RETURN_LABEL',
          filename: 'staged-upload-return-label-put.pdf',
          mimeType: 'application/pdf',
          httpMethod: 'PUT',
        },
      ],
    },
  },
  {
    name: 'disputeFileUploadPost',
    variables: {
      input: [
        {
          resource: 'DISPUTE_FILE_UPLOAD',
          filename: 'staged-upload-dispute.pdf',
          mimeType: 'application/pdf',
          httpMethod: 'POST',
        },
      ],
    },
  },
  {
    name: 'disputeFileUploadPut',
    variables: {
      input: [
        {
          resource: 'DISPUTE_FILE_UPLOAD',
          filename: 'staged-upload-dispute-put.pdf',
          mimeType: 'application/pdf',
          httpMethod: 'PUT',
        },
      ],
    },
  },
  {
    name: 'shopImageScopeBlocked',
    variables: {
      input: [
        {
          resource: 'SHOP_IMAGE',
          filename: 'staged-upload-shop-logo.png',
          mimeType: 'image/png',
          httpMethod: 'POST',
        },
      ],
    },
  },
  {
    name: 'customerImportInvalidResource',
    variables: {
      input: [
        {
          resource: 'CUSTOMER_IMPORT',
          filename: 'staged-upload-customers.csv',
          mimeType: 'text/csv',
          httpMethod: 'POST',
        },
      ],
    },
  },
  {
    name: 'inventoryImportInvalidResource',
    variables: {
      input: [
        {
          resource: 'INVENTORY_IMPORT',
          filename: 'staged-upload-inventory.csv',
          mimeType: 'text/csv',
          httpMethod: 'POST',
        },
      ],
    },
  },
  {
    name: 'articleImageInvalidResource',
    variables: {
      input: [
        {
          resource: 'ARTICLE_IMAGE',
          filename: 'staged-upload-article.png',
          mimeType: 'image/png',
          httpMethod: 'POST',
        },
      ],
    },
  },
  {
    name: 'themeArchiveInvalidResource',
    variables: {
      input: [
        {
          resource: 'THEME_ARCHIVE',
          filename: 'staged-upload-theme.zip',
          mimeType: 'application/zip',
          httpMethod: 'POST',
        },
      ],
    },
  },
  {
    name: 'translationsImportInvalidResource',
    variables: {
      input: [
        {
          resource: 'TRANSLATIONS_IMPORT',
          filename: 'staged-upload-translations.csv',
          mimeType: 'text/csv',
          httpMethod: 'POST',
        },
      ],
    },
  },
];

await mkdir(outputDir, { recursive: true });

const schema = await runGraphqlRequest(schemaIntrospectionQuery);
const cases: CaptureCase[] = [];
for (const request of requests) {
  const response = await runGraphqlRequest<StagedUploadsCreateData>(stagedUploadsCreateMutation, request.variables);
  cases.push({
    name: request.name,
    request: {
      document: stagedUploadsCreateMutation,
      variables: request.variables,
    },
    response,
  });
}

const outputPath = path.join(outputDir, 'media-staged-uploads-create-non-merchandising.json');
const payload = {
  notes:
    'Captures Shopify Admin GraphQL 2026-04 stagedUploadsCreate target metadata for non-merchandising upload resources. No upload bytes are sent; this fixture records signed target metadata, SHOP_IMAGE access denial with the current conformance app scopes, and schema-invalid resource values that are not exposed by the public enum.',
  storeDomain,
  apiVersion,
  schema,
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
