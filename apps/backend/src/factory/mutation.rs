//! Deterministic mutants for the tests specialist's fault check (factory F4,
//! ADR 94ced31c).
//!
//! A test that passes on any code proves nothing. So the source files the new
//! tests exercise get a handful of small faults (one operator flipped at a time)
//! and at least one test must fail for most of them. The mutator is ours, pure
//! and deterministic, so a rerun of the same change gets the same mutants: no
//! mutation framework is installed in the tenant's repository, and nothing here
//! runs repository code (the mutants run in a sandbox commands pod).
//!
//! It is deliberately simple. A small tokenizer skips strings, template
//! literals, comments and regular expressions; relational operators are only
//! flipped with whitespace on both sides, which leaves TypeScript generics and
//! JSX tags alone. Some mutants are equivalent (a flipped `true` in a type), which
//! is why the bar is a share of the mutants, not all of them.

use std::sync::OnceLock;

/// At most this many mutants per change: each one is a full test run in the pod.
pub const MAX_MUTANTS: usize = 10;

/// Source files larger than this are not mutated (generated or bundled code).
const MAX_TARGET_BYTES: usize = 200_000;

/// Extensions of the JavaScript and TypeScript files the specialist handles.
const JS_EXTENSIONS: &[&str] = &["ts", "tsx", "js", "jsx", "mjs", "cjs"];

/// Extensions tried, in order, for an extensionless relative import.
const RESOLVE_EXTENSIONS: &[&str] = &["ts", "tsx", "js", "jsx"];

fn extension(path: &str) -> Option<&str> {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.rsplit_once('.').map(|(_, ext)| ext)
}

fn is_js_source(path: &str) -> bool {
    extension(path).is_some_and(|ext| JS_EXTENSIONS.contains(&ext)) && !path.ends_with(".d.ts")
}

/// A JavaScript or TypeScript test file: `*.test.*`, `*.spec.*`, or any such
/// source under a `__tests__/` directory.
pub fn is_js_test_file(path: &str) -> bool {
    if !is_js_source(path) {
        return false;
    }
    let mut segments: Vec<&str> = path.split('/').collect();
    let name = segments.pop().unwrap_or_default();
    name.contains(".test.") || name.contains(".spec.") || segments.contains(&"__tests__")
}

/// What the mutator reads from a checkout. A trait so everything here is unit
/// tested without a repository.
pub trait SourceTree {
    /// A repository-relative file's content, `None` if it is not a readable file
    /// inside the repository.
    fn read(&self, path: &str) -> Option<String>;
    fn exists(&self, path: &str) -> bool;
}

/// A repository-relative path with `.` and `..` resolved; `None` when it climbs
/// above the root or is absolute.
pub fn normalize(path: &str) -> Option<String> {
    if path.starts_with('/') || path.contains('\\') || path.contains('\0') {
        return None;
    }
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            part => parts.push(part),
        }
    }
    (!parts.is_empty()).then(|| parts.join("/"))
}

/// The directory of a repository-relative path (`""` for the root).
pub fn parent(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(dir, _)| dir)
}

/// One runtime import of a relative module.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Import {
    pub specifier: String,
    /// The exported names a named import (`import { a, b as c } from …`) takes;
    /// `None` when it takes the whole module (default or namespace imports,
    /// `require`, dynamic `import()`, side-effect imports, re-exports).
    pub names: Option<Vec<String>>,
}

/// Relative modules a file imports at runtime (`import … from './x'`,
/// `import './x'`, `export … from '../y'`, `require('./x')`, `import('./x')`).
/// Type-only imports are left out: mutating a file that is only read for its
/// types can never make a test fail.
pub fn relative_imports(source: &str) -> Vec<Import> {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        regex::Regex::new(
            r#"(?m)(?:^\s*import\s+(?P<itype>type\s+)?(?:(?P<clause>[^'";]*?)\s+from\s+)?|^\s*export\s+(?P<etype>type\s+)?[^'";]*?\s+from\s+|\brequire\s*\(\s*|\bimport\s*\(\s*)['"](?P<spec>\.\.?/[^'"\n]+)['"]"#,
        )
        .expect("import pattern")
    });
    let mut found: Vec<Import> = Vec::new();
    for capture in re.captures_iter(source) {
        if capture.name("itype").is_some() || capture.name("etype").is_some() {
            continue;
        }
        let names = match capture.name("clause").map(|clause| clause.as_str().trim()) {
            Some(clause) if clause.starts_with('{') && clause.ends_with('}') => {
                // `{ a, b as c, type T }`: the exported names, inline types dropped.
                let names: Vec<String> = clause[1..clause.len() - 1]
                    .split(',')
                    .map(str::trim)
                    .filter(|name| !name.is_empty() && !name.starts_with("type "))
                    .map(|name| name.split_whitespace().next().unwrap_or_default().to_string())
                    .collect();
                if names.is_empty() {
                    continue; // Only types.
                }
                Some(names)
            }
            _ => None,
        };
        let specifier = capture["spec"].to_string();
        match found.iter_mut().find(|import| import.specifier == specifier) {
            Some(existing) => existing.names = merge_names(existing.names.take(), names),
            None => found.push(Import { specifier, names }),
        }
    }
    found
}

/// Two imports of one module: the whole module wins, else the names add up.
pub fn merge_names(a: Option<Vec<String>>, b: Option<Vec<String>>) -> Option<Vec<String>> {
    let (mut a, b) = (a?, b?);
    for name in b {
        if !a.contains(&name) {
            a.push(name);
        }
    }
    Some(a)
}

/// The file a relative import from `from_file` loads: the path itself, then with
/// each extension, then its `index` file. A `.js` specifier also finds the `.ts`
/// source next to it (TypeScript's ESM convention).
pub fn resolve_import(tree: &dyn SourceTree, from_file: &str, specifier: &str) -> Option<String> {
    let joined = normalize(&format!("{}/{specifier}", parent(from_file)))?;
    let mut candidates = Vec::new();
    if is_js_source(&joined) {
        candidates.push(joined.clone());
        if let Some(stem) = joined.strip_suffix(".js").or_else(|| joined.strip_suffix(".jsx")) {
            candidates.extend(["ts", "tsx"].iter().map(|ext| format!("{stem}.{ext}")));
        }
    }
    candidates.extend(RESOLVE_EXTENSIONS.iter().map(|ext| format!("{joined}.{ext}")));
    candidates.extend(RESOLVE_EXTENSIONS.iter().map(|ext| format!("{joined}/index.{ext}")));
    candidates.into_iter().find(|candidate| tree.exists(candidate))
}

/// A file to mutate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    pub path: String,
    /// The names the tests import from it, sorted; `None` when some test imports
    /// the whole module. Mutants stay inside these names' declarations: a test
    /// of one helper must not be scored on the rest of a large module.
    pub names: Option<Vec<String>>,
}

/// The files to mutate: non-test JS/TS sources the given tests import directly
/// through relative imports, never under `node_modules`. Sorted, so the mutants
/// do not depend on the order the tests were listed in.
pub fn mutation_targets(tree: &dyn SourceTree, tests: &[String]) -> Vec<Target> {
    let mut targets: std::collections::BTreeMap<String, Option<Vec<String>>> = std::collections::BTreeMap::new();
    for test in tests {
        let Some(source) = tree.read(test) else { continue };
        for import in relative_imports(&source) {
            let Some(path) = resolve_import(tree, test, &import.specifier) else { continue };
            let vendored = path.split('/').any(|segment| segment == "node_modules");
            if is_js_source(&path) && !is_js_test_file(&path) && !vendored {
                let names = match targets.remove(&path) {
                    Some(earlier) => merge_names(earlier, import.names),
                    None => import.names,
                };
                targets.insert(path, names);
            }
        }
    }
    targets
        .into_iter()
        .map(|(path, mut names)| {
            if let Some(names) = names.as_mut() {
                names.sort();
            }
            Target { path, names }
        })
        .collect()
}

/// The byte ranges of the top-level declarations of `names`, each from the
/// declaration to the next top-level statement. It relies only on top-level
/// code starting at column 0. `None` when no name is found (a re-export, an
/// unusual layout): the whole file is then in scope.
pub fn declaration_spans(source: &str, names: &[String]) -> Option<Vec<(usize, usize)>> {
    static NEXT: OnceLock<regex::Regex> = OnceLock::new();
    let next = NEXT.get_or_init(|| {
        regex::Regex::new(r"(?m)^(?:export|function|async|const|let|var|class|interface|type|enum|declare|import)\b")
            .expect("declaration pattern")
    });
    let mut spans = Vec::new();
    for name in names {
        let pattern = format!(
            r"(?m)^(?:export\s+)?(?:default\s+)?(?:async\s+)?(?:function\s*\*?\s*|const\s+|let\s+|var\s+|class\s+){}\b",
            regex::escape(name)
        );
        let Ok(declaration) = regex::Regex::new(&pattern) else { continue };
        if let Some(found) = declaration.find(source) {
            let end = next.find_at(source, found.end()).map_or(source.len(), |m| m.start());
            spans.push((found.start(), end));
        }
    }
    (!spans.is_empty()).then_some(spans)
}

/// One place an operator can be flipped.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Site {
    /// Byte offset of the operator in the file.
    pub offset: usize,
    /// 1-based line, for the report.
    pub line: usize,
    pub from: &'static str,
    pub to: &'static str,
}

/// Equality and logic flips, applied wherever they occur in code.
fn flip(operator: &str) -> Option<(&'static str, &'static str)> {
    Some(match operator {
        "===" => ("===", "!=="),
        "!==" => ("!==", "==="),
        "==" => ("==", "!="),
        "!=" => ("!=", "=="),
        "&&" => ("&&", "||"),
        "||" => ("||", "&&"),
        _ => return None,
    })
}

/// Relational flips (the negation, so a boundary test catches them), applied
/// only with whitespace on both sides: `Array<string>` and `<div>` are not
/// comparisons.
fn flip_relational(operator: &str) -> Option<(&'static str, &'static str)> {
    Some(match operator {
        "<" => ("<", ">="),
        ">" => (">", "<="),
        "<=" => ("<=", ">"),
        ">=" => (">=", "<"),
        _ => return None,
    })
}

fn is_ident_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'$' || byte >= 0x80
}

/// Keywords after which a `/` starts a regex literal, not a division
/// (`return /x/.test(s)`).
const REGEX_KEYWORDS: &[&str] = &[
    "return", "typeof", "case", "instanceof", "yield", "throw", "delete", "void", "in", "of", "new", "else", "do", "await",
];

/// Whether a `/` at `at` starts a regex: at the start, after an operator or
/// opening punctuation, or after one of [`REGEX_KEYWORDS`].
fn starts_regex(bytes: &[u8], at: usize) -> bool {
    let end = bytes[..at].iter().rposition(|b| !b.is_ascii_whitespace()).map_or(0, |n| n + 1);
    match end.checked_sub(1).map(|n| bytes[n]) {
        None => true,
        Some(prev) if is_ident_byte(prev) => {
            let start = bytes[..end].iter().rposition(|b| !is_ident_byte(*b)).map_or(0, |n| n + 1);
            let member = start > 0 && bytes[start - 1] == b'.';
            !member && REGEX_KEYWORDS.iter().any(|keyword| keyword.as_bytes() == &bytes[start..end])
        }
        Some(prev) => b"(,=:[!&|?{};+-*%<>~^".contains(&prev),
    }
}

/// Skips a quoted literal starting at `start` (the quote); returns the offset
/// after it. An unterminated string or regex ends at the line end.
fn skip_quoted(bytes: &[u8], start: usize, multiline: bool) -> usize {
    let quote = bytes[start];
    let mut i = start + 1;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 2,
            b'\n' if !multiline => return i,
            b if b == quote => return i + 1,
            _ => i += 1,
        }
    }
    bytes.len()
}

fn skip_regex(bytes: &[u8], start: usize) -> usize {
    let mut i = start + 1;
    let mut in_class = false;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 2,
            b'\n' => return i,
            b'[' => {
                in_class = true;
                i += 1
            }
            b']' => {
                in_class = false;
                i += 1
            }
            b'/' if !in_class => return i + 1,
            _ => i += 1,
        }
    }
    bytes.len()
}

/// Every mutation site of a file, in order. Strings, template literals,
/// comments and regex literals are skipped; so are relational operators in JSX
/// files, where `a > b` may well be text.
pub fn sites(path: &str, source: &str) -> Vec<Site> {
    let bytes = source.as_bytes();
    let jsx = matches!(extension(path), Some("tsx" | "jsx"));
    let mut found = Vec::new();
    let mut i = 0;
    let mut line = 1;
    // Lines are counted as the scan passes them, skipped literals included.
    let mut counted = 0;
    let mut line_at = |offset: usize| {
        line += bytes[counted..offset].iter().filter(|b| **b == b'\n').count();
        counted = offset;
        line
    };
    while i < bytes.len() {
        let byte = bytes[i];
        match byte {
            b'\'' | b'"' => i = skip_quoted(bytes, i, false),
            b'`' => i = skip_quoted(bytes, i, true),
            b'/' if bytes.get(i + 1) == Some(&b'/') => {
                i = bytes[i..].iter().position(|b| *b == b'\n').map_or(bytes.len(), |n| i + n);
            }
            b'/' if bytes.get(i + 1) == Some(&b'*') => {
                i = source[i + 2..].find("*/").map_or(bytes.len(), |n| i + 2 + n + 2);
            }
            b'/' => {
                i = if starts_regex(bytes, i) { skip_regex(bytes, i) } else { i + 1 };
            }
            b'=' | b'!' | b'<' | b'>' | b'&' | b'|' => {
                let end = bytes[i..]
                    .iter()
                    .position(|b| !matches!(b, b'=' | b'!' | b'<' | b'>' | b'&' | b'|'))
                    .map_or(bytes.len(), |n| i + n);
                let operator = &source[i..end];
                let spaced = i > 0
                    && bytes[i - 1].is_ascii_whitespace()
                    && bytes.get(end).is_some_and(|b| b.is_ascii_whitespace());
                let site = flip(operator).or_else(|| flip_relational(operator).filter(|_| spaced && !jsx));
                if let Some((from, to)) = site {
                    found.push(Site { offset: i, line: line_at(i), from, to });
                }
                i = end;
            }
            b if is_ident_byte(b) => {
                let end = bytes[i..].iter().position(|b| !is_ident_byte(*b)).map_or(bytes.len(), |n| i + n);
                let word = &source[i..end];
                // `x.true` is a property name, not the literal.
                let member = i > 0 && bytes[i - 1] == b'.';
                let boolean = match word {
                    "true" => Some(("true", "false")),
                    "false" => Some(("false", "true")),
                    _ => None,
                };
                if let Some((from, to)) = boolean.filter(|_| !member) {
                    found.push(Site { offset: i, line: line_at(i), from, to });
                }
                i = end;
            }
            _ => i += 1,
        }
    }
    found
}

/// One mutated file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mutant {
    pub path: String,
    pub line: usize,
    pub from: &'static str,
    pub to: &'static str,
    /// The whole file with the one change applied.
    pub content: String,
}

impl Mutant {
    pub fn describe(&self) -> String {
        format!("{}:{} `{}` -> `{}`", self.path, self.line, self.from, self.to)
    }
}

/// Up to `cap` mutants over the targets, spread across files then lines: each
/// file gets a share in turn (round robin) and a file's share is spaced evenly
/// over its sites. Same files in, same mutants out.
pub fn mutants(tree: &dyn SourceTree, targets: &[Target], cap: usize) -> Vec<Mutant> {
    let mut files: Vec<(String, String, Vec<Site>)> = Vec::new();
    let mut sorted = targets.to_vec();
    sorted.sort_by(|a, b| a.path.cmp(&b.path));
    sorted.dedup_by(|a, b| a.path == b.path);
    for target in sorted {
        let Some(source) = tree.read(&target.path).filter(|s| s.len() <= MAX_TARGET_BYTES) else { continue };
        let mut found = sites(&target.path, &source);
        if let Some(spans) = target.names.as_deref().and_then(|names| declaration_spans(&source, names)) {
            found.retain(|site| spans.iter().any(|(start, end)| (*start..*end).contains(&site.offset)));
        }
        if !found.is_empty() {
            files.push((target.path, source, found));
        }
    }
    let mut quota = vec![0usize; files.len()];
    let mut assigned = 0;
    while assigned < cap {
        let before = assigned;
        for (index, (_, _, found)) in files.iter().enumerate() {
            if assigned < cap && quota[index] < found.len() {
                quota[index] += 1;
                assigned += 1;
            }
        }
        if assigned == before {
            break;
        }
    }
    let mut out = Vec::new();
    for ((path, source, found), share) in files.iter().zip(quota) {
        for j in 0..share {
            // The centre of each of `share` equal slices of the sites.
            let site = &found[(2 * j + 1) * found.len() / (2 * share)];
            let mut content = String::with_capacity(source.len() + 2);
            content.push_str(&source[..site.offset]);
            content.push_str(site.to);
            content.push_str(&source[site.offset + site.from.len()..]);
            out.push(Mutant { path: path.clone(), line: site.line, from: site.from, to: site.to, content });
        }
    }
    out
}

/// Whether enough mutants were killed: at least 60% (ADR 94ced31c).
pub fn score_passes(killed: usize, total: usize) -> bool {
    total > 0 && killed * 10 >= total * 6
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[derive(Default)]
    struct Tree(HashMap<String, String>);

    impl Tree {
        fn with(mut self, path: &str, content: &str) -> Self {
            self.0.insert(path.into(), content.into());
            self
        }
    }

    impl SourceTree for Tree {
        fn read(&self, path: &str) -> Option<String> {
            self.0.get(path).cloned()
        }
        fn exists(&self, path: &str) -> bool {
            self.0.contains_key(path)
        }
    }

    fn ops(path: &str, source: &str) -> Vec<(&'static str, &'static str)> {
        sites(path, source).into_iter().map(|s| (s.from, s.to)).collect()
    }

    #[test]
    fn test_files_are_js_or_ts_tests_only() {
        for path in [
            "src/a.test.ts", "src/a.spec.tsx", "a.test.js", "x/a.spec.jsx", "a.test.mjs", "a.test.cjs",
            "src/__tests__/util.ts", "src/__tests__/deep/util.tsx",
        ] {
            assert!(is_js_test_file(path), "{path}");
        }
        for path in [
            "src/a.ts", "src/a.test.py", "src/a_test.go", "tests/a.rs", "src/a.test.d.ts", "src/__tests__/data.json",
            "src/a.test.ts.snap", "src/__snapshots__/a.test.ts.snap", "src/testing.ts",
        ] {
            assert!(!is_js_test_file(path), "{path}");
        }
    }

    #[test]
    fn runtime_relative_imports_are_found_and_type_imports_skipped() {
        let source = "import { a } from './a'\nimport b from \"../b.js\"\nimport './side-effect'\n\
                      import type { T } from './types'\nexport { c } from './c'\nexport type { U } from './u'\n\
                      const d = require('./d')\nconst e = await import('./e')\nimport x from 'react'\n\
                      import {\n  multi,\n  line,\n} from './multi'\n// import z from './commented'\nimport { a as again, z } from './a'\n\
                      import { type OnlyType } from './inline-type'\nimport Def, { named } from './mixed'\n";
        let owned: Vec<(String, Option<Vec<String>>)> =
            relative_imports(source).into_iter().map(|import| (import.specifier, import.names)).collect();
        let names = |list: &[&str]| Some(list.iter().map(|s| s.to_string()).collect::<Vec<_>>());
        assert_eq!(
            owned,
            [
                ("./a".to_string(), names(&["a", "z"])),
                ("../b.js".into(), None),
                ("./side-effect".into(), None),
                ("./c".into(), None),
                ("./d".into(), None),
                ("./e".into(), None),
                ("./multi".into(), names(&["multi", "line"])),
                ("./mixed".into(), None),
            ]
        );
    }

    #[test]
    fn named_imports_scope_mutants_to_their_declarations() {
        let module = "export function parseDay(s: string) {\n  return s === '' ? null : s\n}\n\n\
                      interface Form { a: boolean }\n\n\
                      export default function Page() {\n  const ok = a === b && c\n  return ok\n}\n";
        let tree = Tree::default()
            .with("src/Page.tsx", module)
            .with("src/one.test.ts", "import { parseDay } from './Page'\n")
            .with("src/all.test.tsx", "import Page from './Page'\n");
        let one = mutation_targets(&tree, &["src/one.test.ts".to_string()]);
        assert_eq!(one, [Target { path: "src/Page.tsx".into(), names: Some(vec!["parseDay".into()]) }]);
        let scoped = mutants(&tree, &one, MAX_MUTANTS);
        assert_eq!(scoped.iter().map(|m| (m.line, m.from)).collect::<Vec<_>>(), [(2, "===")]);
        // A default import (or any whole-module import) puts the file in scope.
        let both = mutation_targets(&tree, &["src/one.test.ts".to_string(), "src/all.test.tsx".to_string()]);
        assert_eq!(both[0].names, None);
        assert_eq!(mutants(&tree, &both, MAX_MUTANTS).len(), 3);
        // A name with no top-level declaration (a re-export) falls back to the file.
        assert_eq!(declaration_spans(module, &["reexported".to_string()]), None);
        let spans = declaration_spans(module, &["parseDay".to_string()]).unwrap();
        assert_eq!(&module[spans[0].0..spans[0].1], "export function parseDay(s: string) {\n  return s === '' ? null : s\n}\n\n");
    }

    #[test]
    fn imports_resolve_with_extensions_index_files_and_ts_esm_specifiers() {
        let tree = Tree::default()
            .with("src/lib/a.ts", "")
            .with("src/lib/b.tsx", "")
            .with("src/lib/c/index.ts", "")
            .with("src/lib/d.ts", "")
            .with("src/e.js", "");
        let from = "src/lib/a.test.ts";
        assert_eq!(resolve_import(&tree, from, "./a").as_deref(), Some("src/lib/a.ts"));
        assert_eq!(resolve_import(&tree, from, "./b").as_deref(), Some("src/lib/b.tsx"));
        assert_eq!(resolve_import(&tree, from, "./c").as_deref(), Some("src/lib/c/index.ts"));
        assert_eq!(resolve_import(&tree, from, "./d.js").as_deref(), Some("src/lib/d.ts"));
        assert_eq!(resolve_import(&tree, from, "../e.js").as_deref(), Some("src/e.js"));
        assert_eq!(resolve_import(&tree, from, "./missing"), None);
        assert_eq!(resolve_import(&tree, from, "../../../../etc/passwd"), None);
        assert_eq!(normalize("a/./b/../c"), Some("a/c".into()));
        assert_eq!(normalize("/etc/passwd"), None);
        assert_eq!(normalize(".."), None);
    }

    #[test]
    fn targets_are_the_imported_non_test_sources() {
        let tree = Tree::default()
            .with("src/a.test.ts", "import { a } from './a'\nimport { h } from './helpers.test'\nimport type { T } from './types'\nimport x from '../node_modules/x/index'\nimport s from './style.css'")
            .with("src/b.test.ts", "import { a } from './a'\nimport { b } from './b'")
            .with("src/a.ts", "")
            .with("src/b.ts", "")
            .with("src/types.ts", "")
            .with("src/style.css", "")
            .with("src/helpers.test.ts", "")
            .with("node_modules/x/index.js", "");
        let tests = vec!["src/b.test.ts".to_string(), "src/a.test.ts".to_string()];
        let paths: Vec<String> = mutation_targets(&tree, &tests).into_iter().map(|t| t.path).collect();
        assert_eq!(paths, ["src/a.ts", "src/b.ts"]);
    }

    #[test]
    fn operators_flip_in_code_only() {
        assert_eq!(
            ops("a.ts", "if (a === b && c !== d || e == f) return x != y ? true : false"),
            [("===", "!=="), ("&&", "||"), ("!==", "==="), ("||", "&&"), ("==", "!="), ("!=", "=="), ("true", "false"), ("false", "true")]
        );
        assert_eq!(
            ops("a.ts", "if (n < 0 || n > max || n <= 1 || n >= 2) {}"),
            [("<", ">="), ("||", "&&"), (">", "<="), ("||", "&&"), ("<=", ">"), ("||", "&&"), (">=", "<")]
        );
        // Strings, templates, comments and regexes are not code.
        let quiet = "const s = 'a === b && true'\nconst t = \"x < y\"\nconst u = `${a} === ${b} || true`\n\
                     // a === b\n/* true && false */\nconst r = /a||b/.test(s)\nconst esc = 'it\\'s === true'\n";
        assert_eq!(ops("a.ts", quiet), []);
        // Division is not a regex: the comparison after it still counts.
        assert_eq!(ops("a.ts", "const half = total / 2 === 1"), [("===", "!==")]);
        assert_eq!(ops("a.ts", "const r = x.return / 2 === 1"), [("===", "!==")]);
        // After a keyword, `/` opens a regex, whose `==` is not code.
        assert_eq!(ops("a.ts", "function f(x) { return /==/.test(x) }"), []);
        assert_eq!(ops("a.ts", "if (a) x = 1\nelse /a||b/.exec(s)\nconst t = typeof /x/ === 'object'"), [("===", "!==")]);
    }

    #[test]
    fn generics_jsx_arrows_and_compound_operators_are_left_alone() {
        let source = "const m: Map<string, Array<number>> = new Map<string, number[]>()\n\
                      const f = (x: number) => x\nlet y = a >> 1; y >>>= 2; y &&= z; y ||= w; y ??= v\n\
                      const ok = !done\nif (a<b) {}\nconst p = obj.true\n";
        assert_eq!(ops("a.ts", source), []);
        // In JSX files a spaced `>` may be text, so relational flips are off there;
        // equality and logic still flip.
        let jsx = "return <div className=\"x\">{count > 0 && <span>a > b</span>}</div>";
        assert_eq!(ops("a.tsx", jsx), [("&&", "||")]);
    }

    #[test]
    fn mutants_are_deterministic_spread_and_capped() {
        let a = (0..20).map(|n| format!("if (x === {n}) return true\n")).collect::<String>();
        let b = "export const ok = (n) => n > 0 && n < 10\n";
        let tree = Tree::default().with("src/a.ts", &a).with("src/b.ts", b).with("src/empty.ts", "export {}");
        let whole = |path: &str| Target { path: path.into(), names: None };
        let targets = vec![whole("src/b.ts"), whole("src/a.ts"), whole("src/empty.ts")];
        let first = mutants(&tree, &targets, MAX_MUTANTS);
        assert_eq!(first.len(), MAX_MUTANTS);
        assert_eq!(first, mutants(&tree, &targets, MAX_MUTANTS), "same input, same mutants");
        // b.ts has 3 sites and gets all 3; a.ts gets the other 7, spread out.
        assert_eq!(first.iter().filter(|m| m.path == "src/b.ts").count(), 3);
        let lines: Vec<usize> = first.iter().filter(|m| m.path == "src/a.ts").map(|m| m.line).collect();
        assert_eq!(lines.len(), 7);
        assert!(lines.first() < Some(&4) && lines.last() > Some(&16), "{lines:?}");
        let flipped = first.iter().find(|m| m.path == "src/b.ts" && m.from == ">").unwrap();
        assert_eq!(flipped.content, "export const ok = (n) => n <= 0 && n < 10\n");
        assert_eq!(flipped.describe(), "src/b.ts:1 `>` -> `<=`");
        // Fewer sites than the cap: every site, once.
        assert_eq!(mutants(&tree, &[whole("src/b.ts")], MAX_MUTANTS).len(), 3);
        assert!(mutants(&tree, &[whole("src/empty.ts")], MAX_MUTANTS).is_empty());
    }

    #[test]
    fn lines_count_through_skipped_literals() {
        let source = "const s = `a\nb\nc`\n/* x\ny */\nif (a === b) {}\n";
        assert_eq!(sites("a.ts", source)[0].line, 6);
    }

    #[test]
    fn sixty_percent_of_the_mutants_must_be_killed() {
        assert!(score_passes(6, 10) && score_passes(10, 10) && score_passes(3, 5));
        assert!(!score_passes(5, 10) && !score_passes(1, 2) && !score_passes(0, 0));
    }
}
