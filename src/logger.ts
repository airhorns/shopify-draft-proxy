import pino from 'pino';

export const logger = pino({
  name: 'shopify-draft-proxy',
  level: process.env['LOG_LEVEL'] ?? 'info',
});
