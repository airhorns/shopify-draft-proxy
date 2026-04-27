type JsonRecord = Record<string, unknown>;

export type ProductMutationSeed = {
  id: string;
  title: string;
  handle: string;
  status: string;
  vendor: string | null;
  productType: string | null;
};

export type AccessDeniedBlocker = {
  operationName: string;
  message: string;
  requiredAccess: string;
  errorCode: string;
};

export type WriteScopeBlockerNoteInput = {
  title: string;
  whatFailed: string;
  operations: string[];
  blocker: AccessDeniedBlocker;
  whyBlocked: string;
  completedSteps: string[];
  recommendedNextStep: string;
};

export function pickProductMutationSeed(payload: unknown): ProductMutationSeed {
  const productEdges = readPath(payload, ['data', 'products', 'edges']);
  if (Array.isArray(productEdges)) {
    for (const edge of productEdges) {
      const node = readRecordProperty(edge, 'node');
      if (
        typeof node?.['id'] === 'string' &&
        typeof node?.['title'] === 'string' &&
        typeof node?.['handle'] === 'string' &&
        typeof node?.['status'] === 'string'
      ) {
        return {
          id: node['id'],
          title: node['title'],
          handle: node['handle'],
          status: node['status'],
          vendor: typeof node['vendor'] === 'string' ? node['vendor'] : null,
          productType: typeof node['productType'] === 'string' ? node['productType'] : null,
        };
      }
    }
  }

  throw new Error('Could not find a sample product from ProductCatalogPage capture');
}

export function parseAccessDeniedErrors(result: unknown): AccessDeniedBlocker[] {
  const errors = readPath(result, ['payload', 'errors']);
  const blockers: AccessDeniedBlocker[] = [];

  for (const error of Array.isArray(errors) ? errors : []) {
    const extensions = readRecordProperty(error, 'extensions');
    if (extensions?.['code'] !== 'ACCESS_DENIED') {
      continue;
    }

    const errorPath = isRecord(error) ? error['path'] : undefined;
    blockers.push({
      operationName:
        Array.isArray(errorPath) && typeof errorPath[0] === 'string'
          ? errorPath[0]
          : (readStringProperty(error, 'field') ?? 'unknown'),
      message: readStringProperty(error, 'message') ?? 'Access denied',
      requiredAccess: typeof extensions['requiredAccess'] === 'string' ? extensions['requiredAccess'] : 'unknown',
      errorCode: typeof extensions['code'] === 'string' ? extensions['code'] : 'ACCESS_DENIED',
    });
  }

  return blockers;
}

export function parseWriteScopeBlocker(result: unknown): AccessDeniedBlocker | null {
  return parseAccessDeniedErrors(result)[0] ?? null;
}

export function renderWriteScopeBlockerNote({
  title,
  whatFailed,
  operations,
  blocker,
  whyBlocked,
  completedSteps,
  recommendedNextStep,
}: WriteScopeBlockerNoteInput): string {
  const operationLines = operations.map((operation) => `- \`${operation}\``);
  const completedLines = completedSteps.map((step, index) => `${index + 1}. ${step}`);

  return [
    `# ${title}`,
    '',
    '## What failed',
    '',
    whatFailed,
    '',
    ...operationLines,
    '',
    'Live probe still works, but the first mutation capture failed immediately on Shopify Admin GraphQL:',
    '',
    `- \`${blocker.errorCode}\``,
    `- required access: ${blocker.requiredAccess}`,
    '',
    'Observed error excerpt:',
    '',
    `> ${blocker.message}`,
    '',
    '## Why this blocks closure',
    '',
    whyBlocked,
    '',
    '## What was completed anyway',
    '',
    ...completedLines,
    '',
    '## Recommended next step',
    '',
    recommendedNextStep,
    '',
  ].join('\n');
}

function readPath(value: unknown, path: string[]): unknown {
  let current = value;
  for (const key of path) {
    if (!isRecord(current)) {
      return undefined;
    }
    current = current[key];
  }
  return current;
}

function readRecordProperty(value: unknown, key: string): JsonRecord | null {
  if (!isRecord(value)) {
    return null;
  }

  const candidate = value[key];
  return isRecord(candidate) ? candidate : null;
}

function readStringProperty(value: unknown, key: string): string | null {
  if (!isRecord(value)) {
    return null;
  }

  const candidate = value[key];
  return typeof candidate === 'string' ? candidate : null;
}

function isRecord(value: unknown): value is JsonRecord {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}
