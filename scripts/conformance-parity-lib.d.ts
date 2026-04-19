export function classifyParityScenarioState(
  scenario: { status: string },
  paritySpec:
    | {
        comparisonMode?: string;
        comparisons?: unknown[];
      }
    | null
    | undefined,
): 'ready-for-comparison' | 'invalid-missing-comparison-contract' | 'not-yet-implemented';

export function getPathValue(value: unknown, path: string): unknown;

export function compareJson(
  expected: unknown,
  actual: unknown,
  options?: {
    allowedDifferencePaths?: string[];
    mustMatchPaths?: string[];
  },
): {
  pass: boolean;
  differences: string[];
};
