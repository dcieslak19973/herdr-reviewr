# Writing great specs

A spec is a communication medium. It is never a scratchpad. This bar applies to every touch: a
one-line edit meets it, a new doc meets it, and no tier of spec editing is exempt.

## The model

A spec is the set of decisions currently in force, written so that nobody re-decides one unknowingly.

Every sentence in it is a decision. A sentence with no decision behind it is a forgery: it converts an
implementer's free choice into a constraint nobody chose, and it does so invisibly, because forged
sentences are true. Bloat is not verbosity. Bloat is forged decisions.

The decisions come from the design conversation, where each fork was surfaced and resolved. Writing is
serialization, never a second place to decide. A decision you find yourself making at the keyboard goes
back to the conversation first.

## Admission and deletion

A sentence enters when you can name the alternative that lost. "The guard reads the provider live,
never the mirrored ledger" names its fork. "The synced stream is statusless, so the ledger cannot
answer" names none: it is the argument that settled the fork, and the argument's work is done.

Name the two implementations the sentence tells apart. Cannot name them, there was no fork, and the
sentence is a forgery whatever else it is.

This removes, without a rule each: rationale, mechanism the builder chooses, provenance, and anything
an earlier sentence already settled. Rationale lives in the PR description, provenance in git history.
There is no Decisions section, because the whole spec is one.

A sentence leaves when its fork dies. Three ways:

- Superseded: a later decision replaced it. The edit that made the new decision deletes the old
  sentence in the same pass.
- Collapsed: the losing alternative no longer exists, so the sentence constrains nothing.
- Orphaned: nobody can name the fork it resolved. It was never a decision. Delete on sight.

Test the whole document on every edit, never the diff alone. A pure addition resolves new forks and
legitimately deletes nothing.

The spec is bounded by the number of live decisions, so it grows only when the design grows. Code
elaborates without deciding, so the spec stays flat while the code moves. Over ~2,000 words is a
symptom: audit the forks before splitting the doc.

## Where a decision lives

Documents partition the decision set. Decisions that must be revisited together live together, which is
what one concept per doc has always meant. A decision has one home, and every other mention links to it.

- A section groups the decisions about one entity, one operation, or one outcome.
- A list or table states its admission rule and holds every decision that rule admits.
- A dedicated section exists only when its subject holds decisions of its own. A hard investigation or
  an unusual source does not earn structure.
- Order: front matter, a one-line purpose, an Overview, then the concept's nouns, then Non-goals and
  Related specs when either has content.
- Front matter carries `Status` (Draft, Current, Superseded), `Created`, and `Last edited`, ISO dates.
- Model sections define entities and fields. Operation sections use condition → outcome tables.
- Traces exist only for temporal contracts: the duplicate trigger, the concurrent run, the crash.
  Delete the section otherwise. Every trace carries a code.
- Failure semantics carries only what no table already states.
- Non-goals are the decisions to omit. They resolve more arguments than the goals.
- Headers: `###` max, short noun phrases, parallel across siblings.

After drafting, read only the outline and the lead-in to every collection. If they do not explain the
structure without the body text, restructure before polishing.

### Weight

The hardest decisions to reverse sit highest. Every behavior section opens in prose with the decision
the reader must keep if they keep one, and the lists below carry the subordinate ones.

- The promotion test: if reversing a decision would change how someone uses the product, it leads. If
  it would only mishandle an edge, it is a bullet.
- A decision the reader can verify only by simulation carries a one-line scenario in place: "Pause a
  2-day wait for 3 days, and the next message is ready the moment you resume."
- The more likely the reader's prior guess is wrong, the higher the decision sits and the more concrete
  its scenario. A deviation from comparable products with no fork behind it is the worst forgery,
  because the reader will not think to question it.

### Invariants

An invariant is a decision that constrains other decisions. Violating one is a bug by definition, never
a policy outcome.

A candidate joins the collection only when all of these are true:

- It prunes: you can name two designs it forbids.
- It is unconditional. No "usually", no "unless". A qualified promise is one operation's outcome and
  belongs in that operation's table.
- It is non-local. No single field or operation section can own it, the way `DM-ONE-ORG` sits inside no
  one entity's table.
- It holds shape, never numbers. A tunable value is a parameter, changed by decision, not by bug.

Every admitted invariant carries a code: the owning doc's prefix plus an uppercase kebab slug of the
fact, `DM-BORN-WHOLE`, `API-AT-MOST-ONCE`. Register each doc's prefix in the README ownership map. A
shipped code never changes its meaning: retire it, never reuse it for a different fact.

## How it reads

Humans scan. They parse a sentence once, left to right, holding nothing on a stack. A correct spec that
reads slowly has failed. Every spec passes the stop-slop skill in full.

- One fact per sentence. Reading time scales with facts per line, not words per line.
- Linear sentences. No em dashes, no semicolons, no nested asides. Split into two sentences.
  Parentheses carry citations only.
- Subject first, present tense, active voice.
- Plain words over compact jargon. Expand an acronym at first use.
- Never gesture. "Handle errors gracefully" and "optional filters" name nothing and cannot be built or
  reviewed. Name them or cut the line.
- Point at identifiers and paths. Never paste schemas, SQL, or exhaustive field validation: pasting one
  settles every representation fork at once, which is never the intent.

Packing four facts into one line saves words and costs the reader a stack. Split it.

Repeated elements share one grammatical template and one treatment. Same subject class, same verb form,
same clause count. Same identity: if one member of a collection carries a code, every member does. A
difference the reader cannot explain costs them a stop. Divergent treatment inside one collection
usually means two admission rules hiding in it.

- Prefer table schemas that absorb the grammar: condition → outcome, attribute → value, question → answer.
- Bullets in one list share one shape. A fact that does not fit the shape belongs in a different list.
- A trace heading reads `**CODE: what happens**`. Its steps read "actor does X. System does Y (→ CHG-AT-MOST-ONCE)."
- Pad table columns so the raw markdown aligns. Specs are read raw and in diffs, so alignment is part of
  readability. Pad by hand, every edit.

The test: read the list aloud. If the melody changes mid-list, it is not uniform.

Uniformity governs siblings inside a collection. Weight governs which one leaves it and leads. Identity
is uniform, emphasis is not: uniformity settles which members carry a code, Weight which carry a
scenario.

## The teaching budget

Applied alone, the model deletes every orientation paragraph and every example, because none of them
records a decision. The reader needs some. This is the one place a spec carries text that resolves no
fork, and it is bounded on purpose.

- One Overview per doc: the smallest mental model that makes the decisions below legible. Use an example
  when it teaches faster than prose.
- Examples sit inside the decision they illustrate, never in a gallery of their own.
- Nothing else. A second explanatory passage is either a decision you have not found yet, or bloat.

## Lifecycle

- **Draft**: end-state truth for a change in flight. Born here, during brainstorming.
- **Current**: the implemented, reviewed code matches the spec. Planning promotes it before the PR opens.
- **Superseded**: replaced, and moved to `specs/archive/` with a pointer to its replacement.

Update `Last edited` on every edit. Git holds the full history.
