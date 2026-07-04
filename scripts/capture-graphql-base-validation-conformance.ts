import { createConformanceCapture, type JsonRecord } from './conformance-capture-lib.js';

const scenarioId = 'graphql-base-validation-unhappy-paths';
const domain = 'admin-platform';
const specPath = `config/parity-specs/${domain}/${scenarioId}.json`;

type CaptureCase = {
  id: string;
  targetName: string;
  description: string;
  documentFile: string;
  variables: JsonRecord;
};

const cases: CaptureCase[] = [
  {
    id: 'invalidSyntax',
    targetName: 'invalid-syntax-errors',
    description: 'Syntactically invalid GraphQL document.',
    documentFile: 'graphql-base-validation-invalid-syntax.graphql',
    variables: {},
  },
  {
    id: 'missingSubselection',
    targetName: 'missing-subselection-errors',
    description: 'Object field selected without the required subselection.',
    documentFile: 'graphql-base-validation-missing-subselection.graphql',
    variables: {},
  },
  {
    id: 'missingRequiredVariable',
    targetName: 'missing-required-variable-errors',
    description: 'Required variable declared by the operation but omitted from request variables.',
    documentFile: 'graphql-base-validation-missing-required-variable.graphql',
    variables: {},
  },
  {
    id: 'missingRequiredArgument',
    targetName: 'missing-required-root-argument-errors',
    description: 'Mutation root selected without its required argument.',
    documentFile: 'graphql-base-validation-missing-required-argument.graphql',
    variables: {},
  },
  {
    id: 'unknownQueryRoot',
    targetName: 'unknown-query-root-errors',
    description: 'Query root that does not exist in the Admin GraphQL schema.',
    documentFile: 'graphql-base-validation-unknown-query-root.graphql',
    variables: {},
  },
  {
    id: 'unknownMutationRoot',
    targetName: 'unknown-mutation-root-errors',
    description: 'Mutation root that does not exist in the Admin GraphQL schema.',
    documentFile: 'graphql-base-validation-unknown-mutation-root.graphql',
    variables: {},
  },
  {
    id: 'unknownProductField',
    targetName: 'unknown-product-field-errors',
    description: 'Known query root selecting a field that does not exist on the backend type.',
    documentFile: 'graphql-base-validation-unknown-product-field.graphql',
    variables: {},
  },
  {
    id: 'unknownOrderField',
    targetName: 'unknown-order-field-errors',
    description: 'Known non-product connection root selecting a field that does not exist on the backend node type.',
    documentFile: 'graphql-base-validation-unknown-order-field.graphql',
    variables: {},
  },
  {
    id: 'missingInputRequiredProperty',
    targetName: 'missing-input-required-property-errors',
    description: 'Mutation input object variable missing a required input property.',
    documentFile: 'graphql-base-validation-missing-input-required-property.graphql',
    variables: {
      webhookSubscription: {
        pubSubProject: 'shopify-draft-proxy-validation',
      },
    },
  },
];

function documentPath(documentFile: string): string {
  return `config/parity-requests/${domain}/${documentFile}`;
}

function assertExpectedValidationError(id: string, response: { payload: { errors?: unknown } }): void {
  if (!Array.isArray(response.payload.errors) || response.payload.errors.length === 0) {
    throw new Error(`${id} did not return top-level GraphQL errors: ${JSON.stringify(response, null, 2)}`);
  }
}

function buildSpec(apiVersion: string, fixturePath: string): JsonRecord {
  const firstCase = cases[0];
  if (!firstCase) throw new Error('Expected at least one GraphQL validation capture case.');

  return {
    scenarioId,
    operationNames: ['shop', 'products', 'orders', 'productCreate', 'pubSubWebhookSubscriptionCreate'],
    scenarioStatus: 'captured',
    assertionKinds: ['graphql-validation-parity', 'schema-validation', 'no-upstream-passthrough'],
    liveCaptureFiles: [fixturePath],
    proxyRequest: {
      documentPath: documentPath(firstCase.documentFile),
      variablesCapturePath: `$.cases.${firstCase.id}.request.variables`,
      apiVersion,
    },
    comparisonMode: 'captured-vs-proxy-request',
    comparison: {
      mode: 'strict-json',
      expectedDifferences: [],
      targets: cases.map((captureCase) => ({
        name: captureCase.targetName,
        capturePath: `$.cases.${captureCase.id}.expected.body`,
        proxyPath: '$',
        isolatedProxy: true,
        proxyRequest: {
          documentPath: documentPath(captureCase.documentFile),
          variablesCapturePath: `$.cases.${captureCase.id}.request.variables`,
          apiVersion,
        },
        ...(captureCase.id === 'missingRequiredArgument'
          ? {
              expectedDifferences: [
                {
                  path: '$.extensions',
                  ignore: true,
                  reason:
                    'Shopify includes volatile cost/throttle extensions on this schema-validation envelope; the proxy intentionally omits them instead of returning a fixed canned throttle model.',
                },
              ],
            }
          : {}),
      })),
    },
    notes:
      'Captured live Admin GraphQL base validation behavior for parse errors, object subselection rules, omitted required variables, missing root arguments, unknown roots/fields, and missing required input-object properties. Targets compare copied expected bodies instead of response payload paths so the parity runner does not install a cassette fallback; a proxy passthrough to Shopify fails these checks instead of masking missing local validation.',
  };
}

const capture = await createConformanceCapture();
const capturedCases: Record<string, JsonRecord> = {};

for (const captureCase of cases) {
  const query = await capture.readRequestRaw(domain, captureCase.documentFile);
  const response = await capture.runGraphqlRequest(query, captureCase.variables);
  assertExpectedValidationError(captureCase.id, response);
  capturedCases[captureCase.id] = {
    description: captureCase.description,
    request: {
      documentPath: documentPath(captureCase.documentFile),
      query,
      variables: captureCase.variables,
    },
    response,
    expected: {
      body: response.payload,
    },
  };
}

const fixturePath = capture.fixturePath(domain, `${scenarioId}.json`);
await capture.writeJson(fixturePath, {
  capturedAt: new Date().toISOString(),
  source: 'live-shopify-admin-graphql',
  storeDomain: capture.storeDomain,
  apiVersion: capture.apiVersion,
  liveGatewaySideEffects: false,
  notes:
    'Validation-only GraphQL requests. Each case fails before resolver execution and does not create or mutate Shopify resources.',
  cases: capturedCases,
});

await capture.writeJson(specPath, buildSpec(capture.apiVersion, fixturePath));
