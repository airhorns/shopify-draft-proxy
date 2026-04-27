import { Kind, parse } from 'graphql';

export function isObject(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}

export function hasOwnField(value: Record<string, unknown>, key: string): boolean {
  return Object.prototype.hasOwnProperty.call(value, key);
}

export function getOperationPathLabel(document: string): string {
  const ast = parse(document);
  const operation = ast.definitions.find((definition) => definition.kind === Kind.OPERATION_DEFINITION);
  if (!operation || operation.kind !== Kind.OPERATION_DEFINITION) {
    return 'mutation';
  }

  const operationType = operation.operation;
  return operation.name ? `${operationType} ${operation.name.value}` : operationType;
}

export function readLegacyResourceIdFromGid(id: string): string | null {
  const tail = id.split('/').at(-1);
  return tail && /^\d+$/u.test(tail) ? tail : null;
}

export function stripHtmlToDescription(value: string): string {
  return value
    .replace(/<[^>]*>/gu, '')
    .replace(/\s+/gu, ' ')
    .trim();
}
