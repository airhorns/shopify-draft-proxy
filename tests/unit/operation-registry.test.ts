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

  it('declares HAR-120 order management roots as explicit unsupported passthroughs', () => {
    const registry = listOperationRegistryEntries();
    const plannedRoots = [
      'orderClose',
      'orderOpen',
      'orderMarkAsPaid',
      'orderCreateManualPayment',
      'orderCustomerSet',
      'orderCustomerRemove',
      'orderInvoiceSend',
      'taxSummaryCreate',
      'orderCancel',
    ];

    for (const name of plannedRoots) {
      expect(registry.find((entry) => entry.name === name)).toMatchObject({
        name,
        type: 'mutation',
        domain: 'orders',
        execution: 'passthrough',
        implemented: false,
        runtimeTests: [],
      });
    }
  });
});
