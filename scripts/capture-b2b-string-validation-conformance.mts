import 'dotenv/config';

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

import { createAdminGraphqlClient, type ConformanceGraphqlResult } from './conformance-graphql-client.js';
import { readConformanceScriptConfig } from './conformance-script-config.js';
import { buildAdminAuthHeaders, getValidConformanceAccessToken } from './shopify-conformance-auth.mjs';

type JsonRecord = Record<string, unknown>;
type RecordedOperation = {
  request: {
    query: string;
    variables: JsonRecord;
  };
  response: JsonRecord;
};

const scenarioId = 'b2b-string-validation';
const timestamp = Date.now();
const longName = 'x'.repeat(300);
const longTitle = 't'.repeat(300);
const longNote = 'n'.repeat(5001);
const htmlLongNote = `<script>${'x'.repeat(6000)}</script>`;

const { storeDomain, adminOrigin, apiVersion } = readConformanceScriptConfig({ exitOnMissing: true });
const adminAccessToken = await getValidConformanceAccessToken({ adminOrigin, apiVersion });
const { runGraphqlRequest } = createAdminGraphqlClient({
  adminOrigin,
  apiVersion,
  headers: buildAdminAuthHeaders(adminAccessToken),
});

const companyCreateDocument = `#graphql
  mutation B2BStringValidationCompanyCreate($input: CompanyCreateInput!) {
    companyCreate(input: $input) {
      company {
        id
        name
        note
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const companyUpdateDocument = `#graphql
  mutation B2BStringValidationCompanyUpdate($companyId: ID!, $input: CompanyInput!) {
    companyUpdate(companyId: $companyId, input: $input) {
      company {
        id
        note
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const contactCreateDocument = `#graphql
  mutation B2BStringValidationContactCreate($companyId: ID!, $input: CompanyContactInput!) {
    companyContactCreate(companyId: $companyId, input: $input) {
      companyContact {
        id
        title
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const locationCreateDocument = `#graphql
  mutation B2BStringValidationLocationCreate($companyId: ID!, $input: CompanyLocationInput!) {
    companyLocationCreate(companyId: $companyId, input: $input) {
      companyLocation {
        id
        name
        note
      }
      userErrors {
        field
        message
        code
      }
    }
  }
`;

const companyDeleteDocument = `#graphql
  mutation B2BStringValidationCompanyDelete($id: ID!) {
    companyDelete(id: $id) {
      deletedCompanyId
      userErrors {
        field
        message
        code
      }
    }
  }
`;

function readRecord(value: unknown): JsonRecord | null {
  return value && typeof value === 'object' && !Array.isArray(value) ? (value as JsonRecord) : null;
}

function readPath(value: unknown, pathSegments: string[]): unknown {
  let current = value;
  for (const segment of pathSegments) {
    const record = readRecord(current);
    if (!record) {
      return undefined;
    }
    current = record[segment];
  }
  return current;
}

function readStringAtPath(value: unknown, pathSegments: string[]): string | null {
  const pathValue = readPath(value, pathSegments);
  return typeof pathValue === 'string' && pathValue.length > 0 ? pathValue : null;
}

function recordOperation(query: string, variables: JsonRecord, result: ConformanceGraphqlResult): RecordedOperation {
  return {
    request: { query, variables },
    response: {
      status: result.status,
      ...result.payload,
    },
  };
}

async function runOperation(query: string, variables: JsonRecord): Promise<RecordedOperation> {
  return recordOperation(query, variables, await runGraphqlRequest(query, variables));
}

async function runCleanup(companyId: string): Promise<RecordedOperation> {
  const result = await runGraphqlRequest(companyDeleteDocument, { id: companyId });
  return recordOperation(companyDeleteDocument, { id: companyId }, result);
}

const createdCompanyIds = new Set<string>();
const cleanup: Record<string, RecordedOperation> = {};

function rememberCreatedCompany(operation: RecordedOperation): string | null {
  const companyId = readStringAtPath(operation.response, ['data', 'companyCreate', 'company', 'id']);
  if (companyId) {
    createdCompanyIds.add(companyId);
  }
  return companyId;
}

try {
  const companyCreateLongName = await runOperation(companyCreateDocument, {
    input: {
      company: {
        name: longName,
      },
    },
  });
  rememberCreatedCompany(companyCreateLongName);

  const companyCreateLongNote = await runOperation(companyCreateDocument, {
    input: {
      company: {
        name: `HAR-625 long note ${timestamp}`,
        note: longNote,
      },
    },
  });
  rememberCreatedCompany(companyCreateLongNote);

  const companyCreateHtmlNote = await runOperation(companyCreateDocument, {
    input: {
      company: {
        name: `HAR-625 HTML note ${timestamp}`,
        note: '<b>merchant note</b>',
      },
    },
  });
  rememberCreatedCompany(companyCreateHtmlNote);

  const setupCompany = await runOperation(companyCreateDocument, {
    input: {
      company: {
        name: `HAR-625 validation setup ${timestamp}`,
      },
    },
  });
  const setupCompanyId = rememberCreatedCompany(setupCompany);
  if (!setupCompanyId) {
    throw new Error(`Unable to create setup company: ${JSON.stringify(setupCompany.response, null, 2)}`);
  }

  const companyUpdateHtmlAndTooLongNote = await runOperation(companyUpdateDocument, {
    companyId: setupCompanyId,
    input: {
      note: htmlLongNote,
    },
  });

  const contactCreateLongTitle = await runOperation(contactCreateDocument, {
    companyId: setupCompanyId,
    input: {
      email: `har-625-long-title-${timestamp}@example.com`,
      title: longTitle,
    },
  });

  const contactCreateHtmlTitle = await runOperation(contactCreateDocument, {
    companyId: setupCompanyId,
    input: {
      email: `har-625-html-title-${timestamp}@example.com`,
      title: '<b>VP</b>',
    },
  });

  const locationCreateLongName = await runOperation(locationCreateDocument, {
    companyId: setupCompanyId,
    input: {
      name: longName,
    },
  });

  const locationCreateHtmlAndTooLongNote = await runOperation(locationCreateDocument, {
    companyId: setupCompanyId,
    input: {
      name: `HAR-625 note location ${timestamp}`,
      note: htmlLongNote,
    },
  });

  for (const companyId of createdCompanyIds) {
    cleanup[`companyDelete:${companyId}`] = await runCleanup(companyId);
  }
  createdCompanyIds.clear();

  const output = {
    scenarioId,
    capturedAt: new Date().toISOString(),
    storeDomain,
    apiVersion,
    intent: {
      ticket: 'HAR-625',
      plan: 'Record B2B string length and HTML validation branches that the current live Admin API target reproduces, plus live mismatch probes for HTML branches that currently succeed.',
    },
    companyCreateLongName,
    companyCreateLongNote,
    companyCreateHtmlNote,
    setupCompany,
    companyUpdateHtmlAndTooLongNote,
    contactCreateLongTitle,
    contactCreateHtmlTitle,
    locationCreateLongName,
    locationCreateHtmlAndTooLongNote,
    cleanup,
    upstreamCalls: [],
  };

  const outputPath = path.join('fixtures', 'conformance', storeDomain, apiVersion, 'b2b', `${scenarioId}.json`);
  await mkdir(path.dirname(outputPath), { recursive: true });
  await writeFile(outputPath, `${JSON.stringify(output, null, 2)}\n`, 'utf8');

  // oxlint-disable-next-line no-console -- capture scripts report their output path.
  console.log(JSON.stringify({ ok: true, outputPath }, null, 2));
} finally {
  for (const companyId of createdCompanyIds) {
    cleanup[`companyDelete:${companyId}`] = await runCleanup(companyId);
  }
}
