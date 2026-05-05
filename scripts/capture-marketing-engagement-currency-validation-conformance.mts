/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'marketing');
const outputPath = path.join(outputDir, 'marketing-engagement-currency-validation.json');

const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const schemaDocument = `#graphql
  query MarketingEngagementCurrencyValidationSchema {
    currentAppInstallation {
      app {
        id
        handle
        title
      }
      accessScopes {
        handle
      }
    }
    budgetType: __type(name: "MarketingBudgetBudgetType") {
      enumValues {
        name
      }
    }
  }
`;

const createActivityDocument = `#graphql
  mutation CreateMarketingActivity($input: MarketingActivityCreateExternalInput!) {
    marketingActivityCreateExternal(input: $input) {
      marketingActivity {
        id
        adSpend {
          amount
          currencyCode
        }
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const engagementByRemoteIdDocument = `#graphql
  mutation MarketingEngagementCurrencyByRemoteId(
    $remoteId: String!
    $engagement: MarketingEngagementInput!
  ) {
    marketingEngagementCreate(remoteId: $remoteId, marketingEngagement: $engagement) {
      marketingEngagement {
        occurredOn
        adSpend {
          amount
          currencyCode
        }
        sales {
          amount
          currencyCode
        }
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const engagementByActivityIdDocument = `#graphql
  mutation MarketingEngagementCurrencyByActivityId(
    $activityId: ID!
    $engagement: MarketingEngagementInput!
  ) {
    marketingEngagementCreate(marketingActivityId: $activityId, marketingEngagement: $engagement) {
      marketingEngagement {
        occurredOn
        adSpend {
          amount
          currencyCode
        }
        sales {
          amount
          currencyCode
        }
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const engagementByChannelHandleDocument = `#graphql
  mutation MarketingEngagementCurrencyByChannelHandle(
    $channelHandle: String!
    $engagement: MarketingEngagementInput!
  ) {
    marketingEngagementCreate(channelHandle: $channelHandle, marketingEngagement: $engagement) {
      marketingEngagement {
        occurredOn
        channelHandle
        adSpend {
          amount
          currencyCode
        }
        sales {
          amount
          currencyCode
        }
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const deleteActivityDocument = `#graphql
  mutation DeleteMarketingActivity($remoteId: String) {
    marketingActivityDeleteExternal(remoteId: $remoteId) {
      deletedMarketingActivityId
      userErrors {
        field
        message
        code
      }
    }
  }
`;

async function assertHttpOk(label: string, result: { status: number; payload: unknown }): Promise<void> {
  if (result.status >= 200 && result.status < 300) {
    return;
  }

  console.error(JSON.stringify(result.payload, null, 2));
  throw new Error(`${label} failed with HTTP ${result.status}`);
}

function readRecord(value: unknown): Record<string, unknown> | null {
  return value && typeof value === 'object' && !Array.isArray(value) ? (value as Record<string, unknown>) : null;
}

function readPath(value: unknown, pathParts: string[]): unknown {
  let current: unknown = value;
  for (const part of pathParts) {
    const record = readRecord(current);
    if (!record) {
      return undefined;
    }
    current = record[part];
  }
  return current;
}

function readString(value: unknown): string | null {
  return typeof value === 'string' && value.length > 0 ? value : null;
}

function readAccessScopes(schemaPayload: unknown): string[] {
  const scopes = readPath(schemaPayload, ['data', 'currentAppInstallation', 'accessScopes']);
  if (!Array.isArray(scopes)) {
    return [];
  }

  return scopes.flatMap((scope): string[] => {
    const handle = readString(readRecord(scope)?.['handle']);
    return handle ? [handle] : [];
  });
}

function hasUserErrors(payload: unknown, pathParts: string[]): boolean {
  const userErrors = readPath(payload, pathParts);
  return Array.isArray(userErrors) && userErrors.length > 0;
}

function readCreatedActivityId(payload: unknown): string | null {
  return readString(readPath(payload, ['data', 'marketingActivityCreateExternal', 'marketingActivity', 'id']));
}

await mkdir(outputDir, { recursive: true });

const schemaResult = await runGraphqlRequest(schemaDocument);
await assertHttpOk('Marketing engagement currency schema capture', schemaResult);

const runId = Date.now().toString(36);
const remoteId = `har-684-currency-validation-${runId}`;
const activityInput = {
  title: 'HAR-684 Currency Validation Campaign',
  remoteId,
  status: 'ACTIVE',
  remoteUrl: `https://example.com/${remoteId}`,
  tactic: 'NEWSLETTER',
  marketingChannelType: 'EMAIL',
  budget: {
    budgetType: 'DAILY',
    total: {
      amount: '100.00',
      currencyCode: 'USD',
    },
  },
  utm: {
    campaign: remoteId,
    source: 'newsletter',
    medium: 'email',
  },
};
const mismatchedInputEngagement = {
  occurredOn: '2026-04-01',
  isCumulative: false,
  utcOffset: '+00:00',
  adSpend: {
    amount: '10.00',
    currencyCode: 'USD',
  },
  sales: {
    amount: '30.00',
    currencyCode: 'EUR',
  },
};
const activityCurrencyMismatchEngagement = {
  occurredOn: '2026-04-02',
  isCumulative: false,
  utcOffset: '+00:00',
  adSpend: {
    amount: '10.00',
    currencyCode: 'EUR',
  },
};
const remoteActivityCurrencyMismatchEngagement = {
  occurredOn: '2026-04-03',
  isCumulative: false,
  utcOffset: '+00:00',
  sales: {
    amount: '30.00',
    currencyCode: 'EUR',
  },
};

let createdActivityId: string | null = null;
let createActivityPayload: unknown = null;
let inputMismatchPayload: unknown = null;
let activityMismatchByIdPayload: unknown = null;
let activityMismatchByRemoteIdPayload: unknown = null;
let channelInputMismatchPayload: unknown = null;
let cleanupPayload: unknown = null;

try {
  const createActivityResult = await runGraphqlRequest(createActivityDocument, { input: activityInput });
  await assertHttpOk('Marketing engagement currency activity setup', createActivityResult);
  if (hasUserErrors(createActivityResult.payload, ['data', 'marketingActivityCreateExternal', 'userErrors'])) {
    console.error(JSON.stringify(createActivityResult.payload, null, 2));
    throw new Error('Marketing engagement currency activity setup returned userErrors');
  }
  createdActivityId = readCreatedActivityId(createActivityResult.payload);
  if (!createdActivityId) {
    console.error(JSON.stringify(createActivityResult.payload, null, 2));
    throw new Error('Marketing engagement currency activity setup did not return an activity id');
  }
  createActivityPayload = createActivityResult.payload;

  const inputMismatchResult = await runGraphqlRequest(engagementByRemoteIdDocument, {
    remoteId,
    engagement: mismatchedInputEngagement,
  });
  await assertHttpOk('Marketing engagement input currency mismatch capture', inputMismatchResult);
  inputMismatchPayload = inputMismatchResult.payload;

  const activityMismatchByIdResult = await runGraphqlRequest(engagementByActivityIdDocument, {
    activityId: createdActivityId,
    engagement: activityCurrencyMismatchEngagement,
  });
  await assertHttpOk('Marketing engagement activity currency mismatch by id capture', activityMismatchByIdResult);
  activityMismatchByIdPayload = activityMismatchByIdResult.payload;

  const activityMismatchByRemoteIdResult = await runGraphqlRequest(engagementByRemoteIdDocument, {
    remoteId,
    engagement: remoteActivityCurrencyMismatchEngagement,
  });
  await assertHttpOk(
    'Marketing engagement activity currency mismatch by remote id capture',
    activityMismatchByRemoteIdResult,
  );
  activityMismatchByRemoteIdPayload = activityMismatchByRemoteIdResult.payload;

  const channelInputMismatchResult = await runGraphqlRequest(engagementByChannelHandleDocument, {
    channelHandle: 'unknown-channel',
    engagement: mismatchedInputEngagement,
  });
  await assertHttpOk('Marketing engagement channel input currency mismatch capture', channelInputMismatchResult);
  channelInputMismatchPayload = channelInputMismatchResult.payload;
} finally {
  if (createdActivityId) {
    const cleanupResult = await runGraphqlRequest(deleteActivityDocument, { remoteId });
    await assertHttpOk('Marketing engagement currency activity cleanup', cleanupResult);
    cleanupPayload = cleanupResult.payload;
  }
}

const fixture = {
  capturedAt: new Date().toISOString(),
  apiVersion,
  storeDomain,
  scopeEvidence: {
    app: readPath(schemaResult.payload, ['data', 'currentAppInstallation', 'app']),
    accessScopes: readAccessScopes(schemaResult.payload).filter(
      (scope) => scope === 'read_marketing_events' || scope === 'write_marketing_events',
    ),
  },
  schema: {
    budgetTypeEnum: readPath(schemaResult.payload, ['data', 'budgetType', 'enumValues']),
  },
  operations: {
    createActivity: {
      request: {
        query: createActivityDocument,
        variables: {
          input: activityInput,
        },
      },
      response: createActivityPayload,
    },
    inputCurrencyMismatch: {
      request: {
        query: engagementByRemoteIdDocument,
        variables: {
          remoteId,
          engagement: mismatchedInputEngagement,
        },
      },
      response: inputMismatchPayload,
    },
    activityCurrencyMismatchById: {
      request: {
        query: engagementByActivityIdDocument,
        variables: {
          activityId: createdActivityId,
          engagement: activityCurrencyMismatchEngagement,
        },
      },
      response: activityMismatchByIdPayload,
    },
    activityCurrencyMismatchByRemoteId: {
      request: {
        query: engagementByRemoteIdDocument,
        variables: {
          remoteId,
          engagement: remoteActivityCurrencyMismatchEngagement,
        },
      },
      response: activityMismatchByRemoteIdPayload,
    },
    channelInputMismatch: {
      request: {
        query: engagementByChannelHandleDocument,
        variables: {
          channelHandle: 'unknown-channel',
          engagement: mismatchedInputEngagement,
        },
      },
      response: channelInputMismatchPayload,
    },
    cleanup: {
      deleteActivity: {
        request: {
          query: deleteActivityDocument,
          variables: {
            remoteId,
          },
        },
        data: readPath(cleanupPayload, ['data']),
      },
    },
  },
  notes: [
    'Captured currency validation against a disposable external marketing activity with a USD budget and cleaned it up afterward.',
    'Activity currency mismatch is captured through both marketingActivityId and remoteId resolution paths.',
    'The unrecognized channelHandle probe returns INVALID_CHANNEL_HANDLE before currency validation; recognized channel-handle currency validation remains runtime-test-backed until the disposable shop exposes a valid handle.',
  ],
  upstreamCalls: [],
};

await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');

console.log(
  JSON.stringify(
    {
      ok: true,
      outputPath,
      apiVersion,
      storeDomain,
      activityId: createdActivityId,
    },
    null,
    2,
  ),
);
