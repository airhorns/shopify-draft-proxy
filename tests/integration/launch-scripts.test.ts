import { spawn, type ChildProcessWithoutNullStreams } from 'node:child_process';
import { setTimeout as delay } from 'node:timers/promises';
import { describe, expect, it } from 'vitest';

const repoRoot = new URL('../..', import.meta.url);
const testOrigin = 'https://example.myshopify.com';
const pnpmCommand = 'corepack';

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
  const deadline = Date.now() + 15_000;

  while (Date.now() < deadline) {
    if (getOutput().includes('shopify-draft-proxy listening')) {
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
  try {
    if (child.pid) {
      process.kill(-child.pid, signal);
      return;
    }
    child.kill(signal);
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code !== 'ESRCH') {
      throw error;
    }
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

describe('package launch scripts', () => {
  it('starts the dev server and serves health', async () => {
    await expectLaunchScriptHealth('dev', 43_194);
  }, 20_000);

  it('starts the built server and serves health', async () => {
    await runPnpm(['build']);
    await expectLaunchScriptHealth('start', 43_195);
  }, 30_000);
});
