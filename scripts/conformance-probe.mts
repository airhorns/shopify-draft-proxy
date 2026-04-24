// @ts-nocheck
import 'dotenv/config';

import { resolveConformanceTargetEnv } from './conformance-env-guard.js';
import { runAdminGraphqlRequest } from './conformance-graphql-client.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

let conformanceTarget;
try {
  conformanceTarget = resolveConformanceTargetEnv();
} catch (error) {
  // oxlint-disable-next-line no-console -- CLI error output is intentionally written to stderr.
  console.error(error instanceof Error ? error.message : String(error));
  process.exit(1);
}

const { adminOrigin, storeDomain } = conformanceTarget;
const apiVersion = process.env['SHOPIFY_CONFORMANCE_API_VERSION'] || '2025-01';
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });

const query = `#graphql
  query ConformanceProbe {
    shop {
      id
      name
      myshopifyDomain
      primaryDomain {
        host
        url
      }
    }
  }
`;

const { status, payload } = await runAdminGraphqlRequest(
  { adminOrigin, apiVersion, headers: buildAdminAuthHeaders(adminAccessToken) },
  query,
);

if (status < 200 || status >= 300 || payload.errors) {
  // oxlint-disable-next-line no-console -- CLI error output is intentionally written to stderr.
  console.error(JSON.stringify({ status, payload }, null, 2));
  process.exit(1);
}

// oxlint-disable-next-line no-console -- CLI probe result is intentionally written to stdout.
console.log(
  JSON.stringify(
    {
      ok: true,
      apiVersion,
      storeDomain,
      shop: payload.data?.shop ?? null,
    },
    null,
    2,
  ),
);
