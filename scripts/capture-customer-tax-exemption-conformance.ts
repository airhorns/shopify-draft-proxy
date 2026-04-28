// @ts-nocheck
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

function assertNoTopLevelErrors(result, context) {
  if (result.status < 200 || result.status >= 300 || result.payload?.errors) {
    throw new Error(`${context} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function assertGraphqlErrors(result, context) {
  if (result.status < 200 || result.status >= 300 || !Array.isArray(result.payload?.errors)) {
    throw new Error(`${context} did not produce GraphQL errors: ${JSON.stringify(result, null, 2)}`);
  }
}

const customerSlice = `
  id
  firstName
  lastName
  displayName
  email
  taxExempt
  taxExemptions
  tags
  defaultEmailAddress { emailAddress }
  createdAt
  updatedAt
`;

const createMutation = `#graphql
  mutation CustomerTaxExemptionCreate($input: CustomerInput!) {
    customerCreate(input: $input) {
      customer {
        ${customerSlice}
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const addTaxExemptionsMutation = `#graphql
  mutation CustomerAddTaxExemptionsParity($customerId: ID!, $taxExemptions: [TaxExemption!]!) {
    customerAddTaxExemptions(customerId: $customerId, taxExemptions: $taxExemptions) {
      customer {
        ${customerSlice}
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const removeTaxExemptionsMutation = `#graphql
  mutation CustomerRemoveTaxExemptionsParity($customerId: ID!, $taxExemptions: [TaxExemption!]!) {
    customerRemoveTaxExemptions(customerId: $customerId, taxExemptions: $taxExemptions) {
      customer {
        ${customerSlice}
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const replaceTaxExemptionsMutation = `#graphql
  mutation CustomerReplaceTaxExemptionsParity($customerId: ID!, $taxExemptions: [TaxExemption!]!) {
    customerReplaceTaxExemptions(customerId: $customerId, taxExemptions: $taxExemptions) {
      customer {
        ${customerSlice}
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const downstreamReadQuery = `#graphql
  query CustomerTaxExemptionsDownstream(
    $id: ID!
    $identifier: CustomerIdentifierInput!
    $query: String!
    $first: Int!
  ) {
    customer(id: $id) {
      ${customerSlice}
    }
    customerByIdentifier(identifier: $identifier) {
      ${customerSlice}
    }
    customers(first: $first, query: $query, sortKey: UPDATED_AT, reverse: true) {
      nodes {
        ${customerSlice}
      }
      pageInfo {
        hasNextPage
        hasPreviousPage
      }
    }
    customersCount {
      count
      precision
    }
  }
`;

const deleteMutation = `#graphql
  mutation CustomerTaxExemptionDelete($input: CustomerDeleteInput!) {
    customerDelete(input: $input) {
      deletedCustomerId
      userErrors {
        field
        message
      }
    }
  }
`;

async function runTaxMutation(mutation, variables, context) {
  const result = await runGraphql(mutation, variables);
  assertNoTopLevelErrors(result, context);
  return result;
}

async function runInvalidEnum(mutation, customerId, context) {
  const result = await runGraphql(mutation, {
    customerId,
    taxExemptions: ['NOT_A_TAX_EXEMPTION'],
  });
  assertGraphqlErrors(result, context);
  return result;
}

async function main() {
  await mkdir(outputDir, { recursive: true });

  const stamp = Date.now();
  const emailAddress = `hermes-tax-exemptions-${stamp}@example.com`;
  const createVariables = {
    input: {
      email: emailAddress,
      firstName: 'Hermes',
      lastName: 'Tax',
      tags: ['parity', `tax-exemptions-${stamp}`],
      taxExempt: false,
    },
  };

  const createResult = await runTaxMutation(createMutation, createVariables, 'customerCreate for tax exemption parity');
  const customerId = createResult.payload?.data?.customerCreate?.customer?.id;
  if (typeof customerId !== 'string' || !customerId) {
    throw new Error(`customerCreate did not return a customer id: ${JSON.stringify(createResult.payload, null, 2)}`);
  }

  const downstreamVariables = {
    id: customerId,
    identifier: { id: customerId },
    query: `email:${emailAddress}`,
    first: 5,
  };

  const unknownVariables = {
    customerId: 'gid://shopify/Customer/999999999999999',
    taxExemptions: ['CA_BC_RESELLER_EXEMPTION'],
  };

  const addVariables = {
    customerId,
    taxExemptions: ['CA_BC_RESELLER_EXEMPTION', 'US_CA_RESELLER_EXEMPTION'],
  };
  const addResult = await runTaxMutation(addTaxExemptionsMutation, addVariables, 'customerAddTaxExemptions');
  const addDownstreamRead = await runTaxMutation(downstreamReadQuery, downstreamVariables, 'add downstream read');
  const addUnknownCustomer = await runTaxMutation(
    addTaxExemptionsMutation,
    unknownVariables,
    'customerAddTaxExemptions unknown customer',
  );
  const addEmptyList = await runTaxMutation(
    addTaxExemptionsMutation,
    { customerId, taxExemptions: [] },
    'customerAddTaxExemptions empty list',
  );
  const addDuplicateInput = await runTaxMutation(
    addTaxExemptionsMutation,
    { customerId, taxExemptions: ['CA_BC_RESELLER_EXEMPTION', 'CA_BC_RESELLER_EXEMPTION'] },
    'customerAddTaxExemptions duplicate input',
  );
  const addInvalidEnum = await runInvalidEnum(
    addTaxExemptionsMutation,
    customerId,
    'customerAddTaxExemptions invalid enum',
  );

  const removeVariables = {
    customerId,
    taxExemptions: ['US_CA_RESELLER_EXEMPTION'],
  };
  const removeResult = await runTaxMutation(
    removeTaxExemptionsMutation,
    removeVariables,
    'customerRemoveTaxExemptions',
  );
  const removeDownstreamRead = await runTaxMutation(downstreamReadQuery, downstreamVariables, 'remove downstream read');
  const removeUnknownCustomer = await runTaxMutation(
    removeTaxExemptionsMutation,
    unknownVariables,
    'customerRemoveTaxExemptions unknown customer',
  );
  const removeEmptyList = await runTaxMutation(
    removeTaxExemptionsMutation,
    { customerId, taxExemptions: [] },
    'customerRemoveTaxExemptions empty list',
  );
  const removeNoop = await runTaxMutation(
    removeTaxExemptionsMutation,
    { customerId, taxExemptions: ['US_CA_RESELLER_EXEMPTION'] },
    'customerRemoveTaxExemptions no-op remove',
  );
  const removeInvalidEnum = await runInvalidEnum(
    removeTaxExemptionsMutation,
    customerId,
    'customerRemoveTaxExemptions invalid enum',
  );

  const replaceVariables = {
    customerId,
    taxExemptions: ['EU_REVERSE_CHARGE_EXEMPTION_RULE'],
  };
  const replaceResult = await runTaxMutation(
    replaceTaxExemptionsMutation,
    replaceVariables,
    'customerReplaceTaxExemptions',
  );
  const replaceDownstreamRead = await runTaxMutation(
    downstreamReadQuery,
    downstreamVariables,
    'replace downstream read',
  );
  const replaceUnknownCustomer = await runTaxMutation(
    replaceTaxExemptionsMutation,
    unknownVariables,
    'customerReplaceTaxExemptions unknown customer',
  );
  const replaceDuplicateInput = await runTaxMutation(
    replaceTaxExemptionsMutation,
    { customerId, taxExemptions: ['CA_BC_RESELLER_EXEMPTION', 'CA_BC_RESELLER_EXEMPTION'] },
    'customerReplaceTaxExemptions duplicate input',
  );
  const replaceEmptyList = await runTaxMutation(
    replaceTaxExemptionsMutation,
    { customerId, taxExemptions: [] },
    'customerReplaceTaxExemptions empty list',
  );
  const replaceInvalidEnum = await runInvalidEnum(
    replaceTaxExemptionsMutation,
    customerId,
    'customerReplaceTaxExemptions invalid enum',
  );

  const deleteResult = await runTaxMutation(deleteMutation, { input: { id: customerId } }, 'customerDelete cleanup');

  const addCapture = {
    precondition: {
      variables: createVariables,
      response: createResult.payload,
    },
    mutation: {
      variables: addVariables,
      response: addResult.payload,
    },
    downstreamRead: {
      variables: downstreamVariables,
      response: addDownstreamRead.payload,
    },
    validation: {
      unknownCustomer: {
        variables: unknownVariables,
        response: addUnknownCustomer.payload,
      },
      emptyList: {
        variables: { customerId, taxExemptions: [] },
        response: addEmptyList.payload,
      },
      duplicateInput: {
        variables: { customerId, taxExemptions: ['CA_BC_RESELLER_EXEMPTION', 'CA_BC_RESELLER_EXEMPTION'] },
        response: addDuplicateInput.payload,
      },
      invalidEnumVariable: {
        variables: { customerId, taxExemptions: ['NOT_A_TAX_EXEMPTION'] },
        response: addInvalidEnum.payload,
      },
    },
  };

  const removeCapture = {
    precondition: {
      response: addResult.payload,
    },
    mutation: {
      variables: removeVariables,
      response: removeResult.payload,
    },
    downstreamRead: {
      variables: downstreamVariables,
      response: removeDownstreamRead.payload,
    },
    validation: {
      unknownCustomer: {
        variables: unknownVariables,
        response: removeUnknownCustomer.payload,
      },
      emptyList: {
        variables: { customerId, taxExemptions: [] },
        response: removeEmptyList.payload,
      },
      noopRemove: {
        variables: { customerId, taxExemptions: ['US_CA_RESELLER_EXEMPTION'] },
        response: removeNoop.payload,
      },
      invalidEnumVariable: {
        variables: { customerId, taxExemptions: ['NOT_A_TAX_EXEMPTION'] },
        response: removeInvalidEnum.payload,
      },
    },
  };

  const replaceCapture = {
    precondition: {
      response: removeResult.payload,
    },
    mutation: {
      variables: replaceVariables,
      response: replaceResult.payload,
    },
    downstreamRead: {
      variables: downstreamVariables,
      response: replaceDownstreamRead.payload,
    },
    validation: {
      unknownCustomer: {
        variables: unknownVariables,
        response: replaceUnknownCustomer.payload,
      },
      duplicateInput: {
        variables: { customerId, taxExemptions: ['CA_BC_RESELLER_EXEMPTION', 'CA_BC_RESELLER_EXEMPTION'] },
        response: replaceDuplicateInput.payload,
      },
      emptyList: {
        variables: { customerId, taxExemptions: [] },
        response: replaceEmptyList.payload,
      },
      invalidEnumVariable: {
        variables: { customerId, taxExemptions: ['NOT_A_TAX_EXEMPTION'] },
        response: replaceInvalidEnum.payload,
      },
    },
    cleanup: {
      response: deleteResult.payload,
    },
  };

  await Promise.all([
    writeFile(
      path.join(outputDir, 'customer-add-tax-exemptions-parity.json'),
      `${JSON.stringify(addCapture, null, 2)}\n`,
      'utf8',
    ),
    writeFile(
      path.join(outputDir, 'customer-remove-tax-exemptions-parity.json'),
      `${JSON.stringify(removeCapture, null, 2)}\n`,
      'utf8',
    ),
    writeFile(
      path.join(outputDir, 'customer-replace-tax-exemptions-parity.json'),
      `${JSON.stringify(replaceCapture, null, 2)}\n`,
      'utf8',
    ),
  ]);

  console.log(
    JSON.stringify(
      {
        ok: true,
        outputDir,
        files: [
          'customer-add-tax-exemptions-parity.json',
          'customer-remove-tax-exemptions-parity.json',
          'customer-replace-tax-exemptions-parity.json',
        ],
        customerId,
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
