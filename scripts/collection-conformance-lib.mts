type JsonRecord = Record<string, unknown>;

export type CollectionCaptureSeed = {
  id: string;
  title: string;
  handle: string;
};

export function pickCollectionCaptureSeed(payload: unknown): CollectionCaptureSeed {
  const directEdges = readPath(payload, ['data', 'product', 'collections', 'edges']);
  if (Array.isArray(directEdges)) {
    for (const edge of directEdges) {
      const node = readRecordProperty(edge, 'node');
      if (
        typeof node?.['id'] === 'string' &&
        typeof node?.['title'] === 'string' &&
        typeof node?.['handle'] === 'string'
      ) {
        return {
          id: node['id'],
          title: node['title'],
          handle: node['handle'],
        };
      }
    }
  }

  const productEdges = readPath(payload, ['data', 'products', 'edges']);
  if (Array.isArray(productEdges)) {
    for (const productEdge of productEdges) {
      const collectionEdges = readPath(productEdge, ['node', 'collections', 'edges']);
      if (!Array.isArray(collectionEdges)) {
        continue;
      }
      for (const collectionEdge of collectionEdges) {
        const node = readRecordProperty(collectionEdge, 'node');
        if (
          typeof node?.['id'] === 'string' &&
          typeof node?.['title'] === 'string' &&
          typeof node?.['handle'] === 'string'
        ) {
          return {
            id: node['id'],
            title: node['title'],
            handle: node['handle'],
          };
        }
      }
    }
  }

  const topLevelCollectionEdges = readPath(payload, ['data', 'collections', 'edges']);
  if (Array.isArray(topLevelCollectionEdges)) {
    for (const collectionEdge of topLevelCollectionEdges) {
      const node = readRecordProperty(collectionEdge, 'node');
      if (
        typeof node?.['id'] === 'string' &&
        typeof node?.['title'] === 'string' &&
        typeof node?.['handle'] === 'string'
      ) {
        return {
          id: node['id'],
          title: node['title'],
          handle: node['handle'],
        };
      }
    }
  }

  throw new Error('Could not find a sample collection from ProductDetail capture');
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

function isRecord(value: unknown): value is JsonRecord {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}
