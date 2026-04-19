export function printLine(message = '') {
  process.stdout.write(`${message}\n`);
}

export function printJson(payload) {
  printLine(JSON.stringify(payload, null, 2));
}
