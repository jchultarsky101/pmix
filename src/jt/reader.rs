//! Turning a JT file into a [`PmiDocument`].

use crate::model::{
    ContentId, Diagnostic, PmiDocument, Property, PropertyKind, PropertyValue, SCHEMA_VERSION,
    Source, Units, Unknown,
};
use crate::{ExtractOptions, Reader, Result};

use super::element::Elements;
use super::file::{Jt, SegmentKind};
use super::{identity, pmi, presentation, property, semantic};

/// Reader for JT files (ADR 0009).
#[derive(Debug, Clone, Copy, Default)]
pub struct JtReader;

/// The property that names the version of the writing application.
const WRITER_KEYS: &[&str] = &["JT_PROP_APPLICATION", "JT_PROP_CAD_SYSTEM"];

/// Properties that describe the file rather than the design, and so are
/// not part of what `pmix diff` compares.
fn is_file_metadata(key: &str) -> bool {
    key.starts_with("JT_PMI_")
        || key.starts_with("PMISort")
        || key.starts_with("__CAD_INST_UID")
        || key == "JT_PROP_MEASUREMENT_UNITS"
        || key == "PartitionType"
}

impl Reader for JtReader {
    fn read(&self, input: &[u8], file_name: &str, options: &ExtractOptions) -> Result<PmiDocument> {
        let jt = Jt::parse(input)?;
        let mut diagnostics: Vec<Diagnostic> = Vec::new();
        let mut unknown: Vec<Unknown> = Vec::new();

        // The scene graph declares the model units and names the parts.
        let mut scene = property::Properties::default();
        for segment in jt
            .segments()
            .iter()
            .filter(|s| s.kind == SegmentKind::LogicalSceneGraph)
        {
            match jt.segment_data(segment) {
                Ok(data) => scene.by_element.extend(property::read(&data).by_element),
                Err(e) => diagnostics.push(Diagnostic {
                    message: format!("scene graph segment could not be read: {e}"),
                    source_ref: Some(segment.id.to_string()),
                }),
            }
        }
        let declared = scene.find("JT_PROP_MEASUREMENT_UNITS");
        let length = declared.and_then(property::unit_name);
        if let Some(text) = declared {
            if length.is_none() {
                diagnostics.push(Diagnostic {
                    message: format!("model units `{text}` are not a name this reader knows"),
                    source_ref: None,
                });
            }
        } else {
            diagnostics.push(Diagnostic {
                message: "the file does not declare its model units".into(),
                source_ref: None,
            });
        }

        // Every PMI segment holds one PMI Manager element.
        let mut managers = Vec::new();
        for segment in jt.segments().iter().filter(|s| s.kind.carries_pmi()) {
            let data = match jt.segment_data(segment) {
                Ok(data) => data,
                Err(e) => {
                    diagnostics.push(Diagnostic {
                        message: format!("PMI segment could not be read: {e}"),
                        source_ref: Some(segment.id.to_string()),
                    });
                    continue;
                }
            };
            for element in Elements::new(&data) {
                if element.object_type != pmi::PMI_MANAGER {
                    continue;
                }
                match pmi::parse(element.data) {
                    Ok(manager) => managers.push(manager),
                    Err(e) => diagnostics.push(Diagnostic {
                        message: format!("PMI element could not be read: {e}"),
                        source_ref: Some(segment.id.to_string()),
                    }),
                }
            }
        }
        tracing::debug!(
            segments = jt.segments().len(),
            pmi_elements = managers.len(),
            units = ?length,
            "read JT structure"
        );

        let built = semantic::build(&managers, length, &mut unknown);
        let presentation = presentation::build(&managers, length, &built.links, options);
        let semantic = built.semantic;

        // Scene-graph properties become the document's properties.
        let mut ids = ContentId::new();
        let mut properties: Vec<Property> = Vec::new();
        for (element, pairs) in &scene.by_element {
            for (name, value) in pairs {
                if value.is_empty() {
                    continue;
                }
                properties.push(Property {
                    id: ids.make("prop", &[name, value]),
                    name: name.clone(),
                    category: None,
                    kind: if is_file_metadata(name) {
                        PropertyKind::Validation
                    } else {
                        PropertyKind::User
                    },
                    value: PropertyValue::from_text(value),
                    applies_to: None,
                    unmapped: Vec::new(),
                    source_refs: vec![format!("#{element}")],
                });
            }
        }
        properties.sort_by(|a, b| a.id.cmp(&b.id));

        let mut doc = PmiDocument {
            schema_version: SCHEMA_VERSION,
            source: Source {
                file_name: file_name.to_owned(),
                format: "JT".to_owned(),
                schema: Some(jt.header.version.trim().to_owned()),
                writer: WRITER_KEYS
                    .iter()
                    .find_map(|k| scene.find(k))
                    .map(str::to_owned),
                time_stamp: None,
            },
            units: Units {
                length: length.map(str::to_owned),
                angle: Some("deg".to_owned()),
            },
            properties,
            semantic,
            presentation,
            unknown,
            diagnostics,
        };
        identity::finalise(&mut doc, &built.keys);
        Ok(doc)
    }
}
