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
const outputPath = path.join(outputDir, 'marketing-engagement-lifecycle.json');

const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const schemaDocument = `#graphql
  query MarketingEngagementSchema {
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
    mutationRoot: __type(name: "Mutation") {
      fields {
        name
        args {
          name
        }
      }
    }
    engagementInput: __type(name: "MarketingEngagementInput") {
      inputFields {
        name
      }
    }
    engagementType: __type(name: "MarketingEngagement") {
      fields {
        name
      }
    }
    marketingEvents(first: 50, sortKey: ID, reverse: true) {
      nodes {
        id
        remoteId
        channelHandle
        marketingChannelType
        sourceAndMedium
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
        marketingEvent {
          id
          remoteId
          channelHandle
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

const downstreamReadDocument = `#graphql
  query MarketingEngagementDownstream($activityId: ID!) {
    marketingActivity(id: $activityId) {
      id
      adSpend {
        amount
        currencyCode
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

const channelProbeCandidates = [
  'hermes-conformance-products',
  'shopify_email',
  'email',
  'online_store',
  'shop_app',
  'facebook',
  'google',
  'google_shopping',
  'pinterest',
  'instagram',
  'tiktok',
];

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

function readStringList(value: unknown, key: string): string[] {
  const record = readRecord(value);
  const items = record?.[key];
  if (!Array.isArray(items)) {
    return [];
  }

  return items.flatMap((item): string[] => {
    const name = readString(readRecord(item)?.['name']);
    return name ? [name] : [];
  });
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

function readMutationArgs(schemaPayload: unknown, mutationName: string): string[] {
  const fields = readPath(schemaPayload, ['data', 'mutationRoot', 'fields']);
  if (!Array.isArray(fields)) {
    return [];
  }

  const field = fields.find((candidate) => readRecord(candidate)?.['name'] === mutationName);
  return readStringList(field, 'args');
}

function readInputFields(schemaPayload: unknown): string[] {
  return readStringList(readPath(schemaPayload, ['data', 'engagementInput']), 'inputFields');
}

function readExistingChannelHandles(schemaPayload: unknown): string[] {
  const nodes = readPath(schemaPayload, ['data', 'marketingEvents', 'nodes']);
  if (!Array.isArray(nodes)) {
    return [];
  }

  return [
    ...new Set(
      nodes.flatMap((node): string[] => {
        const handle = readString(readRecord(node)?.['channelHandle']);
        return handle ? [handle] : [];
      }),
    ),
  ].sort();
}

function buildEngagementSelection(inputFields: Set<string>): string {
  const conversionFields =
    inputFields.has('primaryConversions') && inputFields.has('allConversions')
      ? `
      primaryConversions
      allConversions`
      : '';

  return `
      occurredOn
      utcOffset
      isCumulative
      impressionsCount
      viewsCount
      clicksCount
      uniqueClicksCount
      adSpend {
        amount
        currencyCode
      }
      sales {
        amount
        currencyCode
      }
      orders${conversionFields}
      firstTimeCustomers
      returningCustomers
      marketingActivity {
        id
        adSpend {
          amount
          currencyCode
        }
      }
  `;
}

function buildEngagementLifecycleDocument(selection: string): string {
  return `#graphql
    mutation MarketingEngagementLifecycle(
      $remoteId: String!
      $invalidRemoteId: String!
      $invalidChannelHandle: String!
      $engagement: MarketingEngagementInput!
      $duplicateEngagement: MarketingEngagementInput!
      $negativeMetricEngagement: MarketingEngagementInput!
      $missingIdentifierEngagement: MarketingEngagementInput!
    ) {
      createByRemoteId: marketingEngagementCreate(remoteId: $remoteId, marketingEngagement: $engagement) {
        marketingEngagement {
          ${selection}
        }
        userErrors {
          field
          message
          code
        }
      }
      duplicateSameDay: marketingEngagementCreate(remoteId: $remoteId, marketingEngagement: $duplicateEngagement) {
        marketingEngagement {
          occurredOn
          impressionsCount
          clicksCount
          adSpend {
            amount
            currencyCode
          }
          marketingActivity {
            id
            adSpend {
              amount
              currencyCode
            }
          }
        }
        userErrors {
          field
          message
          code
        }
      }
      negativeMetric: marketingEngagementCreate(remoteId: $remoteId, marketingEngagement: $negativeMetricEngagement) {
        marketingEngagement {
          occurredOn
          impressionsCount
          clicksCount
        }
        userErrors {
          field
          message
          code
        }
      }
      missingIdentifier: marketingEngagementCreate(marketingEngagement: $missingIdentifierEngagement) {
        marketingEngagement {
          occurredOn
          impressionsCount
        }
        userErrors {
          field
          message
          code
        }
      }
      invalidRemoteId: marketingEngagementCreate(
        remoteId: $invalidRemoteId
        marketingEngagement: $missingIdentifierEngagement
      ) {
        marketingEngagement {
          occurredOn
          impressionsCount
        }
        userErrors {
          field
          message
          code
        }
      }
      multipleIdentifiers: marketingEngagementCreate(
        remoteId: $remoteId
        channelHandle: $invalidChannelHandle
        marketingEngagement: $missingIdentifierEngagement
      ) {
        marketingEngagement {
          occurredOn
          impressionsCount
        }
        userErrors {
          field
          message
          code
        }
      }
      invalidChannel: marketingEngagementCreate(
        channelHandle: $invalidChannelHandle
        marketingEngagement: $missingIdentifierEngagement
      ) {
        marketingEngagement {
          occurredOn
          impressionsCount
          channelHandle
        }
        userErrors {
          field
          message
          code
        }
      }
      deleteAllChannels: marketingEngagementsDelete(deleteEngagementsForAllChannels: true) {
        result
        userErrors {
          field
          message
          code
        }
      }
      deleteMissingSelector: marketingEngagementsDelete {
        result
        userErrors {
          field
          message
          code
        }
      }
    }
  `;
}

function buildChannelProbeDocument(includeConversions: boolean): string {
  const conversionSelection = includeConversions
    ? `
          primaryConversions
          allConversions`
    : '';

  return `#graphql
    mutation ChannelProbe($channelHandle: String!, $engagement: MarketingEngagementInput!) {
      marketingEngagementCreate(channelHandle: $channelHandle, marketingEngagement: $engagement) {
        marketingEngagement {
          occurredOn
          channelHandle
          impressionsCount${conversionSelection}
        }
        userErrors {
          field
          message
          code
        }
      }
    }
  `;
}

const deleteChannelEngagementsDocument = `#graphql
  mutation DeleteChannelEngagements($channelHandle: String!) {
    marketingEngagementsDelete(channelHandle: $channelHandle) {
      result
      userErrors {
        field
        message
        code
      }
    }
  }
`;

function hasUserErrors(payload: unknown, pathParts: string[]): boolean {
  const userErrors = readPath(payload, pathParts);
  return Array.isArray(userErrors) && userErrors.length > 0;
}

function readCreatedActivityId(payload: unknown): string | null {
  return readString(readPath(payload, ['data', 'marketingActivityCreateExternal', 'marketingActivity', 'id']));
}

function compactChannelProbeResult(
  channelHandle: string,
  response: unknown,
  cleanup: unknown,
): Record<string, unknown> {
  const createPayload = readPath(response, ['data', 'marketingEngagementCreate']);
  return {
    channelHandle,
    response: {
      data: {
        marketingEngagementCreate: createPayload,
      },
    },
    cleanup,
  };
}

await mkdir(outputDir, { recursive: true });

const schemaResult = await runGraphqlRequest(schemaDocument);
await assertHttpOk('Marketing engagement schema capture', schemaResult);

const inputFields = new Set(readInputFields(schemaResult.payload));
const includeConversions = inputFields.has('primaryConversions') && inputFields.has('allConversions');
const runId = Date.now().toString(36);
const remoteId = `har-463-engagement-${runId}`;
const invalidRemoteId = `missing-${remoteId}`;
const invalidChannelHandle = 'unknown-channel';
const activityInput = {
  title: 'HAR-463 Engagement Campaign',
  remoteId,
  status: 'ACTIVE',
  remoteUrl: `https://example.com/${remoteId}`,
  tactic: 'NEWSLETTER',
  marketingChannelType: 'EMAIL',
  utm: {
    campaign: remoteId,
    source: 'newsletter',
    medium: 'email',
  },
};
const engagement = {
  occurredOn: '2026-04-26',
  impressionsCount: 7,
  viewsCount: 5,
  clicksCount: 2,
  uniqueClicksCount: 1,
  adSpend: {
    amount: '3.21',
    currencyCode: 'USD',
  },
  sales: {
    amount: '12.34',
    currencyCode: 'USD',
  },
  orders: '1.5',
  ...(includeConversions ? { primaryConversions: '0.75', allConversions: '2.25' } : {}),
  firstTimeCustomers: '1.0',
  returningCustomers: '0.5',
  isCumulative: false,
  utcOffset: '+00:00',
};
const duplicateEngagement = {
  occurredOn: '2026-04-26',
  impressionsCount: 9,
  clicksCount: 4,
  adSpend: {
    amount: '4.56',
    currencyCode: 'USD',
  },
  isCumulative: false,
  utcOffset: '+00:00',
};
const negativeMetricEngagement = {
  occurredOn: '2026-04-25',
  impressionsCount: -1,
  clicksCount: -2,
  isCumulative: false,
  utcOffset: '+00:00',
};
const missingIdentifierEngagement = {
  occurredOn: '2026-04-26',
  impressionsCount: 7,
  isCumulative: false,
  utcOffset: '+00:00',
};

let createdActivityId: string | null = null;
let createActivityPayload: unknown = null;
let lifecyclePayload: unknown = null;
let downstreamReadPayload: unknown = null;
let cleanupPayload: unknown = null;
const channelProbeResults: Array<Record<string, unknown>> = [];

try {
  const createActivityResult = await runGraphqlRequest(createActivityDocument, { input: activityInput });
  await assertHttpOk('Marketing engagement activity setup', createActivityResult);
  if (hasUserErrors(createActivityResult.payload, ['data', 'marketingActivityCreateExternal', 'userErrors'])) {
    console.error(JSON.stringify(createActivityResult.payload, null, 2));
    throw new Error('Marketing engagement activity setup returned userErrors');
  }
  createdActivityId = readCreatedActivityId(createActivityResult.payload);
  if (!createdActivityId) {
    console.error(JSON.stringify(createActivityResult.payload, null, 2));
    throw new Error('Marketing engagement activity setup did not return an activity id');
  }
  createActivityPayload = createActivityResult.payload;

  const lifecycleDocument = buildEngagementLifecycleDocument(buildEngagementSelection(inputFields));
  const lifecycleResult = await runGraphqlRequest(lifecycleDocument, {
    remoteId,
    invalidRemoteId,
    invalidChannelHandle,
    engagement,
    duplicateEngagement,
    negativeMetricEngagement,
    missingIdentifierEngagement,
  });
  await assertHttpOk('Marketing engagement lifecycle capture', lifecycleResult);
  lifecyclePayload = lifecycleResult.payload;

  const downstreamReadResult = await runGraphqlRequest(downstreamReadDocument, { activityId: createdActivityId });
  await assertHttpOk('Marketing engagement downstream read capture', downstreamReadResult);
  downstreamReadPayload = downstreamReadResult.payload;

  const channelProbeDocument = buildChannelProbeDocument(includeConversions);
  for (const channelHandle of channelProbeCandidates) {
    const channelProbeResult = await runGraphqlRequest(channelProbeDocument, {
      channelHandle,
      engagement: {
        occurredOn: '2026-04-29',
        impressionsCount: 1,
        ...(includeConversions ? { primaryConversions: '0.1', allConversions: '0.2' } : {}),
        isCumulative: false,
        utcOffset: '+00:00',
      },
    });
    await assertHttpOk(`Marketing channel probe ${channelHandle}`, channelProbeResult);
    const createdChannelEngagement = readPath(channelProbeResult.payload, [
      'data',
      'marketingEngagementCreate',
      'marketingEngagement',
    ]);
    let channelCleanupPayload: unknown = null;
    if (createdChannelEngagement) {
      const channelCleanupResult = await runGraphqlRequest(deleteChannelEngagementsDocument, { channelHandle });
      await assertHttpOk(`Marketing channel cleanup ${channelHandle}`, channelCleanupResult);
      channelCleanupPayload = channelCleanupResult.payload;
    }
    channelProbeResults.push(
      compactChannelProbeResult(channelHandle, channelProbeResult.payload, channelCleanupPayload),
    );
  }
} finally {
  if (createdActivityId) {
    const cleanupResult = await runGraphqlRequest(deleteActivityDocument, { remoteId });
    await assertHttpOk('Marketing engagement activity cleanup', cleanupResult);
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
    mutationArgs: {
      marketingEngagementCreate: readMutationArgs(schemaResult.payload, 'marketingEngagementCreate'),
      marketingEngagementsDelete: readMutationArgs(schemaResult.payload, 'marketingEngagementsDelete'),
    },
    inputFields: [...inputFields],
    downstreamReadTargets: {
      MarketingActivity: ['adSpend'],
      MarketingEvent: [],
    },
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
    createByRemoteId: {
      response: readPath(lifecyclePayload, ['data', 'createByRemoteId'])
        ? {
            data: {
              marketingEngagementCreate: readPath(lifecyclePayload, ['data', 'createByRemoteId']),
            },
          }
        : null,
    },
    duplicateSameDay: {
      response: readPath(lifecyclePayload, ['data', 'duplicateSameDay'])
        ? {
            data: {
              marketingEngagementCreate: readPath(lifecyclePayload, ['data', 'duplicateSameDay']),
            },
          }
        : null,
    },
    downstreamReadAfterCreate: {
      request: {
        query: downstreamReadDocument,
        variables: {
          activityId: createdActivityId,
        },
      },
      response: downstreamReadPayload,
    },
    deleteAllChannels: {
      response: readPath(lifecyclePayload, ['data', 'deleteAllChannels'])
        ? {
            data: {
              marketingEngagementsDelete: readPath(lifecyclePayload, ['data', 'deleteAllChannels']),
            },
          }
        : null,
    },
    validation: {
      missingIdentifier: {
        data: {
          marketingEngagementCreate: readPath(lifecyclePayload, ['data', 'missingIdentifier']),
        },
      },
      invalidRemoteId: {
        data: {
          marketingEngagementCreate: readPath(lifecyclePayload, ['data', 'invalidRemoteId']),
        },
      },
      multipleIdentifiers: {
        data: {
          marketingEngagementCreate: readPath(lifecyclePayload, ['data', 'multipleIdentifiers']),
        },
      },
      invalidChannel: {
        data: {
          marketingEngagementCreate: readPath(lifecyclePayload, ['data', 'invalidChannel']),
        },
      },
      negativeMetric: {
        data: {
          marketingEngagementCreate: readPath(lifecyclePayload, ['data', 'negativeMetric']),
        },
      },
      deleteMissingSelector: {
        data: {
          marketingEngagementsDelete: readPath(lifecyclePayload, ['data', 'deleteMissingSelector']),
        },
      },
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
    channelHandleSuccess: {
      existingLiveChannelHandles: readExistingChannelHandles(schemaResult.payload),
      candidateProbeResults: channelProbeResults,
      summary:
        'The current conformance shop exposes no marketing events with non-null channelHandle, and common app/channel handles are rejected with INVALID_CHANNEL_HANDLE. Recognized channelHandle success remains blocked until the disposable shop exposes a valid handle.',
    },
  },
  notes: [
    'Activity-level engagement creates were captured against a disposable external marketing activity and cleaned up afterward.',
    includeConversions
      ? 'Admin GraphQL 2026-04 exposes primaryConversions and allConversions on MarketingEngagementInput/MarketingEngagement; both fields are captured in the createByRemoteId success payload.'
      : 'This Admin GraphQL version does not expose primaryConversions or allConversions, so conversion fields were not captured.',
    'Immediate downstream marketingActivity.adSpend remained null after activity-level engagement writes; the proxy therefore records engagement state without inventing aggregate attribution.',
    'No recognized channel handle was available in the conformance shop; unrecognized channelHandle branches return INVALID_CHANNEL_HANDLE.',
  ],
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
      capturedConversions: includeConversions,
      channelProbeCandidates: channelProbeResults.map((probe) => probe['channelHandle']),
    },
    null,
    2,
  ),
);
