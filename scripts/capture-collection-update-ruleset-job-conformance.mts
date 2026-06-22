import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlPayload } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type CollectionCreateData = {
  collectionCreate?: {
    collection?: {
      id?: string;
    } | null;
    userErrors?: unknown[];
  } | null;
};

type CaptureStep = {
  variables: Record<string, unknown>;
  response: ConformanceGraphqlPayload;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'products');
const fixturePath = path.join(outputDir, 'collection-update-ruleset-job-parity.json');
const { runGraphql } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const createDocument = `#graphql
  mutation CollectionUpdateRuleSetJobCreate($input: CollectionInput!) {
    collectionCreate(input: $input) {
      collection {
        id
        title
        handle
        sortOrder
        ruleSet {
          appliedDisjunctively
          rules {
            column
            relation
            condition
          }
        }
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const updateDocument = `#graphql
  mutation CollectionUpdateRuleSetJobUpdate($input: CollectionInput!) {
    collectionUpdate(input: $input) {
      collection {
        id
        title
        handle
        ruleSet {
          appliedDisjunctively
          rules {
            column
            relation
            condition
          }
        }
      }
      job {
        id
        done
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const readDocument = `#graphql
  query CollectionUpdateRuleSetJobRead($id: ID!) {
    collection(id: $id) {
      id
      title
      handle
      ruleSet {
        appliedDisjunctively
        rules {
          column
          relation
          condition
        }
      }
    }
  }
`;

const deleteDocument = `#graphql
  mutation CollectionUpdateRuleSetJobCleanup($input: CollectionDeleteInput!) {
    collectionDelete(input: $input) {
      deletedCollectionId
      userErrors {
        field
        message
      }
    }
  }
`;

function collectionIdFromCreate(response: ConformanceGraphqlPayload<CollectionCreateData>, label: string): string {
  const id = response.data?.collectionCreate?.collection?.id;
  if (typeof id !== 'string' || id.length === 0) {
    throw new Error(`${label} did not return a collection id: ${JSON.stringify(response)}`);
  }
  return id;
}

async function deleteCollection(collectionId: string): Promise<ConformanceGraphqlPayload> {
  return await runGraphql(deleteDocument, { input: { id: collectionId } });
}

async function captureCreate(label: string, variables: Record<string, unknown>): Promise<CaptureStep & { id: string }> {
  const response = await runGraphql<CollectionCreateData>(createDocument, variables);
  return {
    variables,
    response,
    id: collectionIdFromCreate(response, label),
  };
}

const runId = `${Date.now()}`;
const createdIds: string[] = [];
let cleanup: Record<string, ConformanceGraphqlPayload> = {};

try {
  const customCreate = await captureCreate('customCreate', {
    input: {
      title: `Hermes Collection Update Job ${runId}`,
      sortOrder: 'MANUAL',
    },
  });
  createdIds.push(customCreate.id);

  const successfulUpdate: CaptureStep = {
    variables: {
      input: {
        id: customCreate.id,
        title: `Hermes Collection Update Job ${runId} Updated`,
        handle: `hermes-collection-update-job-${runId.toLowerCase()}-updated`,
      },
    },
    response: await runGraphql(updateDocument, {
      input: {
        id: customCreate.id,
        title: `Hermes Collection Update Job ${runId} Updated`,
        handle: `hermes-collection-update-job-${runId.toLowerCase()}-updated`,
      },
    }),
  };

  const customRuleSetUpdate: CaptureStep = {
    variables: {
      input: {
        id: customCreate.id,
        ruleSet: {
          appliedDisjunctively: false,
          rules: [{ column: 'TITLE', relation: 'CONTAINS', condition: 'Hermes' }],
        },
      },
    },
    response: await runGraphql(updateDocument, {
      input: {
        id: customCreate.id,
        ruleSet: {
          appliedDisjunctively: false,
          rules: [{ column: 'TITLE', relation: 'CONTAINS', condition: 'Hermes' }],
        },
      },
    }),
  };

  const postCustomRuleSetRead: CaptureStep = {
    variables: { id: customCreate.id },
    response: await runGraphql(readDocument, { id: customCreate.id }),
  };

  const smartCreate = await captureCreate('smartCreate', {
    input: {
      title: `Hermes Smart Collection Update Job ${runId}`,
      ruleSet: {
        appliedDisjunctively: false,
        rules: [{ column: 'TITLE', relation: 'CONTAINS', condition: 'Hermes' }],
      },
    },
  });
  createdIds.push(smartCreate.id);

  const smartRuleSetUpdate: CaptureStep = {
    variables: {
      input: {
        id: smartCreate.id,
        ruleSet: {
          appliedDisjunctively: false,
          rules: [{ column: 'TITLE', relation: 'CONTAINS', condition: 'Updated Hermes' }],
        },
      },
    },
    response: await runGraphql(updateDocument, {
      input: {
        id: smartCreate.id,
        ruleSet: {
          appliedDisjunctively: false,
          rules: [{ column: 'TITLE', relation: 'CONTAINS', condition: 'Updated Hermes' }],
        },
      },
    }),
  };

  const emptyRulesUpdate: CaptureStep = {
    variables: {
      input: {
        id: smartCreate.id,
        ruleSet: {
          appliedDisjunctively: false,
          rules: [],
        },
      },
    },
    response: await runGraphql(updateDocument, {
      input: {
        id: smartCreate.id,
        ruleSet: {
          appliedDisjunctively: false,
          rules: [],
        },
      },
    }),
  };

  cleanup = Object.fromEntries(
    await Promise.all(
      [...createdIds].reverse().map(async (id) => {
        try {
          return [id, await deleteCollection(id)] as const;
        } catch (error) {
          return [id, { errors: [(error as Error).message] }] as const;
        }
      }),
    ),
  );
  createdIds.length = 0;

  await mkdir(outputDir, { recursive: true });
  await writeFile(
    fixturePath,
    `${JSON.stringify(
      {
        summary:
          'Live 2026-04 collectionUpdate capture for async job payload shape plus ruleSet validation on custom and empty-rules branches.',
        storeDomain,
        apiVersion,
        customCreate,
        successfulUpdate,
        customRuleSetUpdate,
        postCustomRuleSetRead,
        smartCreate,
        smartRuleSetUpdate,
        emptyRulesUpdate,
        cleanup,
      },
      null,
      2,
    )}\n`,
    'utf8',
  );

  // oxlint-disable-next-line no-console -- CLI capture result is intentionally written to stdout.
  console.log(JSON.stringify({ ok: true, fixturePath, cleanupIds: Object.keys(cleanup) }, null, 2));
} finally {
  for (const id of createdIds.reverse()) {
    try {
      await deleteCollection(id);
    } catch {
      // Best-effort cleanup only. The capture should surface the original failure.
    }
  }
}
