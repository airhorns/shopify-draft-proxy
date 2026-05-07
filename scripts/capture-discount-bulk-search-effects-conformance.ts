/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { readConformanceScriptConfig } from './conformance-script-config.js';
import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { assertDiscountConformanceScopes, probeDiscountConformanceScopes } from './discount-conformance-lib.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});

const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'discounts');
const outputPath = path.join(outputDir, 'discount-bulk-search-effects.json');

const [
  setupDocument,
  createCodeDocument,
  createCodeAndAutomaticDocument,
  createAutomaticDocument,
  codeActivateDocument,
  codeDeactivateDocument,
  codeDeleteDocument,
  automaticDeleteDocument,
  activateReadDocument,
  readCodeDocument,
  readCodeDeleteDocument,
  readAutomaticDocument,
] = await Promise.all([
  readFile('config/parity-requests/discounts/discount-bulk-search-effects-setup.graphql', 'utf8'),
  readFile('config/parity-requests/discounts/discount-bulk-search-effects-create-code.graphql', 'utf8'),
  readFile('config/parity-requests/discounts/discount-bulk-search-effects-create-code-and-automatic.graphql', 'utf8'),
  readFile('config/parity-requests/discounts/discount-bulk-search-effects-create-automatic.graphql', 'utf8'),
  readFile('config/parity-requests/discounts/discount-bulk-search-effects-code-activate.graphql', 'utf8'),
  readFile('config/parity-requests/discounts/discount-bulk-search-effects-code-deactivate.graphql', 'utf8'),
  readFile('config/parity-requests/discounts/discount-bulk-search-effects-code-delete.graphql', 'utf8'),
  readFile('config/parity-requests/discounts/discount-bulk-search-effects-automatic-delete.graphql', 'utf8'),
  readFile('config/parity-requests/discounts/discount-bulk-search-effects-activate-read.graphql', 'utf8'),
  readFile('config/parity-requests/discounts/discount-bulk-search-effects-read-code.graphql', 'utf8'),
  readFile('config/parity-requests/discounts/discount-bulk-search-effects-read-code-delete.graphql', 'utf8'),
  readFile('config/parity-requests/discounts/discount-bulk-search-effects-read-automatic.graphql', 'utf8'),
]);

const codeDeleteCleanupDocument = `#graphql
  mutation DiscountBulkSearchEffectsCodeCleanup($id: ID!) {
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

const automaticDeleteCleanupDocument = `#graphql
  mutation DiscountBulkSearchEffectsAutomaticCleanup($id: ID!) {
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

const safetyProbeDocument = `#graphql
  query DiscountBulkSearchEffectsSafetyProbe {
    scheduledCode: codeDiscountNodes(first: 5, query: "status:scheduled") {
      nodes {
        id
      }
    }
    activeCode: codeDiscountNodes(first: 5, query: "status:active") {
      nodes {
        id
      }
    }
    scheduledAutomatic: automaticDiscountNodes(first: 5, query: "status:scheduled") {
      nodes {
        id
      }
    }
  }
`;

const codeSearchProbeDocument = `#graphql
  query DiscountBulkSearchEffectsCodeSearchProbe($query: String!) {
    codeDiscountNodes(first: 5, query: $query) {
      nodes {
        id
        codeDiscount {
          __typename
          ... on DiscountCodeBasic {
            title
            status
          }
        }
      }
    }
  }
`;

const automaticSearchProbeDocument = `#graphql
  query DiscountBulkSearchEffectsAutomaticSearchProbe($query: String!) {
    automaticDiscountNodes(first: 5, query: $query) {
      nodes {
        id
        automaticDiscount {
          __typename
          ... on DiscountAutomaticBasic {
            title
            status
          }
        }
      }
    }
  }
`;

const codeDeleteCodeFieldProbeDocument = `#graphql
  mutation DiscountBulkSearchEffectsCodeDeleteCodeFieldProbe($search: String!) {
    discountCodeBulkDelete(search: $search) {
      job {
        done
      }
      userErrors {
        field
        message
        code
        extraInfo
      }
    }
  }
`;

const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const adminOptions = {
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
};
const { runGraphqlRaw } = createAdminGraphqlClient(adminOptions);

const scopeProbe = await probeDiscountConformanceScopes(adminOptions);
assertDiscountConformanceScopes(scopeProbe);

const runId = Date.now();
const activeStartsAt = '2020-01-01T00:00:00Z';
const activeEndsAt = '2099-01-01T00:00:00Z';
const scheduledStartsAt = '2099-01-02T00:00:00Z';
const scheduledEndsAt = '2099-12-31T00:00:00Z';
const scheduledTitle = `DraftProxyBulkScheduled${runId}`;
const deactivateTitle = `DraftProxyBulkDeactivate${runId}`;
const deleteSharedTitle = `DraftProxyBulkDeleteShared${runId}`;
const automaticDeleteTitle = `DraftProxyBulkAutomatic${runId}`;
const scheduledCode = `DPBSA${runId}`;
const deactivateCode = `DPBDA${runId}`;
const deleteCode = `DPBDL${runId}`;
const createdDiscountIds: string[] = [];

const setupVariables = {
  scheduledCodeInput: codeInput(scheduledTitle, scheduledCode, scheduledStartsAt, scheduledEndsAt),
};
const codeActivateVariables = {
  search: 'status:scheduled',
};
const deactivateSetupVariables = {
  input: codeInput(deactivateTitle, deactivateCode, activeStartsAt, activeEndsAt),
};
const codeDeactivateVariables = {
  search: 'status:active',
};
const codeDeleteSetupVariables = {
  codeInput: codeInput(deleteSharedTitle, deleteCode, activeStartsAt, activeEndsAt),
  automaticInput: automaticInput(deleteSharedTitle, activeStartsAt, activeEndsAt),
};
const codeDeleteVariables = {
  search: 'status:active',
};
const automaticDeleteSetupVariables = {
  input: automaticInput(automaticDeleteTitle, scheduledStartsAt, scheduledEndsAt),
};
const automaticDeleteVariables = {
  search: 'status:scheduled',
};

function codeInput(title: string, code: string, startsAt: string, endsAt: string): JsonRecord {
  return {
    title,
    code,
    startsAt,
    endsAt,
    combinesWith: {
      productDiscounts: false,
      orderDiscounts: true,
      shippingDiscounts: false,
    },
    context: {
      all: 'ALL',
    },
    minimumRequirement: {
      subtotal: {
        greaterThanOrEqualToSubtotal: '1.00',
      },
    },
    customerGets: {
      value: {
        percentage: 0.1,
      },
      items: {
        all: true,
      },
    },
  };
}

function automaticInput(title: string, startsAt: string, endsAt: string): JsonRecord {
  return {
    title,
    startsAt,
    endsAt,
    combinesWith: {
      productDiscounts: false,
      orderDiscounts: true,
      shippingDiscounts: false,
    },
    context: {
      all: 'ALL',
    },
    minimumRequirement: {
      subtotal: {
        greaterThanOrEqualToSubtotal: '1.00',
      },
    },
    customerGets: {
      value: {
        percentage: 0.1,
      },
      items: {
        all: true,
      },
    },
  };
}

function assertSuccess(result: ConformanceGraphqlResult, context: string): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${context} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function readPath(value: unknown, pathSegments: string[]): unknown {
  let current = value;
  for (const segment of pathSegments) {
    if (Array.isArray(current)) {
      const index = Number.parseInt(segment, 10);
      if (!Number.isInteger(index)) {
        return undefined;
      }
      current = current[index];
      continue;
    }
    if (current === null || typeof current !== 'object') {
      return undefined;
    }
    current = (current as JsonRecord)[segment];
  }
  return current;
}

function readRequiredString(result: ConformanceGraphqlResult, pathSegments: string[], context: string): string {
  const value = readPath(result.payload, pathSegments);
  if (typeof value !== 'string' || value.length === 0) {
    throw new Error(`${context} did not return ${pathSegments.join('.')}: ${JSON.stringify(result.payload, null, 2)}`);
  }
  return value;
}

function assertEmptyUserErrors(result: ConformanceGraphqlResult, pathSegments: string[], context: string): void {
  const value = readPath(result.payload, pathSegments);
  if (!Array.isArray(value) || value.length !== 0) {
    throw new Error(`${context} returned userErrors: ${JSON.stringify(value, null, 2)}`);
  }
}

function assertStatus(
  result: ConformanceGraphqlResult,
  pathSegments: string[],
  expected: string,
  context: string,
): void {
  const actual = readPath(result.payload, pathSegments);
  if (actual !== expected) {
    throw new Error(
      `${context} expected status ${expected}, got ${JSON.stringify(actual)} in ${JSON.stringify(
        result.payload,
        null,
        2,
      )}`,
    );
  }
}

function firstDiscountTitle(result: ConformanceGraphqlResult, pathSegments: string[]): unknown {
  return readPath(result.payload, pathSegments);
}

async function sleep(ms: number): Promise<void> {
  await new Promise((resolve) => setTimeout(resolve, ms));
}

async function pollUntil(
  read: () => Promise<ConformanceGraphqlResult>,
  check: (result: ConformanceGraphqlResult) => boolean,
  context: string,
): Promise<ConformanceGraphqlResult> {
  let last: ConformanceGraphqlResult | undefined;
  for (let attempt = 0; attempt < 10; attempt += 1) {
    const result = await read();
    assertSuccess(result, context);
    if (check(result)) {
      return result;
    }
    last = result;
    await sleep(2_000);
  }
  throw new Error(`${context} did not reach expected state: ${JSON.stringify(last?.payload, null, 2)}`);
}

function assertSafetyProbeSafe(result: ConformanceGraphqlResult): void {
  for (const key of ['scheduledCode', 'activeCode', 'scheduledAutomatic']) {
    const nodes = readPath(result.payload, ['data', key, 'nodes']);
    if (!Array.isArray(nodes) || nodes.length !== 0) {
      throw new Error(
        `bulk status-selector capture is unsafe; pre-existing ${key} discounts were found: ${JSON.stringify(
          nodes,
          null,
          2,
        )}`,
      );
    }
  }
}

async function cleanupDiscounts(): Promise<ConformanceGraphqlResult[]> {
  const cleanupResults: ConformanceGraphqlResult[] = [];
  for (const id of createdDiscountIds.slice().reverse()) {
    try {
      if (id.includes('/DiscountCodeNode/')) {
        cleanupResults.push(await runGraphqlRaw(codeDeleteCleanupDocument, { id }));
      } else {
        cleanupResults.push(await runGraphqlRaw(automaticDeleteCleanupDocument, { id }));
      }
    } catch (error) {
      console.error(`cleanup failed for ${id}: ${(error as Error).message}`);
    }
  }
  return cleanupResults;
}

try {
  const safetyProbeBefore = await runGraphqlRaw(safetyProbeDocument);
  assertSuccess(safetyProbeBefore, 'bulk search effects safety probe');
  assertSafetyProbeSafe(safetyProbeBefore);

  const codeDeleteCodeFieldProbe = await runGraphqlRaw(codeDeleteCodeFieldProbeDocument, {
    search: `code:DRAFT_PROXY_NO_MATCH_${runId}`,
  });
  assertSuccess(codeDeleteCodeFieldProbe, 'code delete code-field probe');

  const setup = await runGraphqlRaw(setupDocument, setupVariables);
  assertSuccess(setup, 'setup create');
  assertEmptyUserErrors(setup, ['data', 'scheduledCode', 'userErrors'], 'scheduled code create');
  const scheduledCodeId = readRequiredString(
    setup,
    ['data', 'scheduledCode', 'codeDiscountNode', 'id'],
    'scheduled code create',
  );
  createdDiscountIds.push(scheduledCodeId);

  await pollUntil(
    () => runGraphqlRaw(codeSearchProbeDocument, { query: 'status:scheduled' }),
    (result) =>
      firstDiscountTitle(result, ['data', 'codeDiscountNodes', 'nodes', '0', 'codeDiscount', 'title']) ===
      scheduledTitle,
    'scheduled code visible to bulk search selector',
  );
  const codeActivate = await runGraphqlRaw(codeActivateDocument, codeActivateVariables);
  assertSuccess(codeActivate, 'code bulk activate');
  assertEmptyUserErrors(codeActivate, ['data', 'discountCodeBulkActivate', 'userErrors'], 'code bulk activate');
  const activateReadVariables = {
    activatedQuery: 'status:active',
  };
  const readAfterCodeActivate = await pollUntil(
    () => runGraphqlRaw(activateReadDocument, activateReadVariables),
    (result) =>
      readPath(result.payload, ['data', 'activatedDiscountNodes', 'nodes', '0', 'discount', 'status']) === 'ACTIVE',
    'read after code bulk activate',
  );
  const cleanupAfterCodeActivate = await runGraphqlRaw(codeDeleteCleanupDocument, { id: scheduledCodeId });

  const deactivateSetup = await runGraphqlRaw(createCodeDocument, deactivateSetupVariables);
  assertSuccess(deactivateSetup, 'deactivate setup create');
  assertEmptyUserErrors(deactivateSetup, ['data', 'code', 'userErrors'], 'deactivate setup create');
  const deactivateCodeId = readRequiredString(
    deactivateSetup,
    ['data', 'code', 'codeDiscountNode', 'id'],
    'deactivate setup create',
  );
  createdDiscountIds.push(deactivateCodeId);

  await pollUntil(
    () => runGraphqlRaw(codeSearchProbeDocument, { query: 'status:active' }),
    (result) =>
      firstDiscountTitle(result, ['data', 'codeDiscountNodes', 'nodes', '0', 'codeDiscount', 'title']) ===
      deactivateTitle,
    'active code visible to deactivate bulk search selector',
  );
  const codeDeactivate = await runGraphqlRaw(codeDeactivateDocument, codeDeactivateVariables);
  assertSuccess(codeDeactivate, 'code bulk deactivate');
  assertEmptyUserErrors(codeDeactivate, ['data', 'discountCodeBulkDeactivate', 'userErrors'], 'code bulk deactivate');
  const readAfterCodeDeactivate = await pollUntil(
    () => runGraphqlRaw(readCodeDocument, { id: deactivateCodeId }),
    (result) => readPath(result.payload, ['data', 'code', 'codeDiscount', 'status']) === 'EXPIRED',
    'read after code bulk deactivate',
  );

  const codeDeleteSetup = await runGraphqlRaw(createCodeAndAutomaticDocument, codeDeleteSetupVariables);
  assertSuccess(codeDeleteSetup, 'code delete setup create');
  assertEmptyUserErrors(codeDeleteSetup, ['data', 'code', 'userErrors'], 'code delete setup code create');
  assertEmptyUserErrors(codeDeleteSetup, ['data', 'automatic', 'userErrors'], 'code delete setup automatic create');
  const deleteCodeId = readRequiredString(
    codeDeleteSetup,
    ['data', 'code', 'codeDiscountNode', 'id'],
    'code delete setup code create',
  );
  const deleteAutomaticControlId = readRequiredString(
    codeDeleteSetup,
    ['data', 'automatic', 'automaticDiscountNode', 'id'],
    'code delete setup automatic create',
  );
  createdDiscountIds.push(deleteCodeId, deleteAutomaticControlId);

  await pollUntil(
    () => runGraphqlRaw(codeSearchProbeDocument, { query: 'status:active' }),
    (result) =>
      firstDiscountTitle(result, ['data', 'codeDiscountNodes', 'nodes', '0', 'codeDiscount', 'title']) ===
      deleteSharedTitle,
    'active code visible to delete bulk search selector',
  );
  const codeDelete = await runGraphqlRaw(codeDeleteDocument, codeDeleteVariables);
  assertSuccess(codeDelete, 'code bulk delete');
  assertEmptyUserErrors(codeDelete, ['data', 'discountCodeBulkDelete', 'userErrors'], 'code bulk delete');
  const readAfterCodeDelete = await pollUntil(
    () => runGraphqlRaw(readCodeDeleteDocument, { codeId: deleteCodeId, automaticId: deleteAutomaticControlId }),
    (result) => readPath(result.payload, ['data', 'code']) === null,
    'read after code bulk delete',
  );
  assertStatus(
    readAfterCodeDelete,
    ['data', 'automatic', 'automaticDiscount', 'status'],
    'ACTIVE',
    'read after code bulk delete automatic control',
  );

  const automaticDeleteSetup = await runGraphqlRaw(createAutomaticDocument, automaticDeleteSetupVariables);
  assertSuccess(automaticDeleteSetup, 'automatic delete setup create');
  assertEmptyUserErrors(automaticDeleteSetup, ['data', 'automatic', 'userErrors'], 'automatic delete setup create');
  const automaticDeleteId = readRequiredString(
    automaticDeleteSetup,
    ['data', 'automatic', 'automaticDiscountNode', 'id'],
    'automatic delete setup create',
  );
  createdDiscountIds.push(automaticDeleteId);

  await pollUntil(
    () => runGraphqlRaw(automaticSearchProbeDocument, { query: 'status:scheduled' }),
    (result) =>
      firstDiscountTitle(result, ['data', 'automaticDiscountNodes', 'nodes', '0', 'automaticDiscount', 'title']) ===
      automaticDeleteTitle,
    'scheduled automatic visible to bulk search selector',
  );
  const automaticDelete = await runGraphqlRaw(automaticDeleteDocument, automaticDeleteVariables);
  assertSuccess(automaticDelete, 'automatic bulk delete');
  assertEmptyUserErrors(
    automaticDelete,
    ['data', 'discountAutomaticBulkDelete', 'userErrors'],
    'automatic bulk delete',
  );
  const readAfterAutomaticDelete = await pollUntil(
    () => runGraphqlRaw(readAutomaticDocument, { id: automaticDeleteId }),
    (result) => readPath(result.payload, ['data', 'automatic']) === null,
    'read after automatic bulk delete',
  );

  const output = {
    storeDomain,
    apiVersion,
    scopeProbe,
    variables: {
      runId,
      scheduledTitle,
      deactivateTitle,
      deleteSharedTitle,
      automaticDeleteTitle,
      scheduledCode,
      deactivateCode,
      deleteCode,
      scheduledCodeId,
      deactivateCodeId,
      deleteCodeId,
      deleteAutomaticControlId,
      automaticDeleteId,
    },
    setup: {
      query: setupDocument,
      variables: setupVariables,
      response: setup,
    },
    codeActivate: {
      query: codeActivateDocument,
      variables: codeActivateVariables,
      response: codeActivate,
    },
    readAfterCodeActivate: {
      query: activateReadDocument,
      variables: activateReadVariables,
      response: readAfterCodeActivate,
    },
    cleanupAfterCodeActivate: {
      query: codeDeleteCleanupDocument,
      variables: { id: scheduledCodeId },
      response: cleanupAfterCodeActivate,
    },
    deactivateSetup: {
      query: createCodeDocument,
      variables: deactivateSetupVariables,
      response: deactivateSetup,
    },
    codeDeactivate: {
      query: codeDeactivateDocument,
      variables: codeDeactivateVariables,
      response: codeDeactivate,
    },
    readAfterCodeDeactivate: {
      query: readCodeDocument,
      variables: { id: deactivateCodeId },
      response: readAfterCodeDeactivate,
    },
    codeDeleteSetup: {
      query: createCodeAndAutomaticDocument,
      variables: codeDeleteSetupVariables,
      response: codeDeleteSetup,
    },
    codeDelete: {
      query: codeDeleteDocument,
      variables: codeDeleteVariables,
      response: codeDelete,
    },
    readAfterCodeDelete: {
      query: readCodeDeleteDocument,
      variables: { codeId: deleteCodeId, automaticId: deleteAutomaticControlId },
      response: readAfterCodeDelete,
    },
    automaticDeleteSetup: {
      query: createAutomaticDocument,
      variables: automaticDeleteSetupVariables,
      response: automaticDeleteSetup,
    },
    automaticDelete: {
      query: automaticDeleteDocument,
      variables: automaticDeleteVariables,
      response: automaticDelete,
    },
    readAfterAutomaticDelete: {
      query: readAutomaticDocument,
      variables: { id: automaticDeleteId },
      response: readAfterAutomaticDelete,
    },
    validation: {
      safetyProbeBefore: {
        query: safetyProbeDocument,
        variables: {},
        response: safetyProbeBefore,
      },
      codeDeleteCodeFieldProbe: {
        query: codeDeleteCodeFieldProbeDocument,
        variables: {
          search: `code:DRAFT_PROXY_NO_MATCH_${runId}`,
        },
        response: codeDeleteCodeFieldProbe,
      },
    },
    upstreamCalls: [],
  };

  await mkdir(outputDir, { recursive: true });
  await writeFile(outputPath, `${JSON.stringify(output, null, 2)}\n`, 'utf8');
  const cleanup = await cleanupDiscounts();

  console.log(
    JSON.stringify(
      {
        ok: true,
        apiVersion,
        output: outputPath,
        cleanupCount: cleanup.length,
      },
      null,
      2,
    ),
  );
} catch (error) {
  await cleanupDiscounts();
  throw error;
}
