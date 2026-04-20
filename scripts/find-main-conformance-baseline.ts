import { appendFileSync } from 'node:fs';
import { pathToFileURL } from 'node:url';

const defaultArtifactName = 'conformance-status-main';
const defaultWorkflow = 'ci.yml';
const defaultBranch = 'main';

interface FindMainConformanceBaselineInput {
  repository: string;
  token: string;
  workflow?: string;
  branch?: string;
  artifactName?: string;
}

interface FoundBaseline {
  found: true;
  artifactId: string;
  artifactName: string;
  runId: string;
  runUrl: string;
  headSha: string;
}

interface MissingBaseline {
  found: false;
}

type BaselineSearchResult = FoundBaseline | MissingBaseline;

interface WorkflowRun {
  id: number | string;
  html_url: string;
  head_sha: string;
}

interface WorkflowRunsResponse {
  workflow_runs?: WorkflowRun[];
}

interface Artifact {
  id: number | string;
  name: string;
  expired: boolean;
}

interface ArtifactsResponse {
  artifacts?: Artifact[];
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

async function githubRequest<T>(pathname: string, token: string): Promise<T> {
  const response = await fetch(`https://api.github.com${pathname}`, {
    headers: {
      accept: 'application/vnd.github+json',
      authorization: `Bearer ${token}`,
      'x-github-api-version': '2022-11-28',
    },
  });

  if (!response.ok) {
    const body = await response.text();
    throw new Error(`GitHub API ${pathname} failed with ${response.status}: ${body}`);
  }

  return (await response.json()) as T;
}

export async function findMainConformanceBaseline({
  repository,
  token,
  workflow = defaultWorkflow,
  branch = defaultBranch,
  artifactName = defaultArtifactName,
}: FindMainConformanceBaselineInput): Promise<BaselineSearchResult> {
  const workflowRuns = await githubRequest<WorkflowRunsResponse>(
    `/repos/${repository}/actions/workflows/${encodeURIComponent(workflow)}/runs?branch=${encodeURIComponent(branch)}&event=push&status=success&per_page=20`,
    token,
  );

  for (const run of workflowRuns.workflow_runs ?? []) {
    const artifacts = await githubRequest<ArtifactsResponse>(
      `/repos/${repository}/actions/runs/${run.id}/artifacts?per_page=100`,
      token,
    );
    const artifact = (artifacts.artifacts ?? []).find((candidate) => {
      return candidate.name === artifactName && candidate.expired === false;
    });

    if (artifact) {
      return {
        found: true,
        artifactId: String(artifact.id),
        artifactName: artifact.name,
        runId: String(run.id),
        runUrl: run.html_url,
        headSha: run.head_sha,
      };
    }
  }

  return { found: false };
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

  if (!repository) {
    throw new Error('GITHUB_REPOSITORY or --repository is required.');
  }
  if (!token) {
    throw new Error('GITHUB_TOKEN or GH_TOKEN is required.');
  }

  const result = await findMainConformanceBaseline({
    repository,
    token,
    workflow: args.get('workflow') ?? defaultWorkflow,
    branch: args.get('branch') ?? defaultBranch,
    artifactName: args.get('artifact-name') ?? defaultArtifactName,
  });

  writeGithubOutputs({
    found: result.found ? 'true' : 'false',
    artifact_id: result.found ? result.artifactId : '',
    run_id: result.found ? result.runId : '',
    head_sha: result.found ? result.headSha : '',
  });

  if (result.found) {
    writeLine(`found ${result.artifactName} from run ${result.runId} (${result.headSha})`);
  } else {
    writeLine('no main conformance baseline artifact found');
  }
}
