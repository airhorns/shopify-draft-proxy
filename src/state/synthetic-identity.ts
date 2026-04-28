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

let activeSyntheticIdentity: SyntheticIdentityRegistry | null = null;

export function getCurrentSyntheticIdentity(): SyntheticIdentityRegistry {
  if (!activeSyntheticIdentity) {
    throw new Error(
      'No DraftProxy synthetic identity registry is active. Process requests through a DraftProxy instance.',
    );
  }
  return activeSyntheticIdentity;
}

export function runWithSyntheticIdentity<T>(identity: SyntheticIdentityRegistry, callback: () => T): T {
  const previousIdentity = activeSyntheticIdentity;
  activeSyntheticIdentity = identity;

  try {
    const result = callback();

    if (result instanceof Promise) {
      return result.finally(() => {
        activeSyntheticIdentity = previousIdentity;
      }) as T;
    }

    activeSyntheticIdentity = previousIdentity;
    return result;
  } catch (error) {
    activeSyntheticIdentity = previousIdentity;
    throw error;
  }
}

export function resetSyntheticIdentity(): void {
  getCurrentSyntheticIdentity().reset();
}

export function makeSyntheticGid(resourceType: string): string {
  return getCurrentSyntheticIdentity().makeSyntheticGid(resourceType);
}

export function makeProxySyntheticGid(resourceType: string): string {
  return getCurrentSyntheticIdentity().makeProxySyntheticGid(resourceType);
}

export function isProxySyntheticGid(value: string): boolean {
  return value.startsWith('gid://shopify/') && value.includes('?shopify-draft-proxy=synthetic');
}

export function makeSyntheticTimestamp(): string {
  return getCurrentSyntheticIdentity().makeSyntheticTimestamp();
}
