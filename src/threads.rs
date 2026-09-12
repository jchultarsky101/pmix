//! Reading thread designations out of the text a file carries
//! (ADR 0014).
//!
//! A thread is the one thing a fastener is ordered by, and AP242 has no
//! settled way to state one. Edition 3 defines `thread` as a feature
//! with a major diameter, a minor diameter and a count; nobody writes
//! it. Semantic screw threads arrive with Edition 5, first tested in the
//! summer 2026 interoperability round against a schema that is not yet
//! published. Until writers catch up, what a file actually carries is a
//! *designation*, as text: in a validation property, in a note, in a
//! dimension's displayed text, or in the name of the part itself.
//!
//! **So this reads the text, and the grammar it reads is a standard.**
//! That distinction is the whole justification. `M12x1.75-6H` means
//! what ISO 965 says it means and `1/4-20 UNC-2B` means what ASME B1.1
//! says, in any file, from any exporter — unlike, say, a property key,
//! which means whatever its author decided. A parser for a published
//! grammar is not a guess even when only one file to hand exercises it.
//!
//! **What is read and what is inferred are kept apart.** The
//! designation is quoted exactly as it appeared and the text it came out
//! of is carried with it, so a reader can check the reading rather than
//! trust it. The diameters are computed from the designation by the
//! standard's own rules, and are absent where the standard does not give
//! one — a pipe thread's nominal size is not its diameter, so none is
//! stated.

use serde::{Deserialize, Serialize};

use crate::model::PmiDocument;

/// A thread designation found in a file's text.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Thread {
    /// The designation exactly as the file wrote it, e.g. `M12x1.75-6H`.
    pub designation: String,
    /// Which standard's grammar it is written in.
    pub standard: Standard,
    /// The major diameter in millimetres, where the designation gives
    /// one. A metric designation states it outright; an inch one states
    /// a nominal size this converts. A pipe size is a pipe size and not
    /// a diameter, so it is left out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub major_diameter: Option<f64>,
    /// The pitch in millimetres, for a designation that states one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pitch: Option<f64>,
    /// Threads per inch, for the inch series, which state a count
    /// rather than a pitch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub threads_per_inch: Option<f64>,
    /// The tolerance class — `6H`, `6g`, `2A`, `2B` — where stated.
    /// `H` and `B` are internal threads, `g` and `A` external.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub class: Option<String>,
    /// How many, where the text says so: the `4X` in `4X M12x1.75-6H`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub count: Option<usize>,
    /// The whole text the designation was read out of, so that the
    /// reading can be checked rather than taken.
    pub text: String,
    /// What carried that text.
    pub found_in: Found,
}

/// Which grammar a designation is written in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Standard {
    /// ISO 965 metric: `M12x1.75-6H`.
    Metric,
    /// ASME B1.1 unified inch: `1/4-20 UNC-2B`.
    UnifiedInch,
    /// ASME B1.20.1 taper pipe: `1/2-14 NPT`. Its size names a pipe
    /// bore, not a diameter.
    Pipe,
}

impl Standard {
    pub fn name(self) -> &'static str {
        match self {
            Self::Metric => "metric",
            Self::UnifiedInch => "unified inch",
            Self::Pipe => "pipe",
        }
    }
}

/// Where a designation was found.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Found {
    /// `property`, `note`, or `dimension`.
    pub kind: String,
    /// The record's id, or the property's name.
    pub id: String,
}

/// Every thread designation in a PMI document's text.
///
/// Sorted, and de-duplicated by what was found and where, so that two
/// readings of one file agree (ADR 0004).
pub fn in_document(document: &PmiDocument) -> Vec<Thread> {
    let mut found = Vec::new();

    for p in &document.properties {
        if let Some(t) = read(
            &p.value.canonical(),
            Found {
                kind: "property".into(),
                id: p.name.clone(),
            },
        ) {
            found.push(t);
        }
    }
    for n in &document.semantic.notes {
        if let Some(t) = read(
            &n.text,
            Found {
                kind: "note".into(),
                id: n.meta.id.clone(),
            },
        ) {
            found.push(t);
        }
    }
    for d in &document.semantic.dimensions {
        if let Some(text) = &d.text {
            if let Some(t) = read(
                text,
                Found {
                    kind: "dimension".into(),
                    id: d.meta.id.clone(),
                },
            ) {
                found.push(t);
            }
        }
    }

    found.sort_by(|a, b| {
        (&a.designation, &a.found_in.kind, &a.found_in.id).cmp(&(
            &b.designation,
            &b.found_in.kind,
            &b.found_in.id,
        ))
    });
    found.dedup();
    found
}

/// The thread a piece of text states, if it states one.
///
/// Scans for the first token that parses as a designation, so it reads
/// `DIM\w4X M12x1.75-6H` and `SPLIT LOCK WASHER 3/8-16 UNC` alike
/// without needing to know how the writer decorated it.
pub fn read(text: &str, found_in: Found) -> Option<Thread> {
    let words = split(text);
    for (i, word) in words.iter().enumerate() {
        let parsed = metric(word)
            .or_else(|| inch(word, words.get(i + 1).map(String::as_str)))
            .or_else(|| pipe(word, words.get(i + 1).map(String::as_str)));
        if let Some(mut t) = parsed {
            // A count prefix sits before the designation: `4X M12`.
            t.count = words.get(i.wrapping_sub(1)).and_then(|w| count(w));
            t.text = text.to_owned();
            t.found_in = found_in;
            return Some(t);
        }
    }
    None
}

/// Break text into candidate tokens.
///
/// Separators are whitespace and the escapes writers wrap annotation
/// text in; `x` is *not* one, because it is part of a metric
/// designation.
fn split(text: &str) -> Vec<String> {
    text.split(|c: char| c.is_whitespace() || c == '\\' || c == '|' || c == ',')
        .filter(|w| !w.is_empty())
        .map(str::to_owned)
        .collect()
}

/// `4X` or `4x` before a designation.
///
/// Only the trailing digits are read, because a writer's escape can be
/// stuck to the front of the word — the NIST file writes `\w4X`, where
/// `\w` is its own text control and not part of the count.
fn count(word: &str) -> Option<usize> {
    let body = word.strip_suffix(['X', 'x'])?;
    let digits: String = body
        .chars()
        .rev()
        .take_while(char::is_ascii_digit)
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    digits.parse().ok().filter(|n| *n > 1)
}

/// ISO 965: `M12`, `M12x1.75`, `M12x1.75-6H`, `M12-6g`.
fn metric(word: &str) -> Option<Thread> {
    let rest = word.strip_prefix('M')?;
    // `MNPT` and the like are pipe fittings, not metric threads.
    if !rest.starts_with(|c: char| c.is_ascii_digit()) {
        return None;
    }
    let (body, class) = match rest.split_once('-') {
        Some((b, c)) => (b, tolerance_class(c)?),
        None => (rest, None),
    };
    let (diameter, pitch) = match body.split_once(['x', 'X', '\u{d7}']) {
        Some((d, p)) => (number(d)?, Some(number(p)?)),
        None => (number(body)?, None),
    };
    if diameter <= 0.0 {
        return None;
    }
    Some(Thread {
        designation: word.to_owned(),
        standard: Standard::Metric,
        major_diameter: Some(diameter),
        pitch,
        threads_per_inch: None,
        class,
        count: None,
        text: String::new(),
        found_in: Found {
            kind: String::new(),
            id: String::new(),
        },
    })
}

/// ASME B1.1: `1/4-20`, `1/4-20 UNC`, `1/4-20 UNC-2B`, `#10-32 UNF`.
///
/// The series name may follow as the next word, so it is passed in.
fn inch(word: &str, next: Option<&str>) -> Option<Thread> {
    let (size, rest) = word.split_once('-')?;
    let diameter = inch_size(size)?;
    // The part after the dash is the thread count, possibly with the
    // series and class attached: `20`, `20UNC`, `20UNC-2B`.
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    let tpi: f64 = digits.parse().ok()?;
    if tpi <= 0.0 {
        return None;
    }
    let tail = &rest[digits.len()..];
    let series = series_of(tail).or_else(|| next.and_then(series_of))?;
    if series == "NPT" || series == "NPTF" {
        return None; // a pipe thread; `pipe` reads those.
    }
    let class = tail
        .split_once('-')
        .and_then(|(_, c)| tolerance_class(c))
        .or_else(|| {
            next.and_then(|n| n.split_once('-'))
                .and_then(|(_, c)| tolerance_class(c))
        })
        .flatten();
    Some(Thread {
        designation: match series_of(tail) {
            Some(_) => word.to_owned(),
            None => format!("{word} {}", next.unwrap_or_default()),
        },
        standard: Standard::UnifiedInch,
        major_diameter: Some(diameter * 25.4),
        pitch: None,
        threads_per_inch: Some(tpi),
        class,
        count: None,
        text: String::new(),
        found_in: Found {
            kind: String::new(),
            id: String::new(),
        },
    })
}

/// ASME B1.20.1 taper pipe: `1/2-14 NPT`, and the bare `NPT` a parts
/// list writes as `MNPT` or `FNPT`.
///
/// No diameter: a pipe thread's size names the bore of the pipe it
/// fits, not the diameter of the thread, and stating one would be
/// wrong rather than approximate.
fn pipe(word: &str, next: Option<&str>) -> Option<Thread> {
    let bare = word.trim_start_matches(['M', 'F']);
    let named = matches!(bare, "NPT" | "NPTF" | "NPS" | "NPSM");
    let (designation, tpi) = if named {
        (word.to_owned(), None)
    } else {
        let (_, rest) = word.split_once('-')?;
        let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
        let tpi: f64 = digits.parse().ok()?;
        let tail = &rest[digits.len()..];
        let series = series_of(tail).or_else(|| next.and_then(series_of))?;
        if !series.starts_with("NP") {
            return None;
        }
        (
            match series_of(tail) {
                Some(_) => word.to_owned(),
                None => format!("{word} {}", next.unwrap_or_default()),
            },
            Some(tpi),
        )
    };
    Some(Thread {
        designation,
        standard: Standard::Pipe,
        major_diameter: None,
        pitch: None,
        threads_per_inch: tpi,
        class: None,
        count: None,
        text: String::new(),
        found_in: Found {
            kind: String::new(),
            id: String::new(),
        },
    })
}

/// A series name, if `word` starts with one.
fn series_of(word: &str) -> Option<&'static str> {
    const SERIES: &[&str] = &[
        "UNEF", "UNJC", "UNJF", "UNC", "UNF", "UNS", "NPTF", "NPSM", "NPT", "NPS", "UN",
    ];
    let upper = word.trim_start_matches('-').to_ascii_uppercase();
    SERIES.iter().find(|s| upper.starts_with(**s)).copied()
}

/// A tolerance class: `6H`, `6g`, `6H6g`, `2A`, `2B`.
///
/// Returned as written, because the case carries meaning — `6H` is an
/// internal thread and `6h` an external one.
fn tolerance_class(text: &str) -> Option<Option<String>> {
    let class: String = text
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric())
        .collect();
    if class.is_empty() {
        return Some(None);
    }
    let ok = class.len() <= 4
        && class.starts_with(|c: char| c.is_ascii_digit())
        && class.chars().any(|c| c.is_ascii_alphabetic());
    ok.then_some(Some(class))
}

/// An inch size: `1/4`, `3/8`, `1-1/2` is not handled here, `#10`, or a
/// plain decimal.
fn inch_size(text: &str) -> Option<f64> {
    if let Some(number) = text.strip_prefix('#') {
        // ASME numbered sizes 0 to 12: 0.060in plus 0.013in a step.
        let n: f64 = number.parse().ok()?;
        return (n <= 12.0).then_some(0.060 + 0.013 * n);
    }
    if let Some((num, den)) = text.split_once('/') {
        let (n, d): (f64, f64) = (num.parse().ok()?, den.parse().ok()?);
        return (d > 0.0 && n > 0.0).then_some(n / d);
    }
    let v: f64 = text.parse().ok()?;
    // A bare integer here is a thread count or a part number far more
    // often than a diameter in inches, so only a decimal is taken.
    (text.contains('.') && v > 0.0).then_some(v)
}

fn number(text: &str) -> Option<f64> {
    let v: f64 = text.parse().ok()?;
    v.is_finite().then_some(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at() -> Found {
        Found {
            kind: "test".into(),
            id: "t".into(),
        }
    }

    #[test]
    fn a_metric_designation_states_its_diameter_and_pitch() {
        let t = read("M12x1.75-6H", at()).expect("a thread");
        assert_eq!(t.standard, Standard::Metric);
        assert_eq!(t.major_diameter, Some(12.0));
        assert_eq!(t.pitch, Some(1.75));
        assert_eq!(t.class.as_deref(), Some("6H"));
        assert_eq!(t.designation, "M12x1.75-6H");
    }

    /// The one real designation in the public corpus, as the file writes
    /// it — count prefix, escape and all.
    #[test]
    fn the_nist_callout_reads() {
        let t = read("DIM\\w4X M12x1.75-6H", at()).expect("a thread");
        assert_eq!(t.major_diameter, Some(12.0));
        assert_eq!(t.count, Some(4), "the 4X says how many");
        assert_eq!(t.text, "DIM\\w4X M12x1.75-6H", "the whole text is kept");
    }

    #[test]
    fn a_coarse_metric_thread_states_no_pitch() {
        let t = read("M6", at()).expect("a thread");
        assert_eq!(t.major_diameter, Some(6.0));
        assert_eq!(t.pitch, None, "the coarse pitch is implied, not stated");
        assert_eq!(t.class, None);
    }

    #[test]
    fn an_inch_designation_converts_its_nominal_size() {
        let t = read("1/4-20 UNC-2B", at()).expect("a thread");
        assert_eq!(t.standard, Standard::UnifiedInch);
        assert_eq!(t.threads_per_inch, Some(20.0));
        assert_eq!(t.class.as_deref(), Some("2B"));
        let d = t.major_diameter.expect("a diameter");
        assert!((d - 6.35).abs() < 1e-9, "a quarter inch is 6.35mm, got {d}");
    }

    #[test]
    fn a_numbered_size_is_read_from_the_standard_series() {
        let t = read("#10-32 UNF", at()).expect("a thread");
        let d = t.major_diameter.expect("a diameter");
        assert!((d - 0.190 * 25.4).abs() < 1e-6, "#10 is 0.190in, got {d}");
    }

    /// A pipe thread's size names the bore of the pipe it fits, not the
    /// diameter of the thread. Converting it would be wrong, not
    /// approximate.
    #[test]
    fn a_pipe_thread_states_no_diameter() {
        let t = read("1/2-14 NPT", at()).expect("a thread");
        assert_eq!(t.standard, Standard::Pipe);
        assert_eq!(t.major_diameter, None);
        assert_eq!(t.threads_per_inch, Some(14.0));
    }

    /// Parts lists write pipe fittings this way, without a size.
    #[test]
    fn a_bare_pipe_fitting_is_recognised() {
        let t = read("1/2 MNPT ADAPTER", at()).expect("a thread");
        assert_eq!(t.standard, Standard::Pipe);
        assert_eq!(t.major_diameter, None);
    }

    /// A designation in the middle of a part name is still a
    /// designation — which is where most real files keep them.
    #[test]
    fn a_designation_inside_a_part_name_reads() {
        let t = read("SPLIT LOCK WASHER 3/8-16 UNC SS", at()).expect("a thread");
        assert_eq!(t.threads_per_inch, Some(16.0));
        let d = t.major_diameter.expect("a diameter");
        assert!((d - 9.525).abs() < 1e-6, "3/8in is 9.525mm, got {d}");
    }

    #[test]
    fn text_with_no_thread_in_it_reads_as_none() {
        for text in [
            "PLATE 40 X 30 X 10",
            "MATERIAL: 6061-T6",
            "Mass (g)",
            "M",
            "M-6",
            "1-2-3",
            "",
        ] {
            assert!(read(text, at()).is_none(), "{text:?} is not a thread");
        }
    }

    /// `MNPT` begins with M and is not a metric thread.
    #[test]
    fn a_pipe_fitting_is_not_read_as_metric() {
        let t = read("MNPT", at()).expect("a thread");
        assert_eq!(t.standard, Standard::Pipe);
    }
}
