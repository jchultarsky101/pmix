# NIST MBE PMI test models (STEP AP242)

These 17 STEP AP242 files come from the NIST *MBE PMI Validation and
Conformance Testing* project and are the de-facto public corpus that CAD
vendors test PMI interoperability against. They are the ground truth for
`pmix`'s STEP reader.

- Source: <https://www.nist.gov/document/nist-pmi-step-files> (also shipped
  with the [NIST STEP File Analyzer and Viewer](https://github.com/usnistgov/SFA)).
- Project page and test-case definitions:
  <https://www.nist.gov/ctl/smart-connected-systems-division/smart-connected-manufacturing-systems-group/mbe-pmi-0>
- Expected PMI per test case (CSV): <https://github.com/usnistgov/CAD-PMI-Testing>

## Terms of use

NIST states that "the test cases, CAD models, and STEP files can be used
without any restrictions". NIST software and data are not subject to
copyright in the United States. The NIST logo must not be used to imply
endorsement. The files are redistributed here unmodified.

## What is in the files

| Series | Test cases | PMI content |
| ------ | ---------- | ----------- |
| CTC    | 01–05      | Combined test cases: representative PMI mix |
| FTC    | 06–11      | Fully toleranced per ASME Y14.5, including datum targets |
| STC    | 06–10      | Simplified FTC without datum targets (2024) |

- `*_ap242-e1`, `-e2`, `-e3`, `-e4` mark the AP242 edition the file was
  written against.
- All files contain both **semantic** (machine-readable) and **graphical**
  (tessellated or polyline) PMI, except `nist_ftc_08_asme1_ap242-e4-tg.stp`,
  which has tessellated geometry and graphical PMI only.
- The originating CAD system was deliberately removed from the headers.
- NIST warns these "are NOT reference STEP files without any errors": some
  contain syntax errors that conformance checkers flag. That is intentional
  realism for the parser.
- Line endings are CRLF in most files and are preserved byte-exact via
  `.gitattributes`.
