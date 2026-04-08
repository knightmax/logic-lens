use serde::Serialize;

/// Result of a text-based diff for unsupported languages.
#[derive(Debug, Serialize)]
pub struct TextDiffResult {
    pub old_path: String,
    pub new_path: String,
    pub added_lines: usize,
    pub removed_lines: usize,
    pub changed_lines: Vec<TextChange>,
    pub warning: String,
}

#[derive(Debug, Serialize)]
pub struct TextChange {
    pub kind: TextChangeKind,
    pub line: usize,
    pub content: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TextChangeKind {
    Added,
    Removed,
}

/// Compute a simple line-based diff between two source texts.
pub fn text_diff(old_source: &str, new_source: &str) -> (usize, usize, Vec<TextChange>) {
    let old_lines: Vec<&str> = old_source.lines().collect();
    let new_lines: Vec<&str> = new_source.lines().collect();

    let mut changes = Vec::new();
    let mut added = 0usize;
    let mut removed = 0usize;

    // Simple LCS-based diff
    let lcs = longest_common_subsequence(&old_lines, &new_lines);

    let mut oi = 0;
    let mut ni = 0;
    let mut li = 0;

    while oi < old_lines.len() || ni < new_lines.len() {
        if li < lcs.len() && oi < old_lines.len() && old_lines[oi] == lcs[li] {
            if ni < new_lines.len() && new_lines[ni] == lcs[li] {
                // Common line
                oi += 1;
                ni += 1;
                li += 1;
            } else if ni < new_lines.len() {
                changes.push(TextChange {
                    kind: TextChangeKind::Added,
                    line: ni + 1,
                    content: new_lines[ni].to_string(),
                });
                added += 1;
                ni += 1;
            }
        } else if oi < old_lines.len() && (li >= lcs.len() || old_lines[oi] != lcs[li]) {
            changes.push(TextChange {
                kind: TextChangeKind::Removed,
                line: oi + 1,
                content: old_lines[oi].to_string(),
            });
            removed += 1;
            oi += 1;
        } else if ni < new_lines.len() {
            changes.push(TextChange {
                kind: TextChangeKind::Added,
                line: ni + 1,
                content: new_lines[ni].to_string(),
            });
            added += 1;
            ni += 1;
        } else {
            break;
        }
    }

    (added, removed, changes)
}

fn longest_common_subsequence<'a>(a: &[&'a str], b: &[&'a str]) -> Vec<&'a str> {
    let m = a.len();
    let n = b.len();
    let mut dp = vec![vec![0usize; n + 1]; m + 1];

    for i in 1..=m {
        for j in 1..=n {
            if a[i - 1] == b[j - 1] {
                dp[i][j] = dp[i - 1][j - 1] + 1;
            } else {
                dp[i][j] = dp[i - 1][j].max(dp[i][j - 1]);
            }
        }
    }

    // Backtrack
    let mut result = Vec::new();
    let mut i = m;
    let mut j = n;
    while i > 0 && j > 0 {
        if a[i - 1] == b[j - 1] {
            result.push(a[i - 1]);
            i -= 1;
            j -= 1;
        } else if dp[i - 1][j] > dp[i][j - 1] {
            i -= 1;
        } else {
            j -= 1;
        }
    }
    result.reverse();
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_text_diff_identical() {
        let (added, removed, changes) = text_diff("hello\nworld", "hello\nworld");
        assert_eq!(added, 0);
        assert_eq!(removed, 0);
        assert!(changes.is_empty());
    }

    #[test]
    fn test_text_diff_additions() {
        let (added, removed, _changes) = text_diff("hello", "hello\nworld");
        assert_eq!(added, 1);
        assert_eq!(removed, 0);
    }

    #[test]
    fn test_text_diff_removals() {
        let (added, removed, _changes) = text_diff("hello\nworld", "hello");
        assert_eq!(added, 0);
        assert_eq!(removed, 1);
    }
}
