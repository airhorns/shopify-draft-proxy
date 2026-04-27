import Router from '@koa/router';
import type Koa from 'koa';
import type { AppConfig } from '../config.js';
import { createUpstreamGraphQLClient } from '../shopify/upstream-client.js';
import { requestUpstreamGraphQL, type IncomingGraphQLRequestContext } from '../shopify/upstream-request.js';
import { store } from '../state/store.js';
import { isProxySyntheticGid, resetSyntheticIdentity } from '../state/synthetic-identity.js';
import type { MutationLogEntry } from '../state/types.js';

export interface CommitAttempt {
  logEntryId: string;
  operationName: string | null;
  path: string;
  success: boolean;
  status: MutationLogEntry['status'];
  upstreamStatus: number | null;
  upstreamBody: unknown;
  upstreamError: { message: string } | null;
  responseBody: unknown;
}

export interface MetaHealthResponse {
  ok: true;
  message: string;
}

export interface MetaResetResponse {
  ok: true;
  message: string;
}

export interface MetaCommitResponse {
  ok: boolean;
  stopIndex: number | null;
  attempts: CommitAttempt[];
}

function logEntryRequiresCommit(entry: MutationLogEntry): boolean {
  return entry.status === 'staged';
}

function responseBodyHasGraphQLErrors(body: unknown): boolean {
  if (!body || typeof body !== 'object') {
    return false;
  }

  const errors = (body as Record<string, unknown>)['errors'];
  return Array.isArray(errors) && errors.length > 0;
}

function buildCommitReplayBody(entry: MutationLogEntry): Record<string, unknown> {
  return structuredClone(
    entry.requestBody ?? {
      query: entry.query,
      variables: entry.variables,
    },
  );
}

function readGidResourceType(value: string): string | null {
  const match = /^gid:\/\/shopify\/([^/?]+)\//u.exec(value);
  return match?.[1] ?? null;
}

function replaceMappedSyntheticGids(value: unknown, idMap: Map<string, string>): unknown {
  if (typeof value === 'string') {
    let replaced = idMap.get(value) ?? value;
    for (const [syntheticId, authoritativeId] of idMap.entries()) {
      replaced = replaced.replaceAll(syntheticId, authoritativeId);
    }
    return replaced;
  }

  if (!value || typeof value !== 'object') {
    return value;
  }

  if (Array.isArray(value)) {
    return value.map((item) => replaceMappedSyntheticGids(item, idMap));
  }

  return Object.fromEntries(
    Object.entries(value).map(([key, item]) => {
      return [key, replaceMappedSyntheticGids(item, idMap)];
    }),
  );
}

function collectAuthoritativeGidsByType(
  value: unknown,
  gidsByType = new Map<string, string[]>(),
): Map<string, string[]> {
  if (typeof value === 'string') {
    if (value.startsWith('gid://shopify/') && !isProxySyntheticGid(value)) {
      const resourceType = readGidResourceType(value);
      if (resourceType) {
        const gids = gidsByType.get(resourceType) ?? [];
        if (!gids.includes(value)) {
          gids.push(value);
          gidsByType.set(resourceType, gids);
        }
      }
    }
    return gidsByType;
  }

  if (!value || typeof value !== 'object') {
    return gidsByType;
  }

  if (Array.isArray(value)) {
    for (const item of value) {
      collectAuthoritativeGidsByType(item, gidsByType);
    }
    return gidsByType;
  }

  for (const item of Object.values(value)) {
    collectAuthoritativeGidsByType(item, gidsByType);
  }

  return gidsByType;
}

function recordCommitIdMappings(entry: MutationLogEntry, responseBody: unknown, idMap: Map<string, string>): void {
  const stagedResourceIds = entry.stagedResourceIds ?? [];
  if (stagedResourceIds.length === 0) {
    return;
  }

  const responseGidsByType = collectAuthoritativeGidsByType(responseBody);

  for (const stagedId of stagedResourceIds) {
    if (!isProxySyntheticGid(stagedId) || idMap.has(stagedId)) {
      continue;
    }

    const resourceType = readGidResourceType(stagedId);
    const authoritativeId = resourceType ? responseGidsByType.get(resourceType)?.[0] : null;
    if (authoritativeId) {
      idMap.set(stagedId, authoritativeId);
    }
  }
}

function escapeHtml(value: string): string {
  return value
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;')
    .replaceAll("'", '&#39;');
}

function formatJsonForHtml(value: unknown): string {
  return escapeHtml(JSON.stringify(value, null, 2));
}

function countRecordEntries(value: unknown): number {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    return 0;
  }

  return Object.keys(value).length;
}

function renderObjectCountList(snapshot: Record<string, unknown>): string {
  const items = Object.entries(snapshot)
    .map(([name, value]) => {
      return `<li><span>${escapeHtml(name)}</span><strong>${countRecordEntries(value)}</strong></li>`;
    })
    .join('');

  return `<ul class="summary-list">${items}</ul>`;
}

function renderMutationLogRows(entries: MutationLogEntry[]): string {
  if (entries.length === 0) {
    return '<tr><td colspan="5" class="empty">No operations staged.</td></tr>';
  }

  return entries
    .map((entry) => {
      return `<tr>
        <td>${escapeHtml(entry.receivedAt)}</td>
        <td>${escapeHtml(entry.operationName ?? '(anonymous)')}</td>
        <td><span class="status">${escapeHtml(entry.status)}</span></td>
        <td>${escapeHtml(entry.interpreted.capability.domain)}</td>
        <td>${escapeHtml(entry.path)}</td>
      </tr>`;
    })
    .join('');
}

export function renderMetaWebUi(config: AppConfig): string {
  const log = { entries: store.getLog() };
  const state = store.getState();

  return `<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <title>Shopify Draft Proxy</title>
    <style>
      :root {
        color-scheme: light;
        --bg: #f6f7f2;
        --ink: #1d2320;
        --muted: #5c665f;
        --panel: #ffffff;
        --line: #d9dfd6;
        --accent: #087f5b;
        --accent-dark: #065f46;
        --danger: #b42318;
        --danger-bg: #fff1ef;
        --code: #17211c;
      }

      * {
        box-sizing: border-box;
      }

      body {
        margin: 0;
        background: var(--bg);
        color: var(--ink);
        font-family:
          Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
        line-height: 1.5;
      }

      header,
      main {
        width: min(1180px, calc(100% - 32px));
        margin: 0 auto;
      }

      header {
        padding: 32px 0 20px;
      }

      h1,
      h2 {
        margin: 0;
        letter-spacing: 0;
      }

      h1 {
        font-size: 32px;
        line-height: 1.15;
      }

      h2 {
        font-size: 18px;
      }

      p {
        margin: 6px 0 0;
        color: var(--muted);
      }

      .toolbar,
      section {
        background: var(--panel);
        border: 1px solid var(--line);
      }

      .toolbar {
        display: grid;
        grid-template-columns: 1fr auto auto;
        gap: 12px;
        align-items: end;
        padding: 16px;
        border-radius: 8px;
        margin-bottom: 18px;
      }

      label {
        display: grid;
        gap: 6px;
        color: var(--muted);
        font-size: 13px;
        font-weight: 600;
      }

      input {
        width: 100%;
        min-height: 40px;
        border: 1px solid var(--line);
        border-radius: 6px;
        padding: 8px 10px;
        font: inherit;
        color: var(--ink);
      }

      button {
        min-height: 40px;
        border: 1px solid transparent;
        border-radius: 6px;
        padding: 8px 14px;
        font: inherit;
        font-weight: 700;
        color: #ffffff;
        background: var(--accent);
        cursor: pointer;
      }

      button:hover {
        background: var(--accent-dark);
      }

      button.secondary {
        color: var(--danger);
        background: var(--danger-bg);
        border-color: #ffd2cc;
      }

      button.secondary:hover {
        background: #ffe4e0;
      }

      button:disabled {
        opacity: 0.55;
        cursor: progress;
      }

      #action-status {
        min-height: 24px;
        margin-bottom: 18px;
        color: var(--muted);
        font-weight: 600;
      }

      .grid {
        display: grid;
        grid-template-columns: minmax(0, 1.15fr) minmax(320px, 0.85fr);
        gap: 18px;
        align-items: start;
      }

      section {
        border-radius: 8px;
        overflow: hidden;
      }

      .section-head {
        display: flex;
        justify-content: space-between;
        gap: 16px;
        padding: 16px;
        border-bottom: 1px solid var(--line);
      }

      .pill {
        align-self: start;
        border: 1px solid var(--line);
        border-radius: 999px;
        padding: 4px 10px;
        color: var(--muted);
        font-size: 12px;
        font-weight: 700;
        white-space: nowrap;
      }

      table {
        width: 100%;
        border-collapse: collapse;
        table-layout: fixed;
      }

      th,
      td {
        padding: 10px 12px;
        border-bottom: 1px solid var(--line);
        text-align: left;
        vertical-align: top;
        word-break: break-word;
      }

      th {
        color: var(--muted);
        font-size: 12px;
        text-transform: uppercase;
      }

      .status {
        display: inline-block;
        border-radius: 999px;
        background: #e7f5ef;
        color: var(--accent-dark);
        padding: 2px 8px;
        font-size: 12px;
        font-weight: 800;
      }

      .empty {
        color: var(--muted);
        text-align: center;
      }

      .summary {
        display: grid;
        grid-template-columns: 1fr 1fr;
        gap: 12px;
        padding: 16px;
      }

      .summary h3 {
        margin: 0 0 8px;
        font-size: 14px;
      }

      .summary-list {
        display: grid;
        gap: 6px;
        margin: 0;
        padding: 0;
        list-style: none;
      }

      .summary-list li {
        display: flex;
        justify-content: space-between;
        gap: 12px;
        border-bottom: 1px solid var(--line);
        padding-bottom: 6px;
        color: var(--muted);
      }

      .summary-list strong {
        color: var(--ink);
      }

      details {
        border-top: 1px solid var(--line);
      }

      summary {
        cursor: pointer;
        padding: 12px 16px;
        color: var(--muted);
        font-weight: 700;
      }

      pre {
        margin: 0;
        max-height: 520px;
        overflow: auto;
        padding: 16px;
        background: var(--code);
        color: #edf7f1;
        font-size: 12px;
        line-height: 1.45;
        white-space: pre-wrap;
      }

      @media (max-width: 820px) {
        header,
        main {
          width: min(100% - 24px, 1180px);
        }

        .toolbar,
        .grid,
        .summary {
          grid-template-columns: 1fr;
        }
      }
    </style>
  </head>
  <body>
    <header>
      <h1>Shopify Draft Proxy</h1>
      <p>${escapeHtml(config.readMode)} mode against ${escapeHtml(config.shopifyAdminOrigin)}</p>
    </header>
    <main>
      <div class="toolbar">
        <label>
          Commit access token
          <input id="commit-token" type="password" autocomplete="off" spellcheck="false" placeholder="Optional for commit">
        </label>
        <button type="button" data-action-path="/__meta/commit">Commit</button>
        <button type="button" class="secondary" data-action-path="/__meta/reset">Reset</button>
      </div>
      <div id="action-status" role="status" aria-live="polite"></div>
      <div class="grid">
        <section>
          <div class="section-head">
            <div>
              <h2>Operation Log</h2>
              <p>Original mutation requests in replay order.</p>
            </div>
            <span id="log-count" class="pill">${log.entries.length} entries</span>
          </div>
          <table>
            <thead>
              <tr>
                <th>Received</th>
                <th>Operation</th>
                <th>Status</th>
                <th>Domain</th>
                <th>Path</th>
              </tr>
            </thead>
            <tbody id="operation-log-rows">
              ${renderMutationLogRows(log.entries)}
            </tbody>
          </table>
          <details>
            <summary>Raw log JSON</summary>
            <pre id="operation-log-json">${formatJsonForHtml(log)}</pre>
          </details>
        </section>
        <section>
          <div class="section-head">
            <div>
              <h2>State</h2>
              <p>In-memory base and staged object graph.</p>
            </div>
          </div>
          <div class="summary">
            <div>
              <h3>Base</h3>
              ${renderObjectCountList(state.baseState as unknown as Record<string, unknown>)}
            </div>
            <div>
              <h3>Staged</h3>
              ${renderObjectCountList(state.stagedState as unknown as Record<string, unknown>)}
            </div>
          </div>
          <details open>
            <summary>Raw state JSON</summary>
            <pre id="state-json">${formatJsonForHtml(state)}</pre>
          </details>
        </section>
      </div>
    </main>
    <script>
      const statusEl = document.querySelector('#action-status');
      const tokenEl = document.querySelector('#commit-token');
      const buttons = Array.from(document.querySelectorAll('[data-action-path]'));

      function setBusy(isBusy) {
        for (const button of buttons) {
          button.disabled = isBusy;
        }
      }

      async function runAction(path) {
        setBusy(true);
        statusEl.textContent = 'Running ' + path + '...';
        try {
          const headers = {};
          if (path === '/__meta/commit' && tokenEl.value.trim()) {
            headers['x-shopify-access-token'] = tokenEl.value.trim();
          }
          const response = await fetch(path, { method: 'POST', headers });
          const body = await response.json();
          statusEl.textContent = response.ok && body.ok !== false ? 'Action complete.' : 'Action returned an error.';
          await refreshMeta();
        } catch (error) {
          statusEl.textContent = error instanceof Error ? error.message : String(error);
        } finally {
          setBusy(false);
        }
      }

      async function refreshMeta() {
        const [logResponse, stateResponse] = await Promise.all([fetch('/__meta/log'), fetch('/__meta/state')]);
        const [logBody, stateBody] = await Promise.all([logResponse.json(), stateResponse.json()]);
        document.querySelector('#log-count').textContent = String(logBody.entries.length) + ' entries';
        renderLogRows(logBody.entries);
        document.querySelector('#operation-log-json').textContent = JSON.stringify(logBody, null, 2);
        document.querySelector('#state-json').textContent = JSON.stringify(stateBody, null, 2);
      }

      function renderLogRows(entries) {
        const tbody = document.querySelector('#operation-log-rows');
        tbody.replaceChildren();

        if (entries.length === 0) {
          const row = document.createElement('tr');
          const cell = document.createElement('td');
          cell.colSpan = 5;
          cell.className = 'empty';
          cell.textContent = 'No operations staged.';
          row.append(cell);
          tbody.append(row);
          return;
        }

        for (const entry of entries) {
          const row = document.createElement('tr');
          for (const value of [
            entry.receivedAt,
            entry.operationName ?? '(anonymous)',
            entry.status,
            entry.interpreted?.capability?.domain ?? 'unknown',
            entry.path,
          ]) {
            const cell = document.createElement('td');
            cell.textContent = String(value);
            row.append(cell);
          }
          tbody.append(row);
        }
      }

      for (const button of buttons) {
        button.addEventListener('click', () => runAction(button.dataset.actionPath));
      }
    </script>
  </body>
</html>`;
}

export function getMetaHealth(): MetaHealthResponse {
  return {
    ok: true,
    message: 'shopify-draft-proxy is running',
  };
}

export function getMetaConfig(config: AppConfig): Record<string, unknown> {
  return {
    runtime: {
      readMode: config.readMode,
    },
    proxy: {
      port: config.port,
      shopifyAdminOrigin: config.shopifyAdminOrigin,
    },
    snapshot: {
      enabled: Boolean(config.snapshotPath),
      path: config.snapshotPath ?? null,
    },
  };
}

export function getMetaLog(): { entries: MutationLogEntry[] } {
  return {
    entries: store.getLog(),
  };
}

export function getMetaState(): ReturnType<typeof store.getState> {
  return store.getState();
}

export function resetMetaState(): MetaResetResponse {
  store.restoreInitialState();
  resetSyntheticIdentity();
  return {
    ok: true,
    message: 'state reset',
  };
}

export async function commitMetaState(
  config: AppConfig,
  requestContext: IncomingGraphQLRequestContext,
): Promise<MetaCommitResponse> {
  const upstream = createUpstreamGraphQLClient(config.shopifyAdminOrigin);
  const pendingEntries = store.getLog().filter(logEntryRequiresCommit);
  const attempts: CommitAttempt[] = [];
  const syntheticIdMap = new Map<string, string>();
  let stopIndex: number | null = null;

  for (const [index, entry] of pendingEntries.entries()) {
    try {
      const replayBody = replaceMappedSyntheticGids(buildCommitReplayBody(entry), syntheticIdMap);
      const response = await requestUpstreamGraphQL(upstream, requestContext, {
        path: entry.path,
        body: replayBody,
      });
      const responseBody = await response.json();
      const failed = response.status >= 400 || responseBodyHasGraphQLErrors(responseBody);
      const nextStatus: MutationLogEntry['status'] = failed ? 'failed' : 'committed';

      if (!failed) {
        recordCommitIdMappings(entry, responseBody, syntheticIdMap);
      }

      store.updateLogEntry(entry.id, {
        status: nextStatus,
        notes: failed
          ? 'Commit replay failed against upstream Shopify.'
          : 'Committed to upstream Shopify via __meta/commit replay.',
      });

      attempts.push({
        logEntryId: entry.id,
        operationName: entry.operationName,
        path: entry.path,
        success: !failed,
        status: nextStatus,
        upstreamStatus: response.status,
        upstreamBody: responseBody,
        upstreamError: null,
        responseBody,
      });

      if (failed) {
        stopIndex = index;
        break;
      }
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      store.updateLogEntry(entry.id, {
        status: 'failed',
        notes: `Commit replay failed before an upstream response was received: ${message}`,
      });
      attempts.push({
        logEntryId: entry.id,
        operationName: entry.operationName,
        path: entry.path,
        success: false,
        status: 'failed',
        upstreamStatus: null,
        upstreamBody: null,
        upstreamError: { message },
        responseBody: { errors: [{ message }] },
      });
      stopIndex = index;
      break;
    }
  }

  return {
    ok: stopIndex === null,
    stopIndex,
    attempts,
  };
}

export function createMetaRouter(config: AppConfig): Router {
  const router = new Router();

  router.get('/__meta', (ctx: Koa.Context) => {
    ctx.type = 'html';
    ctx.body = renderMetaWebUi(config);
  });

  router.get('/__meta/health', (ctx: Koa.Context) => {
    ctx.body = getMetaHealth();
  });

  router.get('/__meta/config', (ctx: Koa.Context) => {
    ctx.body = getMetaConfig(config);
  });

  router.get('/__meta/log', (ctx: Koa.Context) => {
    ctx.body = getMetaLog();
  });

  router.get('/__meta/state', (ctx: Koa.Context) => {
    ctx.body = getMetaState();
  });

  router.post('/__meta/reset', (ctx: Koa.Context) => {
    ctx.body = resetMetaState();
  });

  router.post('/__meta/commit', async (ctx: Koa.Context) => {
    ctx.body = await commitMetaState(config, ctx);
  });

  return router;
}
