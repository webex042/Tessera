//! Window blocklists (plan §6.1).
//!
//! Evaluated on every foreground change, before any capture call. A rule matches on the process
//! image, the window title or the browser URL. Title rules carry a **scope**: generic words such as
//! "sign in", "login" or "password" appear in many ordinary window titles (a document called
//! "Login flow redesign", a source file `password_reset.rs`), so they apply to browser windows
//! only; outside browsers only narrow phrases apply, because the process blocklist and password
//! masking (§6.2) already cover credential prompts there.
//!
//! A browser window whose private-browsing state cannot be determined is treated as private and
//! blocked (fail closed).
//!
//! Decisions carry the rule's stable `key`, never the title or URL, so they can be written to the
//! audit log and counted per rule without storing content.

use regex::Regex;

use crate::glob;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RuleKind {
    Process,
    Title,
    Url,
}

impl RuleKind {
    pub fn as_str(self) -> &'static str {
        match self {
            RuleKind::Process => "process",
            RuleKind::Title => "title",
            RuleKind::Url => "url",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "process" => Some(RuleKind::Process),
            "title" => Some(RuleKind::Title),
            "url" => Some(RuleKind::Url),
            _ => None,
        }
    }
}

/// Which windows a rule applies to. Only meaningful for title rules: process rules name a
/// process, and URL rules only ever see browser windows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Scope {
    All,
    Browser,
}

impl Scope {
    pub fn as_str(self) -> &'static str {
        match self {
            Scope::All => "all",
            Scope::Browser => "browser",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "all" => Some(Scope::All),
            "browser" => Some(Scope::Browser),
            _ => None,
        }
    }
}

/// A rule as stored and edited. `key` is stable and content-free (`builtin.title.otp`,
/// `user.7`); it is what the audit log records.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuleSpec {
    pub key: String,
    pub kind: RuleKind,
    pub scope: Scope,
    /// Title rules: a regex if `is_regex`, else a case-insensitive substring.
    /// Process rules: a glob on the image file name, or on the full path if it contains `\`.
    /// URL rules: a glob on the host (`*.bank.example`), host + path (`vault.example/app*`), or the
    /// whole URL if it contains `://` (`chrome://*`).
    pub pattern: String,
    pub is_regex: bool,
    pub builtin: bool,
    pub enabled: bool,
}

/// Private-browsing state as determined by the capture layer (browser-specific UIA signals).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrivateState {
    Private,
    NotPrivate,
    Unknown,
}

/// What the capture layer knows about the foreground window. Borrowed: nothing is stored.
#[derive(Clone, Copy)]
pub struct WindowFacts<'a> {
    /// Image file name, e.g. `chrome.exe`.
    pub exe: &'a str,
    /// Full image path, if known.
    pub exe_path: Option<&'a str>,
    pub title: &'a str,
    /// Address-bar URL for browser windows, if read.
    pub url: Option<&'a str>,
    /// Only consulted for browsers.
    pub private: PrivateState,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockReason {
    Process,
    Title,
    Url,
    PrivateBrowsing,
}

impl BlockReason {
    pub fn as_str(self) -> &'static str {
        match self {
            BlockReason::Process => "process",
            BlockReason::Title => "title",
            BlockReason::Url => "url",
            BlockReason::PrivateBrowsing => "private_browsing",
        }
    }
}

/// Content-free outcome. `rule_key` is `None` for the built-in fail-closed private check.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Decision {
    Allow,
    Block {
        reason: BlockReason,
        rule_key: Option<String>,
    },
}

impl Decision {
    pub fn is_blocked(&self) -> bool {
        matches!(self, Decision::Block { .. })
    }
}

/// Key used for the fail-closed private-browsing block, so it can be counted like a rule.
pub const PRIVATE_BROWSING_KEY: &str = "builtin.private.undetermined_or_private";

/// Browsers whose windows get browser-scoped rules, URL rules and the private-state check.
pub const KNOWN_BROWSERS: &[&str] = &[
    "chrome.exe",
    "msedge.exe",
    "firefox.exe",
    "brave.exe",
    "opera.exe",
    "vivaldi.exe",
    "arc.exe",
    "librewolf.exe",
    "waterfox.exe",
    "chromium.exe",
];

pub fn is_browser(exe: &str) -> bool {
    KNOWN_BROWSERS.iter().any(|b| b.eq_ignore_ascii_case(exe))
}

enum Matcher {
    Regex(Regex),
    Substring(String),
    Glob(String),
}

struct Compiled {
    key: String,
    kind: RuleKind,
    scope: Scope,
    matcher: Matcher,
    full_path: bool,
}

/// A user rule that failed to compile. Reported by key only; the other rules still apply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidRule {
    pub key: String,
}

pub struct Blocklist {
    rules: Vec<Compiled>,
}

impl Blocklist {
    /// Compile the enabled rules. Invalid ones are skipped and returned, so one bad user regex
    /// never switches off the rest of the blocklist.
    pub fn compile(specs: &[RuleSpec]) -> (Self, Vec<InvalidRule>) {
        let mut rules = Vec::new();
        let mut invalid = Vec::new();
        for s in specs.iter().filter(|s| s.enabled) {
            let matcher = match (s.kind, s.is_regex) {
                (RuleKind::Title, true) => match Regex::new(&format!("(?i){}", s.pattern)) {
                    Ok(r) => Matcher::Regex(r),
                    Err(_) => {
                        invalid.push(InvalidRule { key: s.key.clone() });
                        continue;
                    }
                },
                (RuleKind::Title, false) => Matcher::Substring(s.pattern.to_lowercase()),
                (RuleKind::Process | RuleKind::Url, _) => Matcher::Glob(s.pattern.to_lowercase()),
            };
            rules.push(Compiled {
                key: s.key.clone(),
                kind: s.kind,
                scope: s.scope,
                full_path: s.kind == RuleKind::Process && s.pattern.contains('\\'),
                matcher,
            });
        }
        (Blocklist { rules }, invalid)
    }

    pub fn check(&self, w: &WindowFacts<'_>) -> Decision {
        let browser = is_browser(w.exe);

        // 1. Process rules: a blocked app is never read at all.
        for r in self.rules.iter().filter(|r| r.kind == RuleKind::Process) {
            let subject = if r.full_path { w.exe_path.unwrap_or("") } else { w.exe };
            if let Matcher::Glob(g) = &r.matcher
                && glob::matches(g, subject)
            {
                return block(BlockReason::Process, &r.key);
            }
        }

        // 2. Private browsing, fail closed.
        if browser && w.private != PrivateState::NotPrivate {
            return Decision::Block {
                reason: BlockReason::PrivateBrowsing,
                rule_key: Some(PRIVATE_BROWSING_KEY.into()),
            };
        }

        // 3. Title rules, honouring scope.
        let title_lc = w.title.to_lowercase();
        for r in self.rules.iter().filter(|r| r.kind == RuleKind::Title) {
            if r.scope == Scope::Browser && !browser {
                continue;
            }
            let hit = match &r.matcher {
                Matcher::Regex(re) => re.is_match(w.title),
                Matcher::Substring(s) => title_lc.contains(s.as_str()),
                Matcher::Glob(_) => false,
            };
            if hit {
                return block(BlockReason::Title, &r.key);
            }
        }

        // 4. URL rules (browsers only; other apps have no URL).
        if browser && let Some(url) = w.url {
            let url_lc = url.trim().to_lowercase();
            let (host, rest) = split_url(&url_lc);
            for r in self.rules.iter().filter(|r| r.kind == RuleKind::Url) {
                let Matcher::Glob(g) = &r.matcher else { continue };
                let subject = if g.contains("://") {
                    url_lc.clone()
                } else if g.contains('/') {
                    format!("{host}{rest}")
                } else {
                    host.to_owned()
                };
                if glob::matches(g, &subject) {
                    return block(BlockReason::Url, &r.key);
                }
            }
        }
        Decision::Allow
    }
}

fn block(reason: BlockReason, key: &str) -> Decision {
    Decision::Block {
        reason,
        rule_key: Some(key.to_owned()),
    }
}

/// Split a lowercased URL into (host, path-and-rest). Userinfo and port are dropped from the
/// host so `user@bank.example:443` still matches `bank.example`.
fn split_url(url: &str) -> (&str, &str) {
    let after_scheme = url.split_once("://").map_or(url, |(_, r)| r);
    let (authority, rest) = match after_scheme.find(['/', '?', '#']) {
        Some(i) => after_scheme.split_at(i),
        None => (after_scheme, ""),
    };
    let host = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
    let host = if host.starts_with('[') {
        host.split_once(']').map_or(host, |(h, _)| h.trim_start_matches('['))
    } else {
        host.split_once(':').map_or(host, |(h, _)| h)
    };
    (host, rest)
}

/// Built-in rules (plan §6.1 as updated in v0.2). The user can disable any of them and add more.
pub fn builtin_rules() -> Vec<RuleSpec> {
    let mut v = Vec::new();
    let mut add = |key: &str, kind: RuleKind, scope: Scope, pattern: &str, is_regex: bool, enabled: bool| {
        v.push(RuleSpec {
            key: format!("builtin.{key}"),
            kind,
            scope,
            pattern: pattern.into(),
            is_regex,
            builtin: true,
            enabled,
        });
    };

    // Password managers, credential prompts, Windows Security.
    for (k, p) in [
        ("process.keepass", "KeePass*.exe"),
        ("process.1password", "1Password.exe"),
        ("process.bitwarden", "Bitwarden.exe"),
        ("process.dashlane", "Dashlane*.exe"),
        ("process.lastpass", "LastPass*.exe"),
        ("process.credui", "CredentialUIBroker.exe"),
        ("process.consent", "consent.exe"),
        ("process.windows_security", "SecHealthUI.exe"),
    ] {
        add(k, RuleKind::Process, Scope::All, p, false, true);
    }
    // Remote desktop sessions can show anything; off by default, user can turn it on.
    add(
        "process.mstsc",
        RuleKind::Process,
        Scope::All,
        "mstsc.exe",
        false,
        false,
    );

    // Title, all apps: narrow phrases only.
    add("title.otp", RuleKind::Title, Scope::All, r"\bOTP\b", true, true);
    add(
        "title.one_time_code",
        RuleKind::Title,
        Scope::All,
        r"\bone[- ]time (pass)?code\b",
        true,
        true,
    );
    add(
        "title.password_action",
        RuleKind::Title,
        Scope::All,
        r"\b(enter|change|reset) (your )?password\b",
        true,
        true,
    );

    // Title, browsers only: generic words and private-mode markers.
    add(
        "title.inprivate",
        RuleKind::Title,
        Scope::Browser,
        "InPrivate",
        false,
        true,
    );
    add(
        "title.incognito",
        RuleKind::Title,
        Scope::Browser,
        "Incognito",
        false,
        true,
    );
    add(
        "title.private_browsing",
        RuleKind::Title,
        Scope::Browser,
        "Private Browsing",
        false,
        true,
    );
    add(
        "title.sign_in",
        RuleKind::Title,
        Scope::Browser,
        r"\bsign[ -]?in\b",
        true,
        true,
    );
    add(
        "title.log_in",
        RuleKind::Title,
        Scope::Browser,
        r"\blog ?in\b",
        true,
        true,
    );
    add(
        "title.password",
        RuleKind::Title,
        Scope::Browser,
        r"\bpassword\b",
        true,
        true,
    );

    // URLs: identity providers, browser internals, password vaults, banks and UPI.
    // The bank list is a curated starting point (review before beta; plan §6.1).
    for (k, p) in [
        ("url.google_accounts", "accounts.google.com"),
        ("url.microsoft_login", "login.microsoftonline.com"),
        ("url.microsoft_live", "login.live.com"),
        ("url.chrome_internal", "chrome://*"),
        ("url.edge_internal", "edge://*"),
        ("url.about", "about:*"),
        ("url.bitwarden_vault", "vault.bitwarden.com"),
        ("url.1password", "*.1password.com"),
        ("url.lastpass", "*.lastpass.com"),
        ("url.generic_bank", "*.bank"),
        ("url.generic_bank_sub", "*.bank.*"),
        ("url.sbi", "*onlinesbi.sbi"),
        ("url.hdfc", "*hdfcbank.com"),
        ("url.icici", "*icicibank.com"),
        ("url.axis", "*axisbank.com"),
        ("url.kotak", "*kotak.com"),
        ("url.pnb", "*pnbindia.in"),
        ("url.bob", "*bankofbaroda.in"),
        ("url.yes", "*yesbank.in"),
        ("url.idfc", "*idfcfirstbank.com"),
        ("url.indusind", "*indusind.com"),
        ("url.npci", "*npci.org.in"),
        ("url.paytm", "*paytm.com"),
        ("url.phonepe", "*phonepe.com"),
        ("url.gpay", "pay.google.com"),
    ] {
        add(k, RuleKind::Url, Scope::Browser, p, false, true);
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    fn defaults() -> Blocklist {
        let (b, invalid) = Blocklist::compile(&builtin_rules());
        assert!(invalid.is_empty(), "builtin rules must compile: {invalid:?}");
        b
    }

    fn app<'a>(exe: &'a str, title: &'a str) -> WindowFacts<'a> {
        WindowFacts {
            exe,
            exe_path: None,
            title,
            url: None,
            private: PrivateState::Unknown,
        }
    }

    fn browser<'a>(exe: &'a str, title: &'a str, url: Option<&'a str>) -> WindowFacts<'a> {
        WindowFacts {
            exe,
            exe_path: None,
            title,
            url,
            private: PrivateState::NotPrivate,
        }
    }

    fn key(d: &Decision) -> Option<&str> {
        match d {
            Decision::Block { rule_key, .. } => rule_key.as_deref(),
            Decision::Allow => None,
        }
    }

    /// Stage 1 regression test: ordinary non-browser windows whose titles contain generic
    /// words are captured, not blocked.
    #[test]
    fn generic_words_do_not_block_non_browser_windows() {
        let b = defaults();
        for (exe, title) in [
            ("WINWORD.EXE", "Login flow redesign.docx - Word"),
            ("Code.exe", "password_reset.rs - platform - Visual Studio Code"),
            ("OUTLOOK.EXE", "Re: Sign in page review - Message (HTML)"),
            ("notepad.exe", "How to log in.txt - Notepad"),
            ("EXCEL.EXE", "Password policy audit.xlsx - Excel"),
            ("slack.exe", "#auth-login - Northwind - Slack"),
            ("ms-teams.exe", "Chat | Signin bugs | Microsoft Teams"),
        ] {
            assert_eq!(b.check(&app(exe, title)), Decision::Allow, "{exe}: {title}");
        }
    }

    #[test]
    fn generic_words_block_browser_windows() {
        let b = defaults();
        for (title, k) in [
            ("Sign in - Google Accounts - Google Chrome", "builtin.title.sign_in"),
            ("Sign-in | Example - Google Chrome", "builtin.title.sign_in"),
            ("Log in to Example - Personal - Microsoft Edge", "builtin.title.log_in"),
            ("Login | Shop - Google Chrome", "builtin.title.log_in"),
            ("Forgot password? - Google Chrome", "builtin.title.password"),
        ] {
            let d = b.check(&browser("chrome.exe", title, Some("https://shop.example/x")));
            assert_eq!(key(&d), Some(k), "{title}");
        }
    }

    #[test]
    fn narrow_phrases_block_everywhere() {
        let b = defaults();
        for (exe, title, k) in [
            (
                "SystemSettings.exe",
                "Change your password",
                "builtin.title.password_action",
            ),
            ("someapp.exe", "Reset password", "builtin.title.password_action"),
            ("someapp.exe", "Enter OTP", "builtin.title.otp"),
            ("someapp.exe", "Your one-time code", "builtin.title.one_time_code"),
        ] {
            assert_eq!(key(&b.check(&app(exe, title))), Some(k), "{title}");
        }
        // "OTP" must be a word on its own.
        assert_eq!(b.check(&app("someapp.exe", "Hotpot recipes")), Decision::Allow);
    }

    #[test]
    fn private_browsing_fails_closed() {
        let b = defaults();
        let mut w = browser("msedge.exe", "News - Microsoft Edge", Some("https://news.example/"));
        assert_eq!(b.check(&w), Decision::Allow);
        w.private = PrivateState::Unknown;
        assert_eq!(key(&b.check(&w)), Some(PRIVATE_BROWSING_KEY));
        w.private = PrivateState::Private;
        assert!(matches!(
            b.check(&w),
            Decision::Block {
                reason: BlockReason::PrivateBrowsing,
                ..
            }
        ));
        // Title marker catches InPrivate even when the UIA signal said not private.
        let w = browser("msedge.exe", "New tab - [InPrivate] - Microsoft Edge", None);
        assert_eq!(key(&b.check(&w)), Some("builtin.title.inprivate"));
        // Unknown private state never blocks non-browsers.
        assert_eq!(b.check(&app("WINWORD.EXE", "Notes - Word")), Decision::Allow);
    }

    #[test]
    fn password_managers_and_prompts_are_blocked() {
        let b = defaults();
        for exe in [
            "KeePassXC.exe",
            "keepass.exe",
            "1Password.exe",
            "consent.exe",
            "CredentialUIBroker.exe",
        ] {
            assert!(
                matches!(
                    b.check(&app(exe, "anything")),
                    Decision::Block {
                        reason: BlockReason::Process,
                        ..
                    }
                ),
                "{exe}"
            );
        }
        assert_eq!(
            b.check(&app("mstsc.exe", "Remote Desktop")),
            Decision::Allow,
            "mstsc is off by default"
        );
    }

    #[test]
    fn url_rules() {
        let b = defaults();
        let blocked = |u: &str| b.check(&browser("chrome.exe", "Page", Some(u))).is_blocked();
        assert!(blocked("https://accounts.google.com/v3/signin/identifier?x=1"));
        assert!(blocked("https://netbanking.hdfcbank.com/netbanking/"));
        assert!(blocked("https://infinity.icicibank.com/corp/"));
        assert!(blocked("https://user@retail.onlinesbi.sbi:443/retail/login.htm"));
        assert!(blocked("chrome://settings/passwords"));
        assert!(blocked("edge://wallet"));
        assert!(blocked("https://www.mybank.bank/"));
        assert!(!blocked("https://mail.google.com/mail/u/0/"));
        assert!(
            !blocked("https://hdfcbank.com.evil.example/"),
            "host suffix only, not substring"
        );
        assert!(!blocked("https://news.example/banking-trends"));
        // URL rules never apply to non-browsers (they have no URL).
        let w = WindowFacts {
            url: Some("https://accounts.google.com"),
            ..app("WINWORD.EXE", "Doc")
        };
        assert_eq!(b.check(&w), Decision::Allow);
    }

    #[test]
    fn disabled_and_user_rules() {
        let mut specs = builtin_rules();
        for s in specs.iter_mut().filter(|s| s.key == "builtin.title.sign_in") {
            s.enabled = false;
        }
        specs.push(RuleSpec {
            key: "user.1".into(),
            kind: RuleKind::Process,
            scope: Scope::All,
            pattern: "MyBankApp.exe".into(),
            is_regex: false,
            builtin: false,
            enabled: true,
        });
        specs.push(RuleSpec {
            key: "user.2".into(),
            kind: RuleKind::Title,
            scope: Scope::All,
            pattern: "(unclosed".into(),
            is_regex: true,
            builtin: false,
            enabled: true,
        });
        let (b, invalid) = Blocklist::compile(&specs);
        assert_eq!(invalid, vec![InvalidRule { key: "user.2".into() }]);
        assert_eq!(key(&b.check(&app("mybankapp.exe", "x"))), Some("user.1"));
        assert_eq!(
            b.check(&browser("chrome.exe", "Sign in - Shop - Google Chrome", None)),
            Decision::Allow
        );
        // The invalid rule did not switch anything else off.
        assert!(b.check(&app("KeePassXC.exe", "x")).is_blocked());
    }

    #[test]
    fn full_path_process_rule() {
        let specs = vec![RuleSpec {
            key: "user.3".into(),
            kind: RuleKind::Process,
            scope: Scope::All,
            pattern: r"C:\Program Files\Bank\*".into(),
            is_regex: false,
            builtin: false,
            enabled: true,
        }];
        let (b, _) = Blocklist::compile(&specs);
        let w = WindowFacts {
            exe_path: Some(r"C:\Program Files\Bank\client.exe"),
            ..app("client.exe", "x")
        };
        assert!(b.check(&w).is_blocked());
    }

    #[test]
    fn decisions_carry_no_content() {
        let b = defaults();
        let d = b.check(&browser(
            "chrome.exe",
            "Sign in - SECRET-TITLE-CANARY",
            Some("https://x.example/"),
        ));
        assert!(!format!("{d:?}").contains("CANARY"));
    }
}
