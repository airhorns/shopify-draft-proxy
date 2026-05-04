import {
  Kind,
  parse,
  type ArgumentNode,
  type FieldNode,
  type ObjectFieldNode,
  type OperationDefinitionNode,
  type SelectionNode,
  type ValueNode,
} from 'graphql';

function getOperation(document: string): OperationDefinitionNode {
  const ast = parse(document);
  const operation = ast.definitions.find(
    (definition): definition is OperationDefinitionNode => definition.kind === Kind.OPERATION_DEFINITION,
  );

  if (!operation) {
    throw new Error('No GraphQL operation found');
  }

  return operation;
}

export function getRootField(document: string): FieldNode {
  const [field] = getRootFields(document);

  if (!field) {
    throw new Error('No root field found');
  }

  return field;
}

export function getRootFields(document: string): FieldNode[] {
  const operation = getOperation(document);
  return operation.selectionSet.selections.filter((selection): selection is FieldNode => selection.kind === Kind.FIELD);
}

function resolveValueNode(node: ValueNode, variables: Record<string, unknown>): unknown {
  switch (node.kind) {
    case Kind.NULL:
      return null;
    case Kind.STRING:
    case Kind.ENUM:
    case Kind.BOOLEAN:
      return node.value;
    case Kind.INT:
      return Number.parseInt(node.value, 10);
    case Kind.FLOAT:
      return Number.parseFloat(node.value);
    case Kind.LIST:
      return node.values.map((value) => resolveValueNode(value, variables));
    case Kind.OBJECT:
      return Object.fromEntries(
        node.fields.map((field: ObjectFieldNode) => [field.name.value, resolveValueNode(field.value, variables)]),
      );
    case Kind.VARIABLE:
      return variables[node.name.value] ?? null;
  }
}

export function getFieldArguments(field: FieldNode, variables: Record<string, unknown>): Record<string, unknown> {
  return Object.fromEntries(
    (field.arguments ?? []).map((argument: ArgumentNode) => [
      argument.name.value,
      resolveValueNode(argument.value, variables),
    ]),
  );
}

export function getRootFieldArguments(document: string, variables: Record<string, unknown>): Record<string, unknown> {
  return getFieldArguments(getRootField(document), variables);
}

export function getSelectionNames(field: FieldNode): string[] {
  return (field.selectionSet?.selections ?? [])
    .filter((selection): selection is FieldNode => selection.kind === Kind.FIELD)
    .map((selection: SelectionNode) => (selection as FieldNode).name.value);
}
