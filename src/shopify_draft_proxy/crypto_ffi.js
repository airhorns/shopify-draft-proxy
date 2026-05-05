import { createHash } from 'node:crypto';

export function sha256_hex(input) {
  return createHash('sha256').update(input).digest('hex');
}

export function md5_hex(input) {
  return createHash('md5').update(input).digest('hex');
}
