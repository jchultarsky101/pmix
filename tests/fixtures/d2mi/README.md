# NIST D2MI models (STEP AP203 edition 2)

Two of the five STEP files from NIST's *Digital Thread for Smart
Manufacturing* work — the "D2MI" (design to manufacturing and
inspection) model set. They are here for one reason: they are the only
public-domain files that carry the management entities `APPROVAL`,
`PERSON_AND_ORGANIZATION` and `SECURITY_CLASSIFICATION`, which the
product reader reads (ADR 0014) and which no model in the MBE PMI corpus
states.

- Source: <https://www.nist.gov/document/nist-d2mi-modelszip>
  (`NIST-D2MI-Models.zip`, 5.2 MB), retrieved 2026-09-12.
- Files: `827-9999-905.stp` and `827-9999-907.stp`, 12,723 bytes each,
  schema `AP203_CONFIGURATION_CONTROLLED_3D_DESIGN_OF_MECHANICAL_PARTS_AND_ASSEMBLIES_MIM_LF`.
  The other three in the bundle (an AP242 file and two larger AP203
  files) are not committed; nothing they add is needed here.

## Terms of use

From the `README.txt` shipped in the bundle:

> These files were developed at the National Institute of Standards and
> Technology by employees of the Federal Government in the course of
> their official duties. Pursuant to Title 17 Section 105 of the United
> States Code the results are not subject to copyright protection and
> are in the public domain. NIST assumes no responsibility for the
> results for use by other parties and makes no guarantees, expressed or
> implied, about their quality, reliability, or any other characteristic.

The files are redistributed here unmodified.

## What they carry, and what they do not

Each file states the entities with real roles — `design_owner`,
`creator`, `design_supplier`, `classification_officer` — through the
AP203 `CC_DESIGN_*` assignment forms. **Most of their values are
blank.** Every `ORGANIZATION` is `(' ',' ',' ')` and every `PERSON` is
unnamed; the `SECURITY_CLASSIFICATION` has a blank name and purpose but
its level does say `confidential`; the one `APPROVAL_STATUS` is
`not_yet_approved`, dated with zeros.

That makes them the fixture for the harder half of the question — *does
the reader keep exactly what a real writer states, and nothing more?* A
level with no name around it must come through; a zeroed date must not
become the first of January in year nought. `tests/fixtures/synthetic/part_identity.stp`
carries the values, through the AP242 `APPLIED_*` forms, so that between
the two both spellings and both halves are tested.
