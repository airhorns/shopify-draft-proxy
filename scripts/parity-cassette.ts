import { parse } from 'graphql';

export type RecordedUpstreamCall = {
  operationName?: string;
  variables?: unknown;
  query?: string;
  response?: { status?: number; body?: unknown };
};

function isPlainObject(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

export function stableJson(value: unknown): string {
  if (Array.isArray(value)) return `[${value.map((entry) => stableJson(entry)).join(',')}]`;
  if (isPlainObject(value))
    return `{${Object.keys(value)
      .sort()
      .map((key) => `${JSON.stringify(key)}:${stableJson(value[key])}`)
      .join(',')}}`;
  return JSON.stringify(value);
}

export function isGraphqlDocumentText(value: unknown): value is string {
  if (typeof value !== 'string' || value.length === 0) return false;
  try {
    const document = parse(value);
    return document.definitions.length > 0;
  } catch {
    return false;
  }
}

export function recordedCallMatchesBody(call: RecordedUpstreamCall, body: string): boolean {
  try {
    const parsed = JSON.parse(body) as Record<string, unknown>;
    // Normalize trailing whitespace so .graphql files with a trailing newline
    // match cassette entries captured without one (and vice versa).
    const outgoingQuery = typeof parsed['query'] === 'string' ? parsed['query'].trimEnd() : null;
    const recordedQuery = typeof call.query === 'string' ? call.query.trimEnd() : null;
    return (
      outgoingQuery !== null &&
      recordedQuery !== null &&
      outgoingQuery === recordedQuery &&
      stableJson(parsed['variables'] ?? {}) === stableJson(call.variables ?? {})
    );
  } catch {
    return false;
  }
}

export function validateRecordedUpstreamCalls(calls: RecordedUpstreamCall[]): string[] {
  const errors: string[] = [];
  for (let index = 0; index < calls.length; index += 1) {
    const call = calls[index];
    if (!call || typeof call.query !== 'string') {
      errors.push(`upstreamCalls[${index}].query is missing or is not a string`);
      continue;
    }
    if (!isGraphqlDocumentText(call.query)) {
      errors.push(
        `upstreamCalls[${index}].query is not a valid GraphQL document: ${JSON.stringify(call.query).slice(0, 500)}`,
      );
    }
  }
  return errors;
}

function parsedRequestBody(body: string): Record<string, unknown> {
  try {
    return JSON.parse(body) as Record<string, unknown>;
  } catch {
    return {};
  }
}

function truncate(value: string, max = 4000): string {
  return value.length <= max ? value : `${value.slice(0, max)}…<truncated ${value.length - max} chars>`;
}

export function formatRecordedCallMismatch(
  body: string,
  calls: RecordedUpstreamCall[],
  consumedCalls: ReadonlySet<number>,
): string {
  const parsed = parsedRequestBody(body);
  const requestQuery = typeof parsed['query'] === 'string' ? parsed['query'] : '<missing query>';
  const requestVariables = stableJson(parsed['variables'] ?? {});
  const candidates = calls
    .map((call, index) => ({ call, index }))
    .filter(({ index }) => !consumedCalls.has(index));
  const candidateSummary = candidates
    .slice(0, 12)
    .map(({ call, index }) => {
      const variables = stableJson(call.variables ?? {});
      const query = typeof call.query === 'string' ? truncate(call.query, 1200) : '<missing query>';
      return [
        `candidate upstreamCalls[${index}]`,
        `  operationName: ${call.operationName ?? '<missing>'}`,
        `  variables: ${variables}`,
        `  query: ${query}`,
      ].join('\n');
    })
    .join('\n');

  const omitted = candidates.length > 12 ? `\n... ${candidates.length - 12} more unconsumed candidate(s) omitted` : '';
  return [
    'No exact recorded upstream call matched the proxy request.',
    'Cassette matching is strict: recorded query text and variables must exactly equal the outgoing request.',
    `Outgoing variables: ${requestVariables}`,
    'Outgoing query:',
    truncate(requestQuery),
    candidateSummary ? `Unconsumed recorded upstream candidates:\n${candidateSummary}${omitted}` : 'No unconsumed recorded upstream candidates remain.',
  ].join('\n');
}
