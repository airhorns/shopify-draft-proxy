import { readFileSync } from 'node:fs';

import { z, type ZodType } from 'zod';

export type JsonValue = string | number | boolean | null | JsonValue[] | { [key: string]: JsonValue };
export const jsonValueSchema: ZodType<JsonValue> = z.lazy(() =>
  z.union([
    z.string(),
    z.number(),
    z.boolean(),
    z.null(),
    z.array(jsonValueSchema),
    z.record(z.string(), jsonValueSchema),
  ]),
);
export const jsonObjectSchema = z.record(z.string(), jsonValueSchema);

export function parseJsonFileWithSchema<T>(filePath: string, schema: ZodType<T>): T {
  let parsed: unknown;
  try {
    parsed = JSON.parse(readFileSync(filePath, 'utf8')) as unknown;
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    throw new Error(`Invalid JSON in ${filePath}: ${message}`);
  }

  const result = schema.safeParse(parsed);
  if (!result.success) {
    throw new Error(`Invalid JSON schema in ${filePath}: ${z.prettifyError(result.error)}`);
  }

  return result.data;
}

export const graphqlVariablesSchema = z.record(z.string(), z.unknown());

export const operationRegistryEntrySchema = z.strictObject({
  name: z.string().min(1),
  type: z.enum(['query', 'mutation']),
  domain: z.enum([
    'products',
    'admin-platform',
    'b2b',
    'apps',
    'media',
    'bulk-operations',
    'customers',
    'orders',
    'store-properties',
    'discounts',
    'events',
    'functions',
    'payments',
    'marketing',
    'online-store',
    'saved-searches',
    'privacy',
    'segments',
    'shipping-fulfillments',
    'gift-cards',
    'webhooks',
    'localization',
    'metafields',
    'metaobjects',
    'markets',
    'unknown',
  ]),
  execution: z.enum(['overlay-read', 'stage-locally']),
  implemented: z.boolean(),
  matchNames: z.array(z.string().min(1)),
  runtimeTests: z.array(z.string().min(1)),
  supportNotes: z.string().min(1).optional(),
});
export const operationRegistrySchema = z.array(operationRegistryEntrySchema);
export type OperationRegistryEntry = z.infer<typeof operationRegistryEntrySchema>;

export const parityProxyRequestSpecSchema = z.strictObject({
  documentPath: z.string().nullable().optional(),
  documentCapturePath: z.string().nullable().optional(),
  variablesPath: z.string().nullable().optional(),
  variablesCapturePath: z.string().nullable().optional(),
  variables: graphqlVariablesSchema.optional(),
  apiVersion: z
    .string()
    .regex(/^\d{4}-\d{2}$/u)
    .optional(),
  headers: z.record(z.string(), z.string()).optional(),
  waitBeforeMs: z.number().int().nonnegative().optional(),
});
export type ProxyRequestSpec = z.infer<typeof parityProxyRequestSpecSchema>;

export const matcherSchema = z.union([
  z.literal('any-string'),
  z.literal('non-empty-string'),
  z.literal('any-number'),
  z.literal('iso-timestamp'),
  z.literal('storefront-access-token'),
  z.string().regex(/^shopify-gid:[A-Za-z][A-Za-z0-9]*$/),
  z.string().regex(/^shop-policy-url-base:https:\/\/[^/\s]+(?:\/[^\s]*)?$/),
  z.string().regex(/^exact-string:.+$/),
  z.string().regex(/^regex:\^.+$/),
]);
export type Matcher = z.infer<typeof matcherSchema>;

export const expectedDifferenceSchema = z.strictObject({
  path: z.string(),
  ignore: z.boolean().optional(),
  matcher: matcherSchema.optional(),
  reason: z.string().optional(),
  regrettable: z.literal(true).optional(),
});
export type ExpectedDifference = z.infer<typeof expectedDifferenceSchema>;

export const comparisonTargetSchema = z.strictObject({
  name: z.string(),
  capturePath: z.string(),
  proxyPath: z.string().optional(),
  proxyStatePath: z.string().optional(),
  proxyLogPath: z.string().optional(),
  upstreamCapturePath: z.string().nullable().optional(),
  proxyRequest: parityProxyRequestSpecSchema.optional(),
  repeat: z
    .strictObject({
      times: z.number().int().positive(),
      start: z.number().int().optional(),
    })
    .optional(),
  selectedPaths: z.array(z.string()).optional(),
  excludedPaths: z.array(z.string()).optional(),
  expectedDifferences: z.array(expectedDifferenceSchema).optional(),
});
export type ComparisonTarget = z.infer<typeof comparisonTargetSchema>;

export const comparisonContractSchema = z.strictObject({
  mode: z.string().nullable().optional(),
  expectedDifferences: z.array(expectedDifferenceSchema).nullable().optional(),
  targets: z.array(comparisonTargetSchema).nullable().optional(),
});
export type ComparisonContract = z.infer<typeof comparisonContractSchema>;

export const parityComparisonModeSchema = z.enum([
  'planned',
  'captured-vs-proxy-request',
  'captured-compatibility-wrapper',
  'captured-fixture',
]);
export type ParityComparisonMode = z.infer<typeof parityComparisonModeSchema>;

export const paritySpecSchema = z
  .strictObject({
    scenarioId: z.string().optional(),
    operationNames: z.array(z.string()).optional(),
    scenarioStatus: z.string().optional(),
    assertionKinds: z.array(z.string()).optional(),
    comparisonMode: parityComparisonModeSchema.optional(),
    proxyRequest: parityProxyRequestSpecSchema.optional(),
    comparison: comparisonContractSchema.optional(),
    liveCaptureFiles: z.array(z.string()).optional(),
    runtimeTestFiles: z.array(z.string()).optional(),
    notes: z.string().optional(),
  })
  .superRefine((spec, ctx) => {
    if (spec.scenarioStatus !== 'captured') {
      return;
    }

    if (!spec.comparisonMode) {
      ctx.addIssue({
        code: 'custom',
        path: ['comparisonMode'],
        message: 'Captured parity specs must declare an executable comparison mode.',
      });
      return;
    }

    if (spec.comparisonMode === 'planned') {
      ctx.addIssue({
        code: 'custom',
        path: ['comparisonMode'],
        message: 'Captured parity specs must not use planned comparison mode.',
      });
    }
  });
export type ParitySpec = z.infer<typeof paritySpecSchema>;

export const conformanceScenarioOverrideSchema = z.strictObject({
  operationNames: z.array(z.string()).optional(),
  status: z.string().optional(),
  assertionKinds: z.array(z.string()).optional(),
  captureFiles: z.array(z.string()).optional(),
  notes: z.string().optional(),
});
export type ConformanceScenarioOverride = z.infer<typeof conformanceScenarioOverrideSchema>;

export const conformanceScenarioOverridesSchema = z.record(z.string(), conformanceScenarioOverrideSchema);
