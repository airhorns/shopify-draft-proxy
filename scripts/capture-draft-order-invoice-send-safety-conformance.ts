/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlPayload } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'orders');
const { runGraphql, runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

function asRecord(value: unknown): JsonRecord | null {
  return typeof value === 'object' && value !== null && !Array.isArray(value) ? (value as JsonRecord) : null;
}

function readRecord(value: unknown, key: string): JsonRecord | null {
  return asRecord(asRecord(value)?.[key]);
}

function readString(value: unknown, key: string): string | null {
  const fieldValue = asRecord(value)?.[key];
  return typeof fieldValue === 'string' && fieldValue.length > 0 ? fieldValue : null;
}

function mutationField(payload: ConformanceGraphqlPayload<JsonRecord>, name: string): JsonRecord {
  const data = asRecord(payload.data);
  const field = readRecord(data, name);
  if (!field) {
    throw new Error(`Expected ${name} mutation payload, got: ${JSON.stringify(payload, null, 2)}`);
  }
  return field;
}

function draftOrderIdFromPayload(payload: ConformanceGraphqlPayload<JsonRecord>, name: string): string {
  const id = readString(readRecord(mutationField(payload, name), 'draftOrder'), 'id');
  if (!id) {
    throw new Error(`Expected ${name}.draftOrder.id in payload: ${JSON.stringify(payload, null, 2)}`);
  }
  return id;
}

async function createDraftOrder(label: string): Promise<{
  variables: JsonRecord;
  mutation: { response: ConformanceGraphqlPayload<JsonRecord> };
  id: string;
}> {
  const variables = {
    input: {
      note: `HAR-275 draft order invoice safety ${label}`,
      tags: ['har-275', 'invoice-safety', label],
      lineItems: [
        {
          title: `HAR-275 invoice safety item ${label}`,
          quantity: 1,
          originalUnitPrice: '1.00',
        },
      ],
    },
  };
  const response = await runGraphql<JsonRecord>(draftOrderCreateDocument, variables);
  return {
    variables,
    mutation: { response },
    id: draftOrderIdFromPayload(response, 'draftOrderCreate'),
  };
}

const draftOrderCreateDocument = `#graphql
  mutation DraftOrderInvoiceSafetyCreate($input: DraftOrderInput!) {
    draftOrderCreate(input: $input) {
      draftOrder {
        id
        status
        email
        invoiceUrl
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const draftOrderCompleteDocument = `#graphql
  mutation DraftOrderInvoiceSafetyComplete($id: ID!) {
    draftOrderComplete(id: $id, paymentPending: true) {
      draftOrder {
        id
        status
        email
        invoiceUrl
        order {
          id
          name
        }
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const draftOrderDeleteDocument = `#graphql
  mutation DraftOrderInvoiceSafetyDelete($input: DraftOrderDeleteInput!) {
    draftOrderDelete(input: $input) {
      deletedId
      userErrors {
        field
        message
      }
    }
  }
`;

const draftOrderInvoiceSendDocument = `#graphql
  mutation DraftOrderInvoiceSendSafety($id: ID!, $email: EmailInput) {
    draftOrderInvoiceSend(id: $id, email: $email) {
      draftOrder {
        id
        status
        email
        invoiceUrl
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const draftOrderInvoiceSendMissingVariableDocument = `#graphql
  mutation DraftOrderInvoiceSendMissingVariable($id: ID!) {
    draftOrderInvoiceSend(id: $id) {
      draftOrder {
        id
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const draftOrderInvoiceSendInlineMissingIdDocument = `#graphql
  mutation DraftOrderInvoiceSendInlineMissingId {
    draftOrderInvoiceSend {
      draftOrder {
        id
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const draftOrderInvoiceSendInlineNullIdDocument = `#graphql
  mutation DraftOrderInvoiceSendInlineNullId {
    draftOrderInvoiceSend(id: null) {
      draftOrder {
        id
      }
      userErrors {
        field
        message
      }
    }
  }
`;

async function main(): Promise<void> {
  await mkdir(outputDir, { recursive: true });

  const missingVariable = await runGraphqlRequest(draftOrderInvoiceSendMissingVariableDocument, {});
  const inlineMissingId = await runGraphqlRequest(draftOrderInvoiceSendInlineMissingIdDocument, {});
  const inlineNullId = await runGraphqlRequest(draftOrderInvoiceSendInlineNullIdDocument, {});

  const unknownDraftOrderVariables = {
    id: 'gid://shopify/DraftOrder/999999999999999',
    email: {
      to: 'draft-order-invoice-safety@example.com',
      subject: 'HAR-275 unknown draft order safety probe',
      customMessage: 'Unknown-id validation should not send an invoice email.',
    },
  };
  const unknownDraftOrder = await runGraphql<JsonRecord>(draftOrderInvoiceSendDocument, unknownDraftOrderVariables);

  const openNoRecipientSetup = await createDraftOrder('open-no-recipient');
  const openNoRecipientVariables = { id: openNoRecipientSetup.id };
  const openNoRecipient = await runGraphql<JsonRecord>(draftOrderInvoiceSendDocument, openNoRecipientVariables);
  const openNoRecipientCleanupVariables = { input: { id: openNoRecipientSetup.id } };
  const openNoRecipientCleanup = await runGraphql<JsonRecord>(
    draftOrderDeleteDocument,
    openNoRecipientCleanupVariables,
  );

  const deletedSetup = await createDraftOrder('deleted');
  const deletedCleanupVariables = { input: { id: deletedSetup.id } };
  const deletedCleanup = await runGraphql<JsonRecord>(draftOrderDeleteDocument, deletedCleanupVariables);
  const deletedVariables = { id: deletedSetup.id };
  const deletedDraftOrder = await runGraphql<JsonRecord>(draftOrderInvoiceSendDocument, deletedVariables);

  const completedSetup = await createDraftOrder('completed-no-recipient');
  const completedVariables = { id: completedSetup.id };
  const completed = await runGraphql<JsonRecord>(draftOrderCompleteDocument, completedVariables);
  const completedNoRecipient = await runGraphql<JsonRecord>(draftOrderInvoiceSendDocument, completedVariables);

  const capture = {
    safetyPolicy:
      'All captured invoice-send branches avoid sending customer-visible email: validation-only requests use missing/unknown IDs, and live draft-order resolver branches use draft orders without a recipient email.',
    validation: {
      missingVariable: {
        document: draftOrderInvoiceSendMissingVariableDocument,
        variables: {},
        response: missingVariable.payload,
      },
      inlineMissingId: {
        document: draftOrderInvoiceSendInlineMissingIdDocument,
        variables: {},
        response: inlineMissingId.payload,
      },
      inlineNullId: {
        document: draftOrderInvoiceSendInlineNullIdDocument,
        variables: {},
        response: inlineNullId.payload,
      },
      unknownDraftOrder: {
        document: draftOrderInvoiceSendDocument,
        variables: unknownDraftOrderVariables,
        response: unknownDraftOrder,
      },
    },
    recipient: {
      openNoRecipient: {
        setup: {
          draftOrderCreate: openNoRecipientSetup,
        },
        document: draftOrderInvoiceSendDocument,
        variables: openNoRecipientVariables,
        mutation: {
          response: openNoRecipient,
        },
        cleanup: {
          variables: openNoRecipientCleanupVariables,
          mutation: {
            response: openNoRecipientCleanup,
          },
        },
      },
    },
    lifecycle: {
      deletedDraftOrder: {
        setup: {
          draftOrderCreate: deletedSetup,
          draftOrderDelete: {
            variables: deletedCleanupVariables,
            mutation: {
              response: deletedCleanup,
            },
          },
        },
        document: draftOrderInvoiceSendDocument,
        variables: deletedVariables,
        mutation: {
          response: deletedDraftOrder,
        },
      },
      completedNoRecipient: {
        setup: {
          draftOrderCreate: completedSetup,
          draftOrderComplete: {
            variables: completedVariables,
            mutation: {
              response: completed,
            },
          },
        },
        document: draftOrderInvoiceSendDocument,
        variables: completedVariables,
        mutation: {
          response: completedNoRecipient,
        },
      },
    },
  };

  const fileName = 'draft-order-invoice-send-safety.json';
  await writeFile(path.join(outputDir, fileName), `${JSON.stringify(capture, null, 2)}\n`, 'utf8');

  console.log(
    JSON.stringify(
      {
        ok: true,
        storeDomain,
        apiVersion,
        outputDir,
        files: [fileName],
      },
      null,
      2,
    ),
  );
}

main().catch((error) => {
  console.error(error instanceof Error ? error.message : error);
  process.exit(1);
});
