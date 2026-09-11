//! The Model Context Protocol server (ADR 0013), driven with strings.
//!
//! The server is a transport, so what these check is that the protocol
//! holds up — handshake, listing, calling, the errors the protocol
//! reserves — and that what comes back through it is what the library
//! would have said. The tools themselves are tested where they live.

use serde_json::{Value, json};

/// Run the server over `lines` and return one parsed reply per reply.
fn run(lines: &[Value]) -> Vec<Value> {
    let input: String = lines.iter().map(|l| format!("{l}\n")).collect();
    let mut output = Vec::new();
    pmix::mcp::serve(input.as_bytes(), &mut output).unwrap();
    String::from_utf8(output)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).expect("each reply is one JSON line"))
        .collect()
}

fn fixture(name: &str) -> String {
    format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"))
}

fn call(id: u64, tool: &str, args: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "method": "tools/call",
            "params": { "name": tool, "arguments": args } })
}

/// The document a call returned, read from `structuredContent`.
fn structured(reply: &Value) -> &Value {
    assert!(
        reply["result"]["isError"].as_bool() != Some(true),
        "the call failed: {}",
        reply["result"]["content"][0]["text"]
    );
    &reply["result"]["structuredContent"]
}

#[test]
fn the_handshake_echoes_a_known_protocol_version_and_offers_the_newest_otherwise() {
    let replies = run(&[
        json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize",
                "params": { "protocolVersion": "2025-03-26", "capabilities": {},
                            "clientInfo": { "name": "test", "version": "0" } } }),
        // A notification: no id, so no reply.
        json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
        json!({ "jsonrpc": "2.0", "id": 2, "method": "initialize",
                "params": { "protocolVersion": "1999-01-01" } }),
        json!({ "jsonrpc": "2.0", "id": 3, "method": "ping" }),
    ]);
    assert_eq!(replies.len(), 3, "{replies:?}");
    assert_eq!(replies[0]["result"]["protocolVersion"], "2025-03-26");
    assert_eq!(replies[0]["result"]["serverInfo"]["name"], "pmix");
    assert_eq!(
        replies[0]["result"]["serverInfo"]["version"],
        env!("CARGO_PKG_VERSION")
    );
    // What a model is told before it chooses a tool, including the one
    // caution ADR 0013 requires it to carry.
    let instructions = replies[0]["result"]["instructions"].as_str().unwrap();
    assert!(instructions.contains("possible_pairing"), "{instructions}");
    assert!(instructions.contains("change nothing"), "{instructions}");
    assert_eq!(
        replies[1]["result"]["protocolVersion"],
        pmix::mcp::PROTOCOL_VERSIONS[0]
    );
    assert_eq!(replies[2]["result"], json!({}));
}

#[test]
fn the_tools_are_listed_with_schemas() {
    let replies = run(&[json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" })]);
    let tools = replies[0]["result"]["tools"].as_array().unwrap();
    let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert_eq!(
        names,
        [
            "describe_model",
            "compare_models",
            "list_parts",
            "describe_part",
            "extract_pmi",
            "diff_pmi"
        ]
    );
    for t in tools {
        assert!(
            t["description"].as_str().unwrap().len() > 40,
            "{}",
            t["name"]
        );
        assert_eq!(t["inputSchema"]["type"], "object", "{}", t["name"]);
        assert!(t["inputSchema"]["required"].is_array(), "{}", t["name"]);
    }
}

#[test]
fn a_method_the_server_does_not_know_gets_the_error_the_protocol_reserves() {
    let replies = run(&[
        json!({ "jsonrpc": "2.0", "id": 1, "method": "resources/list" }),
        json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/call",
                "params": { "name": "make_coffee", "arguments": {} } }),
    ]);
    assert_eq!(replies[0]["error"]["code"], -32601);
    assert_eq!(replies[1]["error"]["code"], -32602);
}

#[test]
fn a_line_that_is_not_json_gets_a_parse_error_and_the_server_carries_on() {
    let input = "this is not json\n{\"jsonrpc\":\"2.0\",\"id\":7,\"method\":\"ping\"}\n";
    let mut output = Vec::new();
    pmix::mcp::serve(input.as_bytes(), &mut output).unwrap();
    let replies: Vec<Value> = String::from_utf8(output)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(replies[0]["error"]["code"], -32700);
    assert_eq!(replies[1]["id"], 7);
}

#[test]
fn describing_a_model_returns_what_the_library_returns() {
    let path = fixture("synthetic/plate_one_hole.stp");
    let replies = run(&[call(1, "describe_model", json!({ "path": path }))]);
    let got = structured(&replies[0]);
    // The view carries its filter, and with none set the document is the
    // whole one.
    assert_eq!(got["filter"], json!({}));
    let expected = pmix::features::read_path(std::path::Path::new(&path)).unwrap();
    assert_eq!(got["document"], serde_json::to_value(&expected).unwrap());
    // The text content is the same document, for a client that reads
    // only text.
    let text: Value =
        serde_json::from_str(replies[0]["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(text, *got);
}

#[test]
fn a_summary_counts_without_listing_and_a_narrow_view_lists_one_kind() {
    let path = fixture("jt/nist_mtc_assembly.jt");
    let replies = run(&[call(
        1,
        "describe_model",
        json!({ "path": path, "summary": true }),
    )]);
    let overview = structured(&replies[0]);
    let bodies = overview["bodies"].as_array().unwrap();
    assert_eq!(bodies.len(), 8);
    assert!(
        bodies
            .iter()
            .all(|b| b.get("features").is_some() && b.get("unassigned").is_none())
    );

    // Pick the body with the most holes and ask for those alone.
    let (id, holes) = bodies
        .iter()
        .map(|b| {
            (
                b["id"].as_str().unwrap(),
                b["features"]["hole"].as_u64().unwrap_or(0),
            )
        })
        .max_by_key(|(_, n)| *n)
        .unwrap();
    let replies = run(&[call(
        2,
        "describe_model",
        json!({ "path": fixture("jt/nist_mtc_assembly.jt"), "body": id, "kind": "hole" }),
    )]);
    let narrowed = structured(&replies[0]);
    assert_eq!(narrowed["filter"]["kind"], "hole");
    let body = &narrowed["document"]["bodies"][0];
    assert_eq!(body["id"], id);
    assert_eq!(body["features"].as_array().unwrap().len() as u64, holes);
    assert!(
        body["features"]
            .as_array()
            .unwrap()
            .iter()
            .all(|f| f["kind"] == "hole")
    );
    // Nobody asked for the planes, and the counts still describe the
    // whole body.
    assert!(body["unassigned"].as_array().unwrap().is_empty());
    assert!(body["faces"]["unassigned"].as_u64().unwrap() > 0);
}

#[test]
fn comparing_two_models_carries_the_caution_through_the_protocol() {
    let replies = run(&[call(
        1,
        "compare_models",
        json!({ "baseline": fixture("synthetic/plate_one_hole.stp"),
                "compared": fixture("synthetic/plate_hole_larger.stp") }),
    )]);
    let got = structured(&replies[0]);
    let c = &got["comparison"];
    assert_eq!(c["schema_version"], pmix::features::compare::SCHEMA_VERSION);
    let pairing = &c["bodies"][0]["possible_pairings"][0];
    assert_eq!(pairing["paired_on"], "place");
    assert_eq!(pairing["differs"][0]["field"], "diameter");
    // The reason this server exists in the shape it does: the caution
    // reaches the model as data, not as prose it never sees.
    assert_eq!(c["notes"][0]["field"], "possible_pairings");
    assert!(
        c["notes"][0]["note"]
            .as_str()
            .unwrap()
            .contains("not a conclusion"),
        "{}",
        c["notes"][0]
    );
}

#[test]
fn only_changed_leaves_out_identical_pairs_but_not_their_count() {
    let a = fixture("synthetic/plate_four_holes.stp");
    let b = fixture("synthetic/plate_four_holes_one_moved.stp");
    let same = run(&[call(
        1,
        "compare_models",
        json!({ "baseline": a, "compared": a, "only_changed": true }),
    )]);
    let got = structured(&same[0]);
    assert!(got["comparison"]["bodies"].as_array().unwrap().is_empty());
    assert_eq!(got["comparison"]["summary"]["bodies_paired"], 1);
    assert_eq!(got["filter"]["only_changed"], true);

    let differs = run(&[call(
        2,
        "compare_models",
        json!({ "baseline": a, "compared": b, "only_changed": true }),
    )]);
    let got = structured(&differs[0]);
    assert_eq!(got["comparison"]["bodies"].as_array().unwrap().len(), 1);
}

#[test]
fn pmi_tools_return_the_library_documents() {
    let path = fixture("synthetic/dimension_basics.stp");
    let replies = run(&[
        call(1, "extract_pmi", json!({ "path": path })),
        call(
            2,
            "diff_pmi",
            json!({ "baseline": path, "compared": fixture("synthetic/dimension_basics_inches.stp") }),
        ),
    ]);
    let doc = structured(&replies[0]);
    assert_eq!(doc["schema_version"], pmix::model::SCHEMA_VERSION);
    assert!(!doc["semantic"]["dimensions"].as_array().unwrap().is_empty());
    // What came back is the library's own report, untouched. The inch
    // twin keys identically, so nothing is added or removed; its measures
    // are stated in inches, so they differ, and that is the library's
    // answer to give, not the transport's to soften.
    let report = structured(&replies[1]);
    let a = pmix::load(std::path::Path::new(&path)).unwrap();
    let b = pmix::load(std::path::Path::new(&fixture(
        "synthetic/dimension_basics_inches.stp",
    )))
    .unwrap();
    let expected = serde_json::to_value(pmix::diff::diff(&a, &b)).unwrap();
    assert_eq!(*report, expected);
    assert_eq!(report["summary"]["added"], 0);
    assert_eq!(report["summary"]["removed"], 0);
}

#[test]
fn a_tool_that_cannot_do_what_was_asked_says_so_as_its_result() {
    let replies = run(&[
        call(
            1,
            "describe_model",
            json!({ "path": "/nowhere/at/all.stp" }),
        ),
        call(2, "describe_model", json!({})),
        call(
            3,
            "describe_model",
            json!({ "path": fixture("synthetic/plate_one_hole.stp"), "kind": "pocket" }),
        ),
        call(
            4,
            "describe_model",
            json!({ "path": fixture("nist/README.md") }),
        ),
    ]);
    for r in &replies {
        assert_eq!(r["result"]["isError"], true, "{r}");
        assert!(
            r.get("error").is_none(),
            "a tool failure is a result, not a protocol error: {r}"
        );
    }
    // A model that passed a path is told which path failed.
    assert!(
        replies[0]["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("/nowhere/at/all.stp")
    );
    assert!(
        replies[1]["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("`path` is required")
    );
    assert!(
        replies[2]["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("pocket")
    );
    assert!(
        replies[3]["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("unrecognised file format")
    );
}

// --- the product tools (ADR 0014) ---

#[test]
fn listing_parts_counts_occurrences_without_listing_them() {
    let replies = run(&[call(
        1,
        "list_parts",
        json!({ "path": fixture("synthetic/assembly_repeated_part.stp") }),
    )]);
    let doc = structured(&replies[0]);
    let parts = doc["parts"].as_array().unwrap();
    assert_eq!(parts.len(), 3);

    let pin = parts
        .iter()
        .find(|p| p["number"] == "SYN-PIN")
        .expect("the pin");
    assert_eq!(pin["occurrences"], 3);
    assert_eq!(pin["bodies"].as_array().unwrap().len(), 1, "one shape");

    // The occurrences themselves are a lot of JSON to spend on a
    // question about which parts there are, so the count stays and the
    // list goes until it is asked for.
    assert!(doc["relations"].is_null());
    assert_eq!(doc["relations_omitted"]["count"], 4);
}

#[test]
fn the_tree_is_there_when_it_is_asked_for() {
    let replies = run(&[call(
        1,
        "list_parts",
        json!({ "path": fixture("synthetic/assembly_repeated_part.stp"), "tree": true }),
    )]);
    let doc = structured(&replies[0]);
    let relations = doc["relations"].as_array().unwrap();
    assert_eq!(relations.len(), 4);
    assert!(
        relations.iter().all(|r| r["placement"].is_object()),
        "every occurrence states where it sits"
    );
}

#[test]
fn describing_a_part_gathers_all_three_documents() {
    let replies = run(&[call(
        1,
        "describe_part",
        json!({ "path": fixture("synthetic/assembly_repeated_part.stp"), "part": "SYN-PIN" }),
    )]);
    let doc = structured(&replies[0]);
    let parts = doc["parts"].as_array().unwrap();
    assert_eq!(parts.len(), 1, "narrowed to one part");

    let pin = &parts[0];
    // From the product document.
    assert_eq!(pin["revision"], "C");
    assert_eq!(pin["occurrences"], 3);
    // From the features document.
    let envelope = &pin["bodies"][0]["envelope"];
    assert_eq!(envelope["size"], json!([10.0, 4.0, 4.0]));
    assert_eq!(envelope["approximate"], false);
}

#[test]
fn a_promoted_value_names_the_key_it_came_from() {
    let replies = run(&[call(
        1,
        "describe_part",
        json!({ "path": fixture("synthetic/assembly_properties.stp") }),
    )]);
    let doc = structured(&replies[0]);
    let bracket = doc["parts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["number"] == "SYN-BRACKET")
        .expect("the bracket");

    let attributes = bracket["attributes"].as_array().unwrap();
    let material = attributes
        .iter()
        .find(|a| a["field"] == "material")
        .expect("a material");
    assert_eq!(material["value"]["value"], "SYN-ALLOY-17");
    assert_eq!(
        material["from"], "Material",
        "a promotion names its source so it can be checked"
    );

    let mass = attributes
        .iter()
        .find(|a| a["field"] == "mass")
        .expect("a mass");
    assert_eq!(mass["unit"], "g", "the unit the key named");
}

#[test]
fn asking_for_a_part_a_file_does_not_have_says_so() {
    let replies = run(&[call(
        1,
        "describe_part",
        json!({ "path": fixture("synthetic/assembly_repeated_part.stp"), "part": "NOT-A-PART" }),
    )]);
    assert_eq!(replies[0]["result"]["isError"], true);
    let text = replies[0]["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("list_parts"), "{text}");
}

/// The one paragraph a client shows a model before it picks a tool has
/// to carry what a transport must not be able to strip (ADR 0013): here,
/// that these files may be confidential (ADR 0014).
#[test]
fn the_instructions_say_not_to_send_the_data_anywhere() {
    let replies = run(&[json!({
        "jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": { "protocolVersion": "2025-06-18", "capabilities": {},
                    "clientInfo": { "name": "t", "version": "1" } }
    })]);
    let instructions = replies[0]["result"]["instructions"].as_str().unwrap();
    assert!(instructions.contains("confidential"), "{instructions}");
    assert!(instructions.contains("web search"), "{instructions}");
}
