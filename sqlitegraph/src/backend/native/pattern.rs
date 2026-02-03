//! Glob pattern matching utility for pub/sub enhancements
//!
//! Provides simple glob matching with `*` (wildcard) and `?` (single char) support.

/// Match a text string against a glob pattern
///
/// # Pattern Syntax
/// - `*` matches any sequence of characters (including empty)
/// - `?` matches exactly one character
/// - `\*` and `\?` match literal asterisk and question mark
///
/// # Examples
/// ```
/// assert!(glob_matches("agent-*", "agent-123"));
/// assert!(glob_matches("user:???", "user:abc"));
/// assert!(glob_matches("literal\\*", "literal*"));
/// assert!(!glob_matches("agent-*", "user-123"));
/// ```
pub fn glob_matches(pattern: &str, text: &str) -> bool {
    glob_matches_impl(pattern, text)
}

/// Internal recursive glob matcher
fn glob_matches_impl(pattern: &str, text: &str) -> bool {
    let mut pattern_chars = pattern.chars().peekable();
    let mut text_chars = text.chars().peekable();

    while let Some(pc) = pattern_chars.next() {
        match pc {
            '*' => {
                // Greedy match: consume all until next pattern char matches
                // Collect remaining pattern chars
                let next_pattern: String = pattern_chars.collect();
                if next_pattern.is_empty() {
                    // Trailing * matches everything
                    return true;
                }
                // Try to match rest of pattern at each position
                while text_chars.peek().is_some() {
                    let remaining: String = text_chars.clone().collect();
                    if glob_matches_impl(&next_pattern, &remaining) {
                        return true;
                    }
                    text_chars.next();
                }
                // Also check if empty text matches (e.g., "a*" matches "a")
                return glob_matches_impl(&next_pattern, "");
            }
            '?' => {
                if text_chars.next().is_none() {
                    return false;
                }
            }
            '\\' => {
                // Escaped character
                if let Some(literal) = pattern_chars.next() {
                    if text_chars.next() != Some(literal) {
                        return false;
                    }
                } else {
                    // Trailing backslash - invalid but treat as literal
                    return text_chars.next() == Some('\\');
                }
            }
            c => {
                if text_chars.next() != Some(c) {
                    return false;
                }
            }
        }
    }

    // All pattern consumed - check if all text consumed
    text_chars.next().is_none()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_glob_matches_exact() {
        assert!(glob_matches("agent-123", "agent-123"));
        assert!(glob_matches("", ""));
    }

    #[test]
    fn test_glob_matches_wildcard() {
        assert!(glob_matches("agent-*", "agent-123"));
        assert!(glob_matches("agent-*", "agent-"));
        assert!(glob_matches("agent-*", "agent-abc"));
        assert!(!glob_matches("agent-*", "user-123"));
        assert!(!glob_matches("agent-*", "agent-123-extra"));
    }

    #[test]
    fn test_glob_matches_single_char() {
        assert!(glob_matches("agent-???", "agent-123"));
        assert!(glob_matches("agent-???", "agent-abc"));
        assert!(!glob_matches("agent-???", "agent-12"));
        assert!(!glob_matches("agent-???", "agent-1234"));
    }

    #[test]
    fn test_glob_matches_combined() {
        assert!(glob_matches("*-test", "agent-test"));
        assert!(glob_matches("*-test", "123-test"));
        assert!(glob_matches("user:*:msg", "user:123:msg"));
        assert!(!glob_matches("user:*:msg", "user:123:data"));
    }

    #[test]
    fn test_glob_matches_escape() {
        assert!(glob_matches(r"literal\*", "literal*"));
        assert!(glob_matches(r"literal\?", "literal?"));
        assert!(!glob_matches(r"literal\*", "literalX"));
    }

    #[test]
    fn test_glob_matches_multiple_wildcards() {
        assert!(glob_matches("user-*-msg-*", "user-123-msg-456"));
        assert!(glob_matches("*", "anything"));
        assert!(glob_matches("***", "anything"));
    }

    #[test]
    fn test_glob_matches_empty_text() {
        assert!(glob_matches("", ""));
        assert!(glob_matches("*", ""));
        assert!(!glob_matches("?", ""));
    }
}
