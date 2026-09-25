//! Ordered sources with a raw priority, and name lookup across them.
//!
//! Cards, skills, themes, defaults and plugin commands each come from a list
//! of sources. Each list used its own rule for "which one wins", and the rules
//! disagreed. This module holds the one rule, as Vim's `runtimepath` does.
//!
//! A source has a name, a priority and a value. A higher priority wins. The
//! caller enumerates its entries per source, and this module names them:
//!
//! - A full name `source:name` always reaches its entry.
//! - A bare name goes to the entry of the highest source that holds it.
//! - Two entries at the same priority make the bare name ambiguous.
//!
//! The util never merges and never reads the filesystem. The caller filters
//! out the sources an asset may not reach before it calls [`sources_new`].

/// One source of named entries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source<T> {
    /// The prefix of a full name. It is not empty and holds no `:`.
    pub name: String,
    /// A higher priority wins.
    pub priority: i32,
    /// The position inside one priority, lowest first.
    ///
    /// Level 900 holds the personal kiln, then `agent_directories`, then the
    /// config home, in a fixed order. A tie between them is never ambiguous.
    /// Every other source uses 0, so two sources at one priority are a tie.
    pub within: u8,
    /// Usually a directory.
    pub value: T,
}

/// Sources, highest first.
///
/// Sources with an equal priority keep the order the caller gave, for
/// display. The order does not decide a lookup between them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sources<T> {
    list: Vec<Source<T>>,
}

impl<T> Default for Sources<T> {
    /// No sources.
    fn default() -> Self {
        Self { list: Vec::new() }
    }
}

impl<T> Sources<T> {
    /// The sources, highest first. An [`Entry::source`] indexes this slice.
    pub fn list(&self) -> &[Source<T>] {
        &self.list
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SourcesError {
    #[error("two sources are named '{0}'")]
    DuplicateSource(String),
    #[error("'{0}' is not a source name: a name is not empty and holds no ':'")]
    BadSourceName(String),
}

/// One named thing that a source supplies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry<V> {
    /// The index of the source in [`Sources::list`].
    pub source: usize,
    pub name: String,
    pub value: V,
}

/// The result of a lookup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Lookup<V> {
    Found(V),
    Missing,
    /// The full names that the query could mean, sorted.
    Ambiguous(Vec<String>),
}

/// Sort `list` highest first.
///
/// A second source with a name that is already in the list is an error. The
/// caller decides which source to drop before this call.
pub fn sources_new<T>(mut list: Vec<Source<T>>) -> Result<Sources<T>, SourcesError> {
    for (i, source) in list.iter().enumerate() {
        if source.name.is_empty() || source.name.contains(':') {
            return Err(SourcesError::BadSourceName(source.name.clone()));
        }
        if list[..i].iter().any(|s| s.name == source.name) {
            return Err(SourcesError::DuplicateSource(source.name.clone()));
        }
    }
    // A stable sort keeps the caller's order inside one key.
    list.sort_by_key(|s| key(s));
    Ok(Sources { list })
}

/// The sort key: highest priority first, then lowest `within`.
fn key<T>(source: &Source<T>) -> (std::cmp::Reverse<i32>, u8) {
    (std::cmp::Reverse(source.priority), source.within)
}

/// `source:name`.
pub fn full_name<T, V>(sources: &Sources<T>, entry: &Entry<V>) -> String {
    format!("{}:{}", sources.list[entry.source].name, entry.name)
}

/// The entry that `query` names.
///
/// A query with a known source prefix looks in that source only. An unknown
/// prefix is `Missing`: a fallback to the bare name would give one query two
/// meanings. A bare query finds the highest source that holds the name.
pub fn lookup<'a, T, V>(
    sources: &Sources<T>,
    entries: &'a [Entry<V>],
    query: &str,
) -> Lookup<&'a Entry<V>> {
    if let Some((prefix, name)) = query.split_once(':') {
        let Some(index) = sources.list.iter().position(|s| s.name == prefix) else {
            return Lookup::Missing;
        };
        return match entries.iter().find(|e| e.source == index && e.name == name) {
            Some(entry) => Lookup::Found(entry),
            None => Lookup::Missing,
        };
    }
    let top = top_entries(sources, entries, query);
    match top.as_slice() {
        [] => Lookup::Missing,
        [entry] => Lookup::Found(entry),
        _ => {
            let mut names: Vec<String> = top.iter().map(|e| full_name(sources, e)).collect();
            names.sort();
            Lookup::Ambiguous(names)
        }
    }
}

/// The key each entry is listed under, with the index of the entry.
///
/// The one entry at the top of its name takes the bare name. Every other
/// entry takes its full name. The result keeps the order of `entries`.
pub fn listing<T, V>(sources: &Sources<T>, entries: &[Entry<V>]) -> Vec<(String, usize)> {
    entries
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            let top = top_entries(sources, entries, &entry.name);
            let unique_top = top.len() == 1 && std::ptr::eq(top[0], entry);
            let name = if unique_top {
                entry.name.clone()
            } else {
                full_name(sources, entry)
            };
            (name, index)
        })
        .collect()
}

/// The first source, highest first, for which `probe` gives a value.
///
/// For a kind with no names to enumerate, such as the one defaults file.
/// It probes every source of the winning key, so it finds a tie. An
/// ambiguous result holds the names of the tied sources, sorted.
pub fn first<T, R>(
    sources: &Sources<T>,
    mut probe: impl FnMut(&Source<T>) -> Option<R>,
) -> Lookup<(usize, R)> {
    let mut found: Option<(usize, R)> = None;
    let mut tied = Vec::new();
    for (index, source) in sources.list.iter().enumerate() {
        if let Some((winner, _)) = &found {
            if key(source) != key(&sources.list[*winner]) {
                break;
            }
        }
        if let Some(value) = probe(source) {
            tied.push(source.name.clone());
            found.get_or_insert((index, value));
        }
    }
    match found {
        None => Lookup::Missing,
        Some(hit) if tied.len() == 1 => Lookup::Found(hit),
        Some(_) => {
            tied.sort();
            Lookup::Ambiguous(tied)
        }
    }
}

/// The entries named `name` at the highest key that holds the name.
fn top_entries<'a, T, V>(
    sources: &Sources<T>,
    entries: &'a [Entry<V>],
    name: &str,
) -> Vec<&'a Entry<V>> {
    let named = entries.iter().filter(|e| e.name == name);
    let Some(best) = named.clone().map(|e| key(&sources.list[e.source])).min() else {
        return Vec::new();
    };
    named
        .filter(|e| key(&sources.list[e.source]) == best)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(name: &str, priority: i32) -> Source<()> {
        Source {
            name: name.to_string(),
            priority,
            within: 0,
            value: (),
        }
    }

    fn entry(sources: &Sources<()>, source: &str, name: &str) -> Entry<()> {
        Entry {
            source: sources.list.iter().position(|s| s.name == source).unwrap(),
            name: name.to_string(),
            value: (),
        }
    }

    fn found_in(sources: &Sources<()>, lookup: Lookup<&Entry<()>>) -> String {
        match lookup {
            Lookup::Found(entry) => sources.list[entry.source].name.clone(),
            other => panic!("expected a hit, got {other:?}"),
        }
    }

    #[test]
    fn sources_sort_highest_first_and_keep_the_order_of_a_tie() {
        let sources = sources_new(vec![source("low", 1), source("a", 5), source("b", 5)]).unwrap();
        let names: Vec<&str> = sources.list().iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["a", "b", "low"]);
    }

    #[test]
    fn two_sources_of_one_name_are_an_error() {
        assert_eq!(
            sources_new(vec![source("x", 1), source("x", 2)]),
            Err(SourcesError::DuplicateSource("x".into()))
        );
    }

    #[test]
    fn a_source_name_is_not_empty_and_holds_no_colon() {
        for bad in ["", "a:b"] {
            assert_eq!(
                sources_new(vec![source(bad, 1)]),
                Err(SourcesError::BadSourceName(bad.into()))
            );
        }
    }

    #[test]
    fn a_bare_name_goes_to_the_highest_source() {
        let sources = sources_new(vec![source("low", 1), source("high", 9)]).unwrap();
        let entries = [entry(&sources, "low", "x"), entry(&sources, "high", "x")];
        assert_eq!(found_in(&sources, lookup(&sources, &entries, "x")), "high");
    }

    #[test]
    fn a_full_name_always_reaches_its_entry() {
        let sources = sources_new(vec![source("low", 1), source("high", 9)]).unwrap();
        let entries = [entry(&sources, "low", "x"), entry(&sources, "high", "x")];
        assert_eq!(
            found_in(&sources, lookup(&sources, &entries, "low:x")),
            "low"
        );
        assert_eq!(
            found_in(&sources, lookup(&sources, &entries, "high:x")),
            "high"
        );
    }

    #[test]
    fn an_unknown_prefix_is_missing_even_when_the_bare_name_exists() {
        let sources = sources_new(vec![source("a", 1)]).unwrap();
        let entries = [entry(&sources, "a", "x")];
        assert_eq!(lookup(&sources, &entries, "nope:x"), Lookup::Missing);
        assert_eq!(lookup(&sources, &entries, "a:y"), Lookup::Missing);
        assert_eq!(lookup(&sources, &entries, "y"), Lookup::Missing);
    }

    #[test]
    fn a_tie_at_the_top_is_ambiguous_with_sorted_full_names() {
        let sources = sources_new(vec![source("b", 5), source("a", 5), source("low", 1)]).unwrap();
        let entries = [
            entry(&sources, "b", "x"),
            entry(&sources, "a", "x"),
            entry(&sources, "low", "x"),
        ];
        assert_eq!(
            lookup(&sources, &entries, "x"),
            Lookup::Ambiguous(vec!["a:x".into(), "b:x".into()])
        );
    }

    #[test]
    fn a_tie_below_a_unique_top_does_not_make_the_bare_name_ambiguous() {
        let sources = sources_new(vec![source("top", 9), source("a", 5), source("b", 5)]).unwrap();
        let entries = [
            entry(&sources, "a", "x"),
            entry(&sources, "b", "x"),
            entry(&sources, "top", "x"),
        ];
        assert_eq!(found_in(&sources, lookup(&sources, &entries, "x")), "top");
    }

    #[test]
    fn within_orders_one_priority_without_a_tie() {
        let mut second = source("second", 900);
        second.within = 1;
        let sources = sources_new(vec![second, source("first", 900)]).unwrap();
        let entries = [
            entry(&sources, "second", "x"),
            entry(&sources, "first", "x"),
        ];
        assert_eq!(found_in(&sources, lookup(&sources, &entries, "x")), "first");
    }

    #[test]
    fn listing_gives_the_bare_name_to_the_unique_top_only() {
        let sources = sources_new(vec![source("top", 9), source("a", 5), source("b", 5)]).unwrap();
        let entries = [
            entry(&sources, "a", "x"),
            entry(&sources, "top", "x"),
            entry(&sources, "a", "y"),
            entry(&sources, "b", "y"),
            entry(&sources, "b", "z"),
        ];
        let keys: Vec<String> = listing(&sources, &entries)
            .into_iter()
            .map(|(key, _)| key)
            .collect();
        assert_eq!(keys, ["a:x", "x", "a:y", "b:y", "z"]);
    }

    #[test]
    fn first_takes_the_highest_hit() {
        let sources =
            sources_new(vec![source("low", 1), source("mid", 5), source("high", 9)]).unwrap();
        let hit = first(&sources, |s| (s.name != "high").then(|| s.name.clone()));
        assert_eq!(hit, Lookup::Found((1, "mid".to_string())));
    }

    #[test]
    fn first_reports_a_tie_at_the_winning_priority() {
        let sources = sources_new(vec![source("b", 5), source("a", 5), source("low", 1)]).unwrap();
        let hit = first(&sources, |_| Some(()));
        assert_eq!(hit, Lookup::Ambiguous(vec!["a".into(), "b".into()]));
    }

    #[test]
    fn first_with_no_hit_is_missing() {
        let sources = sources_new(vec![source("a", 1)]).unwrap();
        assert_eq!(first(&sources, |_| None::<()>), Lookup::Missing);
    }
}
