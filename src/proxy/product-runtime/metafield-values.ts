import type { ProductMetafieldRecord } from '../../state/types.js';

export function parseMetafieldJsonValue(
  type: string | null,
  value: string | null,
): ProductMetafieldRecord['jsonValue'] {
  if (value === null) {
    return null;
  }

  if (type === 'json' || type?.startsWith('list.')) {
    try {
      return JSON.parse(value) as ProductMetafieldRecord['jsonValue'];
    } catch {
      return value;
    }
  }

  if (type === 'number_integer') {
    const parsed = Number.parseInt(value, 10);
    return Number.isFinite(parsed) ? parsed : value;
  }

  if (type === 'number_decimal') {
    const parsed = Number.parseFloat(value);
    return Number.isFinite(parsed) ? parsed : value;
  }

  if (type === 'boolean') {
    return value === 'true';
  }

  return value;
}

export function makeMetafieldCompareDigest(metafield: {
  namespace: string;
  key: string;
  type: string | null;
  value: string | null;
  jsonValue?: unknown;
  updatedAt?: string | null | undefined;
}): string {
  return `draft:${Buffer.from(
    JSON.stringify([
      metafield.namespace,
      metafield.key,
      metafield.type,
      metafield.value,
      metafield.jsonValue ?? null,
      metafield.updatedAt ?? null,
    ]),
  ).toString('base64url')}`;
}
