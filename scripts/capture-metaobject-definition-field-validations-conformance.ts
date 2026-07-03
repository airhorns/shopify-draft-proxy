/* oxlint-disable no-console -- CLI scripts intentionally write status output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type Capture = {
  name: string;
  request: {
    query: string;
    variables: Record<string, unknown>;
  };
  status: number;
  response: unknown;
};

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({
  defaultApiVersion: '2026-04',
  exitOnMissing: true,
});
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'metaobjects');
const outputPath = path.join(outputDir, 'definition-create-field-validations.json');
const requestPath = 'config/parity-requests/metaobjects/definition-create-field-validations.graphql';
const createDefinitionMutation = await readFile(requestPath, 'utf8');
const runId = Date.now().toString();

const deleteDefinitionMutation = `#graphql
  mutation DeleteMetaobjectDefinition($id: ID!) {
    metaobjectDefinitionDelete(id: $id) {
      deletedId
      userErrors {
        field
        message
        code
        elementKey
        elementIndex
      }
    }
  }
`;

function readObject(value: unknown): Record<string, unknown> | null {
  return value && typeof value === 'object' && !Array.isArray(value) ? (value as Record<string, unknown>) : null;
}

function readPath(value: unknown, pathParts: string[]): unknown {
  let current = value;
  for (const part of pathParts) {
    const object = readObject(current);
    if (object === null) {
      return undefined;
    }
    current = object[part];
  }

  return current;
}

function readUserErrors(payload: unknown): unknown[] {
  const value = readPath(payload, ['data', 'metaobjectDefinitionCreate', 'userErrors']);
  return Array.isArray(value) ? value : [];
}

function readDefinitionId(payload: unknown): string | null {
  const value = readPath(payload, ['data', 'metaobjectDefinitionCreate', 'metaobjectDefinition', 'id']);
  return typeof value === 'string' && value.length > 0 ? value : null;
}

function assertGraphqlOk(result: ConformanceGraphqlResult, label: string): void {
  if (result.status < 200 || result.status >= 300 || readPath(result.payload, ['errors'])) {
    throw new Error(`${label} failed: ${JSON.stringify(result, null, 2)}`);
  }
}

function assertHasUserErrors(capture: Capture): void {
  if (readUserErrors(capture.response).length === 0) {
    throw new Error(`${capture.name} did not return userErrors: ${JSON.stringify(capture.response, null, 2)}`);
  }
}

function assertNoUserErrors(capture: Capture): void {
  const userErrors = readUserErrors(capture.response);
  if (userErrors.length > 0) {
    throw new Error(`${capture.name} returned userErrors: ${JSON.stringify(userErrors, null, 2)}`);
  }
}

function captureFromResult(
  name: string,
  query: string,
  variables: Record<string, unknown>,
  result: ConformanceGraphqlResult,
): Capture {
  return {
    name,
    request: {
      query,
      variables,
    },
    status: result.status,
    response: result.payload,
  };
}

function field(key: string): Record<string, unknown> {
  return {
    key,
    name: key,
    type: 'single_line_text_field',
  };
}

function typedField(key: string, type: string): Record<string, unknown> {
  return {
    key,
    name: key,
    type,
  };
}

function definition(
  type: string,
  name: string,
  displayNameKey: string,
  fieldDefinitions: Array<Record<string, unknown>>,
): Record<string, unknown> {
  return {
    type,
    name,
    displayNameKey,
    fieldDefinitions,
  };
}

const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

async function captureGraphql(name: string, query: string, variables: Record<string, unknown>): Promise<Capture> {
  const result = await runGraphqlRaw(query, variables);
  assertGraphqlOk(result, name);
  return captureFromResult(name, query, variables, result);
}

const cleanup: Capture[] = [];
let hyphenDefinitionId: string | null = null;
let validJurisdictionDefinitionId: string | null = null;
let validListJurisdictionDefinitionId: string | null = null;
let validProductTaxonomyDisclosureReferenceDefinitionId: string | null = null;

const reservedHandle = await captureGraphql('reserved-field-key', createDefinitionMutation, {
  definition: definition(`field_validation_reserved_${runId}`, 'Reserved Field Key', 'handle', [field('handle')]),
});
assertHasUserErrors(reservedHandle);

const duplicateKey = await captureGraphql('duplicate-field-key', createDefinitionMutation, {
  definition: definition(`field_validation_duplicate_${runId}`, 'Duplicate Field Key', 'title', [
    field('title'),
    field('title'),
  ]),
});
assertHasUserErrors(duplicateKey);

const missingDisplayNameKey = await captureGraphql('missing-display-name-key', createDefinitionMutation, {
  definition: definition(`field_validation_display_${runId}`, 'Missing Display Name Key', 'missing', [field('title')]),
});
assertHasUserErrors(missingDisplayNameKey);

const unknownFieldType = await captureGraphql('unknown-field-type', createDefinitionMutation, {
  definition: definition(`field_validation_unknown_type_${runId}`, 'Unknown Field Type', 'title', [
    typedField('title', 'garbage_type'),
  ]),
});
assertHasUserErrors(unknownFieldType);

const unsupportedListFieldType = await captureGraphql('unsupported-list-field-type', createDefinitionMutation, {
  definition: definition(`field_validation_list_type_${runId}`, 'Unsupported List Field Type', 'title', [
    typedField('title', 'list.boolean'),
  ]),
});
assertHasUserErrors(unsupportedListFieldType);

const validJurisdiction = await captureGraphql('valid-jurisdiction-field-type', createDefinitionMutation, {
  definition: definition(`field_validation_jurisdiction_${runId}`, 'Valid Jurisdiction Field Type', 'jurisdiction', [
    typedField('jurisdiction', 'jurisdiction'),
  ]),
});
assertNoUserErrors(validJurisdiction);
validJurisdictionDefinitionId = readDefinitionId(validJurisdiction.response);

const validListJurisdiction = await captureGraphql('valid-list-jurisdiction-field-type', createDefinitionMutation, {
  definition: definition(
    `field_validation_list_jurisdiction_${runId}`,
    'Valid List Jurisdiction Field Type',
    'jurisdictions',
    [typedField('jurisdictions', 'list.jurisdiction')],
  ),
});
assertNoUserErrors(validListJurisdiction);
validListJurisdictionDefinitionId = readDefinitionId(validListJurisdiction.response);

const validProductTaxonomyDisclosureReference = await captureGraphql(
  'valid-product-taxonomy-disclosure-reference-field-type',
  createDefinitionMutation,
  {
    definition: definition(
      `field_validation_product_taxonomy_disclosure_${runId}`,
      'Valid Product Taxonomy Disclosure Reference Field Type',
      'product_taxonomy_disclosure',
      [typedField('product_taxonomy_disclosure', 'product_taxonomy_disclosure_reference')],
    ),
  },
);
assertNoUserErrors(validProductTaxonomyDisclosureReference);
validProductTaxonomyDisclosureReferenceDefinitionId = readDefinitionId(
  validProductTaxonomyDisclosureReference.response,
);

const standardOnlyDisclosureReference = await captureGraphql(
  'standard-only-disclosure-reference-field-type',
  createDefinitionMutation,
  {
    definition: definition(
      `field_validation_disclosure_reference_${runId}`,
      'Standard Only Disclosure Reference Field Type',
      'disclosure',
      [typedField('disclosure', 'disclosure_reference')],
    ),
  },
);
assertHasUserErrors(standardOnlyDisclosureReference);

const standardOnlyListDisclosureReference = await captureGraphql(
  'standard-only-list-disclosure-reference-field-type',
  createDefinitionMutation,
  {
    definition: definition(
      `field_validation_list_disclosure_reference_${runId}`,
      'Standard Only List Disclosure Reference Field Type',
      'disclosures',
      [typedField('disclosures', 'list.disclosure_reference')],
    ),
  },
);
assertHasUserErrors(standardOnlyListDisclosureReference);

const hyphenKey = await captureGraphql('hyphen-field-key', createDefinitionMutation, {
  definition: definition(`field_validation_hyphen_${runId}`, 'Hyphen Field Key', 'field-key', [field('field-key')]),
});
assertNoUserErrors(hyphenKey);
hyphenDefinitionId = readDefinitionId(hyphenKey.response);

const tooManyFields = await captureGraphql('too-many-field-definitions', createDefinitionMutation, {
  definition: definition(
    `field_validation_many_${runId}`,
    'Too Many Field Definitions',
    'field_1',
    Array.from({ length: 41 }, (_, index) => field(`field_${index + 1}`)),
  ),
});
assertHasUserErrors(tooManyFields);

const reservedShopifyFormTooManyFields = await captureGraphql(
  'reserved-shopify-form-too-many-field-definitions',
  createDefinitionMutation,
  {
    definition: definition(
      `shopify--form-field-validation-many-${runId}`,
      'Reserved Shopify Form Too Many Field Definitions',
      'field_1',
      Array.from({ length: 41 }, (_, index) => field(`field_${index + 1}`)),
    ),
  },
);
assertHasUserErrors(reservedShopifyFormTooManyFields);

if (hyphenDefinitionId !== null) {
  cleanup.push(
    await captureGraphql('cleanup-metaobject-definition-delete', deleteDefinitionMutation, { id: hyphenDefinitionId }),
  );
}
if (validJurisdictionDefinitionId !== null) {
  cleanup.push(
    await captureGraphql('cleanup-valid-jurisdiction-definition-delete', deleteDefinitionMutation, {
      id: validJurisdictionDefinitionId,
    }),
  );
}
if (validListJurisdictionDefinitionId !== null) {
  cleanup.push(
    await captureGraphql('cleanup-valid-list-jurisdiction-definition-delete', deleteDefinitionMutation, {
      id: validListJurisdictionDefinitionId,
    }),
  );
}
if (validProductTaxonomyDisclosureReferenceDefinitionId !== null) {
  cleanup.push(
    await captureGraphql(
      'cleanup-valid-product-taxonomy-disclosure-reference-definition-delete',
      deleteDefinitionMutation,
      {
        id: validProductTaxonomyDisclosureReferenceDefinitionId,
      },
    ),
  );
}

await mkdir(outputDir, { recursive: true });
await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      capturedAt: new Date().toISOString(),
      storeDomain,
      apiVersion,
      summary:
        'MetaobjectDefinitionCreate field validation capture for reserved field keys, duplicate field input, displayNameKey resolution, valid current field types, invalid field types, hyphen key acceptance, max field count, and reserved shopify--form type max field count.',
      seed: {
        runId,
        hyphenDefinitionId,
        validJurisdictionDefinitionId,
        validListJurisdictionDefinitionId,
        validProductTaxonomyDisclosureReferenceDefinitionId,
      },
      reservedHandle,
      duplicateKey,
      missingDisplayNameKey,
      unknownFieldType,
      unsupportedListFieldType,
      validJurisdiction,
      validListJurisdiction,
      validProductTaxonomyDisclosureReference,
      standardOnlyDisclosureReference,
      standardOnlyListDisclosureReference,
      hyphenKey,
      tooManyFields,
      reservedShopifyFormTooManyFields,
      cleanup,
      upstreamCalls: [],
    },
    null,
    2,
  )}\n`,
);
console.log(`Wrote ${outputPath}`);
