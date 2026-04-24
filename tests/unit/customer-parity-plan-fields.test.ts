import { existsSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { describe, expect, it } from 'vitest';

describe('customer parity request scaffolds', () => {
  it('keep the richer customer detail and catalog field slices aligned with the supported overlay serializer', () => {
    const repoRoot = resolve(import.meta.dirname, '../..');
    const detailDocumentPath = resolve(repoRoot, 'config/parity-requests/customer-detail-parity-plan.graphql');
    const catalogDocumentPath = resolve(repoRoot, 'config/parity-requests/customers-catalog-parity-plan.graphql');
    const searchDocumentPath = resolve(repoRoot, 'config/parity-requests/customers-search-read.graphql');
    const advancedSearchDocumentPath = resolve(
      repoRoot,
      'config/parity-requests/customers-advanced-search-read.graphql',
    );
    const advancedSearchVariablesPath = resolve(
      repoRoot,
      'config/parity-requests/customers-advanced-search-read.variables.json',
    );
    const sortKeysDocumentPath = resolve(repoRoot, 'config/parity-requests/customers-sort-keys-read.graphql');
    const sortKeysVariablesPath = resolve(repoRoot, 'config/parity-requests/customers-sort-keys-read.variables.json');
    const relevanceDocumentPath = resolve(repoRoot, 'config/parity-requests/customers-relevance-search-read.graphql');
    const relevanceVariablesPath = resolve(
      repoRoot,
      'config/parity-requests/customers-relevance-search-read.variables.json',
    );
    const countDocumentPath = resolve(repoRoot, 'config/parity-requests/customers-count-read.graphql');
    const countVariablesPath = resolve(repoRoot, 'config/parity-requests/customers-count-read.variables.json');
    const countFixturePath = resolve(
      repoRoot,
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/customers-count.json',
    );

    expect(existsSync(detailDocumentPath)).toBe(true);
    expect(existsSync(catalogDocumentPath)).toBe(true);
    expect(existsSync(searchDocumentPath)).toBe(true);
    expect(existsSync(advancedSearchDocumentPath)).toBe(true);
    expect(existsSync(advancedSearchVariablesPath)).toBe(true);
    expect(existsSync(sortKeysDocumentPath)).toBe(true);
    expect(existsSync(sortKeysVariablesPath)).toBe(true);
    expect(existsSync(relevanceDocumentPath)).toBe(true);
    expect(existsSync(relevanceVariablesPath)).toBe(true);
    expect(existsSync(countDocumentPath)).toBe(true);
    expect(existsSync(countVariablesPath)).toBe(true);

    const detailDocument = readFileSync(detailDocumentPath, 'utf8');
    const catalogDocument = readFileSync(catalogDocumentPath, 'utf8');
    const searchDocument = readFileSync(searchDocumentPath, 'utf8');
    const advancedSearchDocument = readFileSync(advancedSearchDocumentPath, 'utf8');
    const advancedSearchVariables = JSON.parse(readFileSync(advancedSearchVariablesPath, 'utf8')) as Record<
      string,
      unknown
    >;
    const sortKeysDocument = readFileSync(sortKeysDocumentPath, 'utf8');
    const sortKeysVariables = JSON.parse(readFileSync(sortKeysVariablesPath, 'utf8')) as Record<string, unknown>;
    const relevanceDocument = readFileSync(relevanceDocumentPath, 'utf8');
    const relevanceVariables = JSON.parse(readFileSync(relevanceVariablesPath, 'utf8')) as Record<string, unknown>;
    const countDocument = readFileSync(countDocumentPath, 'utf8');
    const countVariables = JSON.parse(readFileSync(countVariablesPath, 'utf8')) as Record<string, unknown>;

    for (const document of [detailDocument, catalogDocument]) {
      expect(document).toContain('legacyResourceId');
      expect(document).toContain('locale');
      expect(document).toContain('note');
      expect(document).toContain('canDelete');
      expect(document).toContain('verifiedEmail');
      expect(document).toContain('taxExempt');
      expect(document).toContain('defaultPhoneNumber {');
      expect(document).toContain('phoneNumber');
      expect(document).toContain('defaultAddress {');
      expect(document).toContain('address1');
      expect(document).toContain('city');
      expect(document).toContain('province');
      expect(document).toContain('country');
      expect(document).toContain('zip');
      expect(document).toContain('formattedArea');
    }

    expect(searchDocument).toContain('legacyResourceId');
    expect(searchDocument).toContain('verifiedEmail');
    expect(searchDocument).toContain('defaultPhoneNumber {');
    expect(searchDocument).toContain('defaultAddress {');

    expect(advancedSearchDocument).toContain(
      'query CustomersAdvancedSearchRead($prefixQuery: String!, $orQuery: String!, $groupedQuery: String!)',
    );
    expect(advancedSearchDocument).toContain(
      'prefix: customers(first: 2, query: $prefixQuery, sortKey: UPDATED_AT, reverse: true)',
    );
    expect(advancedSearchDocument).toContain(
      'orMatches: customers(first: 5, query: $orQuery, sortKey: UPDATED_AT, reverse: true)',
    );
    expect(advancedSearchDocument).toContain(
      'groupedExclusion: customers(first: 5, query: $groupedQuery, sortKey: UPDATED_AT, reverse: true)',
    );
    expect(advancedSearchDocument).toContain('legacyResourceId');
    expect(advancedSearchDocument).toContain('updatedAt');
    expect(advancedSearchVariables).toMatchObject({
      prefixQuery: 'How*',
      orQuery: '(tag:VIP OR tag:referral) state:DISABLED',
      groupedQuery: 'state:DISABLED -(tag:VIP OR tag:referral)',
    });

    expect(sortKeysDocument).toContain('query CustomersSortKeysRead($first: Int!)');
    expect(sortKeysDocument).toContain('nameOrder: customers(first: $first, sortKey: NAME)');
    expect(sortKeysDocument).toContain('idOrder: customers(first: $first, sortKey: ID)');
    expect(sortKeysDocument).toContain('locationOrder: customers(first: $first, sortKey: LOCATION)');
    expect(sortKeysDocument).toContain('defaultAddress {');
    expect(sortKeysDocument).toContain('country');
    expect(sortKeysDocument).toContain('province');
    expect(sortKeysDocument).toContain('city');
    expect(sortKeysVariables).toMatchObject({ first: 5 });

    expect(relevanceDocument).toContain('query CustomersRelevanceSearchRead($first: Int!, $query: String!)');
    expect(relevanceDocument).toContain('customers(first: $first, query: $query, sortKey: RELEVANCE)');
    expect(relevanceDocument).toContain('cursor');
    expect(relevanceDocument).toContain('legacyResourceId');
    expect(relevanceDocument).toContain('tags');
    expect(relevanceDocument).toContain('pageInfo {');
    expect(relevanceVariables).toMatchObject({ first: 5, query: 'egnition' });

    expect(countDocument).toContain('query CustomersCountRead($query: String!, $disabledQuery: String!)');
    expect(countDocument).toContain('total: customersCount');
    expect(countDocument).toContain('matching: customersCount(query: $query)');
    expect(countDocument).toContain('disabled: customersCount(query: $disabledQuery)');
    expect(countDocument).toContain('count');
    expect(countDocument).toContain('precision');

    const countFixture = JSON.parse(readFileSync(countFixturePath, 'utf8')) as {
      extensions?: {
        search?: Array<{
          path?: string[];
          query?: string;
          parsed?: { field?: string; match_all?: string };
          warnings?: Array<{ field?: string; message?: string; code?: string }>;
        }>;
      };
    };

    expect(countFixture.extensions?.search).toEqual([
      {
        path: ['matching'],
        query: 'email:grace@example.com',
        parsed: { field: 'email', match_all: 'grace@example.com' },
        warnings: [{ field: 'email', message: 'Invalid search field for this query.', code: 'invalid_field' }],
      },
      {
        path: ['disabled'],
        query: 'state:DISABLED',
        parsed: { field: 'state', match_all: 'DISABLED' },
        warnings: [{ field: 'state', message: 'Invalid search field for this query.', code: 'invalid_field' }],
      },
    ]);
    expect(countVariables).toMatchObject({
      query: 'email:grace@example.com',
      disabledQuery: 'state:DISABLED',
    });
  });

});
