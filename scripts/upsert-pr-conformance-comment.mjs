import { appendFileSync, readFileSync } from 'node:fs';
import { pathToFileURL } from 'node:url';

const defaultMarker = '<!-- shopify-draft-proxy-conformance-status -->';

function parseArgs(argv) {
  const args = new Map();

  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === '--') {
      continue;
    }

    if (!arg.startsWith('--')) {
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

function readPullRequestNumber(args) {
  const fromArgs = args.get('pr-number');
  if (fromArgs) {
    return Number.parseInt(fromArgs, 10);
  }

  const fromEnv = process.env.PR_NUMBER;
  if (fromEnv) {
    return Number.parseInt(fromEnv, 10);
  }

  if (process.env.GITHUB_EVENT_PATH) {
    const event = JSON.parse(readFileSync(process.env.GITHUB_EVENT_PATH, 'utf8'));
    if (event.pull_request?.number) {
      return Number.parseInt(String(event.pull_request.number), 10);
    }
  }

  return Number.NaN;
}

async function githubRequest(pathname, { token, method = 'GET', body = undefined }) {
  const request = {
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

  return response.status === 204 ? null : response.json();
}

async function listIssueComments({ repository, issueNumber, token }) {
  const comments = [];

  for (let page = 1; ; page += 1) {
    const pageComments = await githubRequest(
      `/repos/${repository}/issues/${issueNumber}/comments?per_page=100&page=${page}`,
      { token },
    );
    comments.push(...pageComments);

    if (pageComments.length < 100) {
      return comments;
    }
  }
}

export async function upsertPullRequestComment({ repository, issueNumber, token, marker = defaultMarker, body }) {
  const comments = await listIssueComments({ repository, issueNumber, token });
  const existing = comments.find((comment) => typeof comment.body === 'string' && comment.body.includes(marker));

  if (existing) {
    const updated = await githubRequest(`/repos/${repository}/issues/comments/${existing.id}`, {
      token,
      method: 'PATCH',
      body: { body },
    });
    return { action: 'updated', url: updated.html_url };
  }

  const created = await githubRequest(`/repos/${repository}/issues/${issueNumber}/comments`, {
    token,
    method: 'POST',
    body: { body },
  });
  return { action: 'created', url: created.html_url };
}

function writeGithubOutputs(outputs) {
  if (!process.env.GITHUB_OUTPUT) {
    return;
  }

  const lines = Object.entries(outputs).map(([key, value]) => `${key}=${value}`);
  appendFileSync(process.env.GITHUB_OUTPUT, `${lines.join('\n')}\n`);
}

function writeLine(message) {
  process.stdout.write(`${message}\n`);
}

if (import.meta.url === pathToFileURL(process.argv[1]).href) {
  const args = parseArgs(process.argv.slice(2));
  const repository = args.get('repository') ?? process.env.GITHUB_REPOSITORY;
  const token = process.env.GITHUB_TOKEN ?? process.env.GH_TOKEN;
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
