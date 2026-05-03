import { createApp } from './app.js';
import { loadConfig } from './config.js';

const config = loadConfig();
const app = createApp(config);

const server = app.listen(config.port, () => {
  process.stdout.write(
    JSON.stringify({
      level: 'info',
      msg: 'shopify-draft-proxy listening',
      port: config.port,
      url: `http://localhost:${config.port}`,
    }) + '\n',
  );
});

function shutdown(): void {
  server.close(() => {
    process.exit(0);
  });
}

process.once('SIGTERM', shutdown);
process.once('SIGINT', shutdown);
