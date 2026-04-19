function hasProxyRequest(paritySpec) {
  return !!(paritySpec?.proxyRequest?.documentPath);
}

function hasComparisonContract(paritySpec) {
  return validateComparisonContract(paritySpec?.comparison).length === 0;
}

function isKnownMatcher(matcher) {
  return (
    matcher === 'any-string' ||
    matcher === 'non-empty-string' ||
    matcher === 'any-number' ||
    matcher === 'iso-timestamp' ||
    /^shopify-gid:([A-Za-z][A-Za-z0-9]*)$/.test(matcher)
  );
}

export function validateComparisonContract(comparison) {
  const errors = [];

  if (comparison?.mode !== 'strict-json') {
    errors.push('Comparison contract mode must be `strict-json`.');
  }

  if (!Array.isArray(comparison?.allowedDifferences)) {
    errors.push('Comparison contract must declare an `allowedDifferences` array.');
    return errors;
  }

  for (const [index, rule] of comparison.allowedDifferences.entries()) {
    const label = `allowedDifferences[${index}]`;
    if (typeof rule?.path !== 'string' || rule.path.length === 0) {
      errors.push(`${label} must declare a non-empty JSON path.`);
    }

    if (typeof rule?.reason !== 'string' || rule.reason.length === 0) {
      errors.push(`${label} must document why the difference is nondeterministic.`);
    }

    const hasMatcher = typeof rule?.matcher === 'string';
    const isIgnored = rule?.ignore === true;
    if (hasMatcher === isIgnored) {
      errors.push(`${label} must declare exactly one of \`matcher\` or \`ignore: true\`.`);
    }

    if (hasMatcher && !isKnownMatcher(rule.matcher)) {
      errors.push(`${label} declares unknown matcher \`${rule.matcher}\`.`);
    }
  }

  return errors;
}

export function classifyParityScenarioState(scenario, paritySpec) {
  if (scenario.status === 'captured') {
    if (!hasProxyRequest(paritySpec)) {
      return 'captured-awaiting-proxy-request';
    }

    return hasComparisonContract(paritySpec) ? 'ready-for-comparison' : 'captured-awaiting-comparison-contract';
  }

  return hasProxyRequest(paritySpec) ? 'planned-with-proxy-request' : 'planned';
}

export const parityStatusNote =
  'readyForComparison now means a captured scenario has a proxy request and an explicit strict-json comparison contract. capturedAwaitingComparisonContract scenarios are not parity failures; they are captured scenarios that need scoped nondeterminism rules before payload comparison is meaningful.';

export function summarizeParityResults(results) {
  const readyForComparison = results.filter((result) => result.state === 'ready-for-comparison').length;
  const capturedAwaitingComparisonContract = results.filter((result) => result.state === 'captured-awaiting-comparison-contract').length;
  const capturedAwaitingProxyRequest = results.filter((result) => result.state === 'captured-awaiting-proxy-request').length;
  const plannedWithProxyRequest = results.filter((result) => result.state === 'planned-with-proxy-request').length;
  const planned = results.filter((result) => result.state === 'planned').length;

  return {
    readyForComparison,
    pending: results.length - readyForComparison,
    statusCounts: {
      readyForComparison,
      capturedAwaitingComparisonContract,
      capturedAwaitingProxyRequest,
      plannedWithProxyRequest,
      planned,
    },
    statusNote: parityStatusNote,
  };
}

function isPlainObject(value) {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function appendPath(path, segment) {
  if (typeof segment === 'number') {
    return `${path}[${segment}]`;
  }

  if (/^[A-Za-z_$][\w$]*$/.test(segment)) {
    return `${path}.${segment}`;
  }

  return `${path}[${JSON.stringify(segment)}]`;
}

function parsePath(path) {
  if (typeof path !== 'string' || !path.startsWith('$')) {
    throw new Error(`Invalid comparison path: ${String(path)}`);
  }

  const segments = [];
  let index = 1;
  while (index < path.length) {
    if (path[index] === '.') {
      index += 1;
      const match = /^[A-Za-z_$][\w$]*/.exec(path.slice(index));
      if (!match) {
        throw new Error(`Invalid comparison path segment in: ${path}`);
      }
      segments.push(match[0]);
      index += match[0].length;
      continue;
    }

    if (path[index] === '[') {
      const closeIndex = path.indexOf(']', index);
      if (closeIndex === -1) {
        throw new Error(`Invalid comparison path segment in: ${path}`);
      }
      const raw = path.slice(index + 1, closeIndex);
      if (raw === '*') {
        segments.push('*');
      } else if (/^\d+$/.test(raw)) {
        segments.push(Number.parseInt(raw, 10));
      } else {
        segments.push(JSON.parse(raw));
      }
      index = closeIndex + 1;
      continue;
    }

    throw new Error(`Invalid comparison path segment in: ${path}`);
  }

  return segments;
}

function pathMatches(ruleSegments, pathSegments) {
  if (ruleSegments.length !== pathSegments.length) {
    return false;
  }

  return ruleSegments.every((segment, index) => segment === '*' || segment === pathSegments[index]);
}

function makeRule(rawRule) {
  return {
    ...rawRule,
    segments: parsePath(rawRule.path),
  };
}

function findRule(rules, pathSegments) {
  return rules.find((rule) => pathMatches(rule.segments, pathSegments)) ?? null;
}

function isIsoTimestamp(value) {
  if (typeof value !== 'string') {
    return false;
  }

  const parsed = Date.parse(value);
  return Number.isFinite(parsed) && new Date(parsed).toISOString() === value;
}

function isShopifyGid(value, resourceType) {
  return typeof value === 'string' && value.startsWith(`gid://shopify/${resourceType}/`) && value.length > `gid://shopify/${resourceType}/`.length;
}

function matcherAccepts(matcher, expected, actual) {
  if (matcher === 'any-string') {
    return typeof expected === 'string' && typeof actual === 'string';
  }

  if (matcher === 'non-empty-string') {
    return typeof expected === 'string' && expected.length > 0 && typeof actual === 'string' && actual.length > 0;
  }

  if (matcher === 'any-number') {
    return typeof expected === 'number' && typeof actual === 'number';
  }

  if (matcher === 'iso-timestamp') {
    return isIsoTimestamp(expected) && isIsoTimestamp(actual);
  }

  const gidMatch = /^shopify-gid:([A-Za-z][A-Za-z0-9]*)$/.exec(matcher);
  if (gidMatch) {
    return isShopifyGid(expected, gidMatch[1]) && isShopifyGid(actual, gidMatch[1]);
  }

  throw new Error(`Unknown comparison matcher: ${matcher}`);
}

function diffValues(expected, actual, path, pathSegments, rules, differences) {
  const rule = findRule(rules, pathSegments);
  if (rule?.ignore === true) {
    return;
  }

  if (Object.is(expected, actual)) {
    return;
  }

  if (rule?.matcher && matcherAccepts(rule.matcher, expected, actual)) {
    return;
  }

  if (Array.isArray(expected) || Array.isArray(actual)) {
    if (!Array.isArray(expected) || !Array.isArray(actual)) {
      differences.push({ path, message: 'Expected both values to be arrays.', expected, actual });
      return;
    }

    if (expected.length !== actual.length) {
      differences.push({ path, message: `Array length differs: expected ${expected.length}, received ${actual.length}.`, expected, actual });
      return;
    }

    for (let index = 0; index < expected.length; index += 1) {
      diffValues(expected[index], actual[index], appendPath(path, index), [...pathSegments, index], rules, differences);
    }
    return;
  }

  if (isPlainObject(expected) || isPlainObject(actual)) {
    if (!isPlainObject(expected) || !isPlainObject(actual)) {
      differences.push({ path, message: 'Expected both values to be objects.', expected, actual });
      return;
    }

    const keys = new Set([...Object.keys(expected), ...Object.keys(actual)]);
    for (const key of [...keys].sort()) {
      const childPath = appendPath(path, key);
      const childSegments = [...pathSegments, key];
      const childRule = findRule(rules, childSegments);

      if (childRule?.ignore === true) {
        continue;
      }

      if (!Object.prototype.hasOwnProperty.call(expected, key)) {
        differences.push({ path: childPath, message: 'Unexpected field in actual payload.', expected: undefined, actual: actual[key] });
        continue;
      }

      if (!Object.prototype.hasOwnProperty.call(actual, key)) {
        differences.push({ path: childPath, message: 'Missing field in actual payload.', expected: expected[key], actual: undefined });
        continue;
      }

      diffValues(expected[key], actual[key], childPath, childSegments, rules, differences);
    }
    return;
  }

  differences.push({ path, message: 'Value differs.', expected, actual });
}

export function compareJsonPayloads(expected, actual, comparison = {}) {
  const allowedDifferences = Array.isArray(comparison.allowedDifferences) ? comparison.allowedDifferences : [];
  const rules = allowedDifferences.map(makeRule);
  const differences = [];

  diffValues(expected, actual, '$', [], rules, differences);

  return {
    ok: differences.length === 0,
    differences,
  };
}
