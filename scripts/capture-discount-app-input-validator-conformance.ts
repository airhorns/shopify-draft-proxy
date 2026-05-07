/* oxlint-disable no-console -- CLI scripts intentionally write capture status output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { readConformanceScriptConfig } from './conformance-script-config.js';
import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { assertDiscountConformanceScopes, probeDiscountConformanceScopes } from './discount-conformance-lib.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;
type CaptureCall = {
  operationName: string;
  variables: Record<string, unknown>;
  query: string;
  response: {
    status: number;
    body: unknown;
  };
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});

const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'discounts');
const outputPath = path.join(outputDir, 'app-discount-input-validator.json');

const setupDocumentPath = 'config/parity-requests/discounts/app-discount-input-validator-setup.graphql';
const codeCreateDocumentPath = 'config/parity-requests/discounts/app-discount-input-validator-code-create.graphql';
const automaticCreateDocumentPath =
  'config/parity-requests/discounts/app-discount-input-validator-automatic-create.graphql';
const codeUpdateDocumentPath = 'config/parity-requests/discounts/app-discount-input-validator-code-update.graphql';
const automaticUpdateDocumentPath =
  'config/parity-requests/discounts/app-discount-input-validator-automatic-update.graphql';

const setupDocument = await readFile(setupDocumentPath, 'utf8');
const codeCreateDocument = await readFile(codeCreateDocumentPath, 'utf8');
const automaticCreateDocument = await readFile(automaticCreateDocumentPath, 'utf8');
const codeUpdateDocument = await readFile(codeUpdateDocumentPath, 'utf8');
const automaticUpdateDocument = await readFile(automaticUpdateDocumentPath, 'utf8');

const functionCatalogDocument = `#graphql
  query AppDiscountInputValidatorFunctionCatalog {
    shopifyFunctions(first: 50) {
      nodes {
        id
        title
        handle
        apiType
        description
        appKey
        app {
          id
          title
          handle
          apiKey
        }
      }
    }
  }
`;

const functionHydrateByHandleDocument = `query ShopifyFunctionByHandle($handle: String!) {
  shopifyFunctions(first: 1, handle: $handle) {
    nodes {
      id
      title
      handle
      apiType
      description
      appKey
      app {
        id
        title
        handle
        apiKey
      }
    }
  }
}
`;

const deleteCodeDocument = `#graphql
  mutation AppDiscountInputValidatorDeleteCode($id: ID!) {
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
  mutation AppDiscountInputValidatorDeleteAutomatic($id: ID!) {
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

const automaticChannelIdsDocument = `#graphql
  mutation AppDiscountInputValidatorAutomaticChannelIds($functionHandle: String!, $channelId: ID!) {
    discountAutomaticAppCreate(
      automaticAppDiscount: {
        title: "Conformance automatic channelIds"
        startsAt: "2026-05-05T00:00:00Z"
        functionHandle: $functionHandle
        discountClasses: [ORDER]
        channelIds: [$channelId]
      }
    ) {
      automaticAppDiscount {
        discountId
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

const marketsRemoveAllDocument = `#graphql
  mutation AppDiscountInputValidatorMarketsRemoveAll($functionHandle: String!, $marketId: ID!) {
    discountCodeAppCreate(
      codeAppDiscount: {
        title: "Conformance markets removeAll"
        code: "APPVALIDMARKETS"
        startsAt: "2026-05-05T00:00:00Z"
        functionHandle: $functionHandle
        discountClasses: [ORDER]
        markets: { removeAllMarkets: true, add: [$marketId] }
      }
    ) {
      codeAppDiscount {
        discountId
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
const { runGraphqlRequest, runGraphqlRaw } = createAdminGraphqlClient(adminOptions);

function readRecord(value: unknown): JsonRecord | null {
  return value && typeof value === 'object' && !Array.isArray(value) ? (value as JsonRecord) : null;
}

function readArray(value: unknown): unknown[] {
  return Array.isArray(value) ? value : [];
}

function readString(value: unknown): string | null {
  return typeof value === 'string' && value.length > 0 ? value : null;
}

function readPath(value: unknown, pathSegments: string[]): unknown {
  let current: unknown = value;
  for (const segment of pathSegments) {
    const record = readRecord(current);
    if (!record) {
      return null;
    }
    current = record[segment];
  }

  return current;
}

function assertHttpOk(result: ConformanceGraphqlResult, context: string): void {
  if (result.status < 200 || result.status >= 300) {
    throw new Error(`${context} returned HTTP ${result.status}: ${JSON.stringify(result.payload, null, 2)}`);
  }
}

function readFunctionNodes(catalog: ConformanceGraphqlResult): JsonRecord[] {
  const connection = readRecord(readRecord(catalog.payload.data)?.['shopifyFunctions']);
  return readArray(connection?.['nodes']).flatMap((node) => {
    const record = readRecord(node);
    return record ? [record] : [];
  });
}

function isDiscountApi(apiType: unknown): boolean {
  return (
    apiType === 'discount' ||
    apiType === 'product_discounts' ||
    apiType === 'order_discounts' ||
    apiType === 'shipping_discounts'
  );
}

function findDiscountFunction(nodes: JsonRecord[]): JsonRecord {
  const deployed = nodes.find(
    (node) => readString(node['handle']) === 'conformance-discount' && isDiscountApi(node['apiType']),
  );
  if (!deployed) {
    throw new Error(`Expected deployed conformance-discount Function in catalog: ${JSON.stringify(nodes, null, 2)}`);
  }

  return deployed;
}

function captureHydrateCall(handle: string, node: JsonRecord): CaptureCall {
  return {
    operationName: 'ShopifyFunctionByHandle',
    variables: { handle },
    query: functionHydrateByHandleDocument,
    response: {
      status: 200,
      body: {
        data: {
          shopifyFunctions: {
            nodes: [node],
          },
        },
      },
    },
  };
}

function validCodeInput(stamp: number, functionHandle: string): Record<string, unknown> {
  return {
    title: `Conformance code setup ${stamp}`,
    code: `APPVALIDSETUP${stamp}`,
    startsAt: '2026-05-05T00:00:00Z',
    functionHandle,
    discountClasses: ['ORDER'],
  };
}

function validAutomaticInput(stamp: number, functionHandle: string): Record<string, unknown> {
  return {
    title: `Conformance automatic setup ${stamp}`,
    startsAt: '2026-05-05T00:00:00Z',
    functionHandle,
    discountClasses: ['ORDER'],
  };
}

function baseCodeInput(stamp: number, suffix: string, functionHandle: string): Record<string, unknown> {
  return {
    title: `Conformance code ${suffix} ${stamp}`,
    code: `APPVALID${suffix}${stamp}`,
    startsAt: '2026-05-05T00:00:00Z',
    functionHandle,
    discountClasses: ['ORDER'],
  };
}

function baseAutomaticInput(stamp: number, suffix: string, functionHandle: string): Record<string, unknown> {
  return {
    title: `Conformance automatic ${suffix} ${stamp}`,
    startsAt: '2026-05-05T00:00:00Z',
    functionHandle,
    discountClasses: ['ORDER'],
  };
}

function withInput(base: Record<string, unknown>, patch: Record<string, unknown>): Record<string, unknown> {
  return { ...base, ...patch };
}

async function runCase(
  documentPath: string,
  document: string,
  variables: Record<string, unknown>,
): Promise<{ request: { documentPath: string; variables: Record<string, unknown> }; response: unknown }> {
  const response = await runGraphqlRaw(document, variables);
  assertHttpOk(response, documentPath);
  return {
    request: {
      documentPath,
      variables,
    },
    response: response.payload,
  };
}

async function cleanupDiscounts(codeIds: string[], automaticIds: string[]): Promise<unknown[]> {
  const cleanup: unknown[] = [];
  for (const codeId of new Set(codeIds)) {
    const result = await runGraphqlRequest(deleteCodeDocument, { id: codeId });
    cleanup.push({ kind: 'code', id: codeId, response: result.payload });
  }
  for (const automaticId of new Set(automaticIds)) {
    const result = await runGraphqlRequest(deleteAutomaticDocument, { id: automaticId });
    cleanup.push({ kind: 'automatic', id: automaticId, response: result.payload });
  }
  return cleanup;
}

await mkdir(outputDir, { recursive: true });

const scopeProbe = await probeDiscountConformanceScopes(adminOptions);
assertDiscountConformanceScopes(scopeProbe);

const functionCatalog = await runGraphqlRequest(functionCatalogDocument, {});
assertHttpOk(functionCatalog, 'shopifyFunctions catalog');
const discountFunction = findDiscountFunction(readFunctionNodes(functionCatalog));
const functionHandle = readString(discountFunction['handle']);
if (!functionHandle) {
  throw new Error(`Discount Function is missing a handle: ${JSON.stringify(discountFunction, null, 2)}`);
}

const stamp = Date.now();
const setupVariables = {
  codeInput: validCodeInput(stamp, functionHandle),
  automaticInput: validAutomaticInput(stamp, functionHandle),
};
const setup = await runCase(setupDocumentPath, setupDocument, setupVariables);
const codeSetupId = readString(readPath(setup.response, ['data', 'codeSetup', 'codeAppDiscount', 'discountId']));
const automaticSetupId = readString(
  readPath(setup.response, ['data', 'automaticSetup', 'automaticAppDiscount', 'discountId']),
);
if (!codeSetupId || !automaticSetupId) {
  throw new Error(`Setup did not create both app discounts: ${JSON.stringify(setup.response, null, 2)}`);
}

const cases: Record<string, unknown> = {};
const schemaRejectedCases: Record<string, unknown> = {};
const cleanupCodeIds = codeSetupId ? [codeSetupId] : [];
const cleanupAutomaticIds = automaticSetupId ? [automaticSetupId] : [];
let cleanup: unknown[] = [];

function collectCreatedDiscounts(call: unknown): void {
  const response = readRecord(call)?.['response'];
  const codeId =
    readString(readPath(response, ['data', 'discountCodeAppCreate', 'codeAppDiscount', 'discountId'])) ||
    readString(readPath(response, ['data', 'discountCodeAppUpdate', 'codeAppDiscount', 'discountId']));
  if (codeId) {
    cleanupCodeIds.push(codeId);
  }
  const automaticId =
    readString(readPath(response, ['data', 'discountAutomaticAppCreate', 'automaticAppDiscount', 'discountId'])) ||
    readString(readPath(response, ['data', 'discountAutomaticAppUpdate', 'automaticAppDiscount', 'discountId']));
  if (automaticId) {
    cleanupAutomaticIds.push(automaticId);
  }
}

async function runTrackedCase(
  key: string,
  documentPath: string,
  document: string,
  variables: Record<string, unknown>,
): Promise<void> {
  const call = await runCase(documentPath, document, variables);
  collectCreatedDiscounts(call);
  cases[key] = call;
}

try {
  await runTrackedCase('codeCreateMissingCode', codeCreateDocumentPath, codeCreateDocument, {
    input: withInput(baseCodeInput(stamp, 'MISSINGCODE', functionHandle), { code: undefined }),
  });
  await runTrackedCase('codeCreateBlankCode', codeCreateDocumentPath, codeCreateDocument, {
    input: withInput(baseCodeInput(stamp, 'BLANK', functionHandle), { code: '' }),
  });
  await runTrackedCase('codeCreateMissingStartsAt', codeCreateDocumentPath, codeCreateDocument, {
    input: withInput(baseCodeInput(stamp, 'NOSTART', functionHandle), { startsAt: undefined }),
  });
  await runTrackedCase('codeCreateEmptyDiscountClasses', codeCreateDocumentPath, codeCreateDocument, {
    input: withInput(baseCodeInput(stamp, 'EMPTYCLASS', functionHandle), { discountClasses: [] }),
  });
  await runTrackedCase('codeCreateCombinesWithProductOnOrderClass', codeCreateDocumentPath, codeCreateDocument, {
    input: withInput(baseCodeInput(stamp, 'COMBINES', functionHandle), {
      combinesWith: { productDiscounts: true },
    }),
  });
  await runTrackedCase('codeCreateEmptyCustomerSegmentsAdd', codeCreateDocumentPath, codeCreateDocument, {
    input: withInput(baseCodeInput(stamp, 'SEGEMPTY', functionHandle), {
      customerSelection: { customerSegments: { add: [] } },
    }),
  });
  await runTrackedCase('codeCreateEmptyCustomersAdd', codeCreateDocumentPath, codeCreateDocument, {
    input: withInput(baseCodeInput(stamp, 'CUSTEMPTY', functionHandle), {
      customerSelection: { customers: { add: [] } },
    }),
  });
  await runTrackedCase('automaticCreateBlankTitle', automaticCreateDocumentPath, automaticCreateDocument, {
    input: withInput(baseAutomaticInput(stamp, 'BLANK', functionHandle), { title: '' }),
  });
  await runTrackedCase('automaticCreateMissingStartsAt', automaticCreateDocumentPath, automaticCreateDocument, {
    input: withInput(baseAutomaticInput(stamp, 'NOSTART', functionHandle), { startsAt: undefined }),
  });
  await runTrackedCase('automaticCreateEmptyDiscountClasses', automaticCreateDocumentPath, automaticCreateDocument, {
    input: withInput(baseAutomaticInput(stamp, 'EMPTYCLASS', functionHandle), { discountClasses: [] }),
  });
  await runTrackedCase('codeUpdateBlankCode', codeUpdateDocumentPath, codeUpdateDocument, {
    id: codeSetupId,
    input: withInput(baseCodeInput(stamp, 'UPBLANK', functionHandle), { code: '' }),
  });
  await runTrackedCase('codeUpdateMissingStartsAt', codeUpdateDocumentPath, codeUpdateDocument, {
    id: codeSetupId,
    input: withInput(baseCodeInput(stamp, 'UPNOSTART', functionHandle), { startsAt: undefined }),
  });
  await runTrackedCase('codeUpdateEmptyDiscountClasses', codeUpdateDocumentPath, codeUpdateDocument, {
    id: codeSetupId,
    input: withInput(baseCodeInput(stamp, 'UPEMPTYCLASS', functionHandle), { discountClasses: [] }),
  });
  await runTrackedCase('codeUpdateEmptyCustomersAdd', codeUpdateDocumentPath, codeUpdateDocument, {
    id: codeSetupId,
    input: withInput(baseCodeInput(stamp, 'UPCUSTEMPTY', functionHandle), {
      customerSelection: { customers: { add: [] } },
    }),
  });
  await runTrackedCase('automaticUpdateBlankTitle', automaticUpdateDocumentPath, automaticUpdateDocument, {
    id: automaticSetupId,
    input: withInput(baseAutomaticInput(stamp, 'UPBLANK', functionHandle), { title: '' }),
  });
  await runTrackedCase('automaticUpdateMissingStartsAt', automaticUpdateDocumentPath, automaticUpdateDocument, {
    id: automaticSetupId,
    input: withInput(baseAutomaticInput(stamp, 'UPNOSTART', functionHandle), { startsAt: undefined }),
  });
  await runTrackedCase('automaticUpdateEmptyDiscountClasses', automaticUpdateDocumentPath, automaticUpdateDocument, {
    id: automaticSetupId,
    input: withInput(baseAutomaticInput(stamp, 'UPEMPTYCLASS', functionHandle), { discountClasses: [] }),
  });
  schemaRejectedCases['automaticUpdateCustomerSelection'] = {
    request: {
      documentPath: automaticUpdateDocumentPath,
      variables: {
        id: automaticSetupId,
        input: withInput(baseAutomaticInput(stamp, 'UPSEGEMPTY', functionHandle), {
          customerSelection: { customerSegments: { add: [] } },
        }),
      },
    },
    response: (
      await runGraphqlRaw(automaticUpdateDocument, {
        id: automaticSetupId,
        input: withInput(baseAutomaticInput(stamp, 'UPSEGEMPTY', functionHandle), {
          customerSelection: { customerSegments: { add: [] } },
        }),
      })
    ).payload,
  };
  schemaRejectedCases['automaticCreateCustomerSelection'] = {
    request: {
      documentPath: automaticCreateDocumentPath,
      variables: {
        input: withInput(baseAutomaticInput(stamp, 'CUSTEMPTY', functionHandle), {
          customerSelection: { customers: { add: [] } },
        }),
      },
    },
    response: (
      await runGraphqlRaw(automaticCreateDocument, {
        input: withInput(baseAutomaticInput(stamp, 'CUSTEMPTY', functionHandle), {
          customerSelection: { customers: { add: [] } },
        }),
      })
    ).payload,
  };

  schemaRejectedCases['automaticCreateChannelIds'] = {
    request: {
      query: automaticChannelIdsDocument,
      variables: { functionHandle, channelId: 'gid://shopify/Channel/1' },
    },
    response: (
      await runGraphqlRaw(automaticChannelIdsDocument, {
        functionHandle,
        channelId: 'gid://shopify/Channel/1',
      })
    ).payload,
  };
  schemaRejectedCases['codeCreateMarketsRemoveAllWithAdd'] = {
    request: {
      query: marketsRemoveAllDocument,
      variables: { functionHandle, marketId: 'gid://shopify/Market/1' },
    },
    response: (
      await runGraphqlRaw(marketsRemoveAllDocument, {
        functionHandle,
        marketId: 'gid://shopify/Market/1',
      })
    ).payload,
  };
} finally {
  cleanup = await cleanupDiscounts(cleanupCodeIds, cleanupAutomaticIds);
}

const output = {
  scenarioId: 'app-discount-input-validator',
  capturedAt: new Date().toISOString(),
  storeDomain,
  apiVersion,
  scopeProbe,
  functionCatalog: {
    query: functionCatalogDocument,
    variables: {},
    response: functionCatalog,
  },
  discountFunction,
  setup,
  cases,
  schemaRejectedCases,
  cleanup,
  upstreamCalls: [captureHydrateCall(functionHandle, discountFunction)],
  notes:
    'Live Shopify app-discount input validator capture using a deployed disposable conformance-discount Function. channelIds and markets.removeAllMarkets probes are recorded separately because the public Admin schema may reject them before mutation userErrors.',
};

await writeFile(outputPath, `${JSON.stringify(output, null, 2)}\n`, 'utf8');
console.log(`Wrote ${outputPath}`);
