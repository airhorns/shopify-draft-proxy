import { existsSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { describe, expect, it } from 'vitest';

import {
  classifyParityScenarioState,
  validateComparisonContract,
  type ParitySpec,
} from '../../scripts/conformance-parity-lib.js';

const repoRoot = resolve(import.meta.dirname, '../..');

function readParitySpec(relativePath: string): ParitySpec {
  return JSON.parse(readFileSync(resolve(repoRoot, relativePath), 'utf8')) as ParitySpec;
}

describe('customer mutation parity request scaffolds', () => {
  it('keep the customer CRUD request slices aligned with the supported overlay serializer', () => {
    const createDocumentPath = resolve(repoRoot, 'config/parity-requests/customerCreate-parity-plan.graphql');
    const createVariablesPath = resolve(repoRoot, 'config/parity-requests/customerCreate-parity-plan.variables.json');
    const updateDocumentPath = resolve(repoRoot, 'config/parity-requests/customerUpdate-parity-plan.graphql');
    const updateVariablesPath = resolve(repoRoot, 'config/parity-requests/customerUpdate-parity-plan.variables.json');
    const deleteDocumentPath = resolve(repoRoot, 'config/parity-requests/customerDelete-parity-plan.graphql');
    const deleteVariablesPath = resolve(repoRoot, 'config/parity-requests/customerDelete-parity-plan.variables.json');
    const downstreamDocumentPath = resolve(
      repoRoot,
      'config/parity-requests/customer-mutation-downstream-read.graphql',
    );

    expect(existsSync(createDocumentPath)).toBe(true);
    expect(existsSync(createVariablesPath)).toBe(true);
    expect(existsSync(updateDocumentPath)).toBe(true);
    expect(existsSync(updateVariablesPath)).toBe(true);
    expect(existsSync(deleteDocumentPath)).toBe(true);
    expect(existsSync(deleteVariablesPath)).toBe(true);
    expect(existsSync(downstreamDocumentPath)).toBe(true);

    const createDocument = readFileSync(createDocumentPath, 'utf8');
    const createVariables = JSON.parse(readFileSync(createVariablesPath, 'utf8')) as Record<string, unknown>;
    const updateDocument = readFileSync(updateDocumentPath, 'utf8');
    const updateVariables = JSON.parse(readFileSync(updateVariablesPath, 'utf8')) as Record<string, unknown>;
    const deleteDocument = readFileSync(deleteDocumentPath, 'utf8');
    const deleteVariables = JSON.parse(readFileSync(deleteVariablesPath, 'utf8')) as Record<string, unknown>;
    const downstreamDocument = readFileSync(downstreamDocumentPath, 'utf8');

    for (const document of [createDocument, updateDocument]) {
      expect(document).toContain('firstName');
      expect(document).toContain('lastName');
      expect(document).toContain('displayName');
      expect(document).toContain('email');
      expect(document).toContain('locale');
      expect(document).toContain('note');
      expect(document).toContain('verifiedEmail');
      expect(document).toContain('taxExempt');
      expect(document).toContain('tags');
      expect(document).toContain('state');
      expect(document).toContain('canDelete');
      expect(document).toContain('defaultEmailAddress {');
      expect(document).toContain('defaultPhoneNumber {');
      expect(document).toContain('defaultAddress {');
      expect(document).toContain('createdAt');
      expect(document).toContain('updatedAt');
      expect(document).toContain('userErrors {');
      expect(document).toContain('field');
      expect(document).toContain('message');
    }

    expect(createVariables).toMatchObject({
      input: {
        email: 'hermes-customer-create@example.com',
        firstName: 'Hermes',
        lastName: 'Create',
        locale: 'en',
        note: 'customer create parity probe',
        phone: '+14155550123',
        tags: ['parity', 'create'],
        taxExempt: true,
      },
    });

    expect(updateVariables).toMatchObject({
      input: {
        id: 'gid://shopify/Customer/1',
        firstName: 'Hermes',
        lastName: 'Updated',
        note: 'customer update parity probe',
        tags: ['parity', 'updated'],
        taxExempt: false,
      },
    });

    expect(deleteDocument).toContain('deletedCustomerId');
    expect(deleteDocument).toContain('shop {');
    expect(deleteDocument).toContain('id');
    expect(deleteDocument).toContain('userErrors {');
    expect(deleteDocument).toContain('field');
    expect(deleteDocument).toContain('message');
    expect(deleteVariables).toMatchObject({
      input: { id: 'gid://shopify/Customer/1' },
    });

    expect(downstreamDocument).toContain('query CustomerMutationDownstream($id: ID!, $query: String!, $first: Int!)');
    expect(downstreamDocument).toContain('customer(id: $id)');
    expect(downstreamDocument).toContain('customers(first: $first, query: $query, sortKey: UPDATED_AT, reverse: true)');
    expect(downstreamDocument).toContain('customersCount');
    expect(downstreamDocument).toContain('defaultEmailAddress {');
    expect(downstreamDocument).toContain('defaultPhoneNumber {');
    expect(downstreamDocument).toContain('defaultAddress {');
  });

  it('promote captured customer CRUD specs to ready strict comparison contracts', () => {
    for (const specPath of [
      'config/parity-specs/customerCreate-parity-plan.json',
      'config/parity-specs/customerUpdate-parity-plan.json',
      'config/parity-specs/customerDelete-parity-plan.json',
    ]) {
      const spec = readParitySpec(specPath);
      expect(validateComparisonContract(spec.comparison)).toEqual([]);
      expect(classifyParityScenarioState({ status: 'captured' }, spec)).toBe('ready-for-comparison');
      expect(spec.proxyRequest?.variablesCapturePath).toBe('$.mutation.variables');
      expect(spec.comparison?.targets?.map((target) => target.name)).toEqual([
        'mutation-data',
        'downstream-read-data',
        'validation-data',
      ]);
      expect(spec.comparison?.targets?.[1]?.proxyRequest?.documentPath).toBe(
        'config/parity-requests/customer-mutation-downstream-read.graphql',
      );
      expect(spec.comparison?.expectedDifferences?.every((difference) => difference.ignore !== true)).toBe(true);
    }
  });
});
