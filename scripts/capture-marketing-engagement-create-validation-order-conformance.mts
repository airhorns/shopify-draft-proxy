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
const outputPath = path.join(outputDir, 'marketing-engagement-create-validation-order.json');

const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const schemaDocument = `#graphql
  query MarketingEngagementCreateValidationOrderSchema {
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
  }
`;

const createActivityDocument = `#graphql
  mutation MarketingEngagementCreateValidationOrderSetup(
    $input: MarketingActivityCreateExternalInput!
  ) {
    marketingActivityCreateExternal(input: $input) {
      marketingActivity {
        id
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const multipleActivitySelectorsDocument = `#graphql
  mutation MarketingEngagementCreateValidationOrderMultipleActivitySelectors(
    $activityId: ID!
    $remoteId: String!
    $engagement: MarketingEngagementInput!
  ) {
    marketingEngagementCreate(
      marketingActivityId: $activityId
      remoteId: $remoteId
      marketingEngagement: $engagement
    ) {
      marketingEngagement {
        occurredOn
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const multipleChannelSelectorsDocument = `#graphql
  mutation MarketingEngagementCreateValidationOrderMultipleChannelSelectors(
    $channelHandle: String!
    $remoteId: String!
    $engagement: MarketingEngagementInput!
  ) {
    marketingEngagementCreate(
      channelHandle: $channelHandle
      remoteId: $remoteId
      marketingEngagement: $engagement
    ) {
      marketingEngagement {
        occurredOn
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const unknownChannelCurrencyDocument = `#graphql
  mutation MarketingEngagementCreateValidationOrderUnknownChannelCurrency(
    $channelHandle: String!
    $engagement: MarketingEngagementInput!
  ) {
    marketingEngagementCreate(
      channelHandle: $channelHandle
      marketingEngagement: $engagement
    ) {
      marketingEngagement {
        occurredOn
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const unknownRemoteCurrencyDocument = `#graphql
  mutation MarketingEngagementCreateValidationOrderUnknownRemoteCurrency(
    $remoteId: String!
    $engagement: MarketingEngagementInput!
  ) {
    marketingEngagementCreate(
      remoteId: $remoteId
      marketingEngagement: $engagement
    ) {
      marketingEngagement {
        occurredOn
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const unknownRemoteDocument = `#graphql
  mutation MarketingEngagementCreateValidationOrderUnknownRemote(
    $remoteId: String!
    $engagement: MarketingEngagementInput!
  ) {
    marketingEngagementCreate(
      remoteId: $remoteId
      marketingEngagement: $engagement
    ) {
      marketingEngagement {
        occurredOn
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
  mutation DeleteMarketingEngagementCreateValidationOrderActivity($remoteId: String) {
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
await assertHttpOk('Marketing engagement validation-order schema capture', schemaResult);

const runId = Date.now().toString(36);
const remoteId = `marketing-engagement-validation-order-${runId}`;
const unknownRemoteId = `marketing-engagement-validation-order-missing-${runId}`;
const unknownChannelHandle = `validation-order-channel-${runId}`;
const activityInput = {
  title: `Marketing engagement validation order ${runId}`,
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
const mismatchedCurrencyEngagement = {
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
const validCurrencyEngagement = {
  occurredOn: '2026-04-02',
  isCumulative: false,
  utcOffset: '+00:00',
  adSpend: {
    amount: '10.00',
    currencyCode: 'USD',
  },
};

let createdActivityId: string | null = null;
let createActivityPayload: unknown = null;
let multipleActivitySelectorsPayload: unknown = null;
let multipleChannelSelectorsPayload: unknown = null;
let unknownChannelCurrencyPayload: unknown = null;
let unknownRemoteCurrencyPayload: unknown = null;
let unknownRemotePayload: unknown = null;
let cleanupPayload: unknown = null;

try {
  const createActivityResult = await runGraphqlRequest(createActivityDocument, { input: activityInput });
  await assertHttpOk('Marketing engagement validation-order activity setup', createActivityResult);
  if (hasUserErrors(createActivityResult.payload, ['data', 'marketingActivityCreateExternal', 'userErrors'])) {
    console.error(JSON.stringify(createActivityResult.payload, null, 2));
    throw new Error('Marketing engagement validation-order activity setup returned userErrors');
  }
  createdActivityId = readCreatedActivityId(createActivityResult.payload);
  if (!createdActivityId) {
    console.error(JSON.stringify(createActivityResult.payload, null, 2));
    throw new Error('Marketing engagement validation-order activity setup did not return an activity id');
  }
  createActivityPayload = createActivityResult.payload;

  const multipleActivitySelectorsResult = await runGraphqlRequest(multipleActivitySelectorsDocument, {
    activityId: createdActivityId,
    remoteId,
    engagement: mismatchedCurrencyEngagement,
  });
  await assertHttpOk('Marketing engagement multiple activity selectors capture', multipleActivitySelectorsResult);
  multipleActivitySelectorsPayload = multipleActivitySelectorsResult.payload;

  const multipleChannelSelectorsResult = await runGraphqlRequest(multipleChannelSelectorsDocument, {
    channelHandle: unknownChannelHandle,
    remoteId,
    engagement: mismatchedCurrencyEngagement,
  });
  await assertHttpOk('Marketing engagement multiple channel selectors capture', multipleChannelSelectorsResult);
  multipleChannelSelectorsPayload = multipleChannelSelectorsResult.payload;

  const unknownChannelCurrencyResult = await runGraphqlRequest(unknownChannelCurrencyDocument, {
    channelHandle: unknownChannelHandle,
    engagement: mismatchedCurrencyEngagement,
  });
  await assertHttpOk('Marketing engagement unknown channel currency capture', unknownChannelCurrencyResult);
  unknownChannelCurrencyPayload = unknownChannelCurrencyResult.payload;

  const unknownRemoteCurrencyResult = await runGraphqlRequest(unknownRemoteCurrencyDocument, {
    remoteId: unknownRemoteId,
    engagement: mismatchedCurrencyEngagement,
  });
  await assertHttpOk('Marketing engagement unknown remote currency capture', unknownRemoteCurrencyResult);
  unknownRemoteCurrencyPayload = unknownRemoteCurrencyResult.payload;

  const unknownRemoteResult = await runGraphqlRequest(unknownRemoteDocument, {
    remoteId: unknownRemoteId,
    engagement: validCurrencyEngagement,
  });
  await assertHttpOk('Marketing engagement unknown remote capture', unknownRemoteResult);
  unknownRemotePayload = unknownRemoteResult.payload;
} finally {
  if (createdActivityId) {
    const cleanupResult = await runGraphqlRequest(deleteActivityDocument, { remoteId });
    await assertHttpOk('Marketing engagement validation-order activity cleanup', cleanupResult);
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
    multipleActivitySelectorsCurrencyMismatch: {
      request: {
        query: multipleActivitySelectorsDocument,
        variables: {
          activityId: createdActivityId,
          remoteId,
          engagement: mismatchedCurrencyEngagement,
        },
      },
      response: multipleActivitySelectorsPayload,
    },
    multipleChannelSelectorsCurrencyMismatch: {
      request: {
        query: multipleChannelSelectorsDocument,
        variables: {
          channelHandle: unknownChannelHandle,
          remoteId,
          engagement: mismatchedCurrencyEngagement,
        },
      },
      response: multipleChannelSelectorsPayload,
    },
    unknownChannelCurrencyMismatch: {
      request: {
        query: unknownChannelCurrencyDocument,
        variables: {
          channelHandle: unknownChannelHandle,
          engagement: mismatchedCurrencyEngagement,
        },
      },
      response: unknownChannelCurrencyPayload,
    },
    unknownRemoteCurrencyMismatch: {
      request: {
        query: unknownRemoteCurrencyDocument,
        variables: {
          remoteId: unknownRemoteId,
          engagement: mismatchedCurrencyEngagement,
        },
      },
      response: unknownRemoteCurrencyPayload,
    },
    unknownRemote: {
      request: {
        query: unknownRemoteDocument,
        variables: {
          remoteId: unknownRemoteId,
          engagement: validCurrencyEngagement,
        },
      },
      response: unknownRemotePayload,
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
  blockers: {
    softDeletedMarketingEvent: {
      status: 'not-recordable-through-public-admin-api',
      note: 'The public Admin GraphQL API does not expose a safe setup path that soft-deletes a MarketingEvent while preserving its MarketingActivity for marketingEngagementCreate.',
    },
  },
  notes: [
    'Captured validation-order branches against a disposable external marketing activity and cleaned it up afterward.',
    'Multiple-selector branches intentionally combine mismatched adSpend/sales currencies to prove selector-count validation wins first.',
    'The unknown channel branch proves channel validation wins before input currency validation on the channel-handle path.',
    'The unknown remote branch pair proves input currency validation wins before activity lookup for single remote-id selectors, and a valid-currency unknown remote ID returns MARKETING_ACTIVITY_DOES_NOT_EXIST.',
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
