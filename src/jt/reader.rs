//! Turning a JT file into a [`PmiDocument`].

use crate::model::{
    ContentId, Diagnostic, PmiDocument, Property, PropertyKind, PropertyValue, SCHEMA_VERSION,
    Source, Units, Unknown,
};
use crate::{ExtractOptions, Reader, Result};

use super::element::Elements;
use super::file::{Guid, Jt, SegmentKind};
use super::{features, identity, meta, pmi, presentation, property, semantic, stt};

/// Reader for JT files (ADR 0009).
#[derive(Debug, Clone, Copy, Default)]
pub struct JtReader;

/// The property that names the version of the writing application.
const WRITER_KEYS: &[&str] = &["JT_PROP_APPLICATION", "JT_PROP_CAD_SYSTEM"];

/// Properties that describe the file rather than the design, and so are
/// not part of what `pmix diff` compares.
///
/// The test is on how the file came to be written, not on whether the
/// value looks interesting: a translator version and a level-of-detail
/// setting change when the model is exported again, while a material or
/// a volume changes only when the design does.
fn is_file_metadata(key: &str) -> bool {
    let key = key.trim_end_matches(':');
    key.starts_with("JT_PMI_")
        || key.starts_with("PMISort")
        || key.starts_with("__CAD_INST_UID")
        || key.starts_with("__PLM_")
        || key.starts_with("LAYERFILTER")
        || key.starts_with("TOOLKIT_")
        || key.starts_with("AdvCompress")
        || key == "JT_PROP_MEASUREMENT_UNITS"
        || key == "PartitionType"
        || key == "Translator Version"
        || key == "PMI_TYPE_TABLE"
}

/// The name a scene-graph node gives itself, if it gives one.
fn node_name(scene: &property::Properties, node: i32) -> Option<String> {
    let pairs = scene.by_element.get(&node)?;
    ["CAD_PARTNAME::", "JT_PROP_NAME", "Name::"]
        .iter()
        .find_map(|key| {
            pairs
                .iter()
                .find(|(k, v)| k == key && !v.is_empty())
                .map(|(_, v)| v.clone())
        })
}

/// Which part a segment belongs to.
///
/// A part names itself on its scene-graph node, but the node that owns a
/// segment is often a child of the part rather than the part itself, and
/// children are unnamed. A metadata segment states its own name as well,
/// so the two together cover every segment in the files seen so far.
fn part_of(scene: &property::Properties, segment: Guid, stated: Option<&str>) -> Option<String> {
    scene
        .owner_of(segment)
        .and_then(|node| node_name(scene, node))
        .or_else(|| stated.map(str::to_owned))
        .filter(|n| !n.is_empty())
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
                Ok(data) => {
                    let read = property::read(&data, jt.header.major);
                    scene.by_element.extend(read.by_element);
                    scene.segments.extend(read.segments);
                }
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
        // Version 9 states its element versions in two bytes where
        // version 10 states them in one, which the scene graph reader
        // knows about because a file was there to work it out from. No
        // such file carries PMI, so that reader has never been tried on
        // one, and a wrong reading there would be silent.
        if jt.header.major < 10
            && jt
                .segments()
                .iter()
                .any(|s| s.kind.carries_pmi() || s.kind == SegmentKind::Stt)
        {
            diagnostics.push(Diagnostic {
                message: format!(
                    "this file is JT {}.{}, and its PMI and geometry are read as                      version 10 states them; treat what they yield as unverified",
                    jt.header.major, jt.header.minor
                ),
                source_ref: None,
            });
        }
        tracing::debug!(
            segments = jt.segments().len(),
            pmi_elements = managers.len(),
            units = ?length,
            "read JT structure"
        );

        // The smart topology table says which faces a callout applies
        // to and what they lie on, which is what anchors a dimension on
        // the geometry rather than on where it is drawn (ADR 0010).
        let mut topologies = Vec::new();
        for segment in jt.segments().iter().filter(|s| s.kind == SegmentKind::Stt) {
            let Ok(data) = jt.segment_data(segment) else {
                diagnostics.push(Diagnostic {
                    message: "topology table segment could not be read".into(),
                    source_ref: Some(segment.id.to_string()),
                });
                continue;
            };
            for element in Elements::new(&data) {
                if element.object_type != stt::STT_ELEMENT {
                    continue;
                }
                match stt::parse(element.data) {
                    Ok(topology) => topologies.push(topology),
                    Err(e) => diagnostics.push(Diagnostic {
                        message: format!("topology table could not be read: {e}"),
                        source_ref: Some(segment.id.to_string()),
                    }),
                }
            }
        }
        let per_metre = length.and_then(property::per_metre).unwrap_or(1.0);
        let anchors = features::build(&managers, &topologies, per_metre);
        tracing::debug!(
            parts = topologies.len(),
            features = anchors.features.len(),
            anchored = anchors.by_entity.len(),
            "read JT precise geometry"
        );

        let built = semantic::build(&managers, length, &anchors, &mut unknown);
        let presentation = presentation::build(&managers, length, &built.links, options);
        let mut semantic = built.semantic;
        semantic.features = anchors.features;

        // Scene-graph properties become the document's properties.
        let mut ids = ContentId::new();
        let mut properties: Vec<Property> = Vec::new();
        let mut seen: std::collections::HashSet<(String, String, String)> =
            std::collections::HashSet::new();
        let mut add =
            |name: &str, value: PropertyValue, text: &str, part: Option<String>, source: String| {
                // A key ending in a double colon is marked visible to a
                // viewing application, and one without it hidden; the
                // marker is a display hint rather than part of the name
                // (specification 11.9.1.2). Keeping it would make the
                // same property in two files two records, and would put
                // a JT decoration in a name a STEP file also states.
                let name = name.strip_suffix("::").unwrap_or(name);
                // One part saying a thing twice adds nothing; two parts
                // saying the same thing are two facts.
                let owner = part.clone().unwrap_or_default();
                if text.is_empty()
                    || !seen.insert((owner.clone(), name.to_owned(), text.to_owned()))
                {
                    return;
                }
                properties.push(Property {
                    id: ids.make("prop", &[&owner, name, text]),
                    name: name.to_owned(),
                    category: None,
                    kind: if is_file_metadata(name) {
                        PropertyKind::Validation
                    } else {
                        PropertyKind::User
                    },
                    part,
                    value,
                    applies_to: None,
                    unmapped: Vec::new(),
                    source_refs: vec![source],
                });
            };
        for (element, pairs) in &scene.by_element {
            let part = node_name(&scene, *element);
            for (name, value) in pairs {
                add(
                    name,
                    PropertyValue::from_text(value),
                    value,
                    part.clone(),
                    format!("#{element}"),
                );
            }
        }
        // A part states its own properties in a metadata segment: its
        // material, its volume, the system that wrote it.
        for segment in jt
            .segments()
            .iter()
            .filter(|s| s.kind == SegmentKind::MetaData)
        {
            let Ok(data) = jt.segment_data(segment) else {
                continue;
            };
            for element in Elements::new(&data) {
                if element.object_type != meta::PROPERTY_PROXY {
                    continue;
                }
                let stated = meta::parse(element.data)
                    .into_iter()
                    .find(|(k, _)| k.trim_end_matches(':') == "Name")
                    .map(|(_, v)| v.to_string());
                let part = part_of(&scene, segment.id, stated.as_deref());
                for (name, value) in meta::parse(element.data) {
                    let text = value.to_string();
                    let typed = match value {
                        // Text goes through the reader's usual rule, so a
                        // volume written as digits becomes a number.
                        meta::Value::Text(ref v) => PropertyValue::from_text(v),
                        meta::Value::Integer(v) => PropertyValue::Integer { value: v as i64 },
                        meta::Value::Number(v) => PropertyValue::Number { value: v },
                        meta::Value::Date(ref v) => PropertyValue::Text { value: v.clone() },
                        meta::Value::Unset => continue,
                    };
                    add(
                        &name,
                        typed,
                        &text,
                        part.clone(),
                        format!("meta[{}]", segment.offset),
                    );
                }
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
