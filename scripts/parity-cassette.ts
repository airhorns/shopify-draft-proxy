import { parse } from 'graphql';

export type RecordedUpstreamCall = {
  operationName?: string;
  variables?: unknown;
  query?: string;
  response?: { status?: number; body?: unknown };
};

export type CassetteResponse = { status: number; body: unknown };

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

function recordedCallResponseBody(call: RecordedUpstreamCall): unknown {
  if (call.response?.body !== undefined) return call.response.body;
  const response = call.response as Record<string, unknown> | undefined;
  if (isPlainObject(response) && response['data'] !== undefined) return response;
  return {};
}

function customerMergeRequest(body: string): { ids: string[]; includeAttachedResources: boolean } | null {
  let parsed: Record<string, unknown>;
  try {
    parsed = JSON.parse(body) as Record<string, unknown>;
  } catch {
    return null;
  }
  const operationName = parsed['operationName'];
  if (operationName !== 'CustomerMergeHydrate' && operationName !== 'CustomerMergeAttachedHydrate') return null;
  const query = parsed['query'];
  if (typeof query !== 'string' || !query.includes('nodes(ids: $ids)') || !isGraphqlDocumentText(query)) return null;
  const variables = parsed['variables'];
  if (!isPlainObject(variables)) return null;
  const ids = variables['ids'];
  if (!Array.isArray(ids) || ids.some((id) => typeof id !== 'string')) return null;
  return { ids: ids as string[], includeAttachedResources: operationName === 'CustomerMergeAttachedHydrate' };
}

function legacyCustomerMergeHydrateCallMatchesId(call: RecordedUpstreamCall, id: string): boolean {
  return (
    call.operationName === 'CustomerMergeHydrate' &&
    typeof call.query === 'string' &&
    call.query.includes('customer(id: $id)') &&
    isGraphqlDocumentText(call.query) &&
    stableJson(call.variables ?? {}) === stableJson({ id })
  );
}

function mergeScalarCustomerNode(customer: unknown): unknown {
  if (!isPlainObject(customer)) return customer;
  const scalarKeys = [
    'id',
    'firstName',
    'lastName',
    'displayName',
    'email',
    'phone',
    'locale',
    'note',
    'canDelete',
    'verifiedEmail',
    'dataSaleOptOut',
    'taxExempt',
    'taxExemptions',
    'state',
    'tags',
    'numberOfOrders',
    'createdAt',
    'updatedAt',
    'defaultEmailAddress',
    'defaultPhoneNumber',
    'defaultAddress',
    'lastOrder',
  ];
  const projected: Record<string, unknown> = {};
  for (const key of scalarKeys) {
    if (Object.hasOwn(customer, key)) projected[key] = customer[key];
  }
  return projected;
}

function connectionHasEntries(connection: unknown, field: 'nodes' | 'edges'): boolean {
  if (!isPlainObject(connection)) return false;
  const entries = connection[field];
  return Array.isArray(entries) && entries.length > 0;
}

function mergeAttachedCustomerNode(customer: unknown): unknown {
  if (!isPlainObject(customer)) return customer;
  const projected: Record<string, unknown> = { ...customer };
  if (!connectionHasEntries(projected['addressesV2'], 'nodes')) delete projected['addressesV2'];
  if (!connectionHasEntries(projected['metafields'], 'nodes')) delete projected['metafields'];
  if (!connectionHasEntries(projected['orders'], 'edges')) delete projected['orders'];
  return projected;
}

export function customerMergeHydrateCompatibilityResponse(
  body: string,
  calls: RecordedUpstreamCall[],
): CassetteResponse | null {
  const request = customerMergeRequest(body);
  if (request === null) return null;
  const nodes: unknown[] = [];
  let status = 200;
  for (const id of request.ids) {
    const call = calls.find((candidate) => legacyCustomerMergeHydrateCallMatchesId(candidate, id));
    if (call === undefined) return null;
    status = call.response?.status ?? status;
    const responseBody = recordedCallResponseBody(call);
    const data = isPlainObject(responseBody) ? responseBody['data'] : undefined;
    const customer = isPlainObject(data) ? data['customer'] : undefined;
    nodes.push(
      request.includeAttachedResources
        ? mergeAttachedCustomerNode(customer ?? null)
        : mergeScalarCustomerNode(customer ?? null),
    );
  }
  return { status, body: { data: { nodes } } };
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
  const candidates = calls.map((call, index) => ({ call, index })).filter(({ index }) => !consumedCalls.has(index));
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
    candidateSummary
      ? `Unconsumed recorded upstream candidates:\n${candidateSummary}${omitted}`
      : 'No unconsumed recorded upstream candidates remain.',
  ].join('\n');
}
