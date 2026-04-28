/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'customers');
const { runGraphqlRequest: runGraphql } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

function assertNoTopLevelErrors(result: { status: number; payload?: { errors?: unknown } }, context: string): void {
  if (result.status < 200 || result.status >= 300 || result.payload?.errors) {
    throw new Error(`${context} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

const validationMutation = `#graphql
  mutation CustomerOutboundSideEffectValidation($customerId: ID!, $customerPaymentMethodId: ID!) {
    customerGenerateAccountActivationUrl(customerId: $customerId) {
      accountActivationUrl
      userErrors {
        field
        message
      }
    }
    customerSendAccountInviteEmail(customerId: $customerId) {
      customer {
        id
        state
      }
      userErrors {
        field
        message
      }
    }
    customerPaymentMethodSendUpdateEmail(customerPaymentMethodId: $customerPaymentMethodId) {
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

async function main(): Promise<void> {
  await mkdir(outputDir, { recursive: true });

  const validationVariables = {
    customerId: 'gid://shopify/Customer/999999999999999',
    customerPaymentMethodId: 'gid://shopify/CustomerPaymentMethod/999999999999999',
  };
  const validationResult = await runGraphql(validationMutation, validationVariables);
  assertNoTopLevelErrors(validationResult, 'customer outbound side-effect validation');

  const capture = {
    validation: {
      variables: validationVariables,
      response: validationResult.payload,
    },
  };

  const fileName = 'customer-outbound-side-effect-validation-parity.json';
  await writeFile(path.join(outputDir, fileName), `${JSON.stringify(capture, null, 2)}\n`, 'utf8');

  console.log(
    JSON.stringify(
      {
        ok: true,
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
