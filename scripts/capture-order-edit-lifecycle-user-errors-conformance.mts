/* oxlint-disable no-console -- CLI script intentionally writes status output. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type CaptureCase = {
  name: string;
  query: string;
  variables: Record<string, unknown>;
  response: {
    status: number;
    payload: unknown;
  };
};

const beginNotFound = `#graphql
  mutation OrderEditLifecycleBeginNotFound($id: ID!) {
    orderEditBegin(id: $id) {
      calculatedOrder {
        id
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const addVariantMissingCalculatedOrder = `#graphql
  mutation OrderEditLifecycleAddVariantMissingCalculatedOrder($id: ID!, $variantId: ID!, $quantity: Int!) {
    orderEditAddVariant(id: $id, variantId: $variantId, quantity: $quantity) {
      calculatedOrder {
        id
      }
      calculatedLineItem {
        id
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const setQuantityMissingCalculatedOrder = `#graphql
  mutation OrderEditLifecycleSetQuantityMissingCalculatedOrder($id: ID!, $lineItemId: ID!, $quantity: Int!) {
    orderEditSetQuantity(id: $id, lineItemId: $lineItemId, quantity: $quantity) {
      calculatedOrder {
        id
      }
      calculatedLineItem {
        id
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const commitMissingCalculatedOrder = `#graphql
  mutation OrderEditLifecycleCommitMissingCalculatedOrder($id: ID!) {
    orderEditCommit(id: $id) {
      order {
        id
        name
      }
      userErrors {
        field
        message
      }
    }
  }
`;

function cleanDocument(document: string): string {
  return document.replace(/^#graphql\n/u, '').trim();
}

async function main(): Promise<void> {
  const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
    defaultApiVersion: '2026-04',
    exitOnMissing: true,
  });
  if (apiVersion !== '2026-04') {
    throw new Error(
      `order-edit lifecycle userErrors capture requires SHOPIFY_CONFORMANCE_API_VERSION=2026-04, got ${apiVersion}`,
    );
  }

  const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
  const { runGraphqlRequest } = createAdminGraphqlClient({
    adminOrigin,
    apiVersion,
    headers: buildAdminAuthHeaders(adminAccessToken),
  });

  const caseInputs = [
    {
      name: 'begin-not-found',
      query: beginNotFound,
      variables: { id: 'gid://shopify/Order/0' },
    },
    {
      name: 'add-variant-missing-calculated-order',
      query: addVariantMissingCalculatedOrder,
      variables: {
        id: 'gid://shopify/CalculatedOrder/999999999999999',
        variantId: 'gid://shopify/ProductVariant/999999999999999',
        quantity: 1,
      },
    },
    {
      name: 'set-quantity-missing-calculated-order',
      query: setQuantityMissingCalculatedOrder,
      variables: {
        id: 'gid://shopify/CalculatedOrder/999999999999999',
        lineItemId: 'gid://shopify/CalculatedLineItem/999999999999999',
        quantity: 1,
      },
    },
    {
      name: 'commit-missing-calculated-order',
      query: commitMissingCalculatedOrder,
      variables: { id: 'gid://shopify/CalculatedOrder/999999999999999' },
    },
  ];

  const cases: CaptureCase[] = [];
  for (const input of caseInputs) {
    const response = await runGraphqlRequest(input.query, input.variables);
    cases.push({
      name: input.name,
      query: cleanDocument(input.query),
      variables: input.variables,
      response,
    });
  }

  const fixturePath = path.join(
    'fixtures',
    'conformance',
    storeDomain,
    apiVersion,
    'orders',
    'order-edit-lifecycle-user-errors.json',
  );
  await mkdir(path.dirname(fixturePath), { recursive: true });
  await writeFile(
    fixturePath,
    `${JSON.stringify(
      {
        scenarioId: 'orderEdit-lifecycle-userErrors',
        apiVersion,
        storeDomain,
        cases,
        upstreamCalls: [],
      },
      null,
      2,
    )}\n`,
  );
  console.log(`Wrote ${fixturePath}`);
}

await main();
