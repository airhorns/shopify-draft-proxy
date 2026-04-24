// @ts-nocheck
/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const requiredVars = ['SHOPIFY_CONFORMANCE_STORE_DOMAIN', 'SHOPIFY_CONFORMANCE_ADMIN_ORIGIN'];

const missingVars = requiredVars.filter((name) => !process.env[name]);
if (missingVars.length > 0) {
  console.error(`Missing required environment variables: ${missingVars.join(', ')}`);
  process.exit(1);
}

const storeDomain = process.env['SHOPIFY_CONFORMANCE_STORE_DOMAIN'];
const adminOrigin = process.env['SHOPIFY_CONFORMANCE_ADMIN_ORIGIN'];
const apiVersion = process.env['SHOPIFY_CONFORMANCE_API_VERSION'] || '2025-01';
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const linearIssue = 'HAR-189';
const { runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const createProbeMutation = `#graphql
  mutation ProductVariantCreateCompatibilityProbe($input: ProductVariantInput!) {
    productVariantCreate(input: $input) {
      productVariant {
        id
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const updateProbeMutation = `#graphql
  mutation ProductVariantUpdateCompatibilityProbe($input: ProductVariantInput!) {
    productVariantUpdate(input: $input) {
      productVariant {
        id
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const deleteProbeMutation = `#graphql
  mutation ProductVariantDeleteCompatibilityProbe($id: ID!) {
    productVariantDelete(id: $id) {
      deletedProductVariantId
      userErrors {
        field
        message
      }
    }
  }
`;

const probeDefinitions = [
  {
    operationName: 'productVariantCreate',
    query: createProbeMutation,
    variables: {
      input: {
        productId: 'gid://shopify/Product/1',
        title: 'Compatibility Probe Variant',
      },
    },
  },
  {
    operationName: 'productVariantUpdate',
    query: updateProbeMutation,
    variables: {
      input: {
        id: 'gid://shopify/ProductVariant/1',
        title: 'Compatibility Probe Variant Update',
      },
    },
  },
  {
    operationName: 'productVariantDelete',
    query: deleteProbeMutation,
    variables: {
      id: 'gid://shopify/ProductVariant/1',
    },
  },
];

function parseMissingRootErrors(result, expectedOperationName) {
  const errors = Array.isArray(result?.payload?.errors) ? result.payload.errors : [];

  return errors
    .filter((error) => typeof error?.message === 'string')
    .filter((error) => error.message.includes(`Field '${expectedOperationName}' doesn't exist on type 'Mutation'`))
    .map((error) => ({
      operationName: expectedOperationName,
      message: error.message,
    }));
}

function buildBlockerNote(blockers) {
  const blockerLines = blockers.flatMap((blocker) => [`- \`${blocker.operationName}\``, `  > ${blocker.message}`]);

  return [
    '# Product variant compatibility live schema blocker',
    '',
    '## What failed',
    '',
    'Attempted to probe the legacy single-variant compatibility roots against the live Shopify Admin GraphQL schema used by the conformance store.',
    '',
    '## Evidence',
    '',
    `- store: \`${storeDomain}\``,
    `- api version: \`${apiVersion}\``,
    '- dedicated probe command: `corepack pnpm conformance:probe-product-variant-compatibility-roots`',
    '- live schema rejected all three compatibility roots before any write-path parity capture could run:',
    ...blockerLines,
    '',
    '## Why this blocks closure',
    '',
    'The repo still implements `productVariantCreate`, `productVariantUpdate`, and `productVariantDelete` as compatibility roots, but the current 2025-01 live schema on the conformance store does not expose those mutation fields. Without direct live roots, this family cannot be promoted from declared-gap to covered via first-party mutation capture on the current store/api-version pair.',
    '',
    '## What was completed anyway',
    '',
    '1. added a dedicated live-schema probe command for the single-variant compatibility family instead of leaving the blocker as an inferred sentence in generated docs',
    '2. refreshed durable blocker evidence from the current host token and store so future runs can verify the schema drift explicitly',
    '3. preserved the adjacent live-supported bulk variant family as the real parity baseline (`productVariantsBulkCreate`, `productVariantsBulkUpdate`, `productVariantsBulkDelete`) rather than faking direct coverage for missing roots',
    '',
    '## Recommended next step',
    '',
    'If Shopify reintroduces these compatibility roots on a future API version/store, rerun `corepack pnpm conformance:probe-product-variant-compatibility-roots` and then capture direct live parity. Otherwise keep treating this family as a compatibility-only declared gap while the bulk variant family remains the covered live path.',
    '',
  ].join('\n');
}

const blockers = [];
for (const probe of probeDefinitions) {
  const result = await runGraphqlRaw(probe.query, probe.variables);
  blockers.push(...parseMissingRootErrors(result, probe.operationName));
}

if (blockers.length === probeDefinitions.length) {
  const note = buildBlockerNote(blockers);
  console.log(
    JSON.stringify(
      {
        ok: false,
        blocked: true,
        linearIssue,
        blockerSummary: note,
        blockers,
      },
      null,
      2,
    ),
  );
  process.exit(1);
}

console.log(
  JSON.stringify(
    {
      ok: true,
      blockers,
      message:
        'At least one legacy single-variant compatibility root is present in the live schema. Reassess declared-gap status before keeping the blocker note.',
    },
    null,
    2,
  ),
);
