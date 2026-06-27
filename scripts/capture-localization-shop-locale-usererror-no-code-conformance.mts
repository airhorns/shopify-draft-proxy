import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'localization');
const outputPath = path.join(outputDir, 'localization-shop-locale-usererror-no-code.json');

const codeSelectionMutation = `#graphql
  mutation LocalizationShopLocaleUserErrorNoCode(
    $enableLocale: String!
    $updateLocale: String!
    $updateInput: ShopLocaleInput!
    $disableLocale: String!
  ) {
    enable: shopLocaleEnable(locale: $enableLocale) {
      shopLocale {
        locale
      }
      userErrors {
        field
        message
        code
      }
    }
    update: shopLocaleUpdate(locale: $updateLocale, shopLocale: $updateInput) {
      shopLocale {
        locale
      }
      userErrors {
        field
        message
        code
      }
    }
    disable: shopLocaleDisable(locale: $disableLocale) {
      locale
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const codeSelectionVariables = {
  enableLocale: 'en',
  updateLocale: 'fr',
  updateInput: { published: false },
  disableLocale: 'en',
};

const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const { status, payload } = await runGraphqlRequest(codeSelectionMutation, codeSelectionVariables);
if (status < 200 || status >= 300) {
  throw new Error(`Shopify GraphQL request failed with HTTP ${status}: ${JSON.stringify(payload)}`);
}
if (!isRecord(payload)) {
  throw new Error(`Shopify GraphQL response was not an object: ${JSON.stringify(payload)}`);
}
assertUndefinedFieldErrors(payload);

const capture = {
  capturedAt: new Date().toISOString(),
  storeDomain,
  apiVersion,
  rootAvailability: {
    mutations: ['shopLocaleDisable', 'shopLocaleEnable', 'shopLocaleUpdate'],
  },
  codeSelection: {
    query: codeSelectionMutation,
    request: { variables: codeSelectionVariables },
    response: { status, payload },
  },
  upstreamCalls: [],
};

await mkdir(outputDir, { recursive: true });
await writeFile(outputPath, `${JSON.stringify(capture, null, 2)}\n`, 'utf8');

// oxlint-disable-next-line no-console -- CLI capture output is intentionally written to stdout.
console.log(
  JSON.stringify(
    {
      ok: true,
      outputPath,
      storeDomain,
      apiVersion,
    },
    null,
    2,
  ),
);

function isRecord(value: unknown): value is JsonRecord {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function assertUndefinedFieldErrors(payload: JsonRecord): void {
  const errors = payload['errors'];
  if (!Array.isArray(errors) || errors.length !== 3) {
    throw new Error(`Expected three top-level GraphQL errors: ${JSON.stringify(payload)}`);
  }
  if ('data' in payload && payload['data'] !== null) {
    throw new Error(`Expected no data for schema validation failure: ${JSON.stringify(payload)}`);
  }
  for (const error of errors) {
    if (!isRecord(error)) {
      throw new Error(`Expected error object: ${JSON.stringify(error)}`);
    }
    const extensions = error['extensions'];
    if (
      error['message'] !== "Field 'code' doesn't exist on type 'UserError'" ||
      !isRecord(extensions) ||
      extensions['code'] !== 'undefinedField' ||
      extensions['typeName'] !== 'UserError' ||
      extensions['fieldName'] !== 'code'
    ) {
      throw new Error(`Unexpected UserError.code validation payload: ${JSON.stringify(error)}`);
    }
  }
}
