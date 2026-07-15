/* oxlint-disable no-console -- CLI scripts intentionally write status output to stdio. */
/**
 * Capture the authenticated Storefront GraphQL schema and derive root-operation
 * inventory from that live schema.
 *
 * Output:
 * - `config/storefront-graphql/<api-version>/schema.json`
 * - `config/storefront-graphql/<api-version>/root-inventory.json`
 */
import 'dotenv/config';

import { execFile } from 'node:child_process';
import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { promisify } from 'node:util';

import { runStorefrontGraphql } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { getStoredStorefrontAccessToken } from './shopify-conformance-auth.mjs';

type TypeRef = {
  kind: string;
  name: string | null;
  ofType: TypeRef | null;
};

type SchemaField = {
  name: string;
  description: string | null;
  args: Array<{
    name: string;
    description: string | null;
    defaultValue: string | null;
    type: TypeRef;
  }>;
  type: TypeRef;
  isDeprecated: boolean;
  deprecationReason: string | null;
};

type SchemaRootType = {
  name: string;
  fields: SchemaField[] | null;
} | null;

type StorefrontSchema = {
  queryType: SchemaRootType;
  mutationType: SchemaRootType;
  subscriptionType: { name: string } | null;
  types: unknown[];
  directives: unknown[];
};

type RootInventoryEntry = {
  name: string;
  operationType: 'query' | 'mutation';
  family: StorefrontRootFamily;
  responseType: string;
  namedType: string;
  typeKind: string;
  isDeprecated: boolean;
  deprecationReason: string | null;
  argumentNames: string[];
};

type StorefrontRootFamily =
  | 'catalog'
  | 'content'
  | 'cart'
  | 'customer'
  | 'search'
  | 'metaobject'
  | 'shop-localization'
  | 'shop-pay'
  | 'other';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
  requireAdminOrigin: false,
});
const execFileAsync = promisify(execFile);
const storedAuth = await getStoredStorefrontAccessToken();
if (storedAuth.shop && storedAuth.shop !== storeDomain) {
  throw new Error(
    `Stored Storefront token is for ${storedAuth.shop}, but SHOPIFY_CONFORMANCE_STORE_DOMAIN is ${storeDomain}. ` +
      'Run `corepack pnpm conformance:grant-storefront-token` for the target store.',
  );
}

const schemaQuery = `#graphql
  query StorefrontGraphqlSchemaIntrospection {
    __schema {
      queryType {
        name
        fields(includeDeprecated: true) {
          ...SchemaField
        }
      }
      mutationType {
        name
        fields(includeDeprecated: true) {
          ...SchemaField
        }
      }
      subscriptionType {
        name
      }
      types {
        kind
        name
        description
        fields(includeDeprecated: true) {
          ...SchemaField
        }
        inputFields(includeDeprecated: true) {
          ...InputValue
        }
        interfaces {
          kind
          name
        }
        enumValues(includeDeprecated: true) {
          name
          description
          isDeprecated
          deprecationReason
        }
        possibleTypes {
          kind
          name
        }
      }
      directives {
        name
        description
        locations
        isRepeatable
        args(includeDeprecated: true) {
          ...InputValue
        }
      }
    }
  }

  fragment SchemaField on __Field {
    name
    description
    args(includeDeprecated: true) {
      ...InputValue
    }
    type {
      ...TypeRef0
    }
    isDeprecated
    deprecationReason
  }

  fragment InputValue on __InputValue {
    name
    description
    defaultValue
    type {
      ...TypeRef0
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

function namedType(typeRef: TypeRef | null): string {
  if (!typeRef) return '';
  if (typeRef.name) return typeRef.name;
  return namedType(typeRef.ofType);
}

function namedTypeKind(typeRef: TypeRef | null): string {
  if (!typeRef) return '';
  if (typeRef.name) return typeRef.kind;
  return namedTypeKind(typeRef.ofType);
}

function typeDisplay(typeRef: TypeRef | null): string {
  if (!typeRef) return '';
  if (typeRef.kind === 'NON_NULL') {
    return `${typeDisplay(typeRef.ofType)}!`;
  }
  if (typeRef.kind === 'LIST') {
    return `[${typeDisplay(typeRef.ofType)}]`;
  }
  return typeRef.name ?? '';
}

function rootFamily(rootName: string, namedResponseType: string): StorefrontRootFamily {
  const haystack = `${rootName} ${namedResponseType}`.toLowerCase();
  if (haystack.includes('shoppay') || haystack.includes('shop pay')) return 'shop-pay';
  if (haystack.includes('cart') || haystack.includes('checkout')) return 'cart';
  if (haystack.includes('customer')) return 'customer';
  if (haystack.includes('metaobject') || haystack.includes('metafield')) return 'metaobject';
  if (haystack.includes('search')) return 'search';
  if (
    haystack.includes('article') ||
    haystack.includes('blog') ||
    haystack.includes('menu') ||
    haystack.includes('page') ||
    haystack.includes('urlredirect')
  ) {
    return 'content';
  }
  if (
    haystack.includes('product') ||
    haystack.includes('collection') ||
    haystack.includes('catalog') ||
    haystack.includes('sellingplan')
  ) {
    return 'catalog';
  }
  if (
    haystack.includes('shop') ||
    haystack.includes('localization') ||
    haystack.includes('country') ||
    haystack.includes('language')
  ) {
    return 'shop-localization';
  }
  return 'other';
}

function inventoryEntries(rootType: SchemaRootType, operationType: 'query' | 'mutation'): RootInventoryEntry[] {
  return (rootType?.fields ?? [])
    .map((field) => {
      const responseType = typeDisplay(field.type);
      const responseNamedType = namedType(field.type);
      return {
        name: field.name,
        operationType,
        family: rootFamily(field.name, responseNamedType),
        responseType,
        namedType: responseNamedType,
        typeKind: namedTypeKind(field.type),
        isDeprecated: field.isDeprecated,
        deprecationReason: field.deprecationReason,
        argumentNames: field.args.map((arg) => arg.name).sort((left, right) => left.localeCompare(right)),
      };
    })
    .sort((left, right) => left.name.localeCompare(right.name));
}

function familySummary(entries: RootInventoryEntry[]): Record<StorefrontRootFamily, string[]> {
  const families: Record<StorefrontRootFamily, string[]> = {
    catalog: [],
    content: [],
    cart: [],
    customer: [],
    search: [],
    metaobject: [],
    'shop-localization': [],
    'shop-pay': [],
    other: [],
  };

  for (const entry of entries) {
    families[entry.family].push(entry.name);
  }
  for (const roots of Object.values(families)) {
    roots.sort((left, right) => left.localeCompare(right));
  }
  return families;
}

const response = await runStorefrontGraphql<{ __schema: StorefrontSchema }>(
  {
    storeOrigin: adminOrigin,
    apiVersion,
    storefrontAccessToken: storedAuth.storefront_access_token,
  },
  schemaQuery,
);
const schema = response.data?.__schema;
if (!schema) {
  console.error('No Storefront __schema returned from introspection.');
  console.error(JSON.stringify(response, null, 2));
  process.exit(1);
}

const queryRoots = inventoryEntries(schema.queryType, 'query');
const mutationRoots = inventoryEntries(schema.mutationType, 'mutation');
const allRoots = [...queryRoots, ...mutationRoots];
const outputDir = path.join('config', 'storefront-graphql', apiVersion);
const schemaPath = path.join(outputDir, 'schema.json');
const inventoryPath = path.join(outputDir, 'root-inventory.json');
const capturedAt = new Date().toISOString();
const endpoint = `${adminOrigin}/api/${apiVersion}/graphql.json`;

await mkdir(outputDir, { recursive: true });
await writeFile(
  schemaPath,
  `${JSON.stringify(
    {
      capturedAt,
      storeDomain,
      apiVersion,
      apiSurface: 'storefront',
      endpoint,
      authMode: 'storefront-access-token',
      storefrontToken: {
        id: storedAuth.storefront_token_id || '<unknown>',
        title: storedAuth.storefront_token_title || '<unknown>',
        accessScopes: storedAuth.storefront_access_scopes,
        obtainedAt: storedAuth.obtained_at || '<unknown>',
      },
      schema,
    },
    null,
    2,
  )}\n`,
  'utf8',
);
await writeFile(
  inventoryPath,
  `${JSON.stringify(
    {
      capturedAt,
      storeDomain,
      apiVersion,
      apiSurface: 'storefront',
      sourceSchemaPath: schemaPath,
      rootTypes: {
        query: schema.queryType?.name ?? null,
        mutation: schema.mutationType?.name ?? null,
        subscription: schema.subscriptionType?.name ?? null,
      },
      roots: {
        query: queryRoots,
        mutation: mutationRoots,
      },
      families: Object.fromEntries(
        Object.entries({
          query: familySummary(queryRoots),
          mutation: familySummary(mutationRoots),
          all: familySummary(allRoots),
        }),
      ),
      implementationPolicy:
        'Inventory roots are captured schema coverage only. Storefront registry entries generated from this file are unimplemented until local runtime behavior and captured Storefront parity promote them.',
    },
    null,
    2,
  )}\n`,
  'utf8',
);
await execFileAsync('corepack', ['pnpm', 'exec', 'oxfmt', schemaPath, inventoryPath]);

console.log(
  JSON.stringify(
    {
      ok: true,
      apiVersion,
      schemaPath,
      inventoryPath,
      queryRootCount: queryRoots.length,
      mutationRootCount: mutationRoots.length,
      familyCounts: Object.fromEntries(
        Object.entries(familySummary(allRoots)).map(([family, roots]) => [family, roots.length]),
      ),
    },
    null,
    2,
  ),
);
