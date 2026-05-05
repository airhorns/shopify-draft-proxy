/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlPayload } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type CustomerCreateData = {
  customerCreate?: {
    customer?: { id?: string; state?: string } | null;
    userErrors?: unknown[];
  };
};
type CustomerCreatePayload = ConformanceGraphqlPayload<CustomerCreateData>;

type InviteData = {
  customerSendAccountInviteEmail?: {
    customer?: { id?: string; state?: string } | null;
    userErrors?: Array<{ field?: string[]; message?: string; code?: string | null }>;
  };
};
type InvitePayload = ConformanceGraphqlPayload<InviteData>;

type CustomerReadData = {
  customer?: { id?: string; state?: string } | null;
};
type CustomerReadPayload = ConformanceGraphqlPayload<CustomerReadData>;

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'customers');
const { runGraphqlRequest: runGraphql } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const createMutation = `#graphql
  mutation CustomerInviteEmailValidationCreate($input: CustomerInput!) {
    customerCreate(input: $input) {
      customer {
        id
        state
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const inviteMutation = `#graphql
  mutation CustomerInviteEmailValidationInvite($customerId: ID!, $email: EmailInput) {
    customerSendAccountInviteEmail(customerId: $customerId, email: $email) {
      customer {
        id
        state
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const readMutation = `#graphql
  query CustomerInviteEmailValidationRead($id: ID!) {
    customer(id: $id) {
      id
      state
    }
  }
`;

const deleteMutation = `#graphql
  mutation CustomerInviteEmailValidationDelete($input: CustomerDeleteInput!) {
    customerDelete(input: $input) {
      deletedCustomerId
      userErrors {
        field
        message
      }
    }
  }
`;

function assertNoTopLevelErrors(payload: { errors?: unknown }, context: string): void {
  if (payload.errors) {
    throw new Error(`${context} returned top-level errors: ${JSON.stringify(payload, null, 2)}`);
  }
}

function assertCreatedCustomer(payload: CustomerCreatePayload, context: string): string {
  assertNoTopLevelErrors(payload, context);
  const id = payload.data?.customerCreate?.customer?.id;
  if (typeof id !== 'string' || id.length === 0) {
    throw new Error(`${context} did not create a customer: ${JSON.stringify(payload, null, 2)}`);
  }
  return id;
}

function inviteUserErrors(payload: InvitePayload): Array<{ field?: string[]; message?: string; code?: string | null }> {
  return payload.data?.customerSendAccountInviteEmail?.userErrors ?? [];
}

function inviteState(payload: InvitePayload): string | null {
  return payload.data?.customerSendAccountInviteEmail?.customer?.state ?? null;
}

function readState(payload: CustomerReadPayload): string | null {
  return payload.data?.customer?.state ?? null;
}

function assertInviteResult(
  payload: InvitePayload,
  expected: { label: string; userErrorCount: number; state: string | null },
): void {
  assertNoTopLevelErrors(payload, expected.label);
  const errors = inviteUserErrors(payload);
  if (errors.length !== expected.userErrorCount) {
    throw new Error(
      `${expected.label} returned ${errors.length} userErrors, expected ${expected.userErrorCount}: ${JSON.stringify(
        payload,
        null,
        2,
      )}`,
    );
  }
  if (inviteState(payload) !== expected.state) {
    throw new Error(`${expected.label} returned unexpected customer state: ${JSON.stringify(payload, null, 2)}`);
  }
}

let phoneCounter = 0;

function createInputFor(stamp: number, label: string, phoneOnly: boolean): Record<string, unknown> {
  const base = {
    firstName: 'Hermes',
    lastName: label,
    tags: ['invite-validation'],
  };
  if (phoneOnly) {
    phoneCounter += 1;
    const suffix = String((stamp + phoneCounter) % 10_000).padStart(4, '0');
    return {
      ...base,
      phone: `+1415555${suffix}`,
    };
  }
  return {
    ...base,
    email: `hermes-invite-${label}-${stamp}@example.com`,
  };
}

async function runCase(
  stamp: number,
  label: string,
  email: Record<string, unknown>,
  expected: { userErrorCount: number; inviteState: string | null; readState: string },
  options: { phoneOnly?: boolean } = {},
) {
  const createVariables = {
    input: createInputFor(stamp, label, options.phoneOnly === true),
  };
  const create = await runGraphql<CustomerCreateData>(createMutation, createVariables);
  const customerId = assertCreatedCustomer(create.payload, `${label} customerCreate`);
  const inviteVariables = { customerId, email };
  const invite = await runGraphql<InviteData>(inviteMutation, inviteVariables);
  assertInviteResult(invite.payload, {
    label,
    userErrorCount: expected.userErrorCount,
    state: expected.inviteState,
  });
  const readVariables = { id: customerId };
  const readAfterInvite = await runGraphql<CustomerReadData>(readMutation, readVariables);
  assertNoTopLevelErrors(readAfterInvite.payload, `${label} readAfterInvite`);
  if (readState(readAfterInvite.payload) !== expected.readState) {
    throw new Error(`${label} left unexpected customer state: ${JSON.stringify(readAfterInvite.payload, null, 2)}`);
  }

  return {
    customerId,
    create: {
      variables: createVariables,
      response: create.payload,
      status: create.status,
    },
    invite: {
      variables: inviteVariables,
      response: invite.payload,
      status: invite.status,
    },
    readAfterInvite: {
      variables: readVariables,
      response: readAfterInvite.payload,
      status: readAfterInvite.status,
    },
  };
}

async function main(): Promise<void> {
  await mkdir(outputDir, { recursive: true });
  const stamp = Date.now();
  const cleanupIds: string[] = [];

  const cases: Record<string, Awaited<ReturnType<typeof runCase>>> = {};
  try {
    cases['blankSubject'] = await runCase(
      stamp,
      'blank-subject',
      { subject: '' },
      { userErrorCount: 1, inviteState: null, readState: 'DISABLED' },
    );
    cleanupIds.push(cases['blankSubject'].customerId);

    cases['invalidTo'] = await runCase(
      stamp,
      'invalid-to',
      { subject: 'Account invite', to: 'not-an-email' },
      { userErrorCount: 1, inviteState: null, readState: 'DISABLED' },
      { phoneOnly: true },
    );
    cleanupIds.push(cases['invalidTo'].customerId);

    cases['invalidFrom'] = await runCase(
      stamp,
      'invalid-from',
      { subject: 'Account invite', from: 'not-an-email' },
      { userErrorCount: 1, inviteState: null, readState: 'DISABLED' },
    );
    cleanupIds.push(cases['invalidFrom'].customerId);

    cases['invalidBcc'] = await runCase(
      stamp,
      'invalid-bcc',
      { subject: 'Account invite', bcc: ['bad', 'ok@example.com'] },
      { userErrorCount: 1, inviteState: null, readState: 'DISABLED' },
    );
    cleanupIds.push(cases['invalidBcc'].customerId);

    cases['oversizedSubject'] = await runCase(
      stamp,
      'oversized-subject',
      { subject: 's'.repeat(1001) },
      { userErrorCount: 1, inviteState: null, readState: 'DISABLED' },
    );
    cleanupIds.push(cases['oversizedSubject'].customerId);

    cases['oversizedCustomMessage'] = await runCase(
      stamp,
      'oversized-custom-message',
      { subject: 'Account invite', customMessage: 'm'.repeat(5001) },
      { userErrorCount: 1, inviteState: null, readState: 'DISABLED' },
    );
    cleanupIds.push(cases['oversizedCustomMessage'].customerId);

    cases['htmlCustomMessage'] = await runCase(
      stamp,
      'html-custom-message',
      { subject: 'Account invite', customMessage: '<script>alert(1)</script>' },
      { userErrorCount: 1, inviteState: null, readState: 'DISABLED' },
    );
    cleanupIds.push(cases['htmlCustomMessage'].customerId);
  } finally {
    const cleanup = [];
    for (const customerId of cleanupIds.reverse()) {
      const result = await runGraphql(deleteMutation, { input: { id: customerId } });
      cleanup.push({
        variables: { input: { id: customerId } },
        status: result.status,
        response: result.payload,
      });
    }

    const capture = {
      storeDomain,
      apiVersion,
      cases,
      cleanup,
      upstreamCalls: [],
    };
    const fileName = 'customer-invite-email-validation.json';
    await writeFile(path.join(outputDir, fileName), `${JSON.stringify(capture, null, 2)}\n`, 'utf8');
  }

  console.log(
    JSON.stringify(
      {
        ok: true,
        outputDir,
        files: ['customer-invite-email-validation.json'],
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
