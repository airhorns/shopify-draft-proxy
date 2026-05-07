/* oxlint-disable no-console -- CLI capture script intentionally writes progress to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const fixtureDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'orders');
const fixturePath = path.join(fixtureDir, 'return-decline-request-validation.json');

const schemaQuery = `#graphql
  query ReturnDeclineRequestValidationSchema {
    returnDeclineRequestInput: __type(name: "ReturnDeclineRequestInput") {
      inputFields {
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
            }
          }
        }
      }
    }
    returnApproveRequestInput: __type(name: "ReturnApproveRequestInput") {
      inputFields {
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
            }
          }
        }
      }
    }
    returnRequestInput: __type(name: "ReturnRequestInput") {
      inputFields {
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
            }
          }
        }
      }
    }
    notifyCustomerInput: __type(name: "NotifyCustomerInput") {
      inputFields {
        name
        type {
          kind
          name
          ofType {
            kind
            name
          }
        }
      }
    }
    returnDeclineReason: __type(name: "ReturnDeclineReason") {
      enumValues {
        name
      }
    }
  }
`;

const returnDeclineMutation = `#graphql
  mutation ReturnDeclineRequestInvalidReason($input: ReturnDeclineRequestInput!) {
    returnDeclineRequest(input: $input) {
      return {
        id
        status
        decline {
          reason
          note
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

const returnDeclineUnknownNotifyMutation = `#graphql
  mutation ReturnDeclineRequestUnknownNotifyPayload($input: ReturnDeclineRequestInput!) {
    returnDeclineRequest(input: $input) {
      return {
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

const returnApproveUnknownNotifyMutation = `#graphql
  mutation ReturnApproveRequestUnknownNotifyPayload($input: ReturnApproveRequestInput!) {
    returnApproveRequest(input: $input) {
      return {
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

const returnRequestUnknownNotifyMutation = `#graphql
  mutation ReturnRequestUnknownNotifyPayload($input: ReturnRequestInput!) {
    returnRequest(input: $input) {
      return {
        id
      }
      userErrors {
        field
        message
      }
    }
  }
`;

function stripGraphqlTag(query: string): string {
  return query.replace(/^#graphql\n/u, '').trim();
}

async function writeJson(filePath: string, payload: unknown): Promise<void> {
  await mkdir(path.dirname(filePath), { recursive: true });
  await writeFile(filePath, `${JSON.stringify(payload, null, 2)}\n`);
}

function responseErrors(result: ConformanceGraphqlResult): JsonRecord[] {
  const errors = result.payload.errors;
  return Array.isArray(errors)
    ? errors.filter(
        (error): error is JsonRecord => Boolean(error) && typeof error === 'object' && !Array.isArray(error),
      )
    : [];
}

function assertInvalidVariableProblem(
  label: string,
  result: ConformanceGraphqlResult,
  expectedProblemPath: string,
  expectedExplanation: string,
): void {
  const errors = responseErrors(result);
  const problem = errors
    .flatMap((error) => {
      const extensions = error['extensions'];
      if (!extensions || typeof extensions !== 'object' || Array.isArray(extensions)) {
        return [];
      }
      const problems = (extensions as JsonRecord)['problems'];
      return Array.isArray(problems) ? problems : [];
    })
    .find((candidate) => {
      if (!candidate || typeof candidate !== 'object' || Array.isArray(candidate)) {
        return false;
      }
      const record = candidate as JsonRecord;
      return (
        JSON.stringify(record['path']) === JSON.stringify([expectedProblemPath]) &&
        record['explanation'] === expectedExplanation
      );
    });

  if (errors.length === 0 || !problem) {
    throw new Error(`${label} did not return expected INVALID_VARIABLE problem: ${JSON.stringify(result.payload)}`);
  }
}

const schema = await runGraphqlRequest(schemaQuery);
const invalidDeclineReason = await runGraphqlRequest(returnDeclineMutation, {
  input: {
    id: 'gid://shopify/Return/999999999',
    declineReason: 'BANANAS',
    notifyCustomer: false,
  },
});
assertInvalidVariableProblem(
  'invalid declineReason',
  invalidDeclineReason,
  'declineReason',
  'Expected "BANANAS" to be one of: RETURN_PERIOD_ENDED, FINAL_SALE, OTHER',
);

const declineUnknownNotifyPayload = await runGraphqlRequest(returnDeclineUnknownNotifyMutation, {
  input: {
    id: 'gid://shopify/Return/999999999',
    declineReason: 'OTHER',
    notifyCustomer: true,
    tmp_notify_customer: {
      email_address: 'not-an-email',
    },
  },
});
assertInvalidVariableProblem(
  'decline tmp_notify_customer',
  declineUnknownNotifyPayload,
  'tmp_notify_customer',
  'Field is not defined on ReturnDeclineRequestInput',
);

const approveUnknownNotifyPayload = await runGraphqlRequest(returnApproveUnknownNotifyMutation, {
  input: {
    id: 'gid://shopify/Return/999999999',
    notifyCustomer: true,
    tmp_notify_customer: {
      email_address: 'not-an-email',
    },
  },
});
assertInvalidVariableProblem(
  'approve tmp_notify_customer',
  approveUnknownNotifyPayload,
  'tmp_notify_customer',
  'Field is not defined on ReturnApproveRequestInput',
);

const requestUnknownNotifyPayload = await runGraphqlRequest(returnRequestUnknownNotifyMutation, {
  input: {
    orderId: 'gid://shopify/Order/999999999',
    returnLineItems: [],
    notifyCustomer: true,
    tmp_notify_customer: {
      email_address: 'not-an-email',
    },
  },
});
assertInvalidVariableProblem(
  'request tmp_notify_customer',
  requestUnknownNotifyPayload,
  'tmp_notify_customer',
  'Field is not defined on ReturnRequestInput',
);

await writeJson(fixturePath, {
  capturedAt: new Date().toISOString(),
  storeDomain,
  apiVersion,
  source: 'live-shopify-admin-graphql',
  notes:
    'Public Admin GraphQL evidence for return decline/request payload validation. The public schema exposes declineReason as ReturnDeclineReason and rejects unknown enum variables before resolver execution. The current public schema does not expose tmp_notify_customer/NotifyCustomerInput on returnDeclineRequest, returnApproveRequest, or returnRequest; executable hidden-payload behavior is therefore local-runtime backed in fixtures/conformance/local-runtime/2026-04/orders/return-lifecycle-local-staging.json.',
  schema: schema.payload,
  invalidDeclineReason: {
    query: stripGraphqlTag(returnDeclineMutation),
    variables: {
      input: {
        id: 'gid://shopify/Return/999999999',
        declineReason: 'BANANAS',
        notifyCustomer: false,
      },
    },
    response: invalidDeclineReason.payload,
  },
  hiddenNotifyPayloadPublicSchema: {
    decline: {
      query: stripGraphqlTag(returnDeclineUnknownNotifyMutation),
      variables: {
        input: {
          id: 'gid://shopify/Return/999999999',
          declineReason: 'OTHER',
          notifyCustomer: true,
          tmp_notify_customer: {
            email_address: 'not-an-email',
          },
        },
      },
      response: declineUnknownNotifyPayload.payload,
    },
    approve: {
      query: stripGraphqlTag(returnApproveUnknownNotifyMutation),
      variables: {
        input: {
          id: 'gid://shopify/Return/999999999',
          notifyCustomer: true,
          tmp_notify_customer: {
            email_address: 'not-an-email',
          },
        },
      },
      response: approveUnknownNotifyPayload.payload,
    },
    request: {
      query: stripGraphqlTag(returnRequestUnknownNotifyMutation),
      variables: {
        input: {
          orderId: 'gid://shopify/Order/999999999',
          returnLineItems: [],
          notifyCustomer: true,
          tmp_notify_customer: {
            email_address: 'not-an-email',
          },
        },
      },
      response: requestUnknownNotifyPayload.payload,
    },
  },
  upstreamCalls: [],
});

console.log(`Wrote ${fixturePath}`);
