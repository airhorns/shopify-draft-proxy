import { Kind, type FieldNode, type SelectionNode } from 'graphql';

export function serializeJobSelectionSet(
  job: { id: string; done: boolean },
  selections: readonly SelectionNode[],
): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of selections) {
    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = selection.alias?.value ?? selection.name.value;
    switch (selection.name.value) {
      case 'id':
        result[key] = job.id;
        break;
      case 'done':
        result[key] = job.done;
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}

export function serializeCountValue(field: FieldNode, count: number): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  for (const selection of field.selectionSet?.selections ?? []) {
    if (selection.kind !== Kind.FIELD) {
      continue;
    }

    const key = selection.alias?.value ?? selection.name.value;
    switch (selection.name.value) {
      case 'count':
        result[key] = count;
        break;
      case 'precision':
        result[key] = 'EXACT';
        break;
      default:
        result[key] = null;
    }
  }

  return result;
}
