import { spawn, spawnSync, type ChildProcessWithoutNullStreams } from 'node:child_process';
import { once } from 'node:events';
import { createServer, type Server } from 'node:http';
import type { AddressInfo } from 'node:net';
import { setTimeout as delay } from 'node:timers/promises';
import { describe, expect, it } from 'vitest';

const repoRoot = new URL('../..', import.meta.url);
const testOrigin = 'https://example.myshopify.com';
const pnpmCommand = 'corepack';
const serverStartupTimeoutMs = 60_000;

function pnpmArgs(args: string[]): string[] {
  return ['pnpm', ...args];
}

function collectOutput(child: ChildProcessWithoutNullStreams): { getOutput: () => string } {
  let output = '';
  child.stdout.on('data', (chunk: Buffer) => {
    output += chunk.toString();
  });
  child.stderr.on('data', (chunk: Buffer) => {
    output += chunk.toString();
  });
  return {
    getOutput: () => output,
  };
}

async function runPnpm(args: string[]): Promise<void> {
  const child = spawn(pnpmCommand, pnpmArgs(args), {
    cwd: repoRoot,
    env: process.env,
  });
  const { getOutput } = collectOutput(child);

  const exitCode = await new Promise<number | null>((resolve) => {
    child.on('exit', resolve);
  });

  expect(exitCode, getOutput()).toBe(0);
}

async function waitForServer(child: ChildProcessWithoutNullStreams, getOutput: () => string): Promise<void> {
  const deadline = Date.now() + serverStartupTimeoutMs;

  while (Date.now() < deadline) {
    if (getOutput().includes('shopify-draft-proxy rust runtime listening')) {
      return;
    }
    if (child.exitCode !== null) {
      throw new Error(`server process exited before listening:\n${getOutput()}`);
    }
    await delay(100);
  }

  throw new Error(`server did not start before timeout:\n${getOutput()}`);
}

async function stopServer(child: ChildProcessWithoutNullStreams): Promise<void> {
  if (child.exitCode !== null) {
    return;
  }

  killServerProcess(child, 'SIGTERM');

  for (let attempt = 0; attempt < 20; attempt += 1) {
    if (child.exitCode !== null) {
      return;
    }
    await delay(100);
  }

  killServerProcess(child, 'SIGKILL');
}

function killServerProcess(child: ChildProcessWithoutNullStreams, signal: NodeJS.Signals): void {
  if (!child.pid) {
    child.kill(signal);
    return;
  }

  for (const pid of processTreePids(child.pid)) {
    try {
      process.kill(pid, signal);
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code !== 'ESRCH') {
        throw error;
      }
    }
  }
}

function processTreePids(rootPid: number): number[] {
  const result = spawnSync('ps', ['-eo', 'pid=,ppid='], { encoding: 'utf8' });
  if (result.status !== 0) {
    return [rootPid];
  }

  const childrenByParent = new Map<number, number[]>();
  for (const line of result.stdout.split('\n')) {
    const [pidText, ppidText] = line.trim().split(/\s+/);
    const pid = Number(pidText);
    const ppid = Number(ppidText);
    if (!Number.isFinite(pid) || !Number.isFinite(ppid)) {
      continue;
    }
    const children = childrenByParent.get(ppid) ?? [];
    children.push(pid);
    childrenByParent.set(ppid, children);
  }

  const ordered = [rootPid];
  for (let index = 0; index < ordered.length; index += 1) {
    ordered.push(...(childrenByParent.get(ordered[index] ?? 0) ?? []));
  }
  return ordered.reverse();
}

async function closeServer(server: Server): Promise<void> {
  await new Promise<void>((resolve, reject) => {
    server.close((error) => {
      if (error) reject(error);
      else resolve();
    });
  });
}

async function unusedLocalPort(): Promise<number> {
  const server = createServer();
  server.listen(0, '127.0.0.1');
  await once(server, 'listening');
  const address = server.address() as AddressInfo;
  const { port } = address;
  await closeServer(server);
  return port;
}

async function withUpstream<T>(
  run: (
    origin: string,
    requests: Array<{ url: string | undefined; authorization: string | undefined; body: string }>,
  ) => Promise<T>,
): Promise<T> {
  const requests: Array<{ url: string | undefined; authorization: string | undefined; body: string }> = [];
  const server = createServer((req, res) => {
    let body = '';
    req.setEncoding('utf8');
    req.on('data', (chunk: string) => {
      body += chunk;
    });
    req.on('end', () => {
      requests.push({ url: req.url, authorization: req.headers.authorization, body });
      res.writeHead(202, { 'content-type': 'application/json', 'x-test-upstream': 'rust-http' });
      res.end(JSON.stringify({ data: { upstreamEcho: { ok: true } } }));
    });
  });
  server.listen(0, '127.0.0.1');
  await once(server, 'listening');
  const address = server.address() as AddressInfo;

  try {
    return await run(`http://127.0.0.1:${address.port}`, requests);
  } finally {
    await closeServer(server);
  }
}

async function expectLaunchScriptHealth(script: 'dev' | 'start', port: number): Promise<void> {
  const child = spawn(pnpmCommand, pnpmArgs([script]), {
    cwd: repoRoot,
    detached: true,
    env: {
      ...process.env,
      PORT: String(port),
      SHOPIFY_ADMIN_ORIGIN: testOrigin,
    },
  });
  const { getOutput } = collectOutput(child);

  try {
    await waitForServer(child, getOutput);

    const response = await fetch(`http://127.0.0.1:${port}/__meta/health`);

    expect(response.status).toBe(200);
    await expect(response.json()).resolves.toEqual({
      ok: true,
      message: 'shopify-draft-proxy is running',
    });
  } finally {
    await stopServer(child);
  }
}

async function withLaunchedProxy<T>(
  port: number,
  env: Record<string, string>,
  run: (origin: string) => Promise<T>,
): Promise<T> {
  const child = spawn(pnpmCommand, pnpmArgs(['dev']), {
    cwd: repoRoot,
    detached: true,
    env: {
      ...process.env,
      PORT: String(port),
      ...env,
    },
  });
  const { getOutput } = collectOutput(child);

  try {
    await waitForServer(child, getOutput);
    return await run(`http://127.0.0.1:${port}`);
  } finally {
    await stopServer(child);
  }
}

describe('package launch scripts', () => {
  it('starts the dev server and serves health', async () => {
    await expectLaunchScriptHealth('dev', await unusedLocalPort());
  }, 75_000);

  it('starts the built server and serves health', async () => {
    await runPnpm(['build']);
    await expectLaunchScriptHealth('start', await unusedLocalPort());
  }, 90_000);

  it('forwards live-hybrid passthrough and commit replay through Rust HTTP transport', async () => {
    await withUpstream(async (upstreamOrigin, upstreamRequests) => {
      await withLaunchedProxy(
        await unusedLocalPort(),
        {
          SHOPIFY_ADMIN_ORIGIN: upstreamOrigin,
          SHOPIFY_DRAFT_PROXY_READ_MODE: 'live-hybrid',
          SHOPIFY_DRAFT_PROXY_UNSUPPORTED_MUTATION_MODE: 'passthrough',
        },
        async (proxyOrigin) => {
          const passthrough = await fetch(`${proxyOrigin}/admin/api/2026-04/graphql.json`, {
            method: 'POST',
            headers: { 'content-type': 'application/json', authorization: 'Bearer live-token' },
            body: JSON.stringify({ query: '{ currentAppInstallation { id } }' }),
          });
          expect(passthrough.status).toBe(202);
          expect(passthrough.headers.get('x-test-upstream')).toBe('rust-http');
          await expect(passthrough.json()).resolves.toEqual({ data: { upstreamEcho: { ok: true } } });

          const create = await fetch(`${proxyOrigin}/admin/api/2026-04/graphql.json`, {
            method: 'POST',
            headers: { 'content-type': 'application/json' },
            body: JSON.stringify({
              query:
                'mutation { savedSearchCreate(input: { name: "Promo orders", query: "tag:promo", resourceType: ORDER }) { savedSearch { id name } userErrors { field message } } }',
            }),
          });
          expect(create.status).toBe(200);
          await expect(create.json()).resolves.toMatchObject({
            data: { savedSearchCreate: { savedSearch: { name: 'Promo orders' }, userErrors: [] } },
          });

          const commit = await fetch(`${proxyOrigin}/__meta/commit`, {
            method: 'POST',
            headers: { authorization: 'Bearer commit-token' },
          });
          expect(commit.status).toBe(200);
          await expect(commit.json()).resolves.toMatchObject({ ok: true, committed: 1, failed: 0 });
        },
      );

      expect(upstreamRequests).toHaveLength(2);
      expect(upstreamRequests[0]).toMatchObject({
        url: '/admin/api/2026-04/graphql.json',
        authorization: 'Bearer live-token',
        body: JSON.stringify({ query: '{ currentAppInstallation { id } }' }),
      });
      expect(upstreamRequests[1]).toMatchObject({
        url: '/admin/api/2026-04/graphql.json',
        authorization: 'Bearer commit-token',
      });
      expect(upstreamRequests[1]?.body).toContain('savedSearchCreate');
    });
  }, 75_000);
});
