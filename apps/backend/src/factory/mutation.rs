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

/// Relative module specifiers a file imports at runtime (`import … from './x'`,
/// `import './x'`, `export … from '../y'`, `require('./x')`, `import('./x')`).
/// Type-only imports are left out: mutating a file that is only read for its
/// types can never make a test fail.
pub fn relative_imports(source: &str) -> Vec<String> {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        regex::Regex::new(
            r#"(?m)(?:^\s*import\s+(type\s+)?(?:[^'";]*?\s+from\s+)?|^\s*export\s+(type\s+)?[^'";]*?\s+from\s+|\brequire\s*\(\s*|\bimport\s*\(\s*)['"](\.\.?/[^'"\n]+)['"]"#,
        )
        .expect("import pattern")
    });
    let mut found = Vec::new();
    for capture in re.captures_iter(source) {
        let type_only = capture.get(1).is_some() || capture.get(2).is_some();
        let specifier = capture[3].to_string();
        if !type_only && !found.contains(&specifier) {
            found.push(specifier);
        }
    }
    found
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

/// The files to mutate: non-test JS/TS sources the given tests import directly
/// through relative imports, never under `node_modules`. Sorted, so the mutants
/// do not depend on the order the tests were listed in.
pub fn mutation_targets(tree: &dyn SourceTree, tests: &[String]) -> Vec<String> {
    let mut targets = std::collections::BTreeSet::new();
    for test in tests {
        let Some(source) = tree.read(test) else { continue };
        for specifier in relative_imports(&source) {
            let Some(path) = resolve_import(tree, test, &specifier) else { continue };
            let vendored = path.split('/').any(|segment| segment == "node_modules");
            if is_js_source(&path) && !is_js_test_file(&path) && !vendored {
                targets.insert(path);
            }
        }
    }
    targets.into_iter().collect()
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

/// The last non-whitespace byte before `at`, for telling a regex from a division.
fn previous_significant(bytes: &[u8], at: usize) -> Option<u8> {
    bytes[..at].iter().rev().copied().find(|b| !b.is_ascii_whitespace())
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
                let regex = match previous_significant(bytes, i) {
                    None => true,
                    Some(prev) => b"(,=:[!&|?{};+-*%<>~^".contains(&prev),
                };
                i = if regex { skip_regex(bytes, i) } else { i + 1 };
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
pub fn mutants(tree: &dyn SourceTree, targets: &[String], cap: usize) -> Vec<Mutant> {
    let mut files: Vec<(String, String, Vec<Site>)> = Vec::new();
    let mut sorted = targets.to_vec();
    sorted.sort();
    sorted.dedup();
    for path in sorted {
        let Some(source) = tree.read(&path).filter(|s| s.len() <= MAX_TARGET_BYTES) else { continue };
        let found = sites(&path, &source);
        if !found.is_empty() {
            files.push((path, source, found));
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
                      import {\n  multi,\n  line,\n} from './multi'\n// import z from './commented'\nimport { a as again } from './a'\n";
        assert_eq!(relative_imports(source), ["./a", "../b.js", "./side-effect", "./c", "./d", "./e", "./multi"]);
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
        assert_eq!(mutation_targets(&tree, &tests), ["src/a.ts", "src/b.ts"]);
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
        let targets = vec!["src/b.ts".to_string(), "src/a.ts".into(), "src/empty.ts".into()];
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
        assert_eq!(mutants(&tree, &["src/b.ts".to_string()], MAX_MUTANTS).len(), 3);
        assert!(mutants(&tree, &["src/empty.ts".to_string()], MAX_MUTANTS).is_empty());
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
