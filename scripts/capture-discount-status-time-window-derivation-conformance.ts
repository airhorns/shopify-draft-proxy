/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { assertDiscountConformanceScopes, probeDiscountConformanceScopes } from './discount-conformance-lib.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});

const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'discounts');
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const adminOptions = {
  adminOrigin,
  apiVersion,
  accessToken: adminAccessToken,
  headers: buildAdminAuthHeaders(adminAccessToken),
};
const { runGraphqlRaw } = createAdminGraphqlClient(adminOptions);

await mkdir(outputDir, { recursive: true });

const scopeProbe = await probeDiscountConformanceScopes(adminOptions);
assertDiscountConformanceScopes(scopeProbe);

const runId = Date.now();
const scheduledCode = `HAR593S${runId}`;
const expiredCode = `HAR593E${runId}`;
const activeCode = `HAR593A${runId}`;

const discountStatusSelection = `#graphql
  codeDiscountNode {
    id
    codeDiscount {
      __typename
      ... on DiscountCodeBasic {
        title
        status
        startsAt
        endsAt
      }
    }
  }
  userErrors {
    field
    message
    code
    extraInfo
  }
`;

const createDocument = `#graphql
  mutation DiscountStatusTimeWindowDerivationCreate(
    $scheduled: DiscountCodeBasicInput!
    $expired: DiscountCodeBasicInput!
    $active: DiscountCodeBasicInput!
  ) {
    scheduled: discountCodeBasicCreate(basicCodeDiscount: $scheduled) {
      ${discountStatusSelection}
    }
    expired: discountCodeBasicCreate(basicCodeDiscount: $expired) {
      ${discountStatusSelection}
    }
    active: discountCodeBasicCreate(basicCodeDiscount: $active) {
      ${discountStatusSelection}
    }
  }
`;

const readDocument = `#graphql
  query DiscountStatusTimeWindowDerivationRead(
    $scheduledId: ID!
    $expiredId: ID!
    $activeId: ID!
    $scheduledQuery: String!
    $expiredQuery: String!
  ) {
    scheduledNode: codeDiscountNode(id: $scheduledId) {
      codeDiscount {
        __typename
        ... on DiscountCodeBasic {
          title
          status
          startsAt
          endsAt
        }
      }
    }
    expiredNode: codeDiscountNode(id: $expiredId) {
      codeDiscount {
        __typename
        ... on DiscountCodeBasic {
          title
          status
          startsAt
          endsAt
        }
      }
    }
    activeNode: discountNode(id: $activeId) {
      discount {
        __typename
        ... on DiscountCodeBasic {
          title
          status
          startsAt
          endsAt
        }
      }
    }
    scheduledDiscountNodes: discountNodes(first: 5, query: $scheduledQuery) {
      nodes {
        discount {
          __typename
          ... on DiscountCodeBasic {
            title
            status
          }
        }
      }
    }
    expiredDiscountNodesCount: discountNodesCount(query: $expiredQuery) {
      count
      precision
    }
  }
`;

const listDiscountsDocument = `#graphql
  query DiscountStatusTimeWindowDerivationListExisting {
    discountNodes(first: 50, query: "title:'HAR-593'") {
      nodes {
        id
        discount {
          __typename
        }
      }
    }
  }
`;

const deleteDocument = `#graphql
  mutation DiscountStatusTimeWindowDerivationDelete($id: ID!) {
    discountCodeDelete(id: $id) {
      deletedCodeDiscountId
      userErrors {
        field
        message
        code
        extraInfo
      }
    }
  }
`;

const deleteAutomaticDocument = `#graphql
  mutation DiscountStatusTimeWindowDerivationDeleteAutomatic($id: ID!) {
    discountAutomaticDelete(id: $id) {
      deletedAutomaticDiscountId
      userErrors {
        field
        message
        code
        extraInfo
      }
    }
  }
`;

const createVariables = {
  scheduled: {
    title: `HAR-593 scheduled ${runId}`,
    code: scheduledCode,
    startsAt: '2099-01-01T00:00:00Z',
    context: { all: 'ALL' },
    customerGets: {
      value: { percentage: 0.1 },
      items: { all: true },
    },
  },
  expired: {
    title: `HAR-593 expired ${runId}`,
    code: expiredCode,
    startsAt: '2019-01-01T00:00:00Z',
    endsAt: '2020-01-01T00:00:00Z',
    context: { all: 'ALL' },
    customerGets: {
      value: { percentage: 0.1 },
      items: { all: true },
    },
  },
  active: {
    title: `HAR-593 active ${runId}`,
    code: activeCode,
    startsAt: '2020-01-01T00:00:00Z',
    endsAt: '2099-01-01T00:00:00Z',
    context: { all: 'ALL' },
    customerGets: {
      value: { percentage: 0.1 },
      items: { all: true },
    },
  },
};

const preCleanup = await deleteExistingDiscounts();
const create = await runGraphqlRaw(createDocument, createVariables);
const ids = {
  scheduled: readCreatedDiscountId(create, 'scheduled'),
  expired: readCreatedDiscountId(create, 'expired'),
  active: readCreatedDiscountId(create, 'active'),
};
const readVariables = {
  scheduledId: ids.scheduled,
  expiredId: ids.expired,
  activeId: ids.active,
  scheduledQuery: `status:scheduled title:'${createVariables.scheduled.title}'`,
  expiredQuery: `status:expired title:'${createVariables.expired.title}'`,
};
const read = await pollStatusRead(readVariables);

const cleanup = {
  scheduled: await runGraphqlRaw(deleteDocument, { id: ids.scheduled }),
  expired: await runGraphqlRaw(deleteDocument, { id: ids.expired }),
  active: await runGraphqlRaw(deleteDocument, { id: ids.active }),
};

const output = {
  variables: {
    scheduledCode,
    expiredCode,
    activeCode,
    ...ids,
  },
  requests: {
    create: { query: createDocument, variables: createVariables },
    read: { query: readDocument, variables: readVariables },
    delete: { query: deleteDocument },
  },
  scopeProbe,
  preCleanup,
  create,
  read,
  cleanup,
  upstreamCalls: [],
};

const outputPath = path.join(outputDir, 'discount-status-time-window-derivation.json');
await writeFile(outputPath, `${JSON.stringify(output, null, 2)}\n`, 'utf8');

console.log(
  JSON.stringify(
    {
      ok: true,
      apiVersion,
      outputPath,
      ids,
    },
    null,
    2,
  ),
);

function readCreatedDiscountId(result: unknown, key: 'scheduled' | 'expired' | 'active'): string {
  const id = (
    result as {
      payload?: {
        data?: Record<string, { codeDiscountNode?: { id?: unknown } } | undefined>;
      };
    }
  ).payload?.data?.[key]?.codeDiscountNode?.id;

  if (typeof id !== 'string') {
    throw new Error(`Discount status-window create did not return ${key} id: ${JSON.stringify(result)}`);
  }

  return id;
}

async function deleteExistingDiscounts(): Promise<unknown[]> {
  const listed = await runGraphqlRaw(listDiscountsDocument);
  const nodes =
    (
      listed.payload as {
        data?: {
          discountNodes?: {
            nodes?: Array<{ id?: unknown; discount?: { __typename?: unknown } }>;
          };
        };
      }
    ).data?.discountNodes?.nodes ?? [];

  const deleted: unknown[] = [];
  for (const node of nodes) {
    if (typeof node.id !== 'string') continue;

    const typename = node.discount?.__typename;
    const document =
      typeof typename === 'string' && typename.startsWith('DiscountAutomatic')
        ? deleteAutomaticDocument
        : deleteDocument;
    deleted.push(await runGraphqlRaw(document, { id: node.id }));
  }

  return deleted;
}

async function pollStatusRead(variables: Record<string, unknown>): Promise<unknown> {
  let last = await runGraphqlRaw(readDocument, variables);
  for (let attempt = 0; attempt < 10; attempt += 1) {
    if (statusFiltersAreReady(last)) return last;

    await new Promise((resolve) => setTimeout(resolve, 2000));
    last = await runGraphqlRaw(readDocument, variables);
  }
  return last;
}

function statusFiltersAreReady(result: unknown): boolean {
  const data = (
    result as {
      payload?: {
        data?: {
          scheduledDiscountNodes?: { nodes?: unknown[] };
          expiredDiscountNodesCount?: { count?: unknown };
        };
      };
    }
  ).payload?.data;

  return (
    Array.isArray(data?.scheduledDiscountNodes?.nodes) &&
    data.scheduledDiscountNodes.nodes.length === 1 &&
    data.expiredDiscountNodesCount?.count === 1
  );
}
