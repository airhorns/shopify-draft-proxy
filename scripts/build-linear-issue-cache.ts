import 'dotenv/config';

import { execFileSync, spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import path from 'node:path';

const LINEAR_API_URL = 'https://api.linear.app/graphql';
const REPO_ROOT = path.resolve(import.meta.dirname, '..');
const CACHE_DIR = path.join(REPO_ROOT, '.linear-cache');
const ISSUES_DIR = path.join(CACHE_DIR, 'issues');
const STATE_FILE = path.join(CACHE_DIR, 'state.json');
const COLLECTION_NAME = 'linear-issues';
const PAGE_SIZE = 50;
const COMMENT_PAGE_SIZE = 50;

const COMMENT_FIELDS = `
  id
  body
  createdAt
  updatedAt
  editedAt
  user { displayName }
  parent { id }
`;

const ISSUE_QUERY = `
  query CacheIssues($cursor: String, $first: Int!, $commentFirst: Int!, $filter: IssueFilter) {
    issues(first: $first, after: $cursor, filter: $filter, includeArchived: true, orderBy: updatedAt) {
      pageInfo { hasNextPage endCursor }
      nodes {
        id
        identifier
        title
        description
        url
        branchName
        priority
        estimate
        createdAt
        updatedAt
        completedAt
        canceledAt
        archivedAt
        state { name type }
        team { key name }
        project { name slugId }
        cycle { number name }
        parent { identifier }
        assignee { displayName email }
        creator { displayName }
        labels { nodes { name } }
        comments(first: $commentFirst) {
          pageInfo { hasNextPage endCursor }
          nodes {
            ${COMMENT_FIELDS}
          }
        }
      }
    }
  }
`;

const COMMENTS_PAGE_QUERY = `
  query IssueCommentsPage($issueId: String!, $cursor: String, $first: Int!) {
    issue(id: $issueId) {
      comments(first: $first, after: $cursor) {
        pageInfo { hasNextPage endCursor }
        nodes {
          ${COMMENT_FIELDS}
        }
      }
    }
  }
`;

type Comment = {
  id: string;
  body: string;
  createdAt: string;
  updatedAt: string;
  editedAt: string | null;
  user: { displayName: string } | null;
  parent: { id: string } | null;
};

type CommentsConnection = {
  nodes: Comment[];
  pageInfo: { hasNextPage: boolean; endCursor: string | null };
};

type Issue = {
  id: string;
  identifier: string;
  title: string;
  description: string | null;
  url: string;
  branchName: string | null;
  priority: number;
  estimate: number | null;
  createdAt: string;
  updatedAt: string;
  completedAt: string | null;
  canceledAt: string | null;
  archivedAt: string | null;
  state: { name: string; type: string } | null;
  team: { key: string; name: string } | null;
  project: { name: string; slugId: string } | null;
  cycle: { number: number; name: string | null } | null;
  parent: { identifier: string } | null;
  assignee: { displayName: string; email: string | null } | null;
  creator: { displayName: string } | null;
  labels: { nodes: Array<{ name: string }> };
  comments: CommentsConnection;
};

type IssuesPage = {
  nodes: Issue[];
  pageInfo: { hasNextPage: boolean; endCursor: string | null };
};

type IssuesResponse = {
  data?: { issues: IssuesPage };
  errors?: Array<{ message: string }>;
};

type CommentsPageResponse = {
  data?: { issue: { comments: CommentsConnection } | null };
  errors?: Array<{ message: string }>;
};

type State = { lastSyncedAt: string | null };

type Args = { full: boolean; teams: string[]; skipIndex: boolean };

function info(message: string): void {
  process.stdout.write(`${message}\n`);
}

function warn(message: string): void {
  process.stderr.write(`${message}\n`);
}

function parseArgs(argv: string[]): Args {
  const args: Args = { full: false, teams: [], skipIndex: false };
  for (let i = 2; i < argv.length; i++) {
    const a = argv[i];
    if (a === '--') {
      continue;
    } else if (a === '--full') {
      args.full = true;
    } else if (a === '--no-index') {
      args.skipIndex = true;
    } else if (a === '--team') {
      const value = argv[++i];
      if (!value) throw new Error('--team requires a value');
      args.teams.push(value);
    } else if (a === '--help' || a === '-h') {
      printUsage();
      process.exit(0);
    } else {
      throw new Error(`Unknown argument: ${a}`);
    }
  }
  return args;
}

function printUsage(): void {
  const lines = [
    'Usage: tsx scripts/build-linear-issue-cache.ts [--full] [--team KEY]... [--no-index]',
    '',
    '  --full       Force a full re-fetch (ignore the saved updatedAt cursor).',
    '  --team KEY   Restrict to a Linear team by key. Repeat for multiple teams.',
    '  --no-index   Write markdown only; skip qmd collection add / update / embed.',
    '',
    'Requires LINEAR_API_KEY in the environment (Linear personal API key).',
  ];
  for (const line of lines) info(line);
}

function readState(): State {
  if (!existsSync(STATE_FILE)) return { lastSyncedAt: null };
  try {
    const parsed = JSON.parse(readFileSync(STATE_FILE, 'utf8')) as State;
    return { lastSyncedAt: parsed.lastSyncedAt ?? null };
  } catch {
    return { lastSyncedAt: null };
  }
}

function writeState(state: State): void {
  writeFileSync(STATE_FILE, `${JSON.stringify(state, null, 2)}\n`);
}

async function fetchPage(
  apiKey: string,
  cursor: string | null,
  filter: Record<string, unknown> | null,
): Promise<IssuesPage> {
  const response = await fetch(LINEAR_API_URL, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json', Authorization: apiKey },
    body: JSON.stringify({
      query: ISSUE_QUERY,
      variables: { cursor, first: PAGE_SIZE, commentFirst: COMMENT_PAGE_SIZE, filter },
    }),
  });
  if (!response.ok) {
    throw new Error(`Linear API HTTP ${response.status}: ${await response.text()}`);
  }
  const json = (await response.json()) as IssuesResponse;
  if (json.errors?.length) {
    throw new Error(`Linear GraphQL errors: ${json.errors.map((e) => e.message).join('; ')}`);
  }
  if (!json.data) throw new Error('Linear response missing data');
  return json.data.issues;
}

async function fetchRemainingComments(apiKey: string, issueId: string, startCursor: string): Promise<Comment[]> {
  const collected: Comment[] = [];
  let cursor: string | null = startCursor;
  while (cursor) {
    const response = await fetch(LINEAR_API_URL, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', Authorization: apiKey },
      body: JSON.stringify({
        query: COMMENTS_PAGE_QUERY,
        variables: { issueId, cursor, first: COMMENT_PAGE_SIZE },
      }),
    });
    if (!response.ok) {
      throw new Error(`Linear API HTTP ${response.status} (comments page): ${await response.text()}`);
    }
    const json = (await response.json()) as CommentsPageResponse;
    if (json.errors?.length) {
      throw new Error(`Linear GraphQL errors (comments page): ${json.errors.map((e) => e.message).join('; ')}`);
    }
    const connection = json.data?.issue?.comments;
    if (!connection) break;
    collected.push(...connection.nodes);
    cursor = connection.pageInfo.hasNextPage ? connection.pageInfo.endCursor : null;
  }
  return collected;
}

function buildFilter(args: Args, state: State): Record<string, unknown> | null {
  const parts: Record<string, unknown> = {};
  if (args.teams.length) parts['team'] = { key: { in: args.teams } };
  if (!args.full && state.lastSyncedAt) parts['updatedAt'] = { gt: state.lastSyncedAt };
  return Object.keys(parts).length ? parts : null;
}

function frontmatterScalar(value: unknown): string {
  if (value === null || value === undefined) return 'null';
  if (typeof value === 'number' || typeof value === 'boolean') return String(value);
  return JSON.stringify(String(value));
}

function frontmatterList(values: string[]): string {
  return `[${values.map((v) => JSON.stringify(v)).join(', ')}]`;
}

function safeFilename(identifier: string): string {
  return identifier.replace(/[^A-Za-z0-9_.-]/g, '_');
}

function renderComment(comment: Comment): string {
  const author = comment.user?.displayName ?? 'unknown';
  const edited = comment.editedAt ? ` (edited ${comment.editedAt})` : '';
  const body = comment.body.trim().length > 0 ? comment.body.trim() : '_(empty)_';
  return `### ${author} · ${comment.createdAt}${edited}\n\n${body}\n`;
}

function renderIssue(issue: Issue, comments: Comment[]): string {
  const labelNames = issue.labels.nodes.map((l) => l.name);
  const sortedComments = [...comments].sort((a, b) => a.createdAt.localeCompare(b.createdAt));
  const fields: Array<[string, unknown]> = [
    ['identifier', issue.identifier],
    ['title', issue.title],
    ['url', issue.url],
    ['state', issue.state?.name ?? null],
    ['state_type', issue.state?.type ?? null],
    ['team', issue.team?.key ?? null],
    ['team_name', issue.team?.name ?? null],
    ['project', issue.project?.name ?? null],
    ['cycle', issue.cycle?.number ?? null],
    ['priority', issue.priority],
    ['estimate', issue.estimate],
    ['assignee', issue.assignee?.displayName ?? null],
    ['creator', issue.creator?.displayName ?? null],
    ['parent', issue.parent?.identifier ?? null],
    ['branch_name', issue.branchName ?? null],
    ['comment_count', sortedComments.length],
    ['last_comment_at', sortedComments.at(-1)?.createdAt ?? null],
    ['created_at', issue.createdAt],
    ['updated_at', issue.updatedAt],
    ['completed_at', issue.completedAt],
    ['canceled_at', issue.canceledAt],
    ['archived_at', issue.archivedAt],
  ];
  const lines: string[] = ['---'];
  for (const [key, value] of fields) lines.push(`${key}: ${frontmatterScalar(value)}`);
  lines.push(`labels: ${frontmatterList(labelNames)}`);
  lines.push('---', '');
  lines.push(`# ${issue.identifier}: ${issue.title}`, '');
  const description = issue.description?.trim();
  lines.push(description && description.length > 0 ? description : '_No description._');
  lines.push('');
  if (sortedComments.length > 0) {
    lines.push('## Comments', '');
    for (const comment of sortedComments) {
      lines.push(renderComment(comment));
    }
  }
  return lines.join('\n');
}

function ensureQmd(): boolean {
  return spawnSync('qmd', ['--version'], { stdio: 'ignore' }).status === 0;
}

function ensureCollection(): void {
  const list = execFileSync('qmd', ['collection', 'list'], { encoding: 'utf8' });
  if (list.includes(COLLECTION_NAME)) return;
  execFileSync('qmd', ['collection', 'add', ISSUES_DIR, '--name', COLLECTION_NAME], { stdio: 'inherit' });
}

function reindex(): void {
  info('[qmd] update');
  execFileSync('qmd', ['update'], { stdio: 'inherit' });
  info('[qmd] embed');
  execFileSync('qmd', ['embed'], { stdio: 'inherit' });
}

async function main(): Promise<void> {
  const args = parseArgs(process.argv);
  const apiKey = process.env['LINEAR_API_KEY'];
  if (!apiKey) {
    warn('LINEAR_API_KEY is not set. Create one at https://linear.app/settings/api.');
    process.exit(1);
  }

  mkdirSync(ISSUES_DIR, { recursive: true });
  const state = readState();
  const filter = buildFilter(args, state);

  if (args.full) info('Full sync requested.');
  else if (state.lastSyncedAt) info(`Incremental sync since ${state.lastSyncedAt}.`);
  else info('No prior sync — fetching all issues.');
  if (args.teams.length) info(`Restricted to teams: ${args.teams.join(', ')}.`);

  let cursor: string | null = null;
  let total = 0;
  let extraCommentPages = 0;
  let mostRecentUpdate = state.lastSyncedAt;
  while (true) {
    const page: IssuesPage = await fetchPage(apiKey, cursor, filter);
    for (const issue of page.nodes) {
      const comments: Comment[] = [...issue.comments.nodes];
      if (issue.comments.pageInfo.hasNextPage && issue.comments.pageInfo.endCursor) {
        const more = await fetchRemainingComments(apiKey, issue.id, issue.comments.pageInfo.endCursor);
        comments.push(...more);
        extraCommentPages++;
      }
      const file = path.join(ISSUES_DIR, `${safeFilename(issue.identifier)}.md`);
      writeFileSync(file, renderIssue(issue, comments));
      if (!mostRecentUpdate || issue.updatedAt > mostRecentUpdate) {
        mostRecentUpdate = issue.updatedAt;
      }
    }
    total += page.nodes.length;
    process.stdout.write(`\rFetched ${total} issues...`);
    if (!page.pageInfo.hasNextPage || !page.pageInfo.endCursor) break;
    cursor = page.pageInfo.endCursor;
  }
  process.stdout.write('\n');
  if (extraCommentPages > 0) {
    info(`Paginated comments for ${extraCommentPages} issue(s) with > ${COMMENT_PAGE_SIZE} comments.`);
  }

  writeState({ lastSyncedAt: mostRecentUpdate });
  info(`Wrote ${total} issue files to ${path.relative(REPO_ROOT, ISSUES_DIR)}.`);

  if (args.skipIndex) {
    info('--no-index: skipping qmd reindex.');
    return;
  }
  if (!ensureQmd()) {
    warn('qmd CLI not found on PATH. Install with: bun install -g @tobilu/qmd');
    process.exit(1);
  }
  ensureCollection();
  reindex();
  info(`Search with: qmd query -c ${COLLECTION_NAME} "your question"`);
}

main().catch((err) => {
  warn(err instanceof Error ? (err.stack ?? err.message) : String(err));
  process.exit(1);
});
