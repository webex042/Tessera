//! Case-insensitive glob matching: `*` matches any run of characters, `?` exactly one.

pub fn matches(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.to_lowercase().chars().collect();
    let t: Vec<char> = text.to_lowercase().chars().collect();
    // Iterative matcher with single backtrack point (linear for patterns with only `*`).
    let (mut pi, mut ti) = (0usize, 0usize);
    let mut star: Option<(usize, usize)> = None;
    while ti < t.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some((pi, ti));
            pi += 1;
        } else if let Some((sp, st)) = star {
            pi = sp + 1;
            ti = st + 1;
            star = Some((sp, st + 1));
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

#[cfg(test)]
mod tests {
    use super::matches;

    #[test]
    fn basics() {
        assert!(matches("KeePass*.exe", "keepassxc.exe"));
        assert!(matches("KeePass*.exe", "KeePass.exe"));
        assert!(!matches("KeePass*.exe", "notkeepass.exe"));
        assert!(matches("*.icicibank.com", "infinity.icicibank.com"));
        assert!(!matches("*.icicibank.com", "icicibank.com.evil.example"));
        assert!(matches("a?c", "abc"));
        assert!(matches("*", ""));
        assert!(!matches("", "x"));
    }
}
