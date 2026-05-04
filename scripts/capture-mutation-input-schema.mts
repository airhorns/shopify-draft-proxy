/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
/**
 * Capture the per-mutation argument shapes and per-input-object field shapes
 * (including `includeDeprecated: true`) needed to drive central required-field
 * validation in the proxy.
 *
 * Output: `config/admin-graphql-mutation-schema.json`. Regenerated whenever
 * the targeted API version changes — checked in so the proxy carries it on
 * every target without runtime IO. The `gleam/scripts/sync-mutation-schema.sh`
 * companion script mirrors this JSON into a Gleam source module.
 *
 * Strategy:
 *   1. Introspect Mutation { fields { args { type } } } with deprecated args
 *      included.
 *   2. BFS over the reachable INPUT_OBJECT types. For each, fetch
 *      `inputFields(includeDeprecated: true)` so deprecated aliases like
 *      `WebhookSubscriptionInput.callbackUrl` are preserved.
 *   3. Persist a closed graph: every INPUT_OBJECT referenced by a mutation
 *      arg or another captured input field is itself captured.
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

type SchemaArg = {
  name: string;
  isDeprecated: boolean;
  deprecationReason: string | null;
  defaultValue: string | null;
  type: TypeRef;
};

type SchemaInputField = {
  name: string;
  isDeprecated: boolean;
  deprecationReason: string | null;
  defaultValue: string | null;
  type: TypeRef;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphql } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const mutationsQuery = `#graphql
  query MutationFieldsWithDeprecated {
    type: __type(name: "Mutation") {
      fields(includeDeprecated: true) {
        name
        isDeprecated
        deprecationReason
        args(includeDeprecated: true) {
          name
          isDeprecated
          deprecationReason
          defaultValue
          type {
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

const inputObjectQuery = `#graphql
  query InputObjectFields($name: String!) {
    type: __type(name: $name) {
      name
      kind
      inputFields(includeDeprecated: true) {
        name
        isDeprecated
        deprecationReason
        defaultValue
        type {
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
`;

function namedInputObjects(t: TypeRef | null, into: Set<string>): void {
  if (!t) return;
  if (t.kind === 'INPUT_OBJECT' && t.name) into.add(t.name);
  namedInputObjects(t.ofType, into);
}

const mutationsResp = await runGraphql<{
  type: {
    fields: Array<{
      name: string;
      isDeprecated: boolean;
      deprecationReason: string | null;
      args: SchemaArg[];
    }>;
  };
}>(mutationsQuery);

const mutationFields = mutationsResp.data?.type?.fields;
if (!mutationFields) {
  console.error('No Mutation fields returned from introspection.');
  console.error(JSON.stringify(mutationsResp, null, 2));
  process.exit(1);
}

const mutations = mutationFields
  .map((f) => ({
    name: f.name,
    isDeprecated: f.isDeprecated,
    deprecationReason: f.deprecationReason,
    args: f.args.map((a) => ({
      name: a.name,
      isDeprecated: a.isDeprecated,
      deprecationReason: a.deprecationReason,
      defaultValue: a.defaultValue,
      type: a.type,
    })),
  }))
  .sort((a, b) => a.name.localeCompare(b.name));

// BFS reachable input objects.
const queue: string[] = [];
const seen = new Set<string>();
for (const m of mutations) {
  for (const a of m.args) {
    namedInputObjects(a.type, seen);
  }
}
queue.push(...seen);

const inputObjects: Array<{ name: string; inputFields: SchemaInputField[] }> = [];
let processed = 0;
while (queue.length > 0) {
  const name = queue.shift() as string;
  processed++;
  if (processed % 25 === 0) console.error(`  fetched ${processed} input objects (${queue.length} queued)…`);
  const r = await runGraphql<{
    type: {
      name: string;
      kind: string;
      inputFields: SchemaInputField[];
    } | null;
  }>(inputObjectQuery, { name });
  const t = r.data?.type;
  if (!t) {
    console.error(`  WARN: ${name} not found via __type — skipping`);
    continue;
  }
  if (t.kind !== 'INPUT_OBJECT') {
    console.error(`  WARN: ${name} resolved as ${t.kind}, expected INPUT_OBJECT — skipping`);
    continue;
  }
  inputObjects.push({
    name: t.name,
    inputFields: t.inputFields.map((f) => ({
      name: f.name,
      isDeprecated: f.isDeprecated,
      deprecationReason: f.deprecationReason,
      defaultValue: f.defaultValue,
      type: f.type,
    })),
  });
  for (const f of t.inputFields) {
    const more = new Set<string>();
    namedInputObjects(f.type, more);
    for (const n of more) {
      if (!seen.has(n)) {
        seen.add(n);
        queue.push(n);
      }
    }
  }
}

inputObjects.sort((a, b) => a.name.localeCompare(b.name));

const repoRoot = path.resolve(path.dirname(new URL(import.meta.url).pathname), '..');
const outputPath = path.join(repoRoot, 'config', 'admin-graphql-mutation-schema.json');
await mkdir(path.dirname(outputPath), { recursive: true });
await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      capturedAt: new Date().toISOString(),
      storeDomain,
      apiVersion,
      mutations,
      inputObjects,
    },
    null,
    2,
  )}\n`,
  'utf8',
);

console.log(
  JSON.stringify(
    {
      ok: true,
      outputPath,
      apiVersion,
      mutationCount: mutations.length,
      inputObjectCount: inputObjects.length,
    },
    null,
    2,
  ),
);
