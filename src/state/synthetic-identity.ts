export interface SyntheticIdentityStateDumpV1 {
  version: 1;
  nextSyntheticId: number;
  nextSyntheticTimestamp: string;
}

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

  dumpState(): SyntheticIdentityStateDumpV1 {
    return {
      version: 1,
      nextSyntheticId: this.nextSyntheticId,
      nextSyntheticTimestamp: new Date(this.nextSyntheticTime).toISOString(),
    };
  }

  restoreState(dump: SyntheticIdentityStateDumpV1): void {
    if (dump.version !== 1) {
      throw new Error(`Unsupported synthetic identity state dump version: ${String(dump.version)}`);
    }

    const nextSyntheticTime = Date.parse(dump.nextSyntheticTimestamp);
    if (!Number.isInteger(dump.nextSyntheticId) || dump.nextSyntheticId < 1 || Number.isNaN(nextSyntheticTime)) {
      throw new Error('Invalid synthetic identity state dump.');
    }

    this.nextSyntheticId = dump.nextSyntheticId;
    this.nextSyntheticTime = nextSyntheticTime;
  }
}

export function isProxySyntheticGid(value: string): boolean {
  return value.startsWith('gid://shopify/') && value.includes('?shopify-draft-proxy=synthetic');
}
