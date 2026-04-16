export function classifyParityScenarioState(
  scenario: { status: string },
  paritySpec: { proxyRequest?: { documentPath?: string | null } } | null | undefined,
): 'ready-for-comparison' | 'captured-awaiting-proxy-request' | 'planned-with-proxy-request' | 'planned';
