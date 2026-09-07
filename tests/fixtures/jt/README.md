# NIST MTC assembly (JT)

`nist_mtc_assembly.jt` is the JT export of the NIST *Design, Manufacturing,
and Inspection Data for a Box Assembly* model, written by NX. It is the
project's JT fixture.

- Source: <https://www.nist.gov/document/nist-cad-models-mtc-assembly>, a
  zip of native CAD files; this is `NIST-MTC-Assembly/NX/NIST mtc crada
  assembly.jt`, redistributed unmodified.
- Terms: NIST work is not subject to copyright in the United States and is
  in the public domain. The NIST logo must not be used to imply
  endorsement.

## What it contains

JT version 10.5, little-endian, 107 segments:

| Segments | Kind |
| -------- | ---- |
| 30 | meta data |
| 14 | PMI data |
| 39 | shape levels of detail |
| 8 | XT B-Rep |
| 8 | STT |
| 5 | wireframe |
| 1 each | logical scene graph, multi XT B-Rep, info segment |

It **does contain PMI**, which the earlier survey in
[docs/test-data.md](../../../docs/test-data.md) had recorded as unverified:
the PMI data segments hold PMI Manager elements
(`ce357249-38fb-11d1-a506-006097bdc6e1`) and the metadata segments hold
property proxies (`ce357247-…`) carrying CAD attributes.

Segment payloads are XZ compressed, so reading it exercises the whole
decode path: header, table of contents, segment, decompression, elements.
