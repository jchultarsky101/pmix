# Test data

`pmix` is tested at three levels. This document records what data exists at
each level, where it comes from, and under what terms, so that nobody has to
redo the licence research (done 2026-09-06).

## 1. Synthetic fixtures: controlled tests

Tiny hand-written STEP files under `tests/fixtures/synthetic/`, one construct
each: a position tolerance with a material condition, a datum system with two
compartments, a dimension with a plus-minus tolerance, and so on. They are
authored by the project, so they are MIT like the code. They are the proof
that a specific behaviour works; every extractor feature ships with one.

They carry more weight than "controlled tests" suggests for **product
structure** (ADR 0014). Not one of the 17 NIST models states an assembly
usage, so `assembly_two_parts.stp` and `assembly_repeated_part.stp` are the
*only* committed coverage of parts, occurrences and placements — and they
encode what the reader expects rather than confirming it against a real
writer. Verify product-structure changes against a real export as well.
`flange_bolt_circle.stp`, `bar_hole_row.stp` and
`plate_square_bolt_pattern.stp` do the same job for hole patterns; see that
directory's README for what each one is for.

The JT fixture is `tests/fixtures/jt/nist_mtc_assembly.jt`, public domain
from NIST; see that directory's README.

## 2. NIST corpus: realism

The 17 NIST MBE PMI AP242 models under `tests/fixtures/nist/` (see that
directory's README for provenance and terms). They are public domain and
carry the explicit statement that they "can be used without any
restrictions". They contain both semantic and graphical PMI from several CAD
systems and deliberately include syntax errors.

`tests/fixtures/nist/previous/` holds the June 2024 builds of four of these
models. Each is the same design as its current counterpart, re-exported in
another AP242 edition, and one (STC 09) is a genuine re-export from a newer
CAD version. They are the test set for identity across exports
([ADR 0004](adr/0004-identity.md)).

Three independent oracles exist for what these files should yield:

| Oracle | Where | Notes |
| ------ | ----- | ----- |
| NIST expected PMI | [CAD-PMI-Testing](https://github.com/usnistgov/CAD-PMI-Testing), `CTC-FTC-Test-Cases.csv` and `CTC-FTC-PMI-Results.csv` | PMI per test case as designed |
| NIST STEP File Analyzer | [usnistgov/SFA](https://github.com/usnistgov/SFA), `source/sfa-nist.tcl` | Expected PMI per model, encoded in the analyser |
| OpenCascade GD&T tests | [OCCT `tests/gdt`](https://github.com/Open-Cascade-SAS/OCCT/tree/master/tests/gdt) | Expected dimension, tolerance, and datum counts for these exact files |

## 3. External sources: writer and edition diversity

Not committed. Candidates for a future fetch-on-demand tier, kept here so the
licence status is not lost. Sizes are approximate.

### With PMI

| Source | Content | Licence | Status |
| ------ | ------- | ------- | ------ |
| [NIST PMI zip, nist.gov build](https://www.nist.gov/document/nist-pmi-step-files) (14 MB) | Five AP203 files with graphical-only PMI; earlier editions of CTC 04, FTC 08 tessellated, FTC 11, STC 09 | Public domain | Verified; useful for cross-edition regression |
| [NIST D2MI models](https://www.nist.gov/document/nist-d2mi-modelszip) (5.5 MB) | `827-9999-904 rev c.stp`, AP242 written by CoreTechnologie 3D_Evolution; four AP203 parts | Public domain (17 USC §105 stated in README) | Verified; different AP242 writer |
| [STEP Tools samples](https://www.steptools.com/docs/stpfiles/) | Seven AP214 `boxy_with_*.stp` files (~90 KB) with semantic flatness, cylindricity, size, and limits-and-fits tolerances | None stated | Fetch-only, optional tier; do not vendor |
| [NIST MTC assembly](https://www.nist.gov/document/nist-cad-models-mtc-assembly) (16 MB) | One JT 10.5 assembly written by NX | Public domain | Verified, **and it does carry PMI**: 14 PMI data and 30 meta data segments. Committed as `tests/fixtures/jt/nist_mtc_assembly.jt` |

### Geometry only, for parser robustness

| Source | Content | Licence | Status |
| ------ | ------- | ------- | ------ |
| NIST PMI zip, `AP203 geometry only/` | 11 AP203 files, 7 KB to 1.1 MB, with known syntax errors | Public domain | Verified |
| [stepcode](https://github.com/stepcode/stepcode) `test/p21/`, `data/ap214e3/`, `data/ap209/` | Deliberately malformed edge cases; CATIA and Datakit AP214 exports; AP209 FEA files | BSD-3 | Verified |
| [OCCT](https://github.com/Open-Cascade-SAS/OCCT) `data/step/` | `screw.step`, `linkrods.step`: 1998 Euclid writer, old `AUTOMOTIVE_DESIGN_CC1` dialect | LGPL-2.1 with exception | Verified; data files only, not linked |
| [FreeCAD](https://github.com/FreeCAD/FreeCAD) `data/tests/Step/` | `as1-ac-214.stp` AutoCAD 2000, AP214 DIS dialect; `gasket1.p21`; `Schenkel.stp` | LGPL-2.1; Schenkel CC-BY-SA 4.0 | Verified |
| [BRL-CAD/models](https://github.com/BRL-CAD/models) | FarmBot as AP203, AP214, and AP242 (60 MB each, CC0); ReCurta AP203 and AP242 pairs (public domain); NASA Open Source Rover AP242 (Apache-2); Openmoko Pro/E assemblies up to 45 MB (CC-BY-SA 3.0) | Per folder, see README in each | Verified per folder; skip the `Thingiverse/` and McMaster-Carr files |
| [KiCad packages3D](https://gitlab.com/kicad/libraries/kicad-packages3D) | ~6,000 small OpenCascade-written parts | CC-BY-SA 4.0 with exception | Verified; download a sample, do not redistribute as a collection |
| [FreeCAD-library](https://github.com/FreeCAD/FreeCAD-library) | ~2,900 parts by many authors | CC-BY 3.0, attribution per author | Verified |
| [MFCAD](https://github.com/hducg/MFCAD) | 15,000 synthetic OpenCascade files, ~40 KB each | MIT | Verified; bulk coverage |
| Mayo, Foxtrot, CadQuery, build123d test inputs | A handful of small files each; Mayo's `#332_file.stp` exercises URL encoding | BSD-2, MIT/Apache-2, Apache-2 | Verified |

### JT, licence unverified

FreeCAD's `data/tests/Jt/Engine/` (JT 8.0, the Siemens two-cylinder engine
sample), PyOpenJt's example files (JT 8.0 to 10.3, copied from a repository
with no licence), and assimp's `conrod.jt` all sit in open-source repos but
their original provenance is unstated. Use for local experiments only.

## What is still wanted

Three things, stated exactly, because they are what we have to ask a
supplier or a colleague for.

**1. The STEP counterpart of the JT assembly.** The cross-format identity
check needs one design in both formats. Half of it is already here:
`tests/fixtures/jt/nist_mtc_assembly.jt` carries precise geometry — eight
topology segments, eight bodies — so it is the *STEP* half that is
missing, not the JT.

> This corrects what this document said before 2026-09-11, which was that
> every JT reaching the project had been tessellation-only and a
> geometry-bearing JT was the thing to hunt for. That was true of the
> customer exports and stopped being true when the NIST MTC assembly
> arrived. Anyone acting on the old sentence would have gone looking for
> the wrong file.

Since [ADR 0011](adr/0011-feature-recognition.md) a recognised feature is
keyed on geometry alone, so two *feature* documents of one design test the
shared identity recipe directly; PMI is not needed. A JT can still
legitimately carry no geometry at all, so check any candidate in one
command before building anything on it:

```bash
pmix features candidate.jt
```

It distinguishes the two cases, and the difference decides what to ask
for. A file with **neither** a topology table nor a B-rep segment is
tessellation-only and no export setting will save it. A file with B-rep
segments but **no topology table** has its precise geometry — as
Parasolid XT — and `pmix` cannot reach it: the table is described by the
specification (section 11.14) as "a lightweight abstraction of the
existing precise B-Rep data", and it is optional. That file does not
need replacing, it needs re-exporting by something that writes the
table.

One of the JT files to hand is exactly that second case, which is why
the distinction is worth stating: it looked like a tessellation-only
export for months and is not one.

See [ADR 0004](adr/0004-identity.md) for what the check would settle.

**2. A model that states a thread.** Threads are the one thing a fastener
is ordered by and nothing here carries them: one designation (`M12x1.75-6H`)
in one of the seventeen NIST files, reaching the reader as a property value
rather than as PMI, and none at all in two hundred real exports. What files
do carry is a *name* on a shape aspect — "Thread Cylinder", "Radial Hole
and Thread Callout" — which says a thread exists and not which one. Until a
file states a designation properly, a parser for them would encode its
author's assumptions with nothing to check them against
([ADR 0014](adr/0014-part-description-for-sourcing.md)).

**3. A model that states an approval, an owner, or a security
classification.** Same shape of problem: `PERSON_AND_ORGANIZATION`,
`APPROVAL` and `SECURITY_CLASSIFICATION` appear in neither corpus, and the
last of those gates a caution ADR 0014 puts on part numbers leaving the
machine. A single file with them would let that be built and tested rather
than assumed.

The same check applies to surface finish, which is also absent everywhere:
not one surface-texture entity across all 218 files tested against.

## Local data

The `data/` directory is ignored by git and **may hold proprietary customer
models**. Never commit anything from it, never attach one of its files to
an issue or pull request, and never quote a part name, part number, or any
other value out of one. Nothing in the test suite depends on it; it is for
exploring the readers by hand, for example with `pmix inspect`, and for
checking a change against real parts before release.

It is also where the **scale test** lives. The committed fixtures are
small; a real master assembly is not, and anything touching the STEP
reader should be run against one before release. The largest to hand is
about 158 MB and three million lines, with several hundred product
definitions and over a thousand assembly usages; `pmix product` reads it
in under ten seconds. That is the shape of file that proves the reverse
index in `step/product.rs` was necessary, because `Exchange::referrers`
is a linear scan and a per-part lookup would be quadratic there.

Everything under `tests/fixtures/` is different: it is public-domain NIST
data and synthetic fixtures written for this project, and it is committed
deliberately.

## Do not use

- **Fusion 360 Gallery, SolidLetters, and other Autodesk research datasets:**
  non-commercial research only, no redistribution.
- **ABC dataset:** per-model Onshape terms, indeterminate for older
  documents, and multi-gigabyte chunks.
- **CC3D** (signed agreement), **CADNET** (NC-SA), **FabWave** (no licence).
- **Anything GrabCAD-derived**, including files in AnalysisSitus and the
  Thingiverse folders of BRL-CAD/models. GrabCAD terms forbid redistribution.
- **Manufacturer catalogues:** PARTcommunity/CADENAS explicitly forbids
  distribution and machine-learning use; Misumi, McMaster-Carr, TraceParts,
  and 3DContentCentral either forbid redistribution or hide their terms
  behind logins.
- **Members-only benchmarks:** CAx-IF and prostep test rounds, AFNeT AP242
  benchmark models, the private OpenCascade test dataset, Siemens JT2Go
  bundled samples.
