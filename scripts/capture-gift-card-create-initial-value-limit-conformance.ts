/* oxlint-disable no-console -- CLI scripts intentionally write status output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type CapturedRequest = {
  label: string;
  query: string;
  variables: Record<string, unknown>;
  response: ConformanceGraphqlResult;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2025-01',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'gift-cards');
const outputPath = path.join(outputDir, 'gift-card-create-initial-value-limit.json');

const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

function isObject(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function readPath(value: unknown, pathParts: string[]): unknown {
  return pathParts.reduce<unknown>((cursor, part) => {
    if (!isObject(cursor)) {
      return undefined;
    }
    return cursor[part];
  }, value);
}

function readStringPath(value: unknown, pathParts: string[]): string | null {
  const found = readPath(value, pathParts);
  return typeof found === 'string' ? found : null;
}

function readCreatedGiftCardId(capture: CapturedRequest, alias: string): string | null {
  return readStringPath(capture.response.payload, ['data', alias, 'giftCard', 'id']);
}

function decimalToCents(amount: string): bigint {
  const trimmed = amount.trim();
  const match = /^(\d+)(?:\.(\d{0,2}))?$/u.exec(trimmed);
  if (match === null) {
    throw new Error(`unsupported gift-card issue limit amount: ${amount}`);
  }
  const wholePart = match[1];
  if (wholePart === undefined) {
    throw new Error(`unsupported gift-card issue limit amount: ${amount}`);
  }
  const whole = BigInt(wholePart);
  const cents = BigInt((match[2] ?? '').padEnd(2, '0'));
  return whole * 100n + cents;
}

function centsToDecimal(cents: bigint): string {
  const whole = cents / 100n;
  const fractional = cents % 100n;
  if (fractional === 0n) {
    return `${whole.toString()}.0`;
  }
  return `${whole.toString()}.${fractional.toString().padStart(2, '0')}`;
}

async function capture(
  label: string,
  query: string,
  variables: Record<string, unknown> = {},
): Promise<CapturedRequest> {
  const response = await runGraphqlRequest(query, variables);
  return { label, query, variables, response };
}

async function deactivateGiftCard(label: string, id: string): Promise<CapturedRequest> {
  return capture(
    label,
    `#graphql
      mutation GiftCardCreateInitialValueLimitCleanup($id: ID!) {
        giftCardDeactivate(id: $id) {
          giftCard {
            id
            enabled
            deactivatedAt
          }
          userErrors {
            field
            code
            message
          }
        }
      }
    `,
    { id },
  );
}

const configurationQuery = `#graphql
  query GiftCardCreateConfiguration {
    giftCardConfiguration {
      issueLimit { amount currencyCode }
      purchaseLimit { amount currencyCode }
    }
  }
`;

const limitMutation = `#graphql
  mutation GiftCardCreateInitialValueLimit(
    $boundary: Decimal!
    $overByCent: Decimal!
    $wellOver: Decimal!
  ) {
    boundarySuccess: giftCardCreate(input: { initialValue: $boundary }) {
      giftCard {
        id
        initialValue { amount currencyCode }
        balance { amount currencyCode }
      }
      userErrors {
        field
        code
        message
      }
    }
    overByCent: giftCardCreate(input: { initialValue: $overByCent }) {
      giftCard {
        id
      }
      userErrors {
        field
        code
        message
      }
    }
    wellOver: giftCardCreate(input: { initialValue: $wellOver }) {
      giftCard {
        id
      }
      userErrors {
        field
        code
        message
      }
    }
  }
`;

const configurationRead = await capture('configurationRead', configurationQuery);
const issueLimitAmount = readStringPath(configurationRead.response.payload, [
  'data',
  'giftCardConfiguration',
  'issueLimit',
  'amount',
]);
const issueLimitCurrency = readStringPath(configurationRead.response.payload, [
  'data',
  'giftCardConfiguration',
  'issueLimit',
  'currencyCode',
]);

if (issueLimitAmount === null || issueLimitCurrency === null) {
  throw new Error('giftCardConfiguration.issueLimit was not readable from the conformance shop.');
}

const issueLimitCents = decimalToCents(issueLimitAmount);
if (issueLimitCents <= 0n) {
  throw new Error(
    `giftCardConfiguration.issueLimit must be non-zero for HAR-767 capture; got ${issueLimitAmount} ${issueLimitCurrency}.`,
  );
}

const proxyVariables = {
  boundary: centsToDecimal(issueLimitCents),
  overByCent: centsToDecimal(issueLimitCents + 1n),
  wellOver: '1000000.0',
};

const cleanup: CapturedRequest[] = [];
const initialValueLimit = await capture('initialValueLimit', limitMutation, proxyVariables);
for (const alias of ['boundarySuccess', 'overByCent', 'wellOver']) {
  const id = readCreatedGiftCardId(initialValueLimit, alias);
  if (id !== null) {
    cleanup.push(await deactivateGiftCard(`cleanupDeactivate:${alias}`, id));
  }
}

await mkdir(outputDir, { recursive: true });
await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      capturedAt: new Date().toISOString(),
      storeDomain,
      apiVersion,
      notes: [
        'HAR-767 captures giftCardCreate initialValue validation against giftCardConfiguration.issueLimit.',
        `The captured shop issue limit is ${issueLimitAmount} ${issueLimitCurrency}; boundary uses the exact limit, overByCent adds $0.01, and wellOver uses 1000000.0.`,
        'The boundary success gift card is deactivated during cleanup. Rejected branches should not create gift-card records.',
      ],
      proxyVariables: {
        initialValueLimit: proxyVariables,
      },
      operations: {
        configurationRead,
        initialValueLimit,
      },
      cleanup,
      upstreamCalls: [
        {
          operationName: 'GiftCardCreateConfiguration',
          variables: {},
          query: configurationQuery,
          response: {
            status: 200,
            body: configurationRead.response.payload,
          },
        },
      ],
    },
    null,
    2,
  )}\n`,
  'utf8',
);

console.log(`wrote ${outputPath}`);
