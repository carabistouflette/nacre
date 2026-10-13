const assert = require('node:assert/strict');
const { spawnSync } = require('node:child_process');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const vscodeOniguruma = require('vscode-oniguruma');
const vscodeTextmate = require('vscode-textmate');

const extensionRoot = path.resolve(__dirname, '..');
const repositoryRoot = path.resolve(extensionRoot, '../..');
const grammarPath = path.join(extensionRoot, 'syntaxes/nacre.tmLanguage.json');
const grammarSource = fs.readFileSync(grammarPath, 'utf8');
const rawGrammar = vscodeTextmate.parseRawGrammar(grammarSource, grammarPath);

async function loadGrammar() {
  const wasm = fs.readFileSync(require.resolve('vscode-oniguruma/release/onig.wasm'));
  const wasmBytes = wasm.buffer.slice(wasm.byteOffset, wasm.byteOffset + wasm.byteLength);
  await vscodeOniguruma.loadWASM(wasmBytes);

  const registry = new vscodeTextmate.Registry({
    onigLib: Promise.resolve({
      createOnigScanner: (patterns) => new vscodeOniguruma.OnigScanner(patterns),
      createOnigString: (value) => new vscodeOniguruma.OnigString(value)
    }),
    loadGrammar: async (scopeName) => scopeName === 'source.nacre' ? rawGrammar : null
  });
  return registry.loadGrammar('source.nacre');
}

function tokenize(grammar, source) {
  let ruleStack = vscodeTextmate.INITIAL;
  return source.split(/\r?\n/).map((line) => {
    const result = grammar.tokenizeLine(line, ruleStack);
    ruleStack = result.ruleStack;
    return { line, tokens: result.tokens };
  });
}

function scopesAt(lines, lineIndex, text, occurrence = 0) {
  const entry = lines[lineIndex];
  assert(entry, `missing line ${lineIndex + 1}`);
  let offset = -1;
  let from = 0;
  for (let index = 0; index <= occurrence; index += 1) {
    offset = entry.line.indexOf(text, from);
    assert.notEqual(offset, -1, `could not find ${JSON.stringify(text)} on line ${lineIndex + 1}`);
    from = offset + text.length;
  }
  const token = entry.tokens.find((candidate) => candidate.startIndex <= offset && offset < candidate.endIndex);
  assert(token, `no token covers ${JSON.stringify(text)} on line ${lineIndex + 1}`);
  return token.scopes;
}

function assertHasScope(scopes, expected, description) {
  assert(scopes.includes(expected), `${description}: expected ${expected}, received ${scopes.join(' ')}`);
}

function assertLacksScope(scopes, forbidden, description) {
  assert(!scopes.some((scope) => scope.startsWith(forbidden)), `${description}: unexpectedly received ${forbidden}`);
}

function expandSnippet(body) {
  return body.replace(/\$\{(\d+)(?::([^}]*))?\}|\$(\d+)/g, (_match, _index, defaultValue) => defaultValue ?? '');
}

function sourceForSnippet(name, body) {
  switch (name) {
    case 'Main function':
      return `${body}\n`;
    case 'Class':
    case 'Interface':
      return `${body}\nvoid main() {}\n`;
    case 'Enum declaration':
      return `${body}\nvoid main() {}\n`;
    case 'Variant construction':
      return `void main() {\nOption<int> option = ${body};\nmatch (option) { Some(value) {} None {} }\n}\n`;
    case 'Consuming match':
    case 'Borrowed enum match':
      return `void main() {\nOption<int> option = new Option<int>.Some(42);\n${body}\n}\n`;
    case 'Constructor':
      return `class Name {\n${body}\n}\nvoid main() {}\n`;
    case 'Readonly method':
      return `class Host {\n${body}\n}\nvoid main() {}\n`;
    case 'Borrowed parameter':
      return `class Type {}\nvoid accept(${body}) {}\nvoid main() {}\n`;
    case 'Receiver-bound borrowed getter':
      return `class Host {\nText value = new Text("value");\n${body}\n}\nvoid main() {}\n`;
    case 'Exclusive list element':
    case 'Shared list traversal':
    case 'Exclusive list traversal':
      return `class Contact {}\nvoid main() {\nList<Contact> contacts = new List<Contact>();\ncontacts.add(new Contact());\n${body}\n}\n`;
    case 'Copied list traversal':
      return `void main() {\nList<int> numbers = new List<int>();\nnumbers.add(1);\n${body}\n}\n`;
    case 'Copied map traversal':
      return `void main() {\nMap<int,int> counts = new Map<int,int>();\n${body}\n}\n`;
    case 'Shared map traversal':
    case 'Exclusive map traversal':
      return `class Contact {}\nvoid main() {\nMap<int,Contact> entries = new Map<int,Contact>();\n${body}\n}\n`;
    case 'Text-keyed map':
      return `void main() {\n${body}\n}\n`;
    case 'Shared text key with copied value':
      return `void main() {\nMap<Text,int> counts = new Map<Text,int>();\ncounts.put(new Text("pear"), 4);\nText query = new Text("pear");\nif (counts.containsKey(query)) { println("found"); }\nmatch (counts.tryGet(query)) { Some(existing) { println(existing); } None {} }\n${body}\n}\n`;
    case 'Mutable value with shared text key':
      return `class Counter {\n    int value;\n    Counter(int value) { this.value = value; }\n    public void increment() { this.value = this.value + 1; }\n}\nvoid main() {\nMap<Text,Counter> counters = new Map<Text,Counter>();\ncounters.put(new Text("pear"), new Counter(1));\n${body}\n}\n`;
    case 'Recoverable list removal':
      return `void main() {\nList<int> items = new List<int>();\n${body}\n}\n`;
    case 'Shared map value':
    case 'Exclusive map value':
      return `class Contact {}\nvoid main() {\nMap<int,Contact> entries = new Map<int,Contact>();\nmatch (entries.put(1, new Contact())) { None {} Some(previous) {} }\n${body}\n}\n`;
    case 'Recoverable map removal':
      return `void main() {\nMap<int,int> entries = new Map<int,int>();\nmatch (entries.put(1, 7)) { None {} Some(previous) {} }\n${body}\n}\n`;
    default:
      return `void main() {\n${body}\n}\n`;
  }
}

function validateSnippetsWithCompiler(snippets) {
  const configuredCompiler = process.env.NACRE_BIN;
  const localCompiler = [
    'target/debug/nacre',
    'target/debug/nacre.exe',
    'target/release/nacre',
    'target/release/nacre.exe'
  ].map((relativePath) => path.join(repositoryRoot, relativePath)).find((candidate) => fs.existsSync(candidate));
  const compiler = configuredCompiler
    ? (configuredCompiler.includes(path.sep) ? path.resolve(configuredCompiler) : configuredCompiler)
    : localCompiler || 'nacre';
  const probe = spawnSync(compiler, ['--version'], { cwd: repositoryRoot, encoding: 'utf8' });
  if (probe.error?.code === 'ENOENT' && !configuredCompiler && !localCompiler) {
    console.log('Compiler snippet check skipped (set NACRE_BIN, build target/debug/nacre, or put nacre on PATH to enable it).');
    return;
  }
  assert.ifError(probe.error);
  assert.equal(probe.status, 0, `could not run ${compiler} --version: ${probe.stderr || probe.stdout}`);

  const temporaryDirectory = fs.mkdtempSync(path.join(os.tmpdir(), 'nacre-vscode-'));
  try {
    for (const [name, snippet] of Object.entries(snippets)) {
      const source = sourceForSnippet(name, expandSnippet(snippet.body.join('\n')));
      const sourcePath = path.join(temporaryDirectory, `${name.toLowerCase().replace(/[^a-z0-9]+/g, '-')}.ncr`);
      fs.writeFileSync(sourcePath, source);
      const result = spawnSync(compiler, ['check', sourcePath], {
        cwd: repositoryRoot,
        encoding: 'utf8'
      });
      assert.ifError(result.error);
      assert.equal(result.status, 0, `snippet ${name} failed nacre check:\n${result.stderr || result.stdout}`);
    }
    const invalidTextKeySources = [
      {
        name: 'mutable text map key',
        source: `void main() {\nMap<Text,int> entries = new Map<Text,int>();\nentries.put(new Text("pear"), 1);\nfor (borrow mut Text key, int value : entries) {}\n}\n`
      },
      {
        name: 'owned text map key',
        source: `void main() {\nMap<Text,int> entries = new Map<Text,int>();\nentries.put(new Text("pear"), 1);\nfor (Text key, int value : entries) {}\n}\n`
      },
      {
        name: 'assignment to shared text map key',
        source: `void main() {\nMap<Text,int> entries = new Map<Text,int>();\nentries.put(new Text("pear"), 1);\nfor (borrow Text key, int value : entries) { key = new Text("plum"); }\n}\n`
      }
    ];
    for (const { name, source } of invalidTextKeySources) {
      const sourcePath = path.join(temporaryDirectory, `${name.replace(/[^a-z0-9]+/g, '-')}.ncr`);
      fs.writeFileSync(sourcePath, source);
      const result = spawnSync(compiler, ['check', sourcePath], {
        cwd: repositoryRoot,
        encoding: 'utf8'
      });
      assert.ifError(result.error);
      assert(
        Number.isInteger(result.status) && result.status !== 0,
        `compiler accepted ${name}:\n${result.stderr || result.stdout}`
      );
    }
  } finally {
    fs.rmSync(temporaryDirectory, { recursive: true, force: true });
  }
}

async function main() {

  const grammar = await loadGrammar();
  assert(grammar, 'source.nacre grammar was not registered');

  const sudokuSource = fs.readFileSync(path.join(repositoryRoot, 'examples/apps/sudoku.ncr'), 'utf8');
  const sudoku = tokenize(grammar, sudokuSource);
  const sudokuClassLine = sudoku.findIndex(({ line }) => line.includes('class Sudoku'));
  assert.notEqual(sudokuClassLine, -1, 'Sudoku example did not contain its class declaration');
  assertHasScope(scopesAt(sudoku, sudokuClassLine, 'Sudoku'), 'entity.name.type.class.nacre', 'class declaration name');

  const readonlyLine = sudoku.findIndex(({ line }) => line.includes('private readonly boolean canPlace'));
  assert.notEqual(readonlyLine, -1, 'Sudoku example did not contain the readonly method');
  assertHasScope(scopesAt(sudoku, readonlyLine, 'readonly'), 'storage.modifier.nacre', 'readonly ownership modifier');
  assertHasScope(scopesAt(sudoku, readonlyLine, 'canPlace'), 'entity.name.function.nacre', 'method name');

  const genericLine = sudoku.findIndex(({ line }) => line.includes('List<int> cells'));
  assert.notEqual(genericLine, -1, 'Sudoku example did not contain a generic List type');
  assertHasScope(scopesAt(sudoku, genericLine, 'List'), 'support.type.builtin.nacre', 'generic container type');
  assertHasScope(scopesAt(sudoku, genericLine, 'int'), 'storage.type.primitive.nacre', 'generic primitive type');
  assertHasScope(scopesAt(sudoku, genericLine, '<'), 'punctuation.definition.typeparameters.begin.nacre', 'generic type opening delimiter');

  const genericDeclaration = tokenize(grammar, 'class Box<T> {}');
  assertHasScope(scopesAt(genericDeclaration, 0, 'Box'), 'entity.name.type.class.nacre', 'generic class declaration name');
  assertHasScope(scopesAt(genericDeclaration, 0, 'T'), 'entity.name.type.nacre', 'generic type parameter');

  const stringTypeLine = sudoku.findIndex(({ line }) => line.includes('Sudoku(String puzzle)'));
  assert.notEqual(stringTypeLine, -1, 'Sudoku example did not contain a String parameter');
  assertHasScope(scopesAt(sudoku, stringTypeLine, 'String'), 'support.type.builtin.nacre', 'String built-in type');
  const textTypeLine = sudoku.findIndex(({ line }) => line.includes('Text digits'));
  assert.notEqual(textTypeLine, -1, 'Sudoku example did not contain a Text value');
  assertHasScope(scopesAt(sudoku, textTypeLine, 'Text'), 'support.type.builtin.nacre', 'Text built-in type');

  const memberLine = sudoku.findIndex(({ line }) => line.includes('this.cells.reserve'));
  assert.notEqual(memberLine, -1, 'Sudoku example did not contain a member call');
  assertHasScope(scopesAt(sudoku, memberLine, 'cells'), 'variable.other.property.nacre', 'member field name');
  assertHasScope(scopesAt(sudoku, memberLine, 'reserve'), 'entity.name.function.member.nacre', 'member function name');

  const ownership = tokenize(grammar, 'borrow mut Box value;');
  assertHasScope(scopesAt(ownership, 0, 'borrow'), 'storage.modifier.ownership.nacre', 'borrow modifier');
  assertHasScope(scopesAt(ownership, 0, 'mut'), 'storage.modifier.ownership.nacre', 'mut modifier');

  const nestedGeneric = tokenize(grammar, 'List<Box<List<int>>> values = new List<Box<List<int>>>();');
  assertHasScope(scopesAt(nestedGeneric, 0, 'Box'), 'entity.name.type.nacre', 'nested generic class type');
  assertHasScope(scopesAt(nestedGeneric, 0, 'int'), 'storage.type.primitive.nacre', 'nested generic primitive type');
  assertHasScope(scopesAt(nestedGeneric, 0, 'new'), 'keyword.operator.new.nacre', 'allocation keyword');

  const commentLine = tokenize(grammar, '// class readonly borrow mut while');
  const commentScopes = scopesAt(commentLine, 0, 'readonly');
  assertHasScope(commentScopes, 'comment.line.double-slash.nacre', 'line comment');
  assertLacksScope(commentScopes, 'storage.modifier', 'comment contents');

  const blockComment = tokenize(grammar, '/* class readonly borrow */ class Example {}');
  const blockScopes = scopesAt(blockComment, 0, 'readonly');
  assertHasScope(blockScopes, 'comment.block.nacre', 'block comment');
  assertLacksScope(blockScopes, 'storage.modifier', 'block comment contents');
  assertHasScope(scopesAt(blockComment, 0, 'Example'), 'entity.name.type.class.nacre', 'declaration after a block comment');

  const escapeLine = 'String value = "class readonly borrow mut while \\n \\q //";';
  const escapedString = tokenize(grammar, escapeLine);
  const stringKeywordScopes = scopesAt(escapedString, 0, 'readonly');
  assertHasScope(stringKeywordScopes, 'string.quoted.double.nacre', 'string contents');
  assertLacksScope(stringKeywordScopes, 'storage.modifier', 'string contents');
  assertHasScope(scopesAt(escapedString, 0, '\\n'), 'constant.character.escape.nacre', 'valid string escape');
  assertHasScope(scopesAt(escapedString, 0, '\\q'), 'invalid.illegal.escape.nacre', 'invalid string escape');

  const numberLine = tokenize(grammar, 'int answer = 42;');
  assertHasScope(scopesAt(numberLine, 0, '42'), 'constant.numeric.integer.decimal.nacre', 'decimal number');
  assertHasScope(scopesAt(numberLine, 0, '='), 'keyword.operator.nacre', 'operator');
  const enumAndMatch = tokenize(grammar, [
    'enum Choice<T> {',
    '    Empty;',
    '    Item(T);',
    '}',
    'void main() {',
    '    Choice<Text> choice = new Choice<Text>.Item(new Text("borrow mut readonly"));',
    '    match (choice) {',
    '        Item(value) {',
    '            // borrow mut readonly enum match Item(value)',
    '            println("borrow mut readonly enum match Item(value)");',
    '        }',
    '        Empty {}',
    '    }',
    '}'
  ].join('\n'));
  assertHasScope(scopesAt(enumAndMatch, 0, 'enum'), 'storage.type.enum.nacre', 'enum declaration keyword');
  assertHasScope(scopesAt(enumAndMatch, 0, 'Choice'), 'entity.name.type.enum.nacre', 'enum declaration name');
  assertHasScope(scopesAt(enumAndMatch, 2, 'Item'), 'entity.name.enum-variant.nacre', 'enum variant declaration');
  assertHasScope(scopesAt(enumAndMatch, 5, 'Item'), 'entity.name.enum-variant.nacre', 'variant construction');
  assertHasScope(scopesAt(enumAndMatch, 6, 'match'), 'keyword.control.nacre', 'match keyword');
  assertHasScope(scopesAt(enumAndMatch, 7, 'Item'), 'entity.name.enum-variant.nacre', 'match arm variant');
  assertHasScope(scopesAt(enumAndMatch, 7, 'value'), 'variable.parameter.match.nacre', 'consuming match binding');

  const matchCommentScopes = scopesAt(enumAndMatch, 8, 'borrow');
  assertHasScope(matchCommentScopes, 'comment.line.double-slash.nacre', 'comment inside match arm');
  assertLacksScope(matchCommentScopes, 'storage.modifier', 'ownership keyword inside match comment');
  assertLacksScope(scopesAt(enumAndMatch, 8, 'match'), 'keyword.control', 'keyword inside match comment');
  const matchStringScopes = scopesAt(enumAndMatch, 9, 'mut');
  assertHasScope(matchStringScopes, 'string.quoted.double.nacre', 'string inside match arm');
  assertLacksScope(matchStringScopes, 'storage.modifier', 'ownership keyword inside match string');
  assertLacksScope(scopesAt(enumAndMatch, 9, 'enum'), 'keyword.control', 'keyword inside match string');

  const ownershipInMatch = tokenize(grammar, [
    'void use(borrow mut Text value) {}',
    'void main() { Option<Text> item = new Option<Text>.Some(new Text("ok"));',
    'match (item) { Some(value) { use(value); } None {} }',
    '}'
  ].join('\n'));
  assertHasScope(scopesAt(ownershipInMatch, 0, 'borrow'), 'storage.modifier.ownership.nacre', 'borrow keyword in a file with match arms');
  assertHasScope(scopesAt(ownershipInMatch, 0, 'mut'), 'storage.modifier.ownership.nacre', 'mut keyword in a file with match arms');


  const traversal = tokenize(grammar, [
    'for (int value : numbers) {}',
    'for (borrow Contact contact : contacts) {}',
    'for (borrow mut Contact contact : contacts) {}'
  ].join('\n'));
  for (let line = 0; line < traversal.length; line += 1) {
    assertHasScope(scopesAt(traversal, line, 'for'), 'keyword.control.nacre', 'list traversal keyword');
    assertHasScope(scopesAt(traversal, line, ':'), 'punctuation.separator.nacre', 'traversal separator');
  }
  assertHasScope(scopesAt(traversal, 0, 'int'), 'storage.type.primitive.nacre', 'copied traversal type');
  assertHasScope(scopesAt(traversal, 1, 'borrow'), 'storage.modifier.ownership.nacre', 'shared traversal modifier');
  assertHasScope(scopesAt(traversal, 2, 'mut'), 'storage.modifier.ownership.nacre', 'exclusive traversal modifier');

  const mapTraversal = tokenize(grammar, [
    'for (int key, int value : counts) {}',
    'for (int key, borrow Contact value : entries) {}',
    'for (int key, borrow mut Contact value : entries) {}'
  ].join('\n'));
  for (let line = 0; line < mapTraversal.length; line += 1) {
    assertHasScope(scopesAt(mapTraversal, line, 'for'), 'keyword.control.nacre', 'map traversal keyword');
    assertHasScope(scopesAt(mapTraversal, line, ','), 'punctuation.separator.nacre', 'map binding separator');
    assertHasScope(scopesAt(mapTraversal, line, ':'), 'punctuation.separator.nacre', 'map traversal separator');
    assertHasScope(scopesAt(mapTraversal, line, 'key'), 'variable.other.nacre', 'map key binding');
    assertLacksScope(scopesAt(mapTraversal, line, 'key'), 'storage.modifier', 'map key binding');
    assertHasScope(scopesAt(mapTraversal, line, 'value'), 'variable.other.nacre', 'map value binding');
    assertLacksScope(scopesAt(mapTraversal, line, 'value'), 'storage.modifier', 'map value binding');
  }
  assertHasScope(scopesAt(mapTraversal, 0, 'int'), 'storage.type.primitive.nacre', 'map key type');
  assertHasScope(scopesAt(mapTraversal, 0, 'int', 1), 'storage.type.primitive.nacre', 'copied map value type');
  assertHasScope(scopesAt(mapTraversal, 1, 'borrow'), 'storage.modifier.ownership.nacre', 'shared map value modifier');
  assertHasScope(scopesAt(mapTraversal, 1, 'Contact'), 'entity.name.type.nacre', 'shared map value type');
  assertHasScope(scopesAt(mapTraversal, 2, 'borrow'), 'storage.modifier.ownership.nacre', 'exclusive map borrow modifier');
  assertHasScope(scopesAt(mapTraversal, 2, 'mut'), 'storage.modifier.ownership.nacre', 'exclusive map mut modifier');
  assertHasScope(scopesAt(mapTraversal, 2, 'Contact'), 'entity.name.type.nacre', 'exclusive map value type');
  assertLacksScope(scopesAt(mapTraversal, 1, 'Contact'), 'variable.other', 'shared map value type');
  assertLacksScope(scopesAt(mapTraversal, 2, 'Contact'), 'variable.other', 'exclusive map value type');
  const textMapTraversal = tokenize(grammar, [
    'for (borrow Text key, int count : counts) {}',
    'for (borrow Text key, borrow Counter value : counters) {}',
    'for (borrow Text key, borrow mut Counter value : counters) {} // borrow mut Text key',
    'println("borrow mut Text key");'
  ].join('\n'));
  for (let line = 0; line < 3; line += 1) {
    assertHasScope(scopesAt(textMapTraversal, line, 'for'), 'keyword.control.nacre', 'text-key traversal keyword');
    assertHasScope(scopesAt(textMapTraversal, line, 'key'), 'variable.other.nacre', 'text map key binding');
    assertLacksScope(scopesAt(textMapTraversal, line, 'key'), 'storage.modifier', 'readonly text map key binding');
    assertHasScope(scopesAt(textMapTraversal, line, line === 0 ? 'count' : 'value'), 'variable.other.nacre', 'text map value binding');
  }
  assertHasScope(scopesAt(textMapTraversal, 0, 'borrow'), 'storage.modifier.ownership.nacre', 'shared Text key modifier');
  assertHasScope(scopesAt(textMapTraversal, 0, 'Text'), 'support.type.builtin.nacre', 'Text map key type');
  assertHasScope(scopesAt(textMapTraversal, 0, 'int'), 'storage.type.primitive.nacre', 'copied Text map value type');
  assertHasScope(scopesAt(textMapTraversal, 1, 'borrow', 1), 'storage.modifier.ownership.nacre', 'shared borrowed map value modifier');
  assertHasScope(scopesAt(textMapTraversal, 1, 'Counter'), 'entity.name.type.nacre', 'shared borrowed map value type');
  assertHasScope(scopesAt(textMapTraversal, 2, 'borrow', 1), 'storage.modifier.ownership.nacre', 'exclusive borrowed map value modifier');
  assertHasScope(scopesAt(textMapTraversal, 2, 'mut'), 'storage.modifier.ownership.nacre', 'exclusive borrowed map value mut modifier');
  assertHasScope(scopesAt(textMapTraversal, 2, 'Counter'), 'entity.name.type.nacre', 'exclusive borrowed map value type');
  assertLacksScope(scopesAt(textMapTraversal, 2, 'borrow', 2), 'storage.modifier', 'commented ownership word');
  assertLacksScope(scopesAt(textMapTraversal, 2, 'mut', 1), 'storage.modifier', 'commented mut word');
  assertLacksScope(scopesAt(textMapTraversal, 3, 'borrow'), 'storage.modifier', 'string ownership word');
  assertLacksScope(scopesAt(textMapTraversal, 3, 'mut'), 'storage.modifier', 'string mut word');


  const integerMap = tokenize(grammar, [
    'Map<int,Text> entries = new Map<int,Text>();',
    'borrow mut Text value = entries.getMut(-1);',
    'match (entries.tryRemove(1)) { Some(previous) {} None {} }'
  ].join('\n'));
  assertHasScope(scopesAt(integerMap, 0, 'Map'), 'entity.name.type.nacre', 'map nominal generic type');
  assertHasScope(scopesAt(integerMap, 0, 'int'), 'storage.type.primitive.nacre', 'map integer key type');
  assertHasScope(scopesAt(integerMap, 1, 'borrow'), 'storage.modifier.ownership.nacre', 'map borrowed accessor result');
  assertHasScope(scopesAt(integerMap, 1, 'getMut'), 'entity.name.function.member.nacre', 'map exclusive lookup method');
  assertHasScope(scopesAt(integerMap, 2, 'tryRemove'), 'entity.name.function.member.nacre', 'map recoverable extraction method');
  const snippets = JSON.parse(fs.readFileSync(path.join(extensionRoot, 'snippets/nacre.json'), 'utf8'));
  for (const [name, snippet] of Object.entries(snippets)) {
    assert(typeof snippet.prefix === 'string' && snippet.prefix.length > 0, `snippet ${name} has no prefix`);
    assert(Array.isArray(snippet.body) && snippet.body.every((line) => typeof line === 'string'), `snippet ${name} has an invalid body`);
  }
  validateSnippetsWithCompiler(snippets);
  console.log(`Nacre syntax validation passed (${sudoku.length} Sudoku lines, ${Object.keys(snippets).length} snippets).`);
}

main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
