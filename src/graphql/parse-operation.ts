import { Kind, parse, type FieldNode, type OperationDefinitionNode } from 'graphql';

export type GraphQLOperationType = 'query' | 'mutation';

export interface ParsedOperation {
  type: GraphQLOperationType;
  name: string | null;
  rootFields: string[];
}

export function parseOperation(document: string): ParsedOperation {
  const ast = parse(document);
  const operation = ast.definitions.find(
    (definition): definition is OperationDefinitionNode => definition.kind === 'OperationDefinition',
  );

  if (!operation) {
    throw new Error('No GraphQL operation found');
  }

  if (operation.operation !== 'query' && operation.operation !== 'mutation') {
    throw new Error(`Unsupported GraphQL operation: ${operation.operation}`);
  }

  const rootFields = operation.selectionSet.selections
    .filter((selection): selection is FieldNode => selection.kind === Kind.FIELD)
    .map((selection) => selection.name.value);

  return {
    type: operation.operation,
    name: operation.name?.value ?? null,
    rootFields,
  };
}
