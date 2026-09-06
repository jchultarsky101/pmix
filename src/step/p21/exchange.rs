//! The parsed exchange structure: header, instances, and diagnostics.

use std::collections::{BTreeMap, HashMap};

use serde::Serialize;

use super::value::{Id, Instance, Parameter, Segment, Span};

/// The `HEADER` section, kept as raw records plus convenience accessors.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Header {
    /// All header records in file order (`FILE_DESCRIPTION`, `FILE_NAME`,
    /// `FILE_SCHEMA`, and any others).
    pub entries: Vec<Segment>,
}

impl Header {
    fn entry(&self, keyword: &str) -> Option<&Segment> {
        self.entries
            .iter()
            .find(|e| e.keyword.eq_ignore_ascii_case(keyword))
    }

    fn string_list(p: Option<&Parameter>) -> Vec<String> {
        p.and_then(Parameter::as_list)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|i| i.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// `FILE_DESCRIPTION.description`.
    pub fn description(&self) -> Vec<String> {
        Self::string_list(
            self.entry("FILE_DESCRIPTION")
                .and_then(|e| e.parameters.first()),
        )
    }

    /// `FILE_DESCRIPTION.implementation_level`, e.g. `"2;1"`.
    pub fn implementation_level(&self) -> Option<&str> {
        self.entry("FILE_DESCRIPTION")?.parameters.get(1)?.as_str()
    }

    /// `FILE_NAME.name`.
    pub fn file_name(&self) -> Option<&str> {
        self.entry("FILE_NAME")?.parameters.first()?.as_str()
    }

    /// `FILE_NAME.time_stamp`.
    pub fn time_stamp(&self) -> Option<&str> {
        self.entry("FILE_NAME")?.parameters.get(1)?.as_str()
    }

    /// `FILE_NAME.preprocessor_version`, usually the exporting software.
    pub fn preprocessor_version(&self) -> Option<&str> {
        self.entry("FILE_NAME")?.parameters.get(4)?.as_str()
    }

    /// `FILE_NAME.originating_system`.
    pub fn originating_system(&self) -> Option<&str> {
        self.entry("FILE_NAME")?.parameters.get(5)?.as_str()
    }

    /// `FILE_SCHEMA.schema_identifiers`, e.g.
    /// `["AP242_MANAGED_MODEL_BASED_3D_ENGINEERING_MIM_LF { 1 0 10303 442 1 1 4 }"]`.
    pub fn schema_identifiers(&self) -> Vec<String> {
        Self::string_list(self.entry("FILE_SCHEMA").and_then(|e| e.parameters.first()))
    }

    /// The bare schema name of the first schema identifier, without the
    /// object-identifier suffix, upper case. `"AP242_MANAGED_MODEL_BASED_3D_ENGINEERING_MIM_LF"`
    /// for AP242 files.
    pub fn schema_name(&self) -> Option<String> {
        let id = self.schema_identifiers().into_iter().next()?;
        let name = id.split(['{', ' ']).next()?.trim();
        if name.is_empty() {
            None
        } else {
            Some(name.to_ascii_uppercase())
        }
    }
}

/// How serious a diagnostic is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    /// Something odd was seen but the content was fully read.
    Warning,
    /// Content was lost: a record could not be parsed and was skipped.
    Error,
}

/// A problem found while parsing. Parsing continues past every diagnostic.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Diagnostic {
    pub severity: Severity,
    pub message: String,
    /// Location in the source.
    pub span: Span,
    /// 1-based line of `span.start`.
    pub line: usize,
}

impl std::fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let sev = match self.severity {
            Severity::Warning => "warning",
            Severity::Error => "error",
        };
        write!(f, "line {}: {sev}: {}", self.line, self.message)
    }
}

/// A fully parsed Part 21 file.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Exchange {
    pub header: Header,
    instances: BTreeMap<Id, Instance>,
    /// Upper-case entity keyword to the ids of every instance that has a
    /// segment of that type. Complex instances appear under each of their
    /// segment keywords.
    #[serde(skip)]
    by_type: HashMap<String, Vec<Id>>,
    pub diagnostics: Vec<Diagnostic>,
}

impl Exchange {
    pub(crate) fn new(
        header: Header,
        instances: BTreeMap<Id, Instance>,
        diagnostics: Vec<Diagnostic>,
    ) -> Self {
        let mut by_type: HashMap<String, Vec<Id>> = HashMap::new();
        for inst in instances.values() {
            for seg in &inst.segments {
                by_type
                    .entry(seg.keyword.clone())
                    .or_default()
                    .push(inst.id);
            }
        }
        Self {
            header,
            instances,
            by_type,
            diagnostics,
        }
    }

    /// The instance with the given id.
    pub fn get(&self, id: Id) -> Option<&Instance> {
        self.instances.get(&id)
    }

    /// Follow a parameter if it is a reference.
    pub fn deref(&self, p: &Parameter) -> Option<&Instance> {
        self.get(p.as_ref()?)
    }

    /// All instances in ascending id order.
    pub fn instances(&self) -> impl Iterator<Item = &Instance> {
        self.instances.values()
    }

    /// Number of instances.
    pub fn len(&self) -> usize {
        self.instances.len()
    }

    /// `true` if there are no instances.
    pub fn is_empty(&self) -> bool {
        self.instances.is_empty()
    }

    /// Instances having a segment of the given entity type
    /// (case-insensitive), in ascending id order.
    pub fn of_type(&self, keyword: &str) -> impl Iterator<Item = &Instance> {
        let key = keyword.to_ascii_uppercase();
        self.by_type
            .get(&key)
            .into_iter()
            .flatten()
            .filter_map(|id| self.instances.get(id))
    }

    /// Number of instances having a segment of the given type.
    pub fn count_of_type(&self, keyword: &str) -> usize {
        self.by_type
            .get(&keyword.to_ascii_uppercase())
            .map_or(0, Vec::len)
    }

    /// Every entity keyword that occurs, with the number of instances
    /// having a segment of that type, sorted by keyword.
    pub fn type_counts(&self) -> Vec<(String, usize)> {
        let mut v: Vec<_> = self
            .by_type
            .iter()
            .map(|(k, ids)| (k.clone(), ids.len()))
            .collect();
        v.sort();
        v
    }

    /// Distinct complex-instance combinations (segment keywords joined
    /// with `+`, in file order) with their counts, sorted by key.
    pub fn complex_type_counts(&self) -> Vec<(String, usize)> {
        let mut m: BTreeMap<String, usize> = BTreeMap::new();
        for inst in self.instances.values().filter(|i| i.is_complex()) {
            *m.entry(inst.type_key()).or_default() += 1;
        }
        m.into_iter().collect()
    }

    /// Ids of instances that reference `id` directly.
    ///
    /// This is a linear scan; callers that need many lookups should build
    /// their own reverse index from [`Instance::references`].
    pub fn referrers(&self, id: Id) -> Vec<Id> {
        self.instances
            .values()
            .filter(|i| i.references().contains(&id))
            .map(|i| i.id)
            .collect()
    }

    /// `true` if any diagnostic has [`Severity::Error`].
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error)
    }
}
