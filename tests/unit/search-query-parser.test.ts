import { describe, expect, it } from 'vitest';

import {
  applySearchQuery,
  applySearchQueryTerms,
  matchesSearchQueryDate,
  matchesSearchQueryNumber,
  matchesSearchQueryString,
  matchesSearchQueryText,
  normalizeSearchQueryValue,
  parseSearchQuery,
  parseSearchQueryTerm,
  parseSearchQueryTerms,
  searchQueryTermValue,
  stripSearchQueryValueQuotes,
  type SearchQueryTerm,
} from '../../src/search-query-parser.js';

describe('search query parser', () => {
  it('parses bare terms and field filters into reusable term metadata', () => {
    expect(parseSearchQuery('swoosh status:active inventory_total:<=5')).toEqual({
      type: 'and',
      children: [
        {
          type: 'term',
          term: { raw: 'swoosh', negated: false, field: null, comparator: null, value: 'swoosh' },
        },
        {
          type: 'term',
          term: { raw: 'status:active', negated: false, field: 'status', comparator: null, value: 'active' },
        },
        {
          type: 'term',
          term: {
            raw: 'inventory_total:<=5',
            negated: false,
            field: 'inventory_total',
            comparator: '<=',
            value: '5',
          },
        },
      ],
    });
  });

  it('parses OR groups, implicit AND, grouped negation, and NOT keyword when enabled', () => {
    expect(parseSearchQuery('(vendor:NIKE OR vendor:ADIDAS) NOT tag:sale', { recognizeNotKeyword: true })).toEqual({
      type: 'and',
      children: [
        {
          type: 'or',
          children: [
            {
              type: 'term',
              term: { raw: 'vendor:NIKE', negated: false, field: 'vendor', comparator: null, value: 'NIKE' },
            },
            {
              type: 'term',
              term: { raw: 'vendor:ADIDAS', negated: false, field: 'vendor', comparator: null, value: 'ADIDAS' },
            },
          ],
        },
        {
          type: 'not',
          child: {
            type: 'term',
            term: { raw: 'tag:sale', negated: false, field: 'tag', comparator: null, value: 'sale' },
          },
        },
      ],
    });

    expect(parseSearchQuery('-(tag:sale OR tag:clearance)')).toEqual({
      type: 'not',
      child: {
        type: 'or',
        children: [
          {
            type: 'term',
            term: { raw: 'tag:sale', negated: false, field: 'tag', comparator: null, value: 'sale' },
          },
          {
            type: 'term',
            term: { raw: 'tag:clearance', negated: false, field: 'tag', comparator: null, value: 'clearance' },
          },
        ],
      },
    });
  });

  it('keeps quoted phrases together and supports endpoint-specific quote characters', () => {
    expect(parseSearchQuery('title:"Nike Cap" vendor:\'ACME Supply\'')).toEqual({
      type: 'and',
      children: [
        {
          type: 'term',
          term: { raw: 'title:Nike Cap', negated: false, field: 'title', comparator: null, value: 'Nike Cap' },
        },
        {
          type: 'term',
          term: {
            raw: 'vendor:ACME Supply',
            negated: false,
            field: 'vendor',
            comparator: null,
            value: 'ACME Supply',
          },
        },
      ],
    });

    expect(parseSearchQuery("tag:'summer sale'", { quoteCharacters: ['"'] })).toEqual({
      type: 'and',
      children: [
        {
          type: 'term',
          term: { raw: "tag:'summer", negated: false, field: 'tag', comparator: null, value: "'summer" },
        },
        {
          type: 'term',
          term: { raw: "sale'", negated: false, field: null, comparator: null, value: "sale'" },
        },
      ],
    });
  });

  it('preserves quotes when requested by endpoints that normalize values later', () => {
    expect(parseSearchQuery('email:"a@example.com" status:open', { preserveQuotesInTerms: true })).toEqual({
      type: 'and',
      children: [
        {
          type: 'term',
          term: {
            raw: 'email:"a@example.com"',
            negated: false,
            field: 'email',
            comparator: null,
            value: '"a@example.com"',
          },
        },
        {
          type: 'term',
          term: { raw: 'status:open', negated: false, field: 'status', comparator: null, value: 'open' },
        },
      ],
    });
  });

  it('returns null for empty or group-only invalid input and keeps dangling OR compatibility', () => {
    expect(parseSearchQuery('   ')).toBeNull();
    expect(parseSearchQuery('()')).toBeNull();
    expect(parseSearchQuery('status:active OR')).toEqual({
      type: 'term',
      term: { raw: 'status:active', negated: false, field: 'status', comparator: null, value: 'active' },
    });
  });

  it('parses a single term without building a full expression tree', () => {
    expect(parseSearchQueryTerm('-created_at:>=2026-01-01T00:00:00Z')).toEqual({
      raw: '-created_at:>=2026-01-01T00:00:00Z',
      negated: true,
      field: 'created_at',
      comparator: '>=',
      value: '2026-01-01T00:00:00Z',
    });
  });

  it('parses simple term lists for endpoints without boolean search support', () => {
    expect(
      parseSearchQueryTerms('name:"Order 1001" AND tag:vip', {
        quoteCharacters: ['"'],
        preserveQuotesInTerms: true,
        ignoredKeywords: ['AND'],
      }),
    ).toEqual([
      {
        raw: 'name:"Order 1001"',
        negated: false,
        field: 'name',
        comparator: null,
        value: '"Order 1001"',
      },
      { raw: 'tag:vip', negated: false, field: 'tag', comparator: null, value: 'vip' },
    ]);
  });

  it('normalizes and compares typed term values for endpoint filters', () => {
    expect(normalizeSearchQueryValue(' "ACTIVE" ')).toBe('active');

    expect(matchesSearchQueryText('Summer Campaign', parseSearchQueryTerm('title:summer'))).toBe(true);
    expect(matchesSearchQueryText(null, parseSearchQueryTerm('title:summer'))).toBe(false);

    expect(matchesSearchQueryNumber(5, parseSearchQueryTerm('times_used:>=5'))).toBe(true);
    expect(matchesSearchQueryNumber(4, parseSearchQueryTerm('times_used:>=5'))).toBe(false);
    expect(matchesSearchQueryNumber(null, parseSearchQueryTerm('times_used:>=5'))).toBe(false);

    expect(
      matchesSearchQueryDate('2026-01-02T00:00:00Z', parseSearchQueryTerm('starts_at:>2026-01-01T00:00:00Z')),
    ).toBe(true);
    expect(
      matchesSearchQueryDate(
        '2026-01-02T00:00:00Z',
        parseSearchQueryTerm('starts_at:<=now'),
        Date.parse('2026-01-03T00:00:00Z'),
      ),
    ).toBe(true);
    expect(matchesSearchQueryDate('invalid', parseSearchQueryTerm('starts_at:>2026-01-01T00:00:00Z'))).toBe(false);
  });

  it('applies parsed query trees through a resource-specific positive term matcher', () => {
    type Row = {
      id: string;
      title: string;
      tags: string[];
      status: string;
    };

    const rows: Row[] = [
      { id: '1', title: 'Nike Cap', tags: ['vip'], status: 'active' },
      { id: '2', title: 'Clearance Hat', tags: ['clearance'], status: 'active' },
      { id: '3', title: 'Wholesale Cap', tags: ['wholesale'], status: 'disabled' },
    ];

    const matchesTerm = (row: Row, term: SearchQueryTerm): boolean => {
      const value = searchQueryTermValue(term);
      switch (term.field) {
        case null:
          return matchesSearchQueryString(row.title, value, 'includes', { wordPrefix: true });
        case 'tag':
          return row.tags.some((tag) => matchesSearchQueryString(tag, value));
        case 'status':
          return matchesSearchQueryString(row.status, value);
        default:
          return true;
      }
    };

    expect(
      applySearchQuery(
        rows,
        '(tag:vip OR tag:wholesale) -status:disabled',
        { recognizeNotKeyword: true },
        matchesTerm,
      ).map((row) => row.id),
    ).toEqual(['1']);
  });

  it('applies simple term-list queries with common raw-query guards and negation', () => {
    const rows = [
      { id: '1', status: 'open', tag: 'vip' },
      { id: '2', status: 'closed', tag: 'vip' },
      { id: '3', status: 'open', tag: 'staff' },
    ];

    const matchesTerm = (row: (typeof rows)[number], term: SearchQueryTerm): boolean => {
      const value = searchQueryTermValue(term);
      if (term.field === 'status') {
        return matchesSearchQueryString(row.status, value);
      }
      if (term.field === 'tag') {
        return matchesSearchQueryString(row.tag, value);
      }
      return false;
    };

    expect(
      applySearchQueryTerms(rows, 'tag:vip -status:closed AND', { ignoredKeywords: ['AND'] }, matchesTerm).map(
        (row) => row.id,
      ),
    ).toEqual(['1']);
    expect(applySearchQueryTerms(rows, null, { ignoredKeywords: ['AND'] }, matchesTerm)).toBe(rows);
  });

  it('shares string matching and quote stripping for endpoint filters', () => {
    expect(stripSearchQueryValueQuotes(" 'Canada' ")).toBe('Canada');
    expect(matchesSearchQueryString('Conformance US', 'conformance', 'includes')).toBe(true);
    expect(matchesSearchQueryString('North America Market', 'ame*', 'includes', { wordPrefix: true })).toBe(true);
    expect(matchesSearchQueryString('North America Market', 'ame*', 'includes')).toBe(false);
  });
});
