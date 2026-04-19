export interface ProductMutationSeed {
  id: string;
  title: string;
  handle: string;
  status: string;
  vendor: string | null;
  productType: string | null;
}

export interface WriteScopeBlocker {
  operationName: string;
  message: string;
  requiredAccess: string;
  errorCode: string;
}

export function pickProductMutationSeed(payload: unknown): ProductMutationSeed;

export function parseWriteScopeBlocker(result: unknown): WriteScopeBlocker | null;

export function renderWriteScopeBlockerNote(input: {
  title: string;
  whatFailed: string;
  operations: string[];
  blocker: WriteScopeBlocker;
  whyBlocked: string;
  completedSteps: string[];
  recommendedNextStep: string;
}): string;
