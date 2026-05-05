import { createHash } from 'node:crypto';

export function sha256_hex(input) {
  return createHash('sha256').update(input).digest('hex');
}
