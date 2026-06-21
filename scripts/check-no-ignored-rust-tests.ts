/* oxlint-disable no-console -- guardrail CLI reports offending file locations. */
import { execFileSync } from 'node:child_process';
import { existsSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

export type RustFile = {
  path: string;
  content: string;
};

export type IgnoredRustTestAttribute = {
  path: string;
  line: number;
  attribute: string;
};

const IGNORE_ATTRIBUTE = /^\s*#\s*\[\s*ignore(?:\s*=.*)?\]\s*$/;
const CFG_ATTR_IGNORE_ATTRIBUTE = /^\s*#\s*\[\s*cfg_attr\(.*\bignore\b.*\)\s*\]\s*$/;

export function findIgnoredRustTestAttributes(files: readonly RustFile[]): IgnoredRustTestAttribute[] {
  const violations: IgnoredRustTestAttribute[] = [];
  for (const file of files) {
    const lines = file.content.split(/\r?\n/);
    lines.forEach((line, index) => {
      const trimmed = line.trim();
      if (IGNORE_ATTRIBUTE.test(trimmed) || CFG_ATTR_IGNORE_ATTRIBUTE.test(trimmed)) {
        violations.push({
          path: file.path,
          line: index + 1,
          attribute: trimmed,
        });
      }
    });
  }
  return violations;
}

function trackedAndPendingRustFiles(cwd: string): string[] {
  return execFileSync('git', ['ls-files', '--cached', '--others', '--exclude-standard', '--', '*.rs'], {
    cwd,
    encoding: 'utf8',
  })
    .split('\n')
    .filter(Boolean);
}

function readRustFiles(cwd: string, paths: readonly string[]): RustFile[] {
  return paths
    .filter((path) => existsSync(resolve(cwd, path)))
    .map((path) => ({
      path,
      content: readFileSync(resolve(cwd, path), 'utf8'),
    }));
}

export function checkNoIgnoredRustTests(cwd = process.cwd()): IgnoredRustTestAttribute[] {
  return findIgnoredRustTestAttributes(readRustFiles(cwd, trackedAndPendingRustFiles(cwd)));
}

function main(): void {
  const violations = checkNoIgnoredRustTests();
  if (violations.length === 0) {
    return;
  }

  console.error('Ignored Rust tests are not allowed. Remove the ignore attribute or remove the defunct test.');
  for (const violation of violations) {
    console.error(`- ${violation.path}:${violation.line}: ${violation.attribute}`);
  }
  process.exitCode = 1;
}

const invokedPath = process.argv[1] === undefined ? undefined : resolve(process.argv[1]);
if (invokedPath === fileURLToPath(import.meta.url)) {
  main();
}
