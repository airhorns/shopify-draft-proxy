import { describe, expect, it } from 'vitest';
import {
  listImplementedOperationRegistryEntries,
  listOperationRegistryEntries,
} from '../../src/proxy/operation-registry.js';

describe('operation registry', () => {
  it('keeps implemented capability names unique', () => {
    const implementedNames = listImplementedOperationRegistryEntries().map((entry) => entry.name);
    expect(new Set(implementedNames).size).toBe(implementedNames.length);
  });

  it('requires implemented operations to declare runtime tests without conformance metadata', () => {
    for (const entry of listImplementedOperationRegistryEntries()) {
      expect(entry.runtimeTests.length).toBeGreaterThan(0);
      expect('conformance' in entry).toBe(false);
    }
  });

  it('exposes both overlay-read and stage-locally implemented operations', () => {
    const executions = new Set(listOperationRegistryEntries().map((entry) => entry.execution));
    expect(executions.has('overlay-read')).toBe(true);
    expect(executions.has('stage-locally')).toBe(true);
  });

  it('accounts for audited customer-area roots that are not implemented yet', () => {
    const entriesByName = new Map(listOperationRegistryEntries().map((entry) => [entry.name, entry]));

    const expectedRoots = new Map([
      ['customerByIdentifier', { type: 'query', execution: 'overlay-read' }],
      ['customerMergePreview', { type: 'query', execution: 'overlay-read' }],
      ['customerAddressCreate', { type: 'mutation', execution: 'stage-locally' }],
      ['customerAddressUpdate', { type: 'mutation', execution: 'stage-locally' }],
      ['customerAddressDelete', { type: 'mutation', execution: 'stage-locally' }],
      ['customerUpdateDefaultAddress', { type: 'mutation', execution: 'stage-locally' }],
      ['customerEmailMarketingConsentUpdate', { type: 'mutation', execution: 'stage-locally' }],
      ['customerSmsMarketingConsentUpdate', { type: 'mutation', execution: 'stage-locally' }],
      ['customerAddTaxExemptions', { type: 'mutation', execution: 'stage-locally' }],
      ['customerRemoveTaxExemptions', { type: 'mutation', execution: 'stage-locally' }],
      ['customerReplaceTaxExemptions', { type: 'mutation', execution: 'stage-locally' }],
      ['customerSet', { type: 'mutation', execution: 'stage-locally' }],
      ['customerSendAccountInviteEmail', { type: 'mutation', execution: 'passthrough' }],
      ['customerPaymentMethodSendUpdateEmail', { type: 'mutation', execution: 'passthrough' }],
      ['customerMerge', { type: 'mutation', execution: 'passthrough' }],
    ] as const);

    for (const [name, expected] of expectedRoots) {
      expect(entriesByName.get(name)).toMatchObject({
        name,
        domain: 'customers',
        implemented: false,
        runtimeTests: [],
        ...expected,
      });
    }
  });
});
