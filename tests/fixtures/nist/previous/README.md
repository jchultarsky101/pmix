# Earlier builds of four NIST models

The June 2024 build of NIST's `NIST-PMI-STEP-Files.zip`
(<https://www.nist.gov/document/nist-pmi-step-files>) shipped earlier
exports of four models that the February 2026 build replaced. Each file
here is the *same design* as its counterpart in the parent directory,
written in a different AP242 edition, so the pair exercises identity
across re-exports (ADR 0004).

| Here | Current | What differs |
| ---- | ------- | ------------ |
| `nist_ctc_04_asme1_ap242-e1.stp` | `nist_ctc_04_asme1_ap242-e2.stp` | Edition label only; same export |
| `nist_ftc_08_asme1_ap242-e1-tg.stp` | `nist_ftc_08_asme1_ap242-e4-tg.stp` | Edition label only; same export |
| `nist_ftc_11_asme1_ap242-e2.stp` | `nist_ftc_11_asme1_ap242-e3.stp` | Edition label only; same export |
| `nist_stc_09_asme1_ap242-e3.stp` | `nist_stc_09_asme1_ap242-e4.stp` | Genuine re-export: hole feature definitions added, some geometry entities rewritten |

Terms of use are the same as for the parent directory: NIST public domain,
"can be used without any restrictions".
