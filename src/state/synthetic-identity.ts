let nextSyntheticId = 1;
let nextSyntheticTime = Date.parse('2024-01-01T00:00:00.000Z');

export function resetSyntheticIdentity(): void {
  nextSyntheticId = 1;
  nextSyntheticTime = Date.parse('2024-01-01T00:00:00.000Z');
}

export function makeSyntheticGid(resourceType: string): string {
  const id = nextSyntheticId;
  nextSyntheticId += 1;
  return `gid://shopify/${resourceType}/${id}`;
}

export function makeSyntheticTimestamp(): string {
  const current = new Date(nextSyntheticTime).toISOString();
  nextSyntheticTime += 1000;
  return current;
}
