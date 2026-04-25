// @ts-nocheck
/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
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

function buildSchemaDecisionNote(missingRoots) {
  const missingRootLines = missingRoots.flatMap((missingRoot) => [
    `- \`${missingRoot.operationName}\``,
    `  > ${missingRoot.message}`,
  ]);

  return [
    '# Product variant compatibility live schema decision',
    '',
    '## Probe result',
    '',
    'Attempted to probe the legacy single-variant compatibility roots against the live Shopify Admin GraphQL schema used by the conformance store.',
    '',
    '## Evidence',
    '',
    `- store: \`${storeDomain}\``,
    `- api version: \`${apiVersion}\``,
    '- dedicated probe command: `corepack pnpm conformance:probe-product-variant-compatibility-roots`',
    '- live schema rejected all three compatibility roots before any direct live parity capture could run:',
    ...missingRootLines,
    '',
    '## Decision',
    '',
    'The repo still implements `productVariantCreate`, `productVariantUpdate`, and `productVariantDelete` as compatibility roots, but the current live schema on the conformance store does not expose those mutation fields. Without direct live roots, this family remains compatibility-wrapper parity backed by the adjacent live-supported bulk variant captures.',
    '',
    '## What was completed anyway',
    '',
    '1. refreshed durable schema evidence from the current host token and store so future runs can verify the schema drift explicitly',
    '2. preserved the adjacent live-supported bulk variant family as the real parity baseline (`productVariantsBulkCreate`, `productVariantsBulkUpdate`, `productVariantsBulkDelete`) rather than faking direct coverage for missing roots',
    '3. kept the decision in Linear/HAR-189 and parity metadata instead of repository pending Markdown',
    '',
    '## Recommended next step',
    '',
    'If Shopify reintroduces these compatibility roots on a future API version/store, rerun `corepack pnpm conformance:probe-product-variant-compatibility-roots` and then capture direct live parity. Otherwise keep treating this family as compatibility-wrapper parity while the bulk variant family remains the covered live path.',
    '',
  ].join('\n');
}

const missingRoots = [];
for (const probe of probeDefinitions) {
  const result = await runGraphqlRaw(probe.query, probe.variables);
  missingRoots.push(...parseMissingRootErrors(result, probe.operationName));
}

if (missingRoots.length === probeDefinitions.length) {
  const note = buildSchemaDecisionNote(missingRoots);
  console.log(
    JSON.stringify(
      {
        ok: true,
        legacyRootsExposed: false,
        linearIssue,
        decisionSummary: note,
        missingRoots,
      },
      null,
      2,
    ),
  );
  process.exit(0);
}

console.log(
  JSON.stringify(
    {
      ok: true,
      legacyRootsExposed: true,
      missingRoots,
      message:
        'At least one legacy single-variant compatibility root is present in the live schema. Reassess declared-gap status before keeping the blocker note.',
    },
    null,
    2,
  ),
);
