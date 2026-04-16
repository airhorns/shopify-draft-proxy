export function classifyParityScenarioState(scenario, paritySpec) {
  const hasProxyRequest = !!(paritySpec?.proxyRequest?.documentPath);

  if (scenario.status === 'captured') {
    return hasProxyRequest ? 'ready-for-comparison' : 'captured-awaiting-proxy-request';
  }

  return hasProxyRequest ? 'planned-with-proxy-request' : 'planned';
}
