import { describe, expect, it } from 'vitest';

describe('refresh conformance auth client id resolution', () => {
  it('falls back to saved PKCE/auth metadata when the persisted token payload lacks client_id', async () => {
    const { resolveRefreshClientId } = await import('../../scripts/refresh-conformance-auth-lib.mjs');

    expect(
      resolveRefreshClientId(
        {
          access_token: 'shpca_example',
          refresh_token: 'shprt_example',
        },
        [
          {
            sourceName: '.manual-store-auth-pkce.json',
            payload: {
              client_id: 'pkce-client-id',
            },
          },
          {
            sourceName: '.manual-store-auth.json',
            payload: {
              clientId: 'legacy-client-id',
            },
          },
        ],
      ),
    ).toEqual({
      clientId: 'pkce-client-id',
      sourceName: '.manual-store-auth-pkce.json',
    });
  });
});

describe('refresh conformance auth failure classification', () => {
  it('classifies active-refresh-token failures as requiring manual store reauthorization', async () => {
    const { classifyRefreshFailure } = await import('../../scripts/refresh-conformance-auth-lib.mjs');

    expect(
      classifyRefreshFailure({
        error: 'invalid_request',
        error_description: 'This request requires an active refresh_token',
      }),
    ).toEqual({
      kind: 'inactive-refresh-token',
      recommendedNextStep: 'manual-store-auth-reauthorization',
      summary: 'Shopify rejected the saved refresh grant because it no longer has an active refresh_token.',
    });
  });

  it('does not classify unrelated refresh failures as inactive refresh-token failures', async () => {
    const { classifyRefreshFailure } = await import('../../scripts/refresh-conformance-auth-lib.mjs');

    expect(
      classifyRefreshFailure({
        error: 'invalid_request',
        error_description: 'missing client_id',
      }),
    ).toBeNull();
  });
});
