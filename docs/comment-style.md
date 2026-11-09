<!--
  docs/comment-style.md: the rules for comments in every language in this repository.
  Owns: what a comment must say and must not say. Does not own: formatting tools or lint settings.
-->

# Comment style

Comments here explain what the code cannot say for itself. The rules follow the comment style of
[spawn-at](https://harshp2008.github.io/spawn-at/project/comment-style/), extended to every
language in this repository, and with one deliberate difference: **private items and tests are
documented too**.

## What every comment states

1. **Why.** The constraint, failure or hardware quirk the code exists to handle. "Increment the
   counter" is noise; "the tablet reports pen-up late, so wait 40 ms before ending the stroke" is not.
2. **Invariants.** What must be true before and after (for example "never negative", "sorted by
   time", "caller holds the session lock").
3. **Units.** UScreen thinks in real units. Write millimetres, milliseconds and hertz, and say when
   a number is in device pixels or raw tablet counts. A bare `i32` that is a length always names its unit.
4. **Returns and failure.** What a function returns, when it returns nothing or an error, and what
   panics (Rust) or throws (Kotlin, TypeScript).
5. **Side effects.** Files written, devices opened, processes spawned, messages sent.

## Where comments go

| Language | Required |
|---|---|
| Rust | `//!` module doc on every file; `///` on every item, private items and tests included; `//` for non-obvious lines |
| TypeScript / TSX | A file header; a doc comment on every component, hook, function and exported type |
| Kotlin | A file header; KDoc on every class, object and function |
| JavaScript (GNOME extension) | A file header; a comment on every method |
| C (`host/evdi/`) | A file header; a comment on every function and every global |
| Shell and Python | A header; a comment on every function |
| Config, units, rules, manifests | A top comment saying what the file is for, what it owns and what it does not. Formats that cannot take comments (JSON) are listed in `docs/file-headers.md` |

## The truth rule

- Every statement must be checkable from the code or an existing document. Read the function and
  its callers before writing about it.
- When unsure, say less. A missing comment is better than a wrong one.
- If a comment contradicts the code, the code wins: fix or delete the comment and note it in the
  commit message. Never change code to match a comment.
- No release status, version numbers or commit hashes in comments. That is the changelog's job.
- No invented history. Only claim a past cause if a commit, an experiment record or a doc supports it.

## Public docs versus other files

`///` and `//!` appear on the published rustdoc, so they describe the item and its contract only.
A statement about another file goes stale; put it in a plain `//` comment with a `grep` hint, or leave it out.
Avoid "currently", "for now" and "recently".

## Form

- Short: usually one sentence, at most a short paragraph.
- Rust `///` examples use a `text` fence unless they are real doctests (`cargo test` compiles rust fences).
- Only write intra-doc links that resolve.
- A comment that would only repeat the name is replaced by what the name hides: the unit, the range,
  the reason, the caller.

## Forbidden

- Template filler: "Returns the value", "Creates a new instance", "Getter for x".
- Restating the type or the name.
- Claims about other components (GNOME Shell, Android, the compositor) that were not read in the code or measured in an experiment.
- Comment-only changes that also change code, strings, attributes, imports or whitespace of code.

## Examples

Good:

```rust
/// Pen distance from the glass in millimetres; `None` when the tablet has not reported hover yet.
fn hover_height_mm(&self) -> Option<f32> { /* ... */ }
```

Bad:

```rust
/// Returns the hover height.
fn hover_height_mm(&self) -> Option<f32> { /* ... */ }
```
