export function resolveRefreshClientId(manualStoreAuthPayload, fallbackSources = []) {
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

export function classifyRefreshFailure(payload) {
  const error = typeof payload?.error === 'string' ? payload.error.toLowerCase() : '';
  const description = typeof payload?.error_description === 'string' ? payload.error_description.toLowerCase() : '';

  if (error === 'invalid_request' && description.includes('active refresh_token')) {
    return {
      kind: 'inactive-refresh-token',
      recommendedNextStep: 'manual-store-auth-reauthorization',
      summary: 'Shopify rejected the saved refresh grant because it no longer has an active refresh_token.',
    };
  }

  return null;
}

function pickClientId(payload) {
  const clientId = payload?.['client_id'];
  if (typeof clientId === 'string' && clientId.length > 0) {
    return clientId;
  }

  const legacyClientId = payload?.['clientId'];
  if (typeof legacyClientId === 'string' && legacyClientId.length > 0) {
    return legacyClientId;
  }

  return null;
}
