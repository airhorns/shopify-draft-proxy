import { appendFileSync, readFileSync } from 'node:fs';
import { pathToFileURL } from 'node:url';

const defaultMarker = '<!-- shopify-draft-proxy-conformance-status -->';

interface GithubRequestInput {
  token: string;
  method?: string;
  body?: unknown;
}

interface GithubIssueComment {
  id: number;
  body: string | null;
  html_url: string;
}

interface UpsertPullRequestCommentInput {
  repository: string;
  issueNumber: number;
  token: string;
  marker?: string;
  body: string;
}

interface UpsertPullRequestCommentResult {
  action: 'created' | 'updated';
  url: string;
}

function parseArgs(argv: string[]): Map<string, string> {
  const args = new Map<string, string>();

  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === '--') {
      continue;
    }

    if (!arg?.startsWith('--')) {
      throw new Error(`Unexpected positional argument: ${arg}`);
    }

    const key = arg.slice(2);
    const next = argv[index + 1];
    if (!next || next.startsWith('--')) {
      args.set(key, 'true');
      continue;
    }

    args.set(key, next);
    index += 1;
  }

  return args;
}

function readPullRequestNumber(args: Map<string, string>): number {
  const fromArgs = args.get('pr-number');
  if (fromArgs) {
    return Number.parseInt(fromArgs, 10);
  }

  const fromEnv = process.env['PR_NUMBER'];
  if (fromEnv) {
    return Number.parseInt(fromEnv, 10);
  }

  const eventPath = process.env['GITHUB_EVENT_PATH'];
  if (eventPath) {
    const event = JSON.parse(readFileSync(eventPath, 'utf8')) as { pull_request?: { number?: number | string } };
    if (event.pull_request?.number) {
      return Number.parseInt(String(event.pull_request.number), 10);
    }
  }

  return Number.NaN;
}

async function githubRequest<T>(
  pathname: string,
  { token, method = 'GET', body = undefined }: GithubRequestInput,
): Promise<T> {
  const request: RequestInit = {
    method,
    headers: {
      accept: 'application/vnd.github+json',
      authorization: `Bearer ${token}`,
      'content-type': 'application/json',
      'x-github-api-version': '2022-11-28',
    },
  };

  if (body !== undefined) {
    request.body = JSON.stringify(body);
  }

  const response = await fetch(`https://api.github.com${pathname}`, request);

  if (!response.ok) {
    const responseBody = await response.text();
    throw new Error(`GitHub API ${method} ${pathname} failed with ${response.status}: ${responseBody}`);
  }

  return response.status === 204 ? (null as T) : ((await response.json()) as T);
}

async function listIssueComments({
  repository,
  issueNumber,
  token,
}: {
  repository: string;
  issueNumber: number;
  token: string;
}): Promise<GithubIssueComment[]> {
  const comments: GithubIssueComment[] = [];

  for (let page = 1; ; page += 1) {
    const pageComments = await githubRequest<GithubIssueComment[]>(
      `/repos/${repository}/issues/${issueNumber}/comments?per_page=100&page=${page}`,
      { token },
    );
    comments.push(...pageComments);

    if (pageComments.length < 100) {
      return comments;
    }
  }
}

export async function upsertPullRequestComment({
  repository,
  issueNumber,
  token,
  marker = defaultMarker,
  body,
}: UpsertPullRequestCommentInput): Promise<UpsertPullRequestCommentResult> {
  const comments = await listIssueComments({ repository, issueNumber, token });
  const existing = comments.find((comment) => typeof comment.body === 'string' && comment.body.includes(marker));

  if (existing) {
    const updated = await githubRequest<GithubIssueComment>(`/repos/${repository}/issues/comments/${existing.id}`, {
      token,
      method: 'PATCH',
      body: { body },
    });
    return { action: 'updated', url: updated.html_url };
  }

  const created = await githubRequest<GithubIssueComment>(`/repos/${repository}/issues/${issueNumber}/comments`, {
    token,
    method: 'POST',
    body: { body },
  });
  return { action: 'created', url: created.html_url };
}

function writeGithubOutputs(outputs: Record<string, string>): void {
  if (!process.env['GITHUB_OUTPUT']) {
    return;
  }

  const lines = Object.entries(outputs).map(([key, value]) => `${key}=${value}`);
  appendFileSync(process.env['GITHUB_OUTPUT'], `${lines.join('\n')}\n`);
}

function writeLine(message: string): void {
  process.stdout.write(`${message}\n`);
}

const invokedPath = process.argv[1];

if (invokedPath && import.meta.url === pathToFileURL(invokedPath).href) {
  const args = parseArgs(process.argv.slice(2));
  const repository = args.get('repository') ?? process.env['GITHUB_REPOSITORY'];
  const token = process.env['GITHUB_TOKEN'] ?? process.env['GH_TOKEN'];
  const bodyFile = args.get('body-file');
  const issueNumber = readPullRequestNumber(args);

  if (!repository) {
    throw new Error('GITHUB_REPOSITORY or --repository is required.');
  }
  if (!token) {
    throw new Error('GITHUB_TOKEN or GH_TOKEN is required.');
  }
  if (!bodyFile) {
    throw new Error('--body-file is required.');
  }
  if (!Number.isInteger(issueNumber)) {
    throw new Error('Pull request number is required via --pr-number, PR_NUMBER, or GITHUB_EVENT_PATH.');
  }

  const body = readFileSync(bodyFile, 'utf8');
  const result = await upsertPullRequestComment({
    repository,
    issueNumber,
    token,
    marker: args.get('marker') ?? defaultMarker,
    body,
  });

  writeGithubOutputs({ action: result.action, comment_url: result.url });
  writeLine(`${result.action} conformance status comment: ${result.url}`);
}
