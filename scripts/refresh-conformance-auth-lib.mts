type RefreshClientIdSource = {
  payload?: unknown;
  sourceName?: string;
};

type RefreshFailureClassification = {
  kind: string;
  recommendedNextStep: string;
  summary: string;
};

export function resolveRefreshClientId(
  manualStoreAuthPayload: unknown,
  fallbackSources: RefreshClientIdSource[] = [],
): { clientId: string; sourceName: string } {
  const directClientId = pickClientId(manualStoreAuthPayload);
  if (directClientId) {
    return {
      clientId: directClientId,
      sourceName: '.manual-store-auth-token.json',
    };
  }

  for (const source of fallbackSources) {
    const clientId = pickClientId(source?.payload);
    if (clientId) {
      return {
        clientId,
        sourceName: source?.sourceName ?? 'unknown-fallback-source',
      };
    }
  }

  throw new Error('.manual-store-auth-token.json is missing required string field client_id.');
}

export function classifyRefreshFailure(payload: unknown): RefreshFailureClassification | null {
  const error = readStringProperty(payload, 'error')?.toLowerCase() ?? '';
  const description = readStringProperty(payload, 'error_description')?.toLowerCase() ?? '';

  if (error === 'invalid_request' && description.includes('active refresh_token')) {
    return {
      kind: 'inactive-refresh-token',
      recommendedNextStep: 'manual-store-auth-reauthorization',
      summary: 'Shopify rejected the saved refresh grant because it no longer has an active refresh_token.',
    };
  }

  return null;
}

function pickClientId(payload: unknown): string | null {
  const clientId = readStringProperty(payload, 'client_id');
  if (typeof clientId === 'string' && clientId.length > 0) {
    return clientId;
  }

  const legacyClientId = readStringProperty(payload, 'clientId');
  if (typeof legacyClientId === 'string' && legacyClientId.length > 0) {
    return legacyClientId;
  }

  return null;
}

function readStringProperty(value: unknown, key: string): string | null {
  if (typeof value !== 'object' || value === null) {
    return null;
  }

  const candidate = (value as Record<string, unknown>)[key];
  return typeof candidate === 'string' ? candidate : null;
}
