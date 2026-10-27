//! 64-bit SimHash over word 3-shingles (plan §5.5). Stored in `items.simhash`, so the hash must be
//! stable across builds and Rust versions: FNV-1a, not `std`'s hasher.

pub const SAME_MAX_DISTANCE: u32 = 3;

fn fnv1a(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

fn words(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(|w| w.to_lowercase())
        .collect()
}

pub fn simhash(text: &str) -> u64 {
    let w = words(text);
    if w.is_empty() {
        return 0;
    }
    let mut acc = [0i32; 64];
    let mut add = |h: u64| {
        for (bit, a) in acc.iter_mut().enumerate() {
            if h >> bit & 1 == 1 {
                *a += 1;
            } else {
                *a -= 1;
            }
        }
    };
    if w.len() < 3 {
        add(fnv1a(w.join(" ").as_bytes()));
    } else {
        for s in w.windows(3) {
            add(fnv1a(s.join(" ").as_bytes()));
        }
    }
    acc.iter()
        .enumerate()
        .fold(0u64, |h, (bit, a)| if *a > 0 { h | 1 << bit } else { h })
}

pub fn distance(a: u64, b: u64) -> u32 {
    (a ^ b).count_ones()
}

pub fn same(a: u64, b: u64) -> bool {
    distance(a, b) <= SAME_MAX_DISTANCE
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &str = "Finance flagged that the travel line is 12% over. Can you send the revised sheet with the travel \
numbers split by team? Thanks, Ravi. Sure Ravi, I will send the revised sheet by Friday. I need Meera's headcount \
numbers first. Reply Forward Archive Labels Inbox Starred Snoozed Sent Drafts";

    #[test]
    fn stable_values() {
        // Pinned so a change to the algorithm (which would invalidate stored hashes) is noticed.
        assert_eq!(simhash(""), 0);
        assert_eq!(simhash("hello"), fnv1a(b"hello"));
        assert_eq!(simhash(DOC), simhash(&DOC.to_uppercase()), "case-insensitive");
    }

    #[test]
    fn small_edits_are_the_same_large_ones_are_not() {
        let tick = DOC.replace("Thanks, Ravi.", "Thanks, Ravi!"); // punctuation only
        assert!(
            same(simhash(DOC), simhash(&tick)),
            "{}",
            distance(simhash(DOC), simhash(&tick))
        );
        let other = "Sprint 45 board: onboarding checklist, empty states copy, SSO redirect loop, export timeout, pricing rollout";
        assert!(!same(simhash(DOC), simhash(other)));
        let appended = format!("{DOC} Meera: headcount is 34, sending the sheet now. Ravi: great, thanks both.");
        assert!(distance(simhash(DOC), simhash(&appended)) > 0);
    }
}
