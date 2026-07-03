/* oxlint-disable no-console -- CLI scripts intentionally write status output to stdio. */
/**
 * Capture the Admin GraphQL output-field schema facts used by the
 * bulkOperationRunQuery AdminQuery validator:
 *
 * - field returns a Connection, including the connection node type
 * - field returns a non-connection List of composite values, including the element type
 * - field returns an object/interface/union, for nested traversal
 * - field returns a scalar/enum leaf, so generic selection validation can
 *   distinguish known leaf fields from unknown fields
 *
 * Output: `config/admin-graphql/<api-version>/bulk-query-schema.json`.
 * Regenerate each supported Admin API version independently so runtime
 * validation can use the output schema that matches the request path. The
 * current default-version capture is also mirrored to the legacy
 * `config/admin-graphql-bulk-query-schema.json` path for compatibility with
 * older local tooling.
 */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type TypeRef = {
  kind: string;
  name: string | null;
  ofType: TypeRef | null;
};

type SchemaField = {
  name: string;
  type: TypeRef;
};

type SchemaType = {
  kind: string;
  name: string | null;
  fields: SchemaField[] | null;
};

type CapturedFieldKind =
  | { type: 'connection'; nodeType: string }
  | { type: 'list'; elementType: string }
  | { type: 'object'; typeName: string }
  | { type: 'scalar'; typeName: string }
  | { type: 'enum'; typeName: string };

type CapturedField = {
  parentType: string;
  name: string;
  kind: CapturedFieldKind;
};

const configEnv = {
  ...process.env,
  SHOPIFY_CONFORMANCE_API_VERSION:
    process.env['SHOPIFY_CONFORMANCE_BULK_API_VERSION'] ?? process.env['SHOPIFY_CONFORMANCE_API_VERSION'] ?? '2026-04',
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  env: configEnv,
  exitOnMissing: true,
});
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphql } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const schemaQuery = `#graphql
  query BulkQueryOutputSchema {
    __schema {
      types {
        kind
        name
        fields(includeDeprecated: true) {
          name
          type {
            ...TypeRef0
          }
        }
      }
    }
  }

  fragment TypeRef0 on __Type {
    kind
    name
    ofType {
      kind
      name
      ofType {
        kind
        name
        ofType {
          kind
          name
          ofType {
            kind
            name
            ofType {
              kind
              name
              ofType {
                kind
                name
                ofType {
                  kind
                  name
                  ofType {
                    kind
                    name
                  }
                }
              }
            }
          }
        }
      }
    }
  }
`;

function unwrapNonNull(t: TypeRef): TypeRef {
  return t.kind === 'NON_NULL' && t.ofType ? unwrapNonNull(t.ofType) : t;
}

function namedLeaf(t: TypeRef | null): string | null {
  if (!t) return null;
  if (t.name) return t.name;
  return namedLeaf(t.ofType);
}

function isConnectionName(name: string | null): boolean {
  return typeof name === 'string' && name.endsWith('Connection');
}

function typeKind(typesByName: Map<string, SchemaType>, name: string | null): string | null {
  return name ? (typesByName.get(name)?.kind ?? null) : null;
}

function fieldNamed(type: SchemaType | undefined, name: string): SchemaField | undefined {
  return type?.fields?.find((field) => field.name === name);
}

function connectionNodeType(typesByName: Map<string, SchemaType>, connectionName: string): string | null {
  const connectionType = typesByName.get(connectionName);
  const nodesField = fieldNamed(connectionType, 'nodes');
  const nodesType = namedLeaf(nodesField?.type ?? null);
  if (nodesType) return nodesType;

  const edgesField = fieldNamed(connectionType, 'edges');
  const edgeTypeName = namedLeaf(edgesField?.type ?? null);
  const nodeField = fieldNamed(edgeTypeName ? typesByName.get(edgeTypeName) : undefined, 'node');
  return namedLeaf(nodeField?.type ?? null);
}

function capturedKind(typesByName: Map<string, SchemaType>, field: SchemaField): CapturedFieldKind | null {
  const leaf = namedLeaf(field.type);
  if (isConnectionName(leaf)) {
    const nodeType = connectionNodeType(typesByName, leaf as string);
    return nodeType ? { type: 'connection', nodeType } : null;
  }

  const unwrapped = unwrapNonNull(field.type);
  if (unwrapped.kind === 'LIST') {
    const elementType = namedLeaf(unwrapped.ofType);
    const elementKind = typeKind(typesByName, elementType);
    if (elementType && (elementKind === 'OBJECT' || elementKind === 'INTERFACE' || elementKind === 'UNION')) {
      return { type: 'list', elementType };
    }
    if (elementType && (elementKind === 'SCALAR' || elementKind === 'ENUM')) {
      return { type: elementKind === 'ENUM' ? 'enum' : 'scalar', typeName: elementType };
    }
    return null;
  }

  const leafKind = typeKind(typesByName, leaf);
  if (leaf && (leafKind === 'OBJECT' || leafKind === 'INTERFACE' || leafKind === 'UNION')) {
    return { type: 'object', typeName: leaf };
  }
  if (leaf && (leafKind === 'SCALAR' || leafKind === 'ENUM')) {
    return { type: leafKind === 'ENUM' ? 'enum' : 'scalar', typeName: leaf };
  }

  return null;
}

const response = await runGraphql<{ __schema: { types: SchemaType[] } }>(schemaQuery);
const schemaTypes = response.data?.__schema?.types;
if (!schemaTypes) {
  console.error('No __schema.types returned from introspection.');
  console.error(JSON.stringify(response, null, 2));
  process.exit(1);
}

const typesByName = new Map<string, SchemaType>();
for (const type of schemaTypes) {
  if (type.name) typesByName.set(type.name, type);
}

const capturedFields: CapturedField[] = [];
for (const type of schemaTypes) {
  if (!type.name || !type.fields) continue;
  if (type.kind !== 'OBJECT' && type.kind !== 'INTERFACE') continue;

  for (const field of type.fields) {
    const kind = capturedKind(typesByName, field);
    if (kind) {
      capturedFields.push({
        parentType: type.name,
        name: field.name,
        kind,
      });
    }
  }
}

capturedFields.sort((a, b) => a.parentType.localeCompare(b.parentType) || a.name.localeCompare(b.name));

const repoRoot = path.resolve(path.dirname(new URL(import.meta.url).pathname), '..');
const outputPath = path.join(repoRoot, 'config', 'admin-graphql', apiVersion, 'bulk-query-schema.json');
const legacyOutputPath = path.join(repoRoot, 'config', 'admin-graphql-bulk-query-schema.json');
const output = `${JSON.stringify(
  {
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    fields: capturedFields,
  },
  null,
  2,
)}\n`;
await mkdir(path.dirname(outputPath), { recursive: true });
await writeFile(outputPath, output, 'utf8');
if (apiVersion === '2026-04') {
  await writeFile(legacyOutputPath, output, 'utf8');
}

console.log(
  JSON.stringify(
    {
      ok: true,
      outputPath,
      legacyOutputPath: apiVersion === '2026-04' ? legacyOutputPath : null,
      apiVersion,
      fieldCount: capturedFields.length,
      connectionFieldCount: capturedFields.filter((field) => field.kind.type === 'connection').length,
      listFieldCount: capturedFields.filter((field) => field.kind.type === 'list').length,
      objectFieldCount: capturedFields.filter((field) => field.kind.type === 'object').length,
      scalarFieldCount: capturedFields.filter((field) => field.kind.type === 'scalar').length,
      enumFieldCount: capturedFields.filter((field) => field.kind.type === 'enum').length,
    },
    null,
    2,
  ),
);
