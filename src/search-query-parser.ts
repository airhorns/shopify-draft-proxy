export type SearchQueryComparator = '<=' | '>=' | '<' | '>' | '=';

export type SearchQueryTerm = {
  raw: string;
  negated: boolean;
  field: string | null;
  comparator: SearchQueryComparator | null;
  value: string;
};

export type SearchQueryNode =
  | { type: 'term'; term: SearchQueryTerm }
  | { type: 'and'; children: SearchQueryNode[] }
  | { type: 'or'; children: SearchQueryNode[] }
  | { type: 'not'; child: SearchQueryNode };

type SearchQueryToken =
  | { type: 'term'; value: string }
  | { type: 'or' }
  | { type: 'lparen' }
  | { type: 'rparen' }
  | { type: 'not' };

export type SearchQueryParseOptions = {
  quoteCharacters?: readonly ('"' | "'")[];
  recognizeNotKeyword?: boolean;
  preserveQuotesInTerms?: boolean;
};

export type SearchQueryTermListOptions = {
  quoteCharacters?: readonly ('"' | "'")[];
  preserveQuotesInTerms?: boolean;
  ignoredKeywords?: readonly string[];
};

const DEFAULT_QUOTE_CHARACTERS = ['"', "'"] as const;
const COMPARATORS: SearchQueryComparator[] = ['<=', '>=', '<', '>', '='];

function isQuoteCharacter(character: string, quoteCharacters: readonly ('"' | "'")[]): character is '"' | "'" {
  return quoteCharacters.includes(character as '"' | "'");
}

function canStartQuotedValue(current: string): boolean {
  if (!current) {
    return true;
  }

  return /:(?:<=|>=|<|>|=)?$/u.test(current);
}

function parseTerm(rawValue: string): SearchQueryTerm {
  const raw = rawValue.trim();
  const negated = raw.startsWith('-') && raw.length > 1;
  const normalizedRaw = negated ? raw.slice(1).trim() : raw;
  const separatorIndex = normalizedRaw.indexOf(':');

  if (separatorIndex === -1) {
    return {
      raw,
      negated,
      field: null,
      comparator: null,
      value: normalizedRaw,
    };
  }

  const rawValueWithComparator = normalizedRaw.slice(separatorIndex + 1).trimStart();
  const comparator = COMPARATORS.find((candidate) => rawValueWithComparator.startsWith(candidate)) ?? null;
  const value = comparator ? rawValueWithComparator.slice(comparator.length).trimStart() : rawValueWithComparator;

  return {
    raw,
    negated,
    field: normalizedRaw.slice(0, separatorIndex),
    comparator,
    value,
  };
}

export function parseSearchQueryTerm(rawValue: string): SearchQueryTerm {
  return parseTerm(rawValue);
}

export function parseSearchQueryTerms(query: string, options: SearchQueryTermListOptions = {}): SearchQueryTerm[] {
  const quoteCharacters = options.quoteCharacters ?? DEFAULT_QUOTE_CHARACTERS;
  const ignoredKeywords = new Set((options.ignoredKeywords ?? []).map((keyword) => keyword.toUpperCase()));
  const terms: SearchQueryTerm[] = [];
  let current = '';
  let quoteCharacter: '"' | "'" | null = null;

  const flushCurrent = (): void => {
    const value = current.trim();
    if (value && !ignoredKeywords.has(value.toUpperCase())) {
      terms.push(parseTerm(value));
    }
    current = '';
  };

  for (let index = 0; index < query.length; index += 1) {
    const character = query[index] ?? '';

    if (isQuoteCharacter(character, quoteCharacters)) {
      quoteCharacter = quoteCharacter === character ? null : character;
      if (options.preserveQuotesInTerms === true) {
        current += character;
      }
      continue;
    }

    if (quoteCharacter === null && /\s/u.test(character)) {
      flushCurrent();
      continue;
    }

    current += character;
  }

  flushCurrent();
  return terms;
}

function tokenizeSearchQuery(query: string, options: Required<SearchQueryParseOptions>): SearchQueryToken[] {
  const tokens: SearchQueryToken[] = [];
  let current = '';
  let quoteCharacter: '"' | "'" | null = null;

  const flushCurrent = (): void => {
    const value = current.trim();
    if (!value) {
      current = '';
      return;
    }

    if (value.toUpperCase() === 'OR') {
      tokens.push({ type: 'or' });
    } else if (options.recognizeNotKeyword && value === 'NOT') {
      tokens.push({ type: 'not' });
    } else {
      tokens.push({ type: 'term', value });
    }
    current = '';
  };

  for (let index = 0; index < query.length; index += 1) {
    const character = query[index] ?? '';

    if (
      isQuoteCharacter(character, options.quoteCharacters) &&
      (quoteCharacter === character || (quoteCharacter === null && canStartQuotedValue(current)))
    ) {
      quoteCharacter = quoteCharacter === character ? null : character;
      if (options.preserveQuotesInTerms) {
        current += character;
      }
      continue;
    }

    if (quoteCharacter === null && /\s/u.test(character)) {
      flushCurrent();
      continue;
    }

    if (quoteCharacter === null && character === '(') {
      flushCurrent();
      tokens.push({ type: 'lparen' });
      continue;
    }

    if (quoteCharacter === null && character === ')') {
      flushCurrent();
      tokens.push({ type: 'rparen' });
      continue;
    }

    if (quoteCharacter === null && character === '-' && !current) {
      const nextCharacter = query[index + 1] ?? '';
      if (nextCharacter === '(') {
        tokens.push({ type: 'not' });
        continue;
      }
    }

    current += character;
  }

  flushCurrent();
  return tokens;
}

export function parseSearchQuery(query: string, options: SearchQueryParseOptions = {}): SearchQueryNode | null {
  const tokens = tokenizeSearchQuery(query, {
    quoteCharacters: options.quoteCharacters ?? DEFAULT_QUOTE_CHARACTERS,
    recognizeNotKeyword: options.recognizeNotKeyword ?? false,
    preserveQuotesInTerms: options.preserveQuotesInTerms ?? false,
  });
  if (tokens.length === 0) {
    return null;
  }

  let index = 0;

  const parseOrExpression = (): SearchQueryNode | null => {
    const firstChild = parseAndExpression();
    if (!firstChild) {
      return null;
    }

    const children: SearchQueryNode[] = [firstChild];
    while (tokens[index]?.type === 'or') {
      index += 1;
      const nextChild = parseAndExpression();
      if (!nextChild) {
        break;
      }
      children.push(nextChild);
    }

    return children.length === 1 ? (children[0] ?? null) : { type: 'or', children };
  };

  const parseAndExpression = (): SearchQueryNode | null => {
    const children: SearchQueryNode[] = [];

    while (index < tokens.length) {
      const token = tokens[index];
      if (!token || token.type === 'or' || token.type === 'rparen') {
        break;
      }

      const child = parseUnaryExpression();
      if (!child) {
        break;
      }
      children.push(child);
    }

    if (children.length === 0) {
      return null;
    }

    return children.length === 1 ? (children[0] ?? null) : { type: 'and', children };
  };

  const parseUnaryExpression = (): SearchQueryNode | null => {
    const token = tokens[index];
    if (!token) {
      return null;
    }

    if (token.type === 'not') {
      index += 1;
      const child = parseUnaryExpression();
      return child ? { type: 'not', child } : null;
    }

    if (token.type === 'term') {
      index += 1;
      return { type: 'term', term: parseTerm(token.value) };
    }

    if (token.type === 'lparen') {
      index += 1;
      const child = parseOrExpression();
      if (tokens[index]?.type === 'rparen') {
        index += 1;
      }
      return child;
    }

    return null;
  };

  return parseOrExpression();
}
