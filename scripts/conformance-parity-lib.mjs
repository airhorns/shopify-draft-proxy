export function classifyParityScenarioState(scenario, paritySpec) {
  const hasProxyRequest = !!(paritySpec?.proxyRequest?.documentPath);
  const hasBlocker = !!(paritySpec?.blocker?.kind);

  if (hasBlocker) {
    return hasProxyRequest ? 'blocked-with-proxy-request' : 'blocked';
  }

  if (scenario.status === 'captured') {
    return hasProxyRequest ? 'ready-for-comparison' : 'captured-awaiting-proxy-request';
  }

  return hasProxyRequest ? 'planned-with-proxy-request' : 'planned';
}
