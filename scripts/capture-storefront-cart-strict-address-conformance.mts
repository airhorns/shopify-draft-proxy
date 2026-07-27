/* oxlint-disable no-console -- CLI recorder intentionally writes status output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { runStorefrontGraphqlRequest } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildStorefrontRequestHeaders, getStoredStorefrontAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;

type StorefrontRecord = {
  name: string;
  method: 'POST';
  apiSurface: 'storefront';
  apiVersion: string;
  path: string;
  endpoint: string;
  authMode: 'storefront-access-token';
  headers: Record<string, string>;
  operationName: string;
  query: string;
  variables: JsonRecord;
  response: { status: number; body: unknown };
};

const { storeDomain, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const storefrontAuth = await getStoredStorefrontAccessToken();
if (storefrontAuth.shop && storefrontAuth.shop !== storeDomain) {
  throw new Error(`Stored Storefront credential targets ${storefrontAuth.shop}, not ${storeDomain}.`);
}

const storefrontPath = `/api/${apiVersion}/graphql.json`;
const storefrontEndpoint = `https://${storeDomain}${storefrontPath}`;
const storefrontHeaders = Object.fromEntries(
  Object.keys(buildStorefrontRequestHeaders(storefrontAuth.storefront_access_token)).map((name) => [
    name,
    '<redacted:storefront-access-token>',
  ]),
);
const cartCreateDocument = await readFile(
  'config/parity-requests/storefront/storefront-cart-strict-address-create.graphql',
  'utf8',
);
const addressesAddDocument = await readFile(
  'config/parity-requests/storefront/storefront-cart-strict-address-add.graphql',
  'utf8',
);
const storefrontSchema = JSON.parse(await readFile('config/storefront-graphql/2026-04/schema.json', 'utf8')) as {
  schema?: { types?: Array<{ name?: string; enumValues?: Array<{ name?: string }> }> };
};
const countryCodes =
  storefrontSchema.schema?.types
    ?.find((type) => type.name === 'CountryCode')
    ?.enumValues?.map((value) => value.name)
    .filter((value): value is string => typeof value === 'string') ?? [];
if (countryCodes.length === 0 || countryCodes.length > 250) {
  throw new Error(
    `Expected the captured Storefront CountryCode enum to contain 1..250 values, got ${countryCodes.length}.`,
  );
}

const cartSecrets = new Set<string>();
const redactedCartSecret = '<redacted:storefront-cart-secret>';

function registerCartSecrets(value: unknown): void {
  if (typeof value === 'string') {
    for (const pattern of [
      /gid:\/\/shopify\/Cart\/([^?&#/]+)(?:\?key=([^&#]+))?/gu,
      /\/cart\/c\/([^?&#/]+)(?:\?key=([^&#]+))?/gu,
    ]) {
      for (const match of value.matchAll(pattern)) {
        if (match[1]) cartSecrets.add(match[1]);
        if (match[2]) cartSecrets.add(match[2]);
      }
    }
    return;
  }
  if (Array.isArray(value)) {
    for (const entry of value) registerCartSecrets(entry);
    return;
  }
  if (typeof value === 'object' && value !== null) {
    for (const child of Object.values(value)) registerCartSecrets(child);
  }
}

function redactCartSecrets(value: unknown): unknown {
  if (typeof value === 'string') {
    let redacted = value;
    for (const secret of cartSecrets) redacted = redacted.replaceAll(secret, redactedCartSecret);
    return redacted;
  }
  if (Array.isArray(value)) return value.map(redactCartSecrets);
  if (typeof value === 'object' && value !== null) {
    return Object.fromEntries(Object.entries(value).map(([key, child]) => [key, redactCartSecrets(child)]));
  }
  return value;
}

function pathValue(root: unknown, segments: string[]): unknown {
  return segments.reduce<unknown>((current, segment) => {
    if (typeof current !== 'object' || current === null) return undefined;
    if (Array.isArray(current)) return current[Number(segment)];
    return (current as JsonRecord)[segment];
  }, root);
}

function requiredString(root: unknown, segments: string[], label: string): string {
  const value = pathValue(root, segments);
  if (typeof value !== 'string' || value.length === 0) {
    throw new Error(
      `${label} missing at ${segments.join('.')}: ${JSON.stringify(redactCartSecrets(root)).slice(0, 2000)}`,
    );
  }
  return value;
}

function requiredArray(root: unknown, segments: string[], label: string): unknown[] {
  const value = pathValue(root, segments);
  if (!Array.isArray(value)) {
    throw new Error(
      `${label} missing at ${segments.join('.')}: ${JSON.stringify(redactCartSecrets(root)).slice(0, 2000)}`,
    );
  }
  return value;
}

function assertNoTopLevelErrors(payload: unknown, label: string): void {
  const errors = pathValue(payload, ['errors']);
  if (Array.isArray(errors) && errors.length > 0) {
    throw new Error(`${label} returned top-level errors: ${JSON.stringify(redactCartSecrets(errors), null, 2)}`);
  }
}

function assertNoUserErrors(payload: unknown, segments: string[], label: string): void {
  const errors = requiredArray(payload, segments, `${label} userErrors`);
  if (errors.length > 0) {
    throw new Error(`${label} returned userErrors: ${JSON.stringify(redactCartSecrets(errors), null, 2)}`);
  }
}

async function record(
  name: string,
  operationName: string,
  query: string,
  variables: JsonRecord,
): Promise<{ record: StorefrontRecord; raw: unknown }> {
  const response = await runStorefrontGraphqlRequest(
    {
      storeOrigin: `https://${storeDomain}`,
      apiVersion,
      storefrontAccessToken: storefrontAuth.storefront_access_token,
    },
    query,
    variables,
  );
  registerCartSecrets(response.payload);
  return {
    record: {
      name,
      method: 'POST',
      apiSurface: 'storefront',
      apiVersion,
      path: storefrontPath,
      endpoint: storefrontEndpoint,
      authMode: 'storefront-access-token',
      headers: storefrontHeaders,
      operationName,
      query,
      variables: redactCartSecrets(variables) as JsonRecord,
      response: { status: response.status, body: redactCartSecrets(response.payload) },
    },
    raw: response.payload,
  };
}

const records: Record<string, StorefrontRecord> = {};

async function createCart(key: string, countryCode: string): Promise<string> {
  const result = await record(key, 'StorefrontCartStrictAddressCreate', cartCreateDocument, {
    input: { buyerIdentity: { countryCode } },
  });
  records[key] = result.record;
  assertNoTopLevelErrors(result.raw, key);
  assertNoUserErrors(result.raw, ['data', 'cartCreate', 'userErrors'], key);
  return requiredString(result.raw, ['data', 'cartCreate', 'cart', 'id'], `${key} cart ID`);
}

async function addAddresses(key: string, cartId: string, addresses: unknown[]): Promise<unknown> {
  const result = await record(key, 'StorefrontCartStrictAddressAdd', addressesAddDocument, {
    cartId,
    addresses,
  });
  records[key] = result.record;
  assertNoTopLevelErrors(result.raw, key);
  return result.raw;
}

function strictAddress(countryCode: string, provinceCode?: string, zip?: string): JsonRecord {
  return {
    address: {
      deliveryAddress: {
        firstName: 'Cart',
        lastName: 'Buyer',
        address1: '1 Capture Street',
        city: 'Capture City',
        provinceCode,
        countryCode,
        zip,
      },
    },
    selected: false,
    oneTimeUse: false,
    validationStrategy: 'STRICT',
  };
}

const strictCartId = await createCart('strictAddressCartCreate', 'US');
const australianCartId = await createCart('strictAustralianAddressCartCreate', 'AU');

for (let offset = 0; offset < countryCodes.length; offset += 20) {
  const batch = countryCodes.slice(offset, offset + 20);
  const batchNumber = String(offset / 20 + 1).padStart(2, '0');
  const key = `strictCountryRequirementsMatrix${batchNumber}`;
  const raw = await addAddresses(
    key,
    strictCartId,
    batch.map((countryCode) => ({
      address: { deliveryAddress: { countryCode } },
      selected: false,
      validationStrategy: 'STRICT',
    })),
  );
  const errors = requiredArray(raw, ['data', 'cartDeliveryAddressesAdd', 'userErrors'], `${key} userErrors`);
  for (let index = 0; index < batch.length; index += 1) {
    if (!errors.some((error) => pathValue(error, ['field', '1']) === String(index))) {
      throw new Error(`${key} returned no field evidence for ${batch[index]}.`);
    }
  }
}

await addAddresses('strictAustraliaRequiredFields', australianCartId, [
  {
    address: { deliveryAddress: { countryCode: 'AU' } },
    selected: false,
    validationStrategy: 'STRICT',
  },
]);
await addAddresses(
  'strictAustraliaPostalZoneNormalizations',
  australianCartId,
  [
    ['NSW', '2000'],
    ['ACT', '2600'],
    ['VIC', '3000'],
    ['QLD', '4000'],
    ['SA', '5000'],
    ['WA', '6000'],
    ['TAS', '7000'],
    ['NT', '0800'],
  ].map(([, postalCode]) => strictAddress('AU', 'ZZ', postalCode)),
);
await addAddresses('strictEmiratesRequiredFields', strictCartId, [
  {
    address: { deliveryAddress: { countryCode: 'AE' } },
    selected: false,
    validationStrategy: 'STRICT',
  },
]);
await addAddresses('strictEmiratesInvalidZone', strictCartId, [strictAddress('AE', 'ZZ')]);
await addAddresses(
  'strictEmiratesValidZoneCatalogWithoutPostal',
  strictCartId,
  ['AZ', 'AJ', 'DU', 'FU', 'RK', 'SH', 'UQ'].map((provinceCode) => strictAddress('AE', provinceCode.toLowerCase())),
);

const fixture = redactCartSecrets({
  capturedAt: new Date().toISOString(),
  apiSurface: 'storefront',
  apiVersion,
  authMode: 'storefront-access-token',
  ...records,
});
const outputPath = path.join(
  'fixtures',
  'conformance',
  storeDomain,
  apiVersion,
  'storefront',
  'storefront-cart-strict-address-validation.json',
);
await mkdir(path.dirname(outputPath), { recursive: true });
await writeFile(outputPath, `${JSON.stringify(fixture, null, 2)}\n`, 'utf8');
console.log(`Wrote ${outputPath}`);
console.log(`Captured ${Object.keys(records).length} secret-redacted Storefront cart interactions.`);
