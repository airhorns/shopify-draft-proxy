import { afterEach, describe, expect, it, vi } from 'vitest';

import { findMainConformanceBaseline } from '../../scripts/find-main-conformance-baseline.js';

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'content-type': 'application/json' },
  });
}

describe('main conformance baseline lookup', () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('finds the first matching unexpired artifact from recent main runs', async () => {
    const fetchMock = vi
      .fn<typeof fetch>()
      .mockResolvedValueOnce(
        jsonResponse({
          workflow_runs: [
            { id: 1001, html_url: 'https://github.example/runs/1001', head_sha: 'abc123' },
            { id: 1002, html_url: 'https://github.example/runs/1002', head_sha: 'def456' },
          ],
        }),
      )
      .mockResolvedValueOnce(
        jsonResponse({
          artifacts: [
            { id: 2001, name: 'not-the-baseline', expired: false },
            { id: 2002, name: 'conformance-status-main', expired: true },
          ],
        }),
      )
      .mockResolvedValueOnce(
        jsonResponse({
          artifacts: [{ id: 2003, name: 'conformance-status-main', expired: false }],
        }),
      );
    vi.stubGlobal('fetch', fetchMock);

    await expect(
      findMainConformanceBaseline({
        repository: 'airhorns/shopify-draft-proxy',
        token: 'test-token',
      }),
    ).resolves.toEqual({
      found: true,
      artifactId: '2003',
      artifactName: 'conformance-status-main',
      runId: '1002',
      runUrl: 'https://github.example/runs/1002',
      headSha: 'def456',
    });
  });

  it('treats a GitHub API auth failure as a missing optional baseline', async () => {
    const fetchMock = vi.fn<typeof fetch>().mockResolvedValueOnce(jsonResponse({ message: 'Bad credentials' }, 401));
    vi.stubGlobal('fetch', fetchMock);

    await expect(
      findMainConformanceBaseline({
        repository: 'airhorns/shopify-draft-proxy',
        token: 'bad-token',
      }),
    ).resolves.toEqual({ found: false });
  });

  it('continues past inaccessible artifact listings for older main runs', async () => {
    const fetchMock = vi
      .fn<typeof fetch>()
      .mockResolvedValueOnce(
        jsonResponse({
          workflow_runs: [
            { id: 1001, html_url: 'https://github.example/runs/1001', head_sha: 'abc123' },
            { id: 1002, html_url: 'https://github.example/runs/1002', head_sha: 'def456' },
          ],
        }),
      )
      .mockResolvedValueOnce(jsonResponse({ message: 'Resource not accessible by integration' }, 403))
      .mockResolvedValueOnce(
        jsonResponse({
          artifacts: [{ id: 2003, name: 'conformance-status-main', expired: false }],
        }),
      );
    vi.stubGlobal('fetch', fetchMock);

    await expect(
      findMainConformanceBaseline({
        repository: 'airhorns/shopify-draft-proxy',
        token: 'limited-token',
      }),
    ).resolves.toMatchObject({
      found: true,
      runId: '1002',
      artifactId: '2003',
    });
  });
});
