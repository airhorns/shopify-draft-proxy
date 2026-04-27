import { store } from '../../state/store.js';
import type { ProductRecord } from '../../state/types.js';

const maxProductHandleLength = 255;

function normalizeHandleParts(value: string): string {
  return value
    .trim()
    .toLowerCase()
    .replace(/[^\p{L}\p{N}]+/gu, '-')
    .replace(/^-+|-+$/g, '');
}

export function slugifyHandle(title: string): string {
  const normalized = normalizeHandleParts(title);
  return normalized || 'untitled-product';
}

type ExplicitHandleResolution =
  | { kind: 'normalized-explicit'; handle: string }
  | { kind: 'fallback-explicit'; handle: string }
  | { kind: 'invalid'; error: { field: string[]; message: string } };

export function findEffectiveProductByHandle(handle: string): ProductRecord | null {
  return store.listEffectiveProducts().find((product) => product.handle === handle) ?? null;
}

function readExplicitHandle(input: Record<string, unknown>): ExplicitHandleResolution | null {
  const rawHandle = input['handle'];
  if (typeof rawHandle !== 'string') {
    return null;
  }

  const trimmedHandle = rawHandle.trim();
  if (!trimmedHandle) {
    return null;
  }

  const normalized = normalizeHandleParts(trimmedHandle);
  if (!normalized) {
    return { kind: 'fallback-explicit', handle: 'product' };
  }

  if (Array.from(normalized).length > maxProductHandleLength) {
    return {
      kind: 'invalid',
      error: {
        field: ['handle'],
        message: `Handle is too long (maximum is ${maxProductHandleLength} characters)`,
      },
    };
  }

  return { kind: 'normalized-explicit', handle: normalized };
}

function productHandleInUse(handle: string, excludedProductId?: string): boolean {
  const existing = findEffectiveProductByHandle(handle);
  return Boolean(existing && existing.id !== excludedProductId);
}

function nextProductHandleCandidate(handle: string): string {
  const numericSuffixMatch = handle.match(/^(.*?)(\d+)$/u);
  if (numericSuffixMatch) {
    const prefix = numericSuffixMatch[1] ?? '';
    const numericSuffix = numericSuffixMatch[2] ?? '';
    return `${prefix}${String(Number.parseInt(numericSuffix, 10) + 1)}`;
  }

  const hyphenatedSuffixMatch = handle.match(/^(.*?)-(\d+)$/u);
  if (hyphenatedSuffixMatch) {
    const prefix = hyphenatedSuffixMatch[1] ?? handle;
    const numericSuffix = hyphenatedSuffixMatch[2] ?? '0';
    return `${prefix}-${String(Number.parseInt(numericSuffix, 10) + 1)}`;
  }

  return `${handle}-1`;
}

export function ensureUniqueProductHandle(handle: string, excludedProductId?: string): string {
  let candidate = handle;
  while (productHandleInUse(candidate, excludedProductId)) {
    candidate = nextProductHandleCandidate(candidate);
  }

  return candidate;
}

function productHandleConflictError(handle: string): { field: string[]; message: string } {
  return {
    field: ['input', 'handle'],
    message: `Handle '${handle}' already in use. Please provide a new handle.`,
  };
}

export function prepareProductInputWithResolvedHandle(
  input: Record<string, unknown>,
  existing?: ProductRecord,
): { input: Record<string, unknown>; error: { field: string[]; message: string } | null } {
  const explicitHandle = readExplicitHandle(input);
  if (explicitHandle) {
    if (explicitHandle.kind === 'invalid') {
      return { input, error: explicitHandle.error };
    }

    if (explicitHandle.kind === 'normalized-explicit') {
      if (productHandleInUse(explicitHandle.handle, existing?.id)) {
        return { input, error: productHandleConflictError(explicitHandle.handle) };
      }

      return { input: { ...input, handle: explicitHandle.handle }, error: null };
    }

    return {
      input: {
        ...input,
        handle: ensureUniqueProductHandle(explicitHandle.handle, existing?.id),
      },
      error: null,
    };
  }

  const rawId = input['id'];
  const isSparseUpdate = typeof rawId === 'string' && !existing;
  if (isSparseUpdate) {
    return {
      input: {
        ...input,
        handle: '',
      },
      error: null,
    };
  }

  const rawTitle = input['title'];
  const title =
    typeof rawTitle === 'string' && rawTitle.trim() ? rawTitle.trim() : (existing?.title ?? 'Untitled product');
  const baseHandle = existing?.handle ?? slugifyHandle(title);
  return {
    input: {
      ...input,
      handle: ensureUniqueProductHandle(baseHandle, existing?.id),
    },
    error: null,
  };
}
