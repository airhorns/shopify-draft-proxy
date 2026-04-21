// @ts-nocheck
export function pickProductMutationSeed(payload: any): any {
  const productEdges = payload?.data?.products?.edges;
  if (Array.isArray(productEdges)) {
    for (const edge of productEdges) {
      const node = edge?.node;
      if (
        typeof node?.id === 'string' &&
        typeof node?.title === 'string' &&
        typeof node?.handle === 'string' &&
        typeof node?.status === 'string'
      ) {
        return {
          id: node.id,
          title: node.title,
          handle: node.handle,
          status: node.status,
          vendor: typeof node?.vendor === 'string' ? node.vendor : null,
          productType: typeof node?.productType === 'string' ? node.productType : null,
        };
      }
    }
  }

  throw new Error('Could not find a sample product from ProductCatalogPage capture');
}

export function parseAccessDeniedErrors(result: any): any[] {
  const errors = Array.isArray(result?.payload?.errors) ? result.payload.errors : [];
  const blockers = [];

  for (const error of errors) {
    if (error?.extensions?.code !== 'ACCESS_DENIED') {
      continue;
    }

    blockers.push({
      operationName:
        Array.isArray(error?.path) && typeof error.path[0] === 'string'
          ? error.path[0]
          : typeof error?.field === 'string'
            ? error.field
            : 'unknown',
      message: typeof error?.message === 'string' ? error.message : 'Access denied',
      requiredAccess:
        typeof error?.extensions?.requiredAccess === 'string' ? error.extensions.requiredAccess : 'unknown',
      errorCode: error.extensions.code,
    });
  }

  return blockers;
}

export function parseWriteScopeBlocker(result: any): any {
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
}: any): string {
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
