//! Input file format detection.

use std::fmt;
use std::path::Path;

/// A 3D model file format that `pmix` knows how to identify.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Format {
    /// ISO 10303 (STEP), typically AP242 for PMI content.
    Step,
    /// Siemens JT (ISO 14306).
    Jt,
}

impl Format {
    /// Guess the format from the file extension (case-insensitive).
    ///
    /// Returns `None` for unknown or missing extensions.
    pub fn from_path(path: &Path) -> Option<Self> {
        let ext = path.extension()?.to_str()?.to_ascii_lowercase();
        match ext.as_str() {
            "stp" | "step" | "p21" => Some(Self::Step),
            "jt" => Some(Self::Jt),
            _ => None,
        }
    }
}

impl fmt::Display for Format {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Step => f.write_str("STEP"),
            Self::Jt => f.write_str("JT"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_step_extensions() {
        for name in ["a.stp", "a.STEP", "a.step", "a.p21"] {
            assert_eq!(Format::from_path(Path::new(name)), Some(Format::Step));
        }
    }

    #[test]
    fn detects_jt_extension() {
        assert_eq!(Format::from_path(Path::new("part.JT")), Some(Format::Jt));
    }

    #[test]
    fn rejects_unknown_extensions() {
        assert_eq!(Format::from_path(Path::new("part.obj")), None);
        assert_eq!(Format::from_path(Path::new("noext")), None);
    }
}
