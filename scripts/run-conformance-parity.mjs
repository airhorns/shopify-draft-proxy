import { readFileSync } from 'node:fs';
import path from 'node:path';

import { parseOperation } from '../src/graphql/parse-operation.js';
import { store } from '../src/state/store.js';
import { getOperationCapability } from '../src/proxy/capabilities.js';
import { handleProductMutation, handleProductQuery, hydrateProductsFromUpstreamResponse } from '../src/proxy/products.js';
import { classifyParityScenarioState, compareJson, getPathValue } from './conformance-parity-lib.mjs';

const repoRoot = path.resolve(path.dirname(new URL(import.meta.url).pathname), '..');
const scenarioRegistry = JSON.parse(readFileSync(path.join(repoRoot, 'config', 'conformance-scenarios.json'), 'utf8'));

const filterId = process.argv[2] ?? null;
const selectedScenarios = filterId
  ? scenarioRegistry.filter((scenario) => scenario.id === filterId)
  : scenarioRegistry;

if (selectedScenarios.length === 0) {
  console.error(filterId ? `Unknown conformance scenario id: ${filterId}` : 'No conformance scenarios found.');
  process.exit(1);
}

function readJson(relativePath) {
  return JSON.parse(readFileSync(path.join(repoRoot, relativePath), 'utf8'));
}

function readRequest(proxyRequest, variablesOverride) {
  if (!proxyRequest?.documentPath || !proxyRequest?.variablesPath) {
    throw new Error('Comparison is missing proxyRequest.documentPath or proxyRequest.variablesPath.');
  }

  return {
    query: readFileSync(path.join(repoRoot, proxyRequest.documentPath), 'utf8'),
    variables: variablesOverride ?? readJson(proxyRequest.variablesPath),
  };
}

function executeProxyRequest(proxyRequest, upstreamPayload, variablesOverride) {
  const request = readRequest(proxyRequest, variablesOverride);
  const parsed = parseOperation(request.query);
  const capability = getOperationCapability(parsed);

  if (capability.execution === 'stage-locally' && capability.domain === 'products') {
    return handleProductMutation(request.query, request.variables);
  }

  if (capability.execution === 'overlay-read' && capability.domain === 'products') {
    const upstreamBody = upstreamPayload ?? { data: null };
    hydrateProductsFromUpstreamResponse(upstreamBody);
    return store.hasStagedProducts() ? handleProductQuery(request.query, request.variables, 'live-hybrid') : upstreamBody;
  }

  throw new Error(`Scenario request for ${capability.operationName} cannot be executed without live Shopify passthrough.`);
}

function productSeed(id, title = 'Conformance seed product') {
  return {
    id,
    legacyResourceId: null,
    title,
    handle: title.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '') || 'conformance-seed-product',
    status: 'ACTIVE',
    publicationIds: [],
    createdAt: '1970-01-01T00:00:00.000Z',
    updatedAt: '1970-01-01T00:00:00.000Z',
    vendor: null,
    productType: null,
    tags: [],
    totalInventory: null,
    tracksInventory: null,
    descriptionHtml: null,
    onlineStorePreviewUrl: null,
    templateSuffix: null,
    seo: { title: null, description: null },
    category: null,
  };
}

function seedScenarioState(capture) {
  for (const product of capture?.seedProducts ?? []) {
    hydrateProductsFromUpstreamResponse({ data: { product } });
  }

  const mutationVariables = capture?.mutation?.variables ?? {};
  const mutationData = capture?.mutation?.response?.data ?? {};
  const collectionPayload = Object.values(mutationData)
    .map((value) => value?.collection)
    .find((collection) => collection?.id);
  const collectionId = mutationVariables.id ?? mutationVariables.input?.id ?? collectionPayload?.id;
  if (collectionId) {
    store.upsertBaseCollections([
      {
        id: collectionId,
        title: collectionPayload?.title ?? 'Conformance seed collection',
        handle: collectionPayload?.handle ?? 'conformance-seed-collection',
      },
    ]);
  }

  const productId = mutationVariables.productId ?? mutationVariables.product?.id ?? mutationVariables.id;
  if (typeof productId === 'string') {
    const mutationProduct = Object.values(mutationData)
      .map((value) => value?.product)
      .find((product) => product?.id === productId);
    if (mutationProduct) {
      hydrateProductsFromUpstreamResponse({ data: { product: mutationProduct } });
    } else {
      store.upsertBaseProducts([productSeed(productId)]);
    }
  }

  const productOptionId = mutationVariables.option?.id ?? mutationVariables.options?.[0];
  if (typeof productId === 'string' && typeof productOptionId === 'string') {
    const valueId = mutationVariables.optionValuesToUpdate?.[0]?.id ?? `${productOptionId}/Value/1`;
    store.replaceBaseOptionsForProduct(productId, [
      {
        id: productOptionId,
        productId,
        name: 'Color',
        position: 1,
        optionValues: [
          {
            id: valueId,
            name: 'Red',
            hasVariants: true,
          },
        ],
      },
    ]);
  }
}

function executeComparisons(scenario, paritySpec) {
  const capture = scenario.captureFiles.length > 0 ? readJson(scenario.captureFiles[0]) : null;
  const comparisonResults = [];
  store.reset();
  seedScenarioState(capture);

  for (const comparison of paritySpec.comparisons ?? []) {
    const proxyRequest = comparison.proxyRequest ?? paritySpec.proxyRequest;
    const expected = getPathValue(capture, comparison.capturePath ?? '$');
    const variablesOverride = comparison.variablesCapturePath
      ? getPathValue(capture, comparison.variablesCapturePath)
      : undefined;
    const upstreamPayload =
      comparison.upstreamCapturePath === null
        ? null
        : getPathValue(capture, comparison.upstreamCapturePath ?? comparison.capturePath ?? '$');
    const actual = getPathValue(executeProxyRequest(proxyRequest, upstreamPayload, variablesOverride), comparison.proxyResponsePath ?? '$');
    const result = compareJson(expected, actual, {
      allowedDifferencePaths: comparison.allowedDifferencePaths ?? [],
      mustMatchPaths: comparison.mustMatchPaths ?? [],
    });

    comparisonResults.push({
      name: comparison.name,
      pass: result.pass,
      differences: result.differences,
    });
  }

  return comparisonResults;
}

const results = [];
for (const scenario of selectedScenarios) {
  const paritySpecPath = path.join(repoRoot, scenario.paritySpecPath);
  const paritySpec = JSON.parse(readFileSync(paritySpecPath, 'utf8'));

  const state = classifyParityScenarioState(scenario, paritySpec);
  const comparisonResults = state === 'ready-for-comparison' ? executeComparisons(scenario, paritySpec) : [];
  const comparisonFailures = comparisonResults.flatMap((comparison) =>
    comparison.pass ? [] : comparison.differences.map((difference) => `${comparison.name}: ${difference}`),
  );

  results.push({
    scenarioId: scenario.id,
    operations: scenario.operationNames,
    scenarioStatus: scenario.status,
    paritySpecPath: scenario.paritySpecPath,
    state,
    assertionKinds: scenario.assertionKinds,
    captureFiles: scenario.captureFiles,
    ...(comparisonResults.length > 0
      ? {
          comparisons: comparisonResults,
        }
      : {}),
    ...(comparisonFailures.length > 0
      ? {
          comparisonFailures,
        }
      : {}),
  });
}

const readyForComparison = results.filter((result) => result.state === 'ready-for-comparison');
const notYetImplemented = results.filter((result) => result.state === 'not-yet-implemented');
const invalid = results.filter((result) => result.state === 'invalid-missing-comparison-contract');
const failed = results.filter((result) => (result.comparisonFailures?.length ?? 0) > 0);

console.log(JSON.stringify({
  ok: invalid.length === 0 && failed.length === 0,
  total: results.length,
  readyForComparison: readyForComparison.length,
  notYetImplemented: notYetImplemented.length,
  invalid: invalid.length,
  failed: failed.length,
  results,
}, null, 2));

if (invalid.length > 0 || failed.length > 0) {
  process.exit(1);
}
