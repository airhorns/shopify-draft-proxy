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
    'media',
    'customers',
    'orders',
    'store-properties',
    'discounts',
    'payments',
    'marketing',
    'privacy',
    'segments',
    'shipping-fulfillments',
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
  variablesPath: z.string().nullable().optional(),
  variablesCapturePath: z.string().nullable().optional(),
  variables: graphqlVariablesSchema.optional(),
  waitBeforeMs: z.number().int().nonnegative().optional(),
});
export type ProxyRequestSpec = z.infer<typeof parityProxyRequestSpecSchema>;

export const matcherSchema = z.union([
  z.literal('any-string'),
  z.literal('non-empty-string'),
  z.literal('any-number'),
  z.literal('iso-timestamp'),
  z.string().regex(/^shopify-gid:[A-Za-z][A-Za-z0-9]*$/),
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
  proxyPath: z.string(),
  upstreamCapturePath: z.string().nullable().optional(),
  proxyRequest: parityProxyRequestSpecSchema.optional(),
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

export const blockerDetailsSchema = z
  .object({
    requiredApproval: z.string().optional(),
    requiredScopes: z.array(z.string()).optional(),
    requiredPermissions: z.array(z.string()).optional(),
    requiredTokenMode: z.string().optional(),
    probeRoots: z.array(z.string()).optional(),
    blockedFields: z.array(z.string()).optional(),
    blockedMutations: z.array(z.string()).optional(),
    failingMessage: z.string().optional(),
    docsUrl: z.string().optional(),
    manualStoreAuthStatus: z.string().optional(),
    manualStoreAuthTokenPath: z.string().optional(),
    manualStoreAuthCachedScopes: z.array(z.string()).optional(),
    manualStoreAuthAssociatedUserScopes: z.array(z.string()).optional(),
    appConfigPath: z.string().optional(),
    appId: z.string().optional(),
    appHandle: z.string().optional(),
    publicationTargetStatus: z.string().optional(),
    publicationTargetMessage: z.string().optional(),
    publicationTargetRemediation: z.string().optional(),
    shopifyAppCliAuthStatus: z.string().optional(),
    shopifyAppCliAuthWorkdir: z.string().optional(),
    shopifyAppDeployStatus: z.string().optional(),
    shopifyAppDeployCommand: z.string().optional(),
    shopifyAppDeployVersion: z.string().optional(),
    channelConfigExtensionPath: z.string().optional(),
    channelConfigHandle: z.string().optional(),
    channelConfigCreateLegacyChannelOnAppInstall: z.boolean().optional(),
    activeCredentialTokenFamily: z.string().optional(),
    activeCredentialHeaderMode: z.string().optional(),
    activeCredentialSummary: z.string().optional(),
  })
  .catchall(z.unknown());
export type BlockerDetails = z.infer<typeof blockerDetailsSchema>;

export const paritySpecSchema = z.strictObject({
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
  blocker: z
    .strictObject({
      kind: z.string().optional(),
      blockerPath: z.string().nullable().optional(),
      details: blockerDetailsSchema.optional(),
    })
    .optional(),
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
