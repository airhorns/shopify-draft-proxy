import {
  changedProtectedEvidencePaths,
  findProductsProvenanceFailures,
  findUnregisteredProtectedEvidenceChanges,
} from './protected-evidence-invariants.js';

const changed = changedProtectedEvidencePaths();
const failures = [...findUnregisteredProtectedEvidenceChanges(changed), ...findProductsProvenanceFailures()];

if (failures.length > 0) {
  process.stderr.write('Protected parity evidence invariant failures:\n');
  for (const failure of failures) process.stderr.write(`- ${failure.path}: ${failure.message}\n`);
  process.exit(1);
}

process.stdout.write('Protected parity evidence changes are registered and products provenance checks passed.\n');
