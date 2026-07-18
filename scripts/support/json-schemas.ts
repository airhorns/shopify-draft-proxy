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
export const apiSurfaceSchema = z.enum(['admin', 'storefront']);

export const operationRegistryEntrySchema = z.strictObject({
  apiSurface: apiSurfaceSchema,
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
    'storefront',
    'unknown',
  ]),
  execution: z.enum(['overlay-read', 'stage-locally']),
  implemented: z.boolean(),
  runtimeTests: z.array(z.string().min(1)),
  supportNotes: z.string().min(1).optional(),
});
export const operationRegistrySchema = z.array(operationRegistryEntrySchema);
export type OperationRegistryEntry = z.infer<typeof operationRegistryEntrySchema>;

export const nodeResolverInventoryEntrySchema = z.strictObject({
  typeName: z.string().min(1),
  resolver: z.string().min(1),
  behavior: z.enum(['project-local-record', 'return-known-null']),
});
export const nodeResolverInventorySchema = z.array(nodeResolverInventoryEntrySchema);
export type NodeResolverInventoryEntry = z.infer<typeof nodeResolverInventoryEntrySchema>;

export const parityProxyRequestSpecSchema = z.strictObject({
  documentPath: z.string().nullable().optional(),
  documentCapturePath: z.string().nullable().optional(),
  operationName: z.string().nullable().optional(),
  operationNameCapturePath: z.string().nullable().optional(),
  variablesPath: z.string().nullable().optional(),
  variablesCapturePath: z.string().nullable().optional(),
  variables: graphqlVariablesSchema.optional(),
  apiSurface: apiSurfaceSchema.optional(),
  apiVersion: z
    .string()
    .regex(/^\d{4}-\d{2}$/u)
    .optional(),
  headers: z.record(z.string(), z.string()).optional(),
  waitBeforeMs: z.number().int().nonnegative().optional(),
});
export type ProxyRequestSpec = z.infer<typeof parityProxyRequestSpecSchema>;

export const recordedUpstreamCallSchema = z
  .object({
    method: z.string().min(1).optional(),
    apiSurface: apiSurfaceSchema.optional(),
    apiVersion: z
      .string()
      .regex(/^\d{4}-\d{2}$/u)
      .optional(),
    path: z.string().min(1).optional(),
    endpoint: z.string().url().optional(),
    headers: z.record(z.string(), z.string()).optional(),
    operationName: z.string().optional(),
    query: z.string().optional(),
    variables: z.unknown().optional(),
    response: z
      .strictObject({
        status: z.number().int().optional(),
        body: z.unknown().optional(),
      })
      .optional(),
  })
  .passthrough();
export type RecordedUpstreamCallSchema = z.infer<typeof recordedUpstreamCallSchema>;

export const parityProxyUploadSpecSchema = z.strictObject({
  method: z.string().min(1),
  path: jsonValueSchema,
  body: jsonValueSchema,
  headers: z.record(z.string(), z.string()).optional(),
});
export type ProxyUploadSpec = z.infer<typeof parityProxyUploadSpecSchema>;

export const parityProxyHttpRequestSpecSchema = z.strictObject({
  method: z.string().min(1).optional(),
  path: jsonValueSchema,
  body: jsonValueSchema.optional(),
  headers: z.record(z.string(), z.string()).optional(),
});
export type ProxyHttpRequestSpec = z.infer<typeof parityProxyHttpRequestSpecSchema>;

export const matcherSchema = z.union([
  z.literal('any-string'),
  z.literal('non-empty-string'),
  z.literal('jsonl-string'),
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
  proxyUpload: parityProxyUploadSpecSchema.optional(),
  proxyHttpRequest: parityProxyHttpRequestSpecSchema.optional(),
  isolatedProxy: z.boolean().optional(),
  jsonlRecords: z.boolean().optional(),
  selectedPaths: z.array(z.string()).optional(),
  excludedPaths: z.array(z.string()).optional(),
  expectedDifferences: z.array(expectedDifferenceSchema).optional(),
  preserveProxyState: z.boolean().optional(),
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

export const parityProxyConfigSchema = z.strictObject({
  readMode: z.enum(['snapshot', 'live-hybrid', 'passthrough']).optional(),
});
export type ParityProxyConfig = z.infer<typeof parityProxyConfigSchema>;

export const paritySpecSchema = z
  .strictObject({
    scenarioId: z.string().optional(),
    operationNames: z.array(z.string()).optional(),
    scenarioStatus: z.string().optional(),
    assertionKinds: z.array(z.string()).optional(),
    comparisonMode: parityComparisonModeSchema.optional(),
    proxyConfig: parityProxyConfigSchema.optional(),
    proxyRequest: parityProxyRequestSpecSchema.optional(),
    comparison: comparisonContractSchema.optional(),
    liveCaptureFiles: z.array(z.string()).optional(),
    runtimeTestFiles: z.array(z.string()).optional(),
    nowIso: z.string().min(1).optional(),
    notes: z.string().optional(),
  })
  .superRefine((spec, ctx) => {
    if (spec.scenarioStatus !== 'captured') {
      return;
    }

    const localRuntimeOnlineStoreCapture = spec.liveCaptureFiles?.find(
      (captureFile) =>
        captureFile.startsWith('fixtures/conformance/local-runtime/') && captureFile.includes('/online-store/'),
    );
    if (localRuntimeOnlineStoreCapture) {
      ctx.addIssue({
        code: 'custom',
        path: ['liveCaptureFiles'],
        message: `Online-store parity specs must not use local-runtime fixtures as capture evidence: ${localRuntimeOnlineStoreCapture}`,
      });
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
