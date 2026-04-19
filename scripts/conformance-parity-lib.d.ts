export function classifyParityScenarioState(
  scenario: { status: string },
  paritySpec:
    | {
        proxyRequest?: { documentPath?: string | null };
        comparison?: { mode?: string | null; allowedDifferences?: unknown[] | null };
      }
    | null
    | undefined,
):
  | 'ready-for-comparison'
  | 'captured-awaiting-proxy-request'
  | 'captured-awaiting-comparison-contract'
  | 'planned-with-proxy-request'
  | 'planned';

export function compareJsonPayloads(
  expected: unknown,
  actual: unknown,
  comparison?: {
    allowedDifferences?: Array<{
      path: string;
      ignore?: boolean;
      matcher?: 'any-string' | 'non-empty-string' | 'any-number' | 'iso-timestamp' | `shopify-gid:${string}`;
      reason?: string;
    }>;
  },
): { ok: boolean; differences: Array<{ path: string; message: string; expected: unknown; actual: unknown }> };

export function validateComparisonContract(comparison: unknown): string[];

export const parityStatusNote: string;

export function summarizeParityResults(
  results: Array<{
    state:
      | 'ready-for-comparison'
      | 'captured-awaiting-proxy-request'
      | 'captured-awaiting-comparison-contract'
      | 'planned-with-proxy-request'
      | 'planned';
  }>,
): {
  readyForComparison: number;
  pending: number;
  statusCounts: {
    readyForComparison: number;
    capturedAwaitingComparisonContract: number;
    capturedAwaitingProxyRequest: number;
    plannedWithProxyRequest: number;
    planned: number;
  };
  statusNote: string;
};
