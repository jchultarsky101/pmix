# ADR 0013: Serving the documents to a language model

- **Status:** Accepted, 2026-09-10
- **Deciders:** Julian Chultarsky
- **Depends on:** [ADR 0011](0011-feature-recognition.md), [ADR 0012](0012-geometric-comparison.md)

## Context

ADR 0012 divided the work: `pmix` extracts base data specific and
complete enough to reason over, and deciding what the differences *mean*
belongs to a consumer. The consumer it had in mind was a language model.

Nothing delivers the data to one. A person can run `pmix features a b`
and read the output, and a script can parse the JSON, but a model
answering "what changed between these two revisions?" has no way to ask.
The Model Context Protocol is the ordinary way to give it one.

**What is already right for this.** The comparison document was designed
for exactly this reader: ordered by the weight each part carries, with a
displacement stated once rather than repeated across every feature, and
with what is provable separated from what is merely observed. Little of
the work is in the protocol.

**What is not.** Three things.

- **Size.** A real assembly is not small. Models in the local corpus run
  to fourteen bodies, and NIST parts to over a hundred features. Handing
  a whole document to a model spends most of a context window on faces
  nobody asked about.
- **The hedging does not travel.** `pmix features` prints "a candidate
  is an observation, not a conclusion" beneath its output, and that
  sentence exists only in the text renderer. The JSON says `candidates`
  and stops. A model reading the JSON sees a list that looks like
  findings, and will report it as findings — the exact failure ADR 0012
  set out to avoid, reintroduced by the transport.
- **Noise becomes prose.** ADR 0012 already noted that a model consuming
  a noisy document turns the noise into confident sentences. A protocol
  that makes the documents easy to consume makes that easier too.

## Decision

**The server lives in this repository**, behind a Cargo feature, as a
`pmix mcp` subcommand speaking stdio.

The reason is this project's own policy of shipping without
compatibility promises. There are three independently versioned
documents now and they move freely. A separate repository would pin a
version of `pmix` and break on each change, buying a cross-repository
release for what is a thin wrapper over the library API. In one
repository it moves in lockstep and the rule that docs change with the
code covers it. A feature gate keeps the protocol's dependencies out of
the default build, so `cargo install pmix` stays what it is.

**The server computes nothing.** It is transport and shaping. Anything
that needs logic — a filter, a summary, a grouping — goes in the library
first, where the test suite reaches it, and the server calls it. A
server with its own analysis in it would be analysis nobody tests.

**Tools are shaped for a context window, not for completeness.** Each
takes the filters that let a caller ask a narrow question: a body, a kind
of feature, only what changed. The full document remains available, but
asking for it is a choice rather than the only option.

**What `pmix` refuses to conclude is marked in the payload**, not in
prose beside it. A candidate pairing carries, in the data, the fact that
it is an observation and the distance it rests on. A transport must not
be able to strip the caveat by rendering only the fields it understands.

**Read-only.** The server reads model files and returns documents. It
does not write, convert, or modify anything. A tool that a model can call
without a person watching should not be able to change a file.

### What the model is for

Recorded so the boundary stays visible. The interesting answers are the
ones this project deliberately does not produce:

| the model does | because |
| --- | --- |
| names a pattern — "the bolt circle went from Ø100 to Ø110" | `pmix` states four positions; seeing four positions as a pattern is not a local geometric fact |
| joins PMI to geometry — "the hole grew but its position tolerance did not" | the two documents are kept separate on purpose (ADR 0011); the insight exists only in the join |
| settles move against delete-and-add | it can weigh part names, properties, revision context, none of which is geometry |
| triages — "37 of these 41 are the same 0.003mm shift, so this is a re-export" | `pmix` reports each difference; deciding they are one story is a judgment |

## Alternatives considered

- **A separate repository.** Cleaner in scope, and wrong for this
  project: with no compatibility promises the pin would break constantly.
  Revisit if the server grows a domain of its own — caching, corpora,
  authentication — which would say it is a different product.
- **Expose the documents whole and let the model filter.** Simple, and it
  spends a context window on a hundred faces to answer a question about
  one hole.
- **Let the server compose the narrative** — call the model itself, hand
  back prose. That makes `pmix` an opinion rather than a source, and the
  opinion would be untestable.
- **HTTP rather than stdio.** Needs authentication and a deployment
  story, for a tool that runs beside the files it reads.

## Consequences

- The features and comparison documents gain a field carrying what the
  text output already says: that a candidate is an observation. This
  changes the JSON, so it is a schema change, and the CLI's text renderer
  should read that field rather than restating it.
- Determinism stops being tidiness. A model consuming a document that
  differs run to run will explain the difference.
- The tests for the server are the library's tests plus a thin check that
  each tool returns what the library returned. If a tool needs a test of
  its own, the logic is in the wrong place.
- `pmix` acquires a second audience. The CLI is read by people and the
  protocol by models, and where those want different things — brevity
  against completeness — the documents answer to the model and the text
  renderer answers to the person.
