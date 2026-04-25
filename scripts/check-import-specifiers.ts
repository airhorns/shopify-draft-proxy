/* oxlint-disable no-console -- CLI lint helper intentionally writes failures to stderr. */
import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';

import ts from 'typescript';

const SOURCE_EXTENSIONS = ['.ts', '.mts', '.cts'];
const CHECKED_FILE_EXTENSIONS = ['.ts', '.mts', '.cts'];

type ImportSpecifierFailure = {
  file: string;
  line: number;
  specifier: string;
};

function listCandidateFiles(): string[] {
  return execFileSync('git', ['ls-files', '--cached', '--others', '--exclude-standard'], {
    encoding: 'utf8',
  })
    .split('\n')
    .filter((file) => CHECKED_FILE_EXTENSIONS.some((extension) => file.endsWith(extension)));
}

function isRelativeSourceSpecifier(specifier: string): boolean {
  return (
    (specifier.startsWith('./') || specifier.startsWith('../')) &&
    SOURCE_EXTENSIONS.some((extension) => specifier.endsWith(extension))
  );
}

function collectStringLiteralSpecifiers(
  sourceFile: ts.SourceFile,
): Array<{ node: ts.StringLiteral; specifier: string }> {
  const specifiers: Array<{ node: ts.StringLiteral; specifier: string }> = [];

  function visit(node: ts.Node): void {
    if (
      (ts.isImportDeclaration(node) || ts.isExportDeclaration(node)) &&
      node.moduleSpecifier !== undefined &&
      ts.isStringLiteral(node.moduleSpecifier)
    ) {
      specifiers.push({ node: node.moduleSpecifier, specifier: node.moduleSpecifier.text });
    }

    if (
      ts.isCallExpression(node) &&
      node.expression.kind === ts.SyntaxKind.ImportKeyword &&
      node.arguments.length === 1
    ) {
      const [argument] = node.arguments;
      if (argument !== undefined && ts.isStringLiteral(argument)) {
        specifiers.push({ node: argument, specifier: argument.text });
      }
    }

    ts.forEachChild(node, visit);
  }

  visit(sourceFile);
  return specifiers;
}

const failures: ImportSpecifierFailure[] = [];

for (const file of listCandidateFiles()) {
  const text = readFileSync(file, 'utf8');
  const sourceFile = ts.createSourceFile(file, text, ts.ScriptTarget.Latest, true);

  for (const { node, specifier } of collectStringLiteralSpecifiers(sourceFile)) {
    if (!isRelativeSourceSpecifier(specifier)) {
      continue;
    }

    const position = sourceFile.getLineAndCharacterOfPosition(node.getStart(sourceFile));
    failures.push({
      file,
      line: position.line + 1,
      specifier,
    });
  }
}

if (failures.length > 0) {
  console.error('Relative import specifiers must use emitted extensions (.js/.mjs/.cjs), not source extensions.');
  for (const failure of failures) {
    console.error(`${failure.file}:${failure.line} imports ${failure.specifier}`);
  }
  process.exit(1);
}
