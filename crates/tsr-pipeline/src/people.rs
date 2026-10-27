//! People resolver and suppression checks (plan §8.5, §15.5).
//!
//! Conservative by design:
//! - an e-mail address or handle identifies a person exactly;
//! - a name links to an existing person only if exactly one person has that name **and** no
//!   identifier contradicts it (same name + different e-mail = a different person);
//! - two or more people with the name → unresolved (the fact keeps the name as text);
//! - near matches ("Meera" vs "Meera Nair", same first name and e-mail domain) are only
//!   *suggested* to the user, never merged.
//!
//! Suppression wins over everything: a forgotten alias is never created, linked or stored, and
//! text that mentions one is not stored as a fact.

use std::collections::HashSet;
use std::sync::LazyLock;

use regex::Regex;
use rusqlite::Connection;
use tsr_store::StoreError;
use tsr_store::people::{self, AliasKind};

use crate::normalize::{is_word_char, normalize};

static EMAIL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[a-z0-9._%+\-]+@[a-z0-9\-]+(\.[a-z0-9\-]+)+$").expect("email regex"));

/// Not names: owner placeholders and group words the model may produce.
const NOT_NAMES: &[&str] = &[
    "me",
    "unknown",
    "you",
    "i",
    "we",
    "us",
    "everyone",
    "everybody",
    "team",
    "all",
    "someone",
    "somebody",
    "nobody",
    "user",
    "the user",
    "they",
    "them",
    "he",
    "she",
    "him",
    "her",
];

/// Normalised alias, or `None` if it cannot identify anyone.
pub fn normalize_alias(kind: AliasKind, s: &str) -> Option<String> {
    let n = normalize(s).text;
    let n = n.trim().trim_end_matches("'s").trim();
    match kind {
        AliasKind::Email => {
            let e = n.trim_start_matches("mailto:");
            EMAIL.is_match(e).then(|| e.to_owned())
        }
        AliasKind::Handle => {
            let h: String = n.chars().filter(|c| !c.is_whitespace()).collect();
            (h.chars().filter(|c| is_word_char(*c)).count() >= 2).then_some(h)
        }
        AliasKind::Name => {
            let cleaned: String = n
                .chars()
                .map(|c| {
                    if is_word_char(c) || c == '-' || c == '\'' || c == '.' {
                        c
                    } else {
                        ' '
                    }
                })
                .collect();
            let words: Vec<&str> = cleaned.split_whitespace().collect();
            let name = words.join(" ");
            (name.chars().filter(|c| is_word_char(*c)).count() >= 2
                && !NOT_NAMES.contains(&name.as_str())
                && words.len() <= 5)
                .then_some(name)
        }
    }
}

/// E-mail if it looks like one, otherwise a handle.
pub fn handle_kind(h: &str) -> AliasKind {
    match normalize_alias(AliasKind::Email, h) {
        Some(_) => AliasKind::Email,
        None => AliasKind::Handle,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PersonRef {
    SelfPerson(i64),
    Person(i64),
    /// Several people share the name, or it could not be normalised: keep the text only.
    Unresolved,
    Suppressed,
}

pub struct Suppression {
    salt: String,
    set: HashSet<(AliasKind, String)>,
}

impl Suppression {
    pub fn load(conn: &Connection) -> Result<Self, StoreError> {
        Ok(Suppression {
            salt: people::suppression_salt(conn)?,
            set: people::suppressed_digests(conn)?,
        })
    }

    pub fn contains(&self, kind: AliasKind, normalized: &str) -> bool {
        !self.set.is_empty()
            && self
                .set
                .contains(&(kind, people::alias_digest(&self.salt, kind, normalized)))
    }

    /// Does free text mention a suppressed name (any 1–4 word window) or e-mail/handle?
    pub fn mentions(&self, text: &str) -> bool {
        if self.set.is_empty() {
            return false;
        }
        let n = normalize(text).text;
        for tok in n.split_whitespace() {
            let t = tok.trim_matches(|c: char| !(is_word_char(c) || c == '@' || c == '.'));
            let t = t.trim_end_matches('.');
            if self.contains(AliasKind::Email, t) || self.contains(AliasKind::Handle, t) {
                return true;
            }
        }
        let words: Vec<&str> = n
            .split(|c: char| !(is_word_char(c) || c == '-' || c == '\''))
            .filter(|w| !w.is_empty())
            .collect();
        for len in 1..=4 {
            for w in words.windows(len) {
                let cand = w.join(" ");
                let cand = cand.trim_end_matches("'s");
                if self.contains(AliasKind::Name, cand) {
                    return true;
                }
            }
        }
        false
    }

    pub fn is_empty(&self) -> bool {
        self.set.is_empty()
    }
}

/// Resolves names within one extraction transaction.
pub struct Resolver<'c> {
    conn: &'c Connection,
    sup: &'c Suppression,
    pub self_id: i64,
    self_names: Vec<String>,
    now: i64,
    pub created: u32,
    pub suggestions: u32,
}

impl<'c> Resolver<'c> {
    /// Ensures the self person exists with the user's name and aliases.
    pub fn new(
        conn: &'c Connection,
        sup: &'c Suppression,
        user_name: &str,
        user_aliases: &[&str],
        now: i64,
    ) -> Result<Self, StoreError> {
        let mut aliases = Vec::new();
        let mut self_names = Vec::new();
        for a in std::iter::once(&user_name).chain(user_aliases.iter()) {
            let k = if normalize_alias(AliasKind::Email, a).is_some() {
                AliasKind::Email
            } else {
                AliasKind::Name
            };
            if let Some(n) = normalize_alias(k, a) {
                if k == AliasKind::Name {
                    self_names.push(n.clone());
                }
                aliases.push((k, n));
            }
        }
        let refs: Vec<(AliasKind, &str)> = aliases.iter().map(|(k, a)| (*k, a.as_str())).collect();
        let display = if user_name.trim().is_empty() {
            "Me"
        } else {
            user_name.trim()
        };
        let self_id = people::ensure_self(conn, display, &refs, now)?;
        Ok(Resolver {
            conn,
            sup,
            self_id,
            self_names,
            now,
            created: 0,
            suggestions: 0,
        })
    }

    pub fn self_names(&self) -> &[String] {
        &self.self_names
    }

    pub fn is_self(&self, name: &str) -> bool {
        let n = normalize(name).text;
        n == "me" || normalize_alias(AliasKind::Name, name).is_some_and(|a| self.self_names.contains(&a))
    }

    pub fn resolve(&mut self, name: &str, handle: Option<&str>) -> Result<PersonRef, StoreError> {
        if self.is_self(name) {
            return Ok(PersonRef::SelfPerson(self.self_id));
        }
        let norm_name = normalize_alias(AliasKind::Name, name);
        let id_alias = handle.and_then(|h| {
            let k = handle_kind(h);
            normalize_alias(k, h).map(|a| (k, a))
        });
        if norm_name
            .as_deref()
            .is_some_and(|n| self.sup.contains(AliasKind::Name, n))
            || id_alias.as_ref().is_some_and(|(k, a)| self.sup.contains(*k, a))
        {
            return Ok(PersonRef::Suppressed);
        }
        // 1. Identifier match.
        if let Some((k, a)) = &id_alias {
            let ids = people::find_by_alias(self.conn, *k, a)?;
            if let Some(&id) = ids.first() {
                if id == self.self_id {
                    return Ok(PersonRef::SelfPerson(id));
                }
                if let Some(n) = &norm_name {
                    people::add_alias(self.conn, id, AliasKind::Name, n)?;
                }
                return Ok(PersonRef::Person(id));
            }
        }
        let Some(n) = norm_name else {
            return Ok(PersonRef::Unresolved);
        };
        // 2. Name match, only when unambiguous and not contradicted by an identifier.
        let ids = people::find_by_alias(self.conn, AliasKind::Name, &n)?;
        match ids.as_slice() {
            [] => {}
            [id] => {
                let contradicts = match &id_alias {
                    Some((k, _)) => people::aliases_of(self.conn, *id)?.iter().any(|(ak, _)| ak == k),
                    None => false,
                };
                if !contradicts {
                    if let Some((k, a)) = &id_alias {
                        people::add_alias(self.conn, *id, *k, a)?;
                    }
                    return Ok(PersonRef::Person(*id));
                }
                // Same name, different e-mail/handle: a different person; suggest, don't merge.
                let new = self.create(&n, id_alias.as_ref())?;
                people::suggest_merge(self.conn, *id, new, "same_name", self.now)?;
                self.suggestions += 1;
                return Ok(PersonRef::Person(new));
            }
            _ => return Ok(PersonRef::Unresolved),
        }
        // 3. New person; suggest near matches by name prefix ("Meera" ~ "Meera Nair").
        let new = self.create(&n, id_alias.as_ref())?;
        let first = n.split(' ').next().unwrap_or("");
        let mut st = self
            .conn
            .prepare("SELECT DISTINCT person_id, alias FROM person_aliases WHERE kind = 'name' AND person_id <> ?1 AND (alias = ?2 OR alias LIKE ?3)")?;
        let near: Vec<(i64, String)> = st
            .query_map(rusqlite::params![new, first, format!("{first} %")], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })?
            .collect::<Result<_, _>>()?;
        for (other, _) in near {
            if other != self.self_id {
                people::suggest_merge(self.conn, other, new, "name_prefix", self.now)?;
                self.suggestions += 1;
            }
        }
        Ok(PersonRef::Person(new))
    }

    fn create(&mut self, name: &str, id: Option<&(AliasKind, String)>) -> Result<i64, StoreError> {
        let display: String = name
            .split(' ')
            .map(|w| {
                let mut c = w.chars();
                match c.next() {
                    Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                    None => String::new(),
                }
            })
            .collect::<Vec<_>>()
            .join(" ");
        let mut aliases = vec![(AliasKind::Name, name)];
        if let Some((k, a)) = id {
            aliases.push((*k, a.as_str()));
        }
        self.created += 1;
        people::create_person(self.conn, &display, &aliases, self.now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alias_normalisation() {
        assert_eq!(
            normalize_alias(AliasKind::Name, "  Ravi  IYER "),
            Some("ravi iyer".into())
        );
        assert_eq!(normalize_alias(AliasKind::Name, "Meera's"), Some("meera".into()));
        assert_eq!(normalize_alias(AliasKind::Name, "me"), None);
        assert_eq!(normalize_alias(AliasKind::Name, "Unknown"), None);
        assert_eq!(normalize_alias(AliasKind::Name, "J"), None);
        assert_eq!(
            normalize_alias(AliasKind::Email, "Ravi.Iyer@Northwind.example"),
            Some("ravi.iyer@northwind.example".into())
        );
        assert_eq!(normalize_alias(AliasKind::Email, "not an email"), None);
        assert_eq!(handle_kind("@meera"), AliasKind::Handle);
        assert_eq!(handle_kind("a@b.co"), AliasKind::Email);
    }
}
