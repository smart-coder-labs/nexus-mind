//! Retrieval quality metrics for the golden questions (factory F2 exit gate).
//!
//! A question is a golden task's title and its gold set the files the accepted
//! change modified. Rankings are lists of distinct file paths, best first.

use std::collections::BTreeSet;

/// Share of gold files in the first `k` ranked files.
pub fn recall_at(ranked: &[String], gold: &BTreeSet<String>, k: usize) -> f64 {
    if gold.is_empty() {
        return 0.0;
    }
    let found = ranked.iter().take(k).filter(|f| gold.contains(*f)).count();
    found as f64 / gold.len() as f64
}

/// Whether any gold file is in the first `k`: the agent starts in the right place.
pub fn hit_at(ranked: &[String], gold: &BTreeSet<String>, k: usize) -> bool {
    ranked.iter().take(k).any(|f| gold.contains(f))
}

/// `1 / rank` of the first gold file, 0 when none is ranked.
pub fn reciprocal_rank(ranked: &[String], gold: &BTreeSet<String>) -> f64 {
    ranked
        .iter()
        .position(|f| gold.contains(f))
        .map_or(0.0, |at| 1.0 / (at + 1) as f64)
}

/// The query a golden title stands for: without its conventional-commit
/// `type(scope):` prefix, whose scope usually names the module to find.
pub fn query_from_title(title: &str) -> String {
    let trimmed = title.trim();
    let Some((head, rest)) = trimmed.split_once(':') else {
        return trimmed.to_string();
    };
    let kind = head.split('(').next().unwrap_or(head).trim_end_matches('!');
    let conventional = !kind.is_empty()
        && kind.chars().all(|c| c.is_ascii_lowercase())
        && (head.ends_with(')') || head.ends_with('!') || !head.contains(' '));
    if conventional && !rest.trim().is_empty() {
        rest.trim().to_string()
    } else {
        trimmed.to_string()
    }
}

/// Files the change modified, from `git diff --name-status` output: added files
/// did not exist when the task was posed, and a rename's gold is its old path.
pub fn modified_files(name_status: &str) -> BTreeSet<String> {
    name_status
        .lines()
        .filter_map(|line| {
            let mut fields = line.split('\t');
            let status = fields.next()?;
            let path = fields.next()?;
            (status.starts_with('M') || status.starts_with('R')).then(|| path.to_string())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(files: &[&str]) -> BTreeSet<String> {
        files.iter().map(|f| f.to_string()).collect()
    }

    fn list(files: &[&str]) -> Vec<String> {
        files.iter().map(|f| f.to_string()).collect()
    }

    #[test]
    fn recall_counts_gold_files_within_k() {
        let ranked = list(&["a", "x", "b", "y"]);
        let gold = set(&["a", "b", "c", "d"]);
        assert_eq!(recall_at(&ranked, &gold, 1), 0.25);
        assert_eq!(recall_at(&ranked, &gold, 3), 0.5);
        assert_eq!(recall_at(&ranked, &gold, 100), 0.5);
        assert_eq!(recall_at(&ranked, &set(&[]), 5), 0.0);
    }

    #[test]
    fn a_hit_needs_one_gold_file_within_k() {
        let ranked = list(&["x", "y", "b"]);
        assert!(!hit_at(&ranked, &set(&["b"]), 2));
        assert!(hit_at(&ranked, &set(&["b", "z"]), 3));
    }

    #[test]
    fn reciprocal_rank_uses_the_first_gold_file() {
        assert_eq!(
            reciprocal_rank(&list(&["x", "b", "a"]), &set(&["a", "b"])),
            0.5
        );
        assert_eq!(reciprocal_rank(&list(&["x"]), &set(&["a"])), 0.0);
    }

    #[test]
    fn titles_lose_their_conventional_prefix() {
        assert_eq!(
            query_from_title("feat(refund-receipt): raise a refund from the sale detail page"),
            "raise a refund from the sale detail page"
        );
        assert_eq!(query_from_title("fix!: drop the cache"), "drop the cache");
        assert_eq!(query_from_title("chore: bump deps"), "bump deps");
        // Not a conventional prefix: kept whole.
        assert_eq!(
            query_from_title("Sale detail: refund button missing"),
            "Sale detail: refund button missing"
        );
        assert_eq!(query_from_title("plain title"), "plain title");
    }

    #[test]
    fn gold_is_modified_and_renamed_files_only() {
        let out = "M\tsrc/a.rs\nA\tsrc/new.rs\nD\tsrc/gone.rs\nR087\tsrc/old.rs\tsrc/moved.rs\n";
        assert_eq!(modified_files(out), set(&["src/a.rs", "src/old.rs"]));
    }
}
