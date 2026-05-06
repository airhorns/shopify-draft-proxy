/* oxlint-disable no-console -- CLI scripts intentionally write status output to stdio. */
import 'dotenv/config';

import { readFile, mkdir, writeFile } from 'node:fs/promises';
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

type RecordedCall = {
  operationName: string;
  variables: Record<string, unknown>;
  query: string;
  response: {
    status: number;
    body: unknown;
  };
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2025-01',
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'gift-cards');
const outputPath = path.join(outputDir, 'gift-card-user-error-typename.json');
const parityDocumentPath = path.join(
  'config',
  'parity-requests',
  'gift-cards',
  'gift-card-user-error-typename.graphql',
);
const parityDocument = await readFile(parityDocumentPath, 'utf8');

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

function readCreatedGiftCardId(capture: CapturedRequest): string | null {
  return readStringPath(capture.response.payload, ['data', 'giftCardCreate', 'giftCard', 'id']);
}

function fieldNames(value: unknown): string[] {
  return Array.isArray(value)
    ? value
        .map((field) => (isObject(field) && typeof field['name'] === 'string' ? field['name'] : null))
        .filter((name): name is string => name !== null)
    : [];
}

function namedLeaf(type: unknown): string | null {
  let cursor = type;
  while (isObject(cursor)) {
    const name = cursor['name'];
    if (typeof name === 'string' && name.length > 0) {
      return name;
    }
    cursor = cursor['ofType'];
  }
  return null;
}

function userErrorsLeafName(typeRecord: unknown): string | null {
  if (!isObject(typeRecord)) {
    return null;
  }
  const fields = typeRecord['fields'];
  if (!Array.isArray(fields)) {
    return null;
  }
  const userErrors = fields.find((field) => isObject(field) && field['name'] === 'userErrors');
  return isObject(userErrors) ? namedLeaf(userErrors['type']) : null;
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
      mutation GiftCardUserErrorTypenameCleanup($id: ID!) {
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

async function hydrateGiftCard(id: string): Promise<RecordedCall> {
  const variables = { id };
  const query = `#graphql
    query GiftCardHydrate($id: ID!) {
      giftCard(id: $id) {
        id
        lastCharacters
        maskedCode
        enabled
        deactivatedAt
        expiresOn
        note
        templateSuffix
        createdAt
        updatedAt
        initialValue { amount currencyCode }
        balance { amount currencyCode }
        customer {
          id
          email
          defaultEmailAddress { emailAddress }
          defaultPhoneNumber { phoneNumber }
        }
        recipientAttributes {
          message
          preferredName
          sendNotificationAt
          recipient {
            id
            email
            defaultEmailAddress { emailAddress }
            defaultPhoneNumber { phoneNumber }
          }
        }
        transactions(first: 250) {
          nodes {
            __typename
            id
            note
            processedAt
            amount { amount currencyCode }
          }
        }
      }
      giftCardConfiguration {
        issueLimit { amount currencyCode }
        purchaseLimit { amount currencyCode }
      }
    }
  `;
  const response = await runGraphqlRequest(query, variables);
  return {
    operationName: 'GiftCardHydrate',
    variables,
    query,
    response: {
      status: response.status,
      body: response.payload,
    },
  };
}

const schemaIntrospection = await capture(
  'schemaIntrospection',
  `#graphql
    query GiftCardUserErrorTypeIntrospection {
      giftCardCreatePayload: __type(name: "GiftCardCreatePayload") {
        name
        fields {
          name
          type {
            kind
            name
            ofType {
              kind
              name
              ofType {
                kind
                name
                ofType {
                  kind
                  name
                  ofType {
                    kind
                    name
                  }
                }
              }
            }
          }
        }
      }
      giftCardUpdatePayload: __type(name: "GiftCardUpdatePayload") {
        name
        fields {
          name
          type {
            kind
            name
            ofType {
              kind
              name
              ofType {
                kind
                name
                ofType {
                  kind
                  name
                  ofType {
                    kind
                    name
                  }
                }
              }
            }
          }
        }
      }
      giftCardDeactivatePayload: __type(name: "GiftCardDeactivatePayload") {
        name
        fields {
          name
          type {
            kind
            name
            ofType {
              kind
              name
              ofType {
                kind
                name
                ofType {
                  kind
                  name
                  ofType {
                    kind
                    name
                  }
                }
              }
            }
          }
        }
      }
      giftCardCreditPayload: __type(name: "GiftCardCreditPayload") {
        name
        fields {
          name
          type {
            kind
            name
            ofType {
              kind
              name
              ofType {
                kind
                name
                ofType {
                  kind
                  name
                  ofType {
                    kind
                    name
                  }
                }
              }
            }
          }
        }
      }
      giftCardDebitPayload: __type(name: "GiftCardDebitPayload") {
        name
        fields {
          name
          type {
            kind
            name
            ofType {
              kind
              name
              ofType {
                kind
                name
                ofType {
                  kind
                  name
                  ofType {
                    kind
                    name
                  }
                }
              }
            }
          }
        }
      }
      giftCardSendNotificationToCustomerPayload: __type(
        name: "GiftCardSendNotificationToCustomerPayload"
      ) {
        name
        fields {
          name
          type {
            kind
            name
            ofType {
              kind
              name
              ofType {
                kind
                name
                ofType {
                  kind
                  name
                  ofType {
                    kind
                    name
                  }
                }
              }
            }
          }
        }
      }
      giftCardSendNotificationToRecipientPayload: __type(
        name: "GiftCardSendNotificationToRecipientPayload"
      ) {
        name
        fields {
          name
          type {
            kind
            name
            ofType {
              kind
              name
              ofType {
                kind
                name
                ofType {
                  kind
                  name
                  ofType {
                    kind
                    name
                  }
                }
              }
            }
          }
        }
      }
      userError: __type(name: "UserError") {
        name
        fields {
          name
        }
      }
      giftCardUserError: __type(name: "GiftCardUserError") {
        name
        fields {
          name
        }
      }
      giftCardTransactionUserError: __type(name: "GiftCardTransactionUserError") {
        name
        fields {
          name
        }
      }
      giftCardDeactivateUserError: __type(name: "GiftCardDeactivateUserError") {
        name
        fields {
          name
        }
      }
      giftCardSendNotificationToCustomerUserError: __type(
        name: "GiftCardSendNotificationToCustomerUserError"
      ) {
        name
        fields {
          name
        }
      }
      giftCardSendNotificationToRecipientUserError: __type(
        name: "GiftCardSendNotificationToRecipientUserError"
      ) {
        name
        fields {
          name
        }
      }
    }
  `,
);

const runSuffix = Date.now().toString(36).slice(-8);
const setupSmallBalance = await capture(
  'setupSmallBalance',
  `#graphql
    mutation GiftCardUserErrorTypenameSetup($input: GiftCardCreateInput!) {
      giftCardCreate(input: $input) {
        giftCard {
          id
          enabled
          balance {
            amount
            currencyCode
          }
        }
        giftCardCode
        userErrors {
          field
          code
          message
        }
      }
    }
  `,
  {
    input: {
      initialValue: '5.00',
      code: `typename${runSuffix}`,
      note: 'Disposable gift-card user-error typename capture card.',
      expiresOn: '2099-01-01',
    },
  },
);

const smallBalanceId = readCreatedGiftCardId(setupSmallBalance);
if (smallBalanceId === null) {
  throw new Error('Unable to create disposable small-balance gift card for user-error typename capture.');
}

const missingGiftCardId = 'gid://shopify/GiftCard/999999999999';
const upstreamCalls = [await hydrateGiftCard(smallBalanceId), await hydrateGiftCard(missingGiftCardId)];

const proxyVariables = {
  smallBalanceId,
  negativeCreditInput: {
    creditAmount: {
      amount: '-1.00',
      currencyCode: 'CAD',
    },
  },
  insufficientDebitInput: {
    debitAmount: {
      amount: '9999.00',
      currencyCode: 'CAD',
    },
  },
};

const userErrorTypename = await capture('userErrorTypename', parityDocument, proxyVariables);
const cleanup = [await deactivateGiftCard('cleanupDeactivate:smallBalance', smallBalanceId)];

const schemaPayload = isObject(schemaIntrospection.response.payload)
  ? schemaIntrospection.response.payload['data']
  : undefined;
const schemaSummary = isObject(schemaPayload)
  ? {
      payloadUserErrorTypes: {
        GiftCardCreatePayload: userErrorsLeafName(schemaPayload['giftCardCreatePayload']),
        GiftCardUpdatePayload: userErrorsLeafName(schemaPayload['giftCardUpdatePayload']),
        GiftCardDeactivatePayload: userErrorsLeafName(schemaPayload['giftCardDeactivatePayload']),
        GiftCardCreditPayload: userErrorsLeafName(schemaPayload['giftCardCreditPayload']),
        GiftCardDebitPayload: userErrorsLeafName(schemaPayload['giftCardDebitPayload']),
        GiftCardSendNotificationToCustomerPayload: userErrorsLeafName(
          schemaPayload['giftCardSendNotificationToCustomerPayload'],
        ),
        GiftCardSendNotificationToRecipientPayload: userErrorsLeafName(
          schemaPayload['giftCardSendNotificationToRecipientPayload'],
        ),
      },
      userErrorFields: {
        UserError: fieldNames(isObject(schemaPayload['userError']) ? schemaPayload['userError']['fields'] : undefined),
        GiftCardUserError: fieldNames(
          isObject(schemaPayload['giftCardUserError']) ? schemaPayload['giftCardUserError']['fields'] : undefined,
        ),
        GiftCardTransactionUserError: fieldNames(
          isObject(schemaPayload['giftCardTransactionUserError'])
            ? schemaPayload['giftCardTransactionUserError']['fields']
            : undefined,
        ),
        GiftCardDeactivateUserError: fieldNames(
          isObject(schemaPayload['giftCardDeactivateUserError'])
            ? schemaPayload['giftCardDeactivateUserError']['fields']
            : undefined,
        ),
        GiftCardSendNotificationToCustomerUserError: fieldNames(
          isObject(schemaPayload['giftCardSendNotificationToCustomerUserError'])
            ? schemaPayload['giftCardSendNotificationToCustomerUserError']['fields']
            : undefined,
        ),
        GiftCardSendNotificationToRecipientUserError: fieldNames(
          isObject(schemaPayload['giftCardSendNotificationToRecipientUserError'])
            ? schemaPayload['giftCardSendNotificationToRecipientUserError']['fields']
            : undefined,
        ),
      },
    }
  : null;

await mkdir(outputDir, { recursive: true });
await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      capturedAt: new Date().toISOString(),
      storeDomain,
      apiVersion,
      notes: [
        'Captures gift-card mutation userErrors __typename values for schema-valid typed user-error payloads.',
        'GiftCardUpdatePayload.userErrors is generic UserError in the public schema. UserError exposes field/message but not code, so update is intentionally excluded from the strict code-selecting live request and covered by schema evidence plus local runtime tests.',
        'The small-balance setup gift card is deactivated during cleanup. The replay request earns gift-card state through cassette-backed hydration, not parity runner seeding.',
      ],
      schemaSummary,
      proxyVariables: {
        userErrorTypename: proxyVariables,
      },
      setup: {
        smallBalance: setupSmallBalance,
      },
      operations: {
        schemaIntrospection,
        userErrorTypename,
      },
      cleanup,
      upstreamCalls,
    },
    null,
    2,
  )}\n`,
);

console.log(`wrote ${outputPath}`);
