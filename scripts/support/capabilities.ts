import type { ParsedOperation } from './graphql/parse-operation.js';
import {
  listImplementedOperationRegistryEntries,
  type CapabilityDomain,
  type CapabilityExecution,
} from './operation-registry.js';

export interface OperationCapability {
  type: ParsedOperation['type'];
  operationName: string | null;
  domain: CapabilityDomain;
  execution: CapabilityExecution;
}

const implementedEntries = listImplementedOperationRegistryEntries();
const CAPABILITY_ENTRY_BY_MATCH_NAME = new Map(
  implementedEntries.flatMap((entry) => entry.matchNames.map((matchName) => [matchName, entry] as const)),
);

function getCandidateOperationNames(operation: ParsedOperation): string[] {
  const names = [...(operation.rootFields ?? []), operation.name].filter(
    (value): value is string => typeof value === 'string' && value.length > 0,
  );

  return [...new Set(names)];
}

export function getOperationCapability(operation: ParsedOperation): OperationCapability {
  const candidates = getCandidateOperationNames(operation);
  const matchedRootField = operation.rootFields.find((candidate) => {
    const entry = CAPABILITY_ENTRY_BY_MATCH_NAME.get(candidate);
    return entry?.type === operation.type;
  });
  const matchedCandidate =
    matchedRootField ??
    candidates.find((candidate) => {
      const entry = CAPABILITY_ENTRY_BY_MATCH_NAME.get(candidate);
      return entry?.type === operation.type;
    });
  const matchedEntry = matchedCandidate ? (CAPABILITY_ENTRY_BY_MATCH_NAME.get(matchedCandidate) ?? null) : null;

  if (matchedCandidate && matchedEntry) {
    const operationNameEntry =
      operation.name && CAPABILITY_ENTRY_BY_MATCH_NAME.get(operation.name)?.name === matchedEntry.name
        ? operation.name
        : matchedCandidate;
    return {
      type: operation.type,
      operationName: operationNameEntry,
      domain: matchedEntry.domain,
      execution: matchedEntry.execution,
    };
  }

  return {
    type: operation.type,
    operationName: operation.name ?? operation.rootFields[0] ?? null,
    domain: 'unknown',
    execution: 'passthrough',
  };
}
