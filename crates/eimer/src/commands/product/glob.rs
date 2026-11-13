//! Minimal path globs for platform rules: `**` (any depth), `*` and `?` (within a segment).

use std::collections::BTreeMap;

pub fn matches(pattern: &str, path: &str) -> bool {
    match_bytes(pattern.as_bytes(), path.as_bytes())
}

fn match_bytes(pattern: &[u8], path: &[u8]) -> bool {
    match pattern {
        [] => path.is_empty(),
        [b'*', b'*', rest @ ..] => {
            // `**/x` also matches `x` at the top level.
            if let [b'/', after @ ..] = rest
                && match_bytes(after, path)
            {
                return true;
            }
            (0..=path.len()).any(|i| match_bytes(rest, &path[i..]))
        }
        [b'*', rest @ ..] => {
            for i in 0..=path.len() {
                if match_bytes(rest, &path[i..]) {
                    return true;
                }
                if path.get(i) == Some(&b'/') {
                    break;
                }
            }
            false
        }
        [b'?', rest @ ..] => {
            matches!(path.first(), Some(c) if *c != b'/') && match_bytes(rest, &path[1..])
        }
        [c, rest @ ..] => path.first() == Some(c) && match_bytes(rest, &path[1..]),
    }
}

/// A file belongs to a platform when it matches one of its globs and none of its `!` globs.
fn file_matches(globs: &[String], file: &str) -> bool {
    let (excludes, includes): (Vec<&String>, Vec<&String>) =
        globs.iter().partition(|g| g.starts_with('!'));
    includes.iter().any(|g| matches(g, file)) && !excludes.iter().any(|g| matches(&g[1..], file))
}

/// Platform tags touched by `files`, in config order.
pub fn platforms_for(rules: &BTreeMap<String, Vec<String>>, files: &[String]) -> Vec<String> {
    rules
        .iter()
        .filter(|(_, globs)| files.iter().any(|f| file_matches(globs, f)))
        .map(|(tag, _)| tag.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn star_stays_in_segment() {
        assert!(matches("src/*.rs", "src/main.rs"));
        assert!(!matches("src/*.rs", "src/a/main.rs"));
        assert!(matches("src/?.rs", "src/a.rs"));
    }

    #[test]
    fn double_star_spans_directories() {
        assert!(matches("apps/android/**", "apps/android/app/src/Main.kt"));
        assert!(matches(
            "apps/**/Mac*/**",
            "apps/ios/Sources/MacPlanner/View.swift"
        ));
        assert!(matches("**/CHANGELOG.md", "CHANGELOG.md"));
        assert!(matches("**/CHANGELOG.md", "packages/a/CHANGELOG.md"));
        assert!(!matches("apps/android/**", "apps/ios/a.swift"));
    }

    #[test]
    fn excludes_win() {
        let mut rules = BTreeMap::new();
        rules.insert(
            "iphone".to_string(),
            vec![
                "apps/ios/**".to_string(),
                "!apps/ios/**/Mac*/**".to_string(),
            ],
        );
        rules.insert("mac".to_string(), vec!["apps/ios/**/Mac*/**".to_string()]);
        let mac_only = vec!["apps/ios/Sources/MacPlanner/A.swift".to_string()];
        assert_eq!(platforms_for(&rules, &mac_only), ["mac"]);
        let both = vec![
            "apps/ios/Sources/MacPlanner/A.swift".to_string(),
            "apps/ios/Sources/Events/B.swift".to_string(),
        ];
        assert_eq!(platforms_for(&rules, &both), ["iphone", "mac"]);
    }
}
