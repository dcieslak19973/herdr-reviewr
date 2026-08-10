---
Status: Draft
Created: YYYY-MM-DD
Last edited: YYYY-MM-DD
---

<!--
A spec is the set of decisions currently in force, written so nobody re-decides one unknowingly.
Every sentence is a decision: name the alternative that lost, or it is a forgery.
A sentence leaves when its fork dies. The full bar lives in AGENTS.md.

- One concept per doc. End-state truth: what must be TRUE when the change is done.
- Invariants are the decisions that constrain other decisions. Every one carries a code.
- The Overview is the whole teaching budget. Examples sit inside the decision they illustrate.
- One fact per sentence. One grammatical template per list or table. Columns padded to align raw.
- One home per fact. Link to that home everywhere else.
- Under ~2,000 words. Over it, audit the forks before splitting.
-->

# <Concept name>

<One sentence: what this is and why it exists.>

## Overview

<The smallest mental model that makes the decisions below legible. Use an example when it teaches
faster than prose. This is the whole teaching budget.>

```json
{ "id": "chg_1A2b3C", "amount": 1099, "status": "succeeded" }
```

| field    | type    | meaning                                          |
| -------- | ------- | ------------------------------------------------ |
| `id`     | string  | unique charge identifier                         |
| `amount` | integer | amount in the smallest currency unit             |
| `status` | enum    | `pending`, `succeeded`, `failed`, or `unknown`   |

## Invariants

<Only decisions that constrain other decisions, each naming two designs it forbids.
Every one carries a code. Delete the section otherwise.>

| code                 | Always true                                            |
| -------------------- | ------------------------------------------------------ |
| `CHG-AT-MOST-ONCE`   | A charge captures at most once, however many retries.  |

## Behavior

<State operations as complete condition → outcome rows. Keep local rules beside the operation.>

| request                                    | outcome                                          |
| ------------------------------------------ | ------------------------------------------------ |
| valid `amount`, chargeable `source`        | `2xx`, one `pending` charge committed            |
| invalid `amount`                           | `400 invalid_request`, nothing persists          |
| same `Idempotency-Key` replayed within 24h | the original response, nothing charged twice     |

## Traces

<Only for temporal contracts: the duplicate, the race, the crash. Delete otherwise.
Steps share one shape: "actor does X. System does Y." Every trace carries a code.>

**CHG-CRASH-MID-CAPTURE: crash between debit and record**

1. The caller creates a charge. The row commits `pending`.
2. The processor debits the card.
3. The service crashes before recording the outcome.
4. Recovery marks the charge `unknown`, terminal.

## Failure semantics

<Only what no table above states: the second run, the concurrent run, the crash.>

## Non-goals

<The decisions to omit. One shape per bullet.>

- Does not handle refunds. See the refunds spec.

## Related specs

- [refunds](./refunds.md)
