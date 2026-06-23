/* oxlint-disable no-console -- CLI scripts intentionally write status and error output to stdio. */
import 'dotenv/config';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const outputDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'segments');
const outputPath = path.join(outputDir, 'segments-baseline.json');
const documentPath = path.join('config', 'parity-requests', 'segments', 'segments-baseline-read.graphql');
const variablesPath = path.join('config', 'parity-requests', 'segments', 'segments-baseline-read.variables.json');

const { runGraphql, runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const document = await readFile(documentPath, 'utf8');

// The segment catalog read is de-seeded: the proxy cannot reconstruct Shopify's
// opaque pagination cursors, server-side query filtering, or the segment
// filter / suggestion / migration taxonomy, so it forwards the read upstream and
// returns Shopify's response verbatim. The recorded fixture therefore carries the
// forwarded `upstreamCalls` (the read the proxy makes) instead of a
// `/__meta/seed`-style `seedSegments` / `seedSegmentCatalog` precondition, and the
// spec's variables file is rewritten in lockstep so the cassette's variables match
// the runner's outgoing request on this store.
//
// `knownSegmentId` must reference a real segment so the detail read returns a
// non-null node. The conformance store has no standing segments, so this capture
// creates a disposable segment, records the baseline read against it, then deletes
// it (live create+cleanup, mirroring the other segment capture scripts). The
// recorded response is a point-in-time snapshot; parity replays it by forwarding
// the same read, so catalog search-index lag at capture time does not affect the
// comparison.
const createMutation = `#graphql
  mutation SegmentBaselineCaptureCreate($name: String!, $query: String!) {
    segmentCreate(name: $name, query: $query) {
      segment {
        id
      }
      userErrors {
        field
        message
      }
    }
  }
`;
const deleteMutation = `#graphql
  mutation SegmentBaselineCaptureDelete($id: ID!) {
    segmentDelete(id: $id) {
      deletedSegmentId
      userErrors {
        field
        message
      }
    }
  }
`;

const createResult = (await runGraphql(createMutation, {
  name: `Baseline read capture ${Date.now()}`,
  query: "customer_tags CONTAINS 'baseline-capture'",
})) as {
  data?: { segmentCreate?: { segment?: { id?: string }; userErrors?: { message?: string }[] } };
};
const userErrors = createResult.data?.segmentCreate?.userErrors ?? [];
if (userErrors.length > 0) {
  throw new Error(`segmentCreate returned userErrors: ${JSON.stringify(userErrors, null, 2)}`);
}
const knownSegmentId = createResult.data?.segmentCreate?.segment?.id;
if (!knownSegmentId) {
  throw new Error('segmentCreate did not return a segment id for the baseline read.');
}

const variables = {
  first: 3,
  knownSegmentId,
  missingSegmentId: 'gid://shopify/Segment/999999999999',
  filterSearch: 'email',
  valueSearch: '',
  valueFilterQueryName: 'customer_tags',
};

let result;
try {
  result = await runGraphqlRequest(document, variables);
} finally {
  await runGraphql(deleteMutation, { id: knownSegmentId });
}

if (result.status < 200 || result.status >= 300) {
  console.error(JSON.stringify(result.payload, null, 2));
  throw new Error(`Segment conformance capture failed with HTTP ${result.status}`);
}

await mkdir(outputDir, { recursive: true });
await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      ...result.payload,
      upstreamCalls: [
        {
          operationName: 'SegmentsBaselineRead',
          variables,
          query: document,
          response: { status: result.status, body: result.payload },
        },
      ],
    },
    null,
    2,
  )}\n`,
);
await writeFile(variablesPath, `${JSON.stringify(variables, null, 2)}\n`);
console.log(`Wrote ${outputPath}`);
