/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import {
  createAdminGraphqlClient,
  type ConformanceGraphqlPayload,
  type ConformanceGraphqlResult,
} from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type CapturedRequest = {
  variables: Record<string, unknown>;
  response: ConformanceGraphqlPayload;
  status: number;
  downstreamRead?: CapturedRequest;
};

function isObject(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'customers');
const { runGraphqlRequest: runGraphql } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const createMutation = await readText('config/parity-requests/customers/customer-tag-note-limits-create.graphql');
const updateMutation = await readText('config/parity-requests/customers/customer-tag-note-limits-update.graphql');
const setMutation = await readText('config/parity-requests/customers/customer-tag-note-limits-set.graphql');
const readQuery = await readText('config/parity-requests/customers/customer-tag-note-limits-read.graphql');

const deleteMutation = `#graphql
  mutation CustomerTagNoteLimitsCleanup($input: CustomerDeleteInput!) {
    customerDelete(input: $input) {
      deletedCustomerId
      userErrors {
        field
        message
      }
    }
  }
`;

const createdCustomerIds = new Set<string>();
const deletedCustomerIds = new Set<string>();

async function readText(filePath: string): Promise<string> {
  return readFile(filePath, 'utf8');
}

function assertNoGraphqlFailure(result: ConformanceGraphqlResult, context: string): void {
  if (result.status < 200 || result.status >= 300 || result.payload.errors) {
    throw new Error(`${context} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function readMutationCustomerId(payload: ConformanceGraphqlPayload): string | null {
  const data = payload.data;
  if (!isObject(data)) {
    return null;
  }

  for (const key of ['customerCreate', 'customerSet', 'customerUpdate']) {
    const mutationPayload = data[key];
    if (!isObject(mutationPayload)) {
      continue;
    }

    const customer = mutationPayload['customer'];
    if (!isObject(customer)) {
      continue;
    }

    const id = customer['id'];
    if (typeof id === 'string' && id) {
      return id;
    }
  }

  return null;
}

function recordCreatedCustomerId(payload: ConformanceGraphqlPayload): string | null {
  const id = readMutationCustomerId(payload);
  if (typeof id === 'string' && id) {
    createdCustomerIds.add(id);
  }
  return id;
}

function emailFor(stamp: number, label: string): string {
  return `customer-tag-note-limits-${label}-${stamp}@example.com`;
}

function tooLongNote(): string {
  return 'N'.repeat(5001);
}

async function runCase(
  document: string,
  variables: Record<string, unknown>,
  context: string,
): Promise<CapturedRequest> {
  const result = await runGraphql(document, variables);
  assertNoGraphqlFailure(result, context);
  recordCreatedCustomerId(result.payload);
  return {
    variables,
    response: result.payload,
    status: result.status,
  };
}

async function readCustomer(customerId: string): Promise<CapturedRequest> {
  return runCase(readQuery, { id: customerId }, `read customer ${customerId}`);
}

async function cleanupCustomers(): Promise<CapturedRequest[]> {
  const cleanup: CapturedRequest[] = [];
  for (const customerId of [...createdCustomerIds].reverse()) {
    if (deletedCustomerIds.has(customerId)) {
      continue;
    }
    const result = await runGraphql(deleteMutation, { input: { id: customerId } });
    const data = result.payload.data;
    const customerDelete = isObject(data) ? data['customerDelete'] : null;
    const deletedCustomerId = isObject(customerDelete) ? customerDelete['deletedCustomerId'] : null;
    if (!result.payload.errors && typeof deletedCustomerId === 'string' && deletedCustomerId) {
      deletedCustomerIds.add(customerId);
    }
    cleanup.push({
      variables: { input: { id: customerId } },
      status: result.status,
      response: result.payload,
    });
  }
  return cleanup;
}

async function main() {
  await mkdir(outputDir, { recursive: true });

  const stamp = Date.now();
  const commaSplit = await runCase(
    createMutation,
    {
      input: {
        email: emailFor(stamp, 'comma-split'),
        tags: ['a, b , c'],
      },
    },
    'comma-split customerCreate',
  );
  const commaSplitCustomerId = readMutationCustomerId(commaSplit.response);
  if (typeof commaSplitCustomerId !== 'string' || !commaSplitCustomerId) {
    throw new Error(`comma-split create did not return a customer id: ${JSON.stringify(commaSplit.response, null, 2)}`);
  }
  commaSplit.downstreamRead = await readCustomer(commaSplitCustomerId);

  const caseDedupe = await runCase(
    createMutation,
    {
      input: {
        email: emailFor(stamp, 'case-dedupe'),
        tags: ['VIP', 'vip'],
      },
    },
    'case-dedupe customerCreate',
  );

  const tooManyTags = await runCase(
    createMutation,
    {
      input: {
        email: emailFor(stamp, 'too-many-tags'),
        tags: [Array.from({ length: 251 }, (_, index) => `tag-${index}`).join(',')],
      },
    },
    'too-many-tags customerCreate',
  );

  const tooLongNoteCreate = await runCase(
    createMutation,
    {
      input: {
        email: emailFor(stamp, 'too-long-note-create'),
        note: tooLongNote(),
      },
    },
    'too-long-note customerCreate',
  );

  const tooLongNoteUpdate = await runCase(
    updateMutation,
    {
      input: {
        id: commaSplitCustomerId,
        note: tooLongNote(),
      },
    },
    'too-long-note customerUpdate',
  );

  const tooLongNoteSet = await runCase(
    setMutation,
    {
      identifier: {
        id: commaSplitCustomerId,
      },
      input: {
        note: tooLongNote(),
      },
    },
    'too-long-note customerSet',
  );

  const cleanup = await cleanupCustomers();
  const capture = {
    storeDomain,
    apiVersion,
    commaSplit,
    caseDedupe,
    tooManyTags,
    tooLongNoteCreate,
    tooLongNoteUpdate,
    tooLongNoteSet,
    cleanup,
  };

  const outputFile = path.join(outputDir, 'customer-tag-note-limits-parity.json');
  await writeFile(outputFile, `${JSON.stringify(capture, null, 2)}\n`, 'utf8');

  console.log(
    JSON.stringify(
      {
        ok: true,
        outputFile,
        createdCustomers: createdCustomerIds.size,
        cleanupAttempts: cleanup.length,
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
