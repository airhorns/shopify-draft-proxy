/* oxlint-disable no-console -- CLI scripts intentionally write status output to stdio. */
/**
 * Capture the complete public Shopify Admin GraphQL schema for one API version.
 *
 * The output is normalized SDL built from Shopify's standard introspection
 * response. It is the runtime schema source for the local GraphQL executor, so
 * every supported API version must be captured independently.
 */
import 'dotenv/config';

import { spawnSync } from 'node:child_process';
import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { buildClientSchema, buildSchema, getIntrospectionQuery, printSchema, type IntrospectionQuery } from 'graphql';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const accessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphql } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(accessToken),
});

const introspectionQuery = getIntrospectionQuery({
  descriptions: true,
  specifiedByUrl: true,
  directiveIsRepeatable: true,
  schemaDescription: true,
  inputValueDeprecation: true,
  oneOf: false,
});
const response = await runGraphql<IntrospectionQuery>(introspectionQuery);
const introspection = response.data;
if (!introspection?.__schema) {
  throw new Error(`Admin GraphQL ${apiVersion} introspection returned no __schema`);
}

// Preserve Shopify's introspection declaration order. The GraphQL type system
// is order-insensitive, but Shopify's variable coercion problems follow input
// field declaration order and the executable twin needs that observable detail.
const schema = buildClientSchema(introspection);
const schemaSdl = printSchema(schema);

// Parse the normalized output before writing so a partial or unsupported
// introspection response can never replace a known-good checked-in schema.
buildSchema(schemaSdl);

const capturedAt = new Date().toISOString();
const output = [
  `# Shopify Admin GraphQL ${apiVersion}`,
  `# Captured from ${storeDomain} at ${capturedAt}`,
  '# Generated from a live standard introspection response; do not hand-edit.',
  '',
  schemaSdl.trimEnd(),
  '',
].join('\n');
const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const outputPath = path.join(repoRoot, 'config', 'admin-graphql', apiVersion, 'schema.graphql');
await mkdir(path.dirname(outputPath), { recursive: true });
await writeFile(outputPath, output, 'utf8');
const formatResult = spawnSync('pnpm', ['exec', 'oxfmt', '--write', outputPath], {
  stdio: 'inherit',
});
if (formatResult.status !== 0) {
  throw new Error(`Could not format captured Admin GraphQL ${apiVersion} schema with oxfmt`);
}

console.log(
  JSON.stringify(
    {
      ok: true,
      apiVersion,
      storeDomain,
      capturedAt,
      outputPath,
      typeCount: Object.keys(schema.getTypeMap()).length,
      queryFieldCount: Object.keys(schema.getQueryType()?.getFields() ?? {}).length,
      mutationFieldCount: Object.keys(schema.getMutationType()?.getFields() ?? {}).length,
    },
    null,
    2,
  ),
);
