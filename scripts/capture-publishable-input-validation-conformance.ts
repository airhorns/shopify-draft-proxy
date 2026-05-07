/* oxlint-disable no-console -- CLI scripts intentionally write capture status output to stdio. */
import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import {
  createAdminGraphqlClient,
  type ConformanceGraphqlPayload,
  type ConformanceGraphqlResult,
} from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const fixtureDir = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'store-properties');
const fixturePath = path.join(fixtureDir, 'publishable-input-validation.json');

const { runGraphql, runGraphqlRaw } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

type ProductCreateData = {
  productCreate: {
    product: { id: string; title: string; status: string } | null;
    userErrors: Array<{ field: string[] | null; message: string }>;
  };
};

type ProductDeleteData = {
  productDelete: {
    deletedProductId: string | null;
    userErrors: Array<{ field: string[] | null; message: string }>;
  };
};

type PublicationsData = {
  publications: {
    nodes: Array<{ id: string; name: string | null }>;
  };
};

type PublicationInput = {
  publicationId?: string | null;
  publishDate?: string | null;
};

type PublishableVariables = {
  id: string;
  input: PublicationInput[];
};

type ValidationCase = {
  variables: PublishableVariables;
  response: ConformanceGraphqlResult;
};

const createProductMutation = `#graphql
  mutation PublishableInputValidationCreateProduct($product: ProductCreateInput!) {
    productCreate(product: $product) {
      product {
        id
        title
        status
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const deleteProductMutation = `#graphql
  mutation PublishableInputValidationDeleteProduct($input: ProductDeleteInput!) {
    productDelete(input: $input) {
      deletedProductId
      userErrors {
        field
        message
      }
    }
  }
`;

const publicationsQuery = `#graphql
  query PublishableInputValidationPublications {
    publications(first: 20) {
      nodes {
        id
        name
      }
    }
  }
`;

const publishablePublishMutation = `#graphql
  mutation PublishableInputValidationPublish($id: ID!, $input: [PublicationInput!]!) {
    publishablePublish(id: $id, input: $input) {
      publishable {
        ... on Product {
          id
          publishedOnCurrentPublication
          resourcePublicationsCount {
            count
            precision
          }
        }
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const publishableUnpublishMutation = `#graphql
  mutation PublishableInputValidationUnpublish($id: ID!, $input: [PublicationInput!]!) {
    publishableUnpublish(id: $id, input: $input) {
      publishable {
        ... on Product {
          id
          publishedOnCurrentPublication
          resourcePublicationsCount {
            count
            precision
          }
        }
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const publishablePublishToCurrentChannelMutation = `#graphql
  mutation PublishableInputValidationPublishCurrent($id: ID!) {
    publishablePublishToCurrentChannel(id: $id) {
      publishable {
        ... on Product {
          id
          publishedOnCurrentPublication
          resourcePublicationsCount {
            count
            precision
          }
        }
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const publishableUnpublishToCurrentChannelMutation = `#graphql
  mutation PublishableInputValidationUnpublishCurrent($id: ID!) {
    publishableUnpublishToCurrentChannel(id: $id) {
      publishable {
        ... on Product {
          id
          publishedOnCurrentPublication
          resourcePublicationsCount {
            count
            precision
          }
        }
      }
      userErrors {
        field
        message
      }
    }
  }
`;

const publishableHydrateQuery = `#graphql
  query StorePropertiesPublishableInputValidationHydrate($id: ID!) {
    publishable: node(id: $id) {
      ... on Product {
        id
        publishedOnCurrentPublication
        resourcePublicationsCount {
          count
          precision
        }
      }
    }
    shop {
      publicationCount
    }
    publications(first: 20) {
      nodes {
        id
        name
      }
    }
  }
`;

function validationVariables(productId: string, input: PublicationInput[]): PublishableVariables {
  return { id: productId, input };
}

async function captureCase(query: string, variables: PublishableVariables): Promise<ValidationCase> {
  return {
    variables,
    response: await runGraphqlRaw(query, variables),
  };
}

await mkdir(fixtureDir, { recursive: true });

const runId = Date.now().toString(36);
let productId: string | null = null;
let setup: ConformanceGraphqlPayload<ProductCreateData> | null = null;
let cleanup: ConformanceGraphqlResult<ProductDeleteData> | null = null;
let hydrateResponse: ConformanceGraphqlResult | null = null;
let publicationsResponse: ConformanceGraphqlPayload<PublicationsData> | null = null;
const cases: Record<string, ValidationCase> = {};

try {
  setup = await runGraphql<ProductCreateData>(createProductMutation, {
    product: {
      title: `Publishable input validation ${runId}`,
      status: 'DRAFT',
    },
  });
  productId = setup.data?.productCreate.product?.id ?? null;
  if (!productId) {
    throw new Error(`Product setup failed: ${JSON.stringify(setup)}`);
  }

  publicationsResponse = await runGraphql<PublicationsData>(publicationsQuery);
  const publicationId = publicationsResponse.data?.publications.nodes[0]?.id ?? null;
  if (!publicationId) {
    throw new Error('Publishable input validation capture needs at least one shop publication.');
  }

  const duplicateInput = [{ publicationId }, { publicationId }];
  const pastDateInput = [{ publicationId, publishDate: '1900-01-01T00:00:00Z' }];
  const blankInput = [{}];
  const emptyStringInput = [{ publicationId: '' }];
  const unknownInput = [{ publicationId: 'gid://shopify/Publication/999999999999' }];

  cases['publishDuplicate'] = await captureCase(
    publishablePublishMutation,
    validationVariables(productId, duplicateInput),
  );
  cases['publishPastDate'] = await captureCase(
    publishablePublishMutation,
    validationVariables(productId, pastDateInput),
  );
  cases['publishBlankPublication'] = await captureCase(
    publishablePublishMutation,
    validationVariables(productId, blankInput),
  );
  cases['publishEmptyStringPublication'] = await captureCase(
    publishablePublishMutation,
    validationVariables(productId, emptyStringInput),
  );
  cases['publishUnknownPublication'] = await captureCase(
    publishablePublishMutation,
    validationVariables(productId, unknownInput),
  );
  cases['unpublishDuplicate'] = await captureCase(
    publishableUnpublishMutation,
    validationVariables(productId, duplicateInput),
  );
  cases['unpublishPastDate'] = await captureCase(
    publishableUnpublishMutation,
    validationVariables(productId, pastDateInput),
  );
  cases['unpublishBlankPublication'] = await captureCase(
    publishableUnpublishMutation,
    validationVariables(productId, blankInput),
  );
  cases['unpublishEmptyStringPublication'] = await captureCase(
    publishableUnpublishMutation,
    validationVariables(productId, emptyStringInput),
  );
  cases['unpublishUnknownPublication'] = await captureCase(
    publishableUnpublishMutation,
    validationVariables(productId, unknownInput),
  );

  const currentChannelVariables = { id: productId, input: [] };
  cases['publishToCurrentChannel'] = {
    variables: currentChannelVariables,
    response: await runGraphqlRaw(publishablePublishToCurrentChannelMutation, { id: productId }),
  };
  cases['unpublishToCurrentChannel'] = {
    variables: currentChannelVariables,
    response: await runGraphqlRaw(publishableUnpublishToCurrentChannelMutation, { id: productId }),
  };

  hydrateResponse = await runGraphqlRaw(publishableHydrateQuery, { id: productId });
} finally {
  if (productId) {
    try {
      cleanup = await runGraphqlRaw<ProductDeleteData>(deleteProductMutation, {
        input: { id: productId },
      });
    } catch (error) {
      console.warn(
        JSON.stringify(
          {
            ok: false,
            cleanup: 'productDelete',
            productId,
            error: error instanceof Error ? error.message : String(error),
          },
          null,
          2,
        ),
      );
    }
  }
}

if (!productId || !setup || !hydrateResponse || !publicationsResponse || Object.keys(cases).length === 0) {
  throw new Error('Publishable input validation capture did not produce required setup/cases.');
}

await writeFile(
  fixturePath,
  `${JSON.stringify(
    {
      scenarioId: 'publishable-input-validation',
      storeDomain,
      apiVersion,
      capturedAt: new Date().toISOString(),
      setup: {
        product: setup.data?.productCreate.product ?? null,
      },
      publications: publicationsResponse.data?.publications.nodes ?? [],
      cases,
      cleanup,
      notes: [
        'Live Admin API validation for generic publishable PublicationInput branches.',
        'Current-channel publishable siblings are captured with their schema-supported id-only shape.',
      ],
      upstreamCalls: [
        {
          operationName: 'StorePropertiesPublishablePublishHydrate',
          variables: { id: productId },
          query: publishableHydrateQuery,
          response: {
            status: hydrateResponse.status,
            body: hydrateResponse.payload,
          },
        },
        {
          operationName: 'StorePropertiesPublishableUnpublishHydrate',
          variables: { id: productId },
          query: publishableHydrateQuery,
          response: {
            status: hydrateResponse.status,
            body: hydrateResponse.payload,
          },
        },
        {
          operationName: 'StorePropertiesPublishablePublishToCurrentChannelHydrate',
          variables: { id: productId },
          query: publishableHydrateQuery,
          response: {
            status: hydrateResponse.status,
            body: hydrateResponse.payload,
          },
        },
        {
          operationName: 'StorePropertiesPublishableUnpublishToCurrentChannelHydrate',
          variables: { id: productId },
          query: publishableHydrateQuery,
          response: {
            status: hydrateResponse.status,
            body: hydrateResponse.payload,
          },
        },
      ],
    },
    null,
    2,
  )}\n`,
  'utf8',
);

console.log(
  JSON.stringify(
    {
      ok: true,
      fixturePath,
      productId,
      caseCount: Object.keys(cases).length,
      cleanupDeletedProductId: cleanup?.payload.data?.productDelete.deletedProductId ?? null,
    },
    null,
    2,
  ),
);
