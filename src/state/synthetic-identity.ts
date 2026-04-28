export class SyntheticIdentityRegistry {
  private nextSyntheticId = 1;
  private nextSyntheticTime = Date.parse('2024-01-01T00:00:00.000Z');

  reset(): void {
    this.nextSyntheticId = 1;
    this.nextSyntheticTime = Date.parse('2024-01-01T00:00:00.000Z');
  }

  makeSyntheticGid(resourceType: string): string {
    const id = this.nextSyntheticId;
    this.nextSyntheticId += 1;
    return `gid://shopify/${resourceType}/${id}`;
  }

  makeProxySyntheticGid(resourceType: string): string {
    const id = this.nextSyntheticId;
    this.nextSyntheticId += 1;
    return `gid://shopify/${resourceType}/${id}?shopify-draft-proxy=synthetic`;
  }

  makeSyntheticTimestamp(): string {
    const current = new Date(this.nextSyntheticTime).toISOString();
    this.nextSyntheticTime += 1000;
    return current;
  }
}

export function isProxySyntheticGid(value: string): boolean {
  return value.startsWith('gid://shopify/') && value.includes('?shopify-draft-proxy=synthetic');
}
