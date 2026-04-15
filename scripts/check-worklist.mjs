import { readFileSync } from 'node:fs';

const content = readFileSync(new URL('../docs/shopify-admin-worklist.md', import.meta.url), 'utf8');

if (!content.includes('## Product domain')) {
  throw new Error('Worklist must include the product domain section.');
}

console.log('worklist ok');
