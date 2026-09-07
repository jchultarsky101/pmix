# ADR 0008: Reading numeric property values

- **Status:** Accepted, 2026-09-07
- **Deciders:** Julian Chultarsky
- **Refines:** [ADR 0007](0007-properties.md)

## Context

ADR 0007 gave properties typed values but left one case unstated. A
`descriptive_representation_item` carries a string, and files routinely
write counts, quantities, and prices as strings rather than as integer or
real items. In a local corpus of 200 production models, 3,000 of the
8,800 descriptive values are numeric: part counts, edge and face counts,
bounding-box dimensions, volumes, lead times, prices.

Left as text, none of them can be compared as numbers, sorted, or
summed by a consumer, and `pmix diff` would compare them as strings.

Coercing every numeric-looking string is not safe either. `007` is a
serial number, not the integer seven; `2.50` states a precision that
`2.5` drops; `1e5` may be a part code; `inf` and `NaN` parse as floats.
Reading those as numbers would silently destroy information the file
carries.

## Decision

A descriptive value is read as a number when it is **written in plain
decimal notation and nothing else**: an optional minus, then `0` or a
digit string not starting with `0`, then optionally a decimal point and
one or more digits. A value with no fractional part must fit in a 64-bit
integer. Everything else stays text.

| Text | Read as | Why |
| ---- | ------- | --- |
| `12`, `-4`, `0` | integer | |
| `18.75`, `0.1`, `64.0` | number | |
| `007` | text | A leading zero belongs to the serial, not to a count |
| `+5`, `1e5`, `1,234` | text | Notation a plain number does not carry |
| `3.`, `-.5`, ` 12`, `12 ` | text | Not a number's own spelling |
| `inf`, `NaN`, `12mm` | text | Not a number at all |
| `9223372036854775808` | text | More digits than an integer holds |

Two boundaries are deliberate.

**Leading zeros keep the text.** `007` and `7` are different part numbers,
and a rule that conflated them would corrupt identifiers silently.

**Trailing zeros do not.** `64.0` reads as the number `64.0`, and `2.50`
as `2.5`. An earlier draft of this rule required the text to be exactly
what re-rendering the number produces, which rejected `64.0`. Measured
against the local corpus that turned out to be wrong: `bbox_x_mm` came out
as a number for `64.5` and as text for `64.0`, so the type of a field
depended on whether a measurement happened to land on a whole number, and
no consumer could rely on it. What that stricter rule protected was
display precision, which is worth less than a stable type.

Measured against the local corpus, all 3,000 numeric values are read as
numbers, no field has more than one type, and none of the other 5,800
values is disturbed.

The rule lives in the model (`PropertyValue::from_text`), not in the STEP
reader, so a JT reader gets the same behaviour.

## Alternatives considered

- **Coerce anything that parses.** `f64` accepts `007`, `1e5`, `inf`, and
  `NaN`, so part numbers and sentinel strings would become numbers.
- **Require the text to be exactly the number's own rendering.** Safe, but
  it splits a field's type on whether its value is whole, as measured
  above.
- **Leave every descriptive value as text.** Faithful to the file, but a
  count is then not a number to any consumer, which was the complaint.
- **Coerce and keep the original text alongside.** No information lost,
  but it adds a field to every property to serve a case the round-trip
  rule already handles.
- **Decide by field name**, coercing `*_count` and `*_price`. Brittle
  across vendors and silently wrong for a field the list has not seen.

## Consequences

- Only descriptive items are affected; measure, integer, real, and boolean
  items are already typed by the file.
- A consumer reading a property must handle any of the value types, which
  ADR 0007's tagged representation already required.
- The synthetic fixture carries both outcomes, including a whole-number
  measurement, so a future change to the rule fails a test rather than
  passing quietly.
- Display precision is not preserved: a file writing `2.50` yields `2.5`.
  The measured value is unchanged, and the original text remains in the
  file, which `source_refs` points at.
