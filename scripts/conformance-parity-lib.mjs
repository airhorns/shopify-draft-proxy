function hasComparisonContract(paritySpec) {
  return (
    paritySpec?.comparisonMode === 'captured-vs-proxy-request' &&
    Array.isArray(paritySpec?.comparisons) &&
    paritySpec.comparisons.length > 0
  );
}

export function classifyParityScenarioState(scenario, paritySpec) {
  if (scenario.status === 'planned') {
    return 'not-yet-implemented';
  }

  return hasComparisonContract(paritySpec) ? 'ready-for-comparison' : 'invalid-missing-comparison-contract';
}

function splitPath(path) {
  if (path === '$') {
    return [];
  }

  if (typeof path !== 'string' || !path.startsWith('$.')) {
    throw new Error(`Unsupported comparison path: ${path}`);
  }

  const parts = [];
  const pathPattern = /\.([A-Za-z0-9_$-]+)|\[(\d+)\]/g;
  let match;
  while ((match = pathPattern.exec(path)) !== null) {
    parts.push(match[1] ?? Number.parseInt(match[2], 10));
  }
  return parts;
}

export function getPathValue(value, path) {
  let current = value;
  for (const part of splitPath(path)) {
    if (current === null || typeof current !== 'object') {
      return undefined;
    }
    current = current[part];
  }
  return current;
}

function isPathCovered(path, allowedPaths) {
  return allowedPaths.some((allowedPath) => {
    if (path === allowedPath) {
      return true;
    }
    return path.startsWith(`${allowedPath}.`) || path.startsWith(`${allowedPath}[`);
  });
}

function childPath(parentPath, key) {
  return typeof key === 'number' ? `${parentPath}[${key}]` : `${parentPath}.${key}`;
}

function formatValue(value) {
  return JSON.stringify(value);
}

export function compareJson(expected, actual, options = {}) {
  const allowedDifferencePaths = options.allowedDifferencePaths ?? [];
  const differences = [];

  function compareAt(path, left, right) {
    if (isPathCovered(path, allowedDifferencePaths)) {
      return;
    }

    if (Object.is(left, right)) {
      return;
    }

    if (left === null || right === null || typeof left !== 'object' || typeof right !== 'object') {
      differences.push(`${path}: expected ${formatValue(left)}, received ${formatValue(right)}`);
      return;
    }

    if (Array.isArray(left) || Array.isArray(right)) {
      if (!Array.isArray(left) || !Array.isArray(right)) {
        differences.push(`${path}: expected ${Array.isArray(left) ? 'array' : typeof left}, received ${Array.isArray(right) ? 'array' : typeof right}`);
        return;
      }

      if (left.length !== right.length) {
        differences.push(`${path}: expected array length ${left.length}, received ${right.length}`);
      }

      for (let index = 0; index < Math.min(left.length, right.length); index += 1) {
        compareAt(childPath(path, index), left[index], right[index]);
      }
      return;
    }

    const leftKeys = Object.keys(left).sort();
    const rightKeys = Object.keys(right).sort();
    const keys = new Set([...leftKeys, ...rightKeys]);

    for (const key of keys) {
      const nextPath = childPath(path, key);
      if (!(key in left)) {
        if (!isPathCovered(nextPath, allowedDifferencePaths)) {
          differences.push(`${nextPath}: extra field ${formatValue(right[key])}`);
        }
        continue;
      }
      if (!(key in right)) {
        if (!isPathCovered(nextPath, allowedDifferencePaths)) {
          differences.push(`${nextPath}: missing field, expected ${formatValue(left[key])}`);
        }
        continue;
      }
      compareAt(nextPath, left[key], right[key]);
    }
  }

  compareAt('$', expected, actual);

  for (const mustMatchPath of options.mustMatchPaths ?? []) {
    const expectedValue = getPathValue(expected, mustMatchPath);
    const actualValue = getPathValue(actual, mustMatchPath);
    compareJson(expectedValue, actualValue).differences.forEach((difference) => {
      differences.push(`${mustMatchPath} must match: ${difference}`);
    });
  }

  return {
    pass: differences.length === 0,
    differences,
  };
}
