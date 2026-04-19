import pino from 'pino';

function shouldUsePrettyLogs(): boolean {
  return process.env['NODE_ENV'] === 'development';
}

export const logger = pino({
  name: 'shopify-draft-proxy',
  level: process.env['LOG_LEVEL'] ?? 'info',
  ...(shouldUsePrettyLogs()
    ? {
        transport: {
          target: 'pino-pretty',
          options: {
            ignore: 'pid,hostname',
            singleLine: true,
          },
        },
      }
    : {}),
});
