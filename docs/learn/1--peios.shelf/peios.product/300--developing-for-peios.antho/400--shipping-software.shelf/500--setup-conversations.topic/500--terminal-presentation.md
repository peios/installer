---
title: Terminal Presentation
type: concept
description: Understand how installer terminal interfaces render MSIP pages and optional presentation hints.
---

The installer's `msip-tui` renderer draws pages for `install-tui` and
`oobe-tui`. Daemons supply typed elements through the multi-surface
interaction
protocol (MSIP); the renderer chooses their arrangement and appearance.
A page still needs to work when another surface ignores its presentation
hints.

## Page arrangement

Without a recognized turn class, the renderer places the heading and elements
in a column, with actions together below them. The `menu` class moves actions
into the body as a list. The `confirm` class marks the heading as a
confirmation.
Neither class changes the daemon's validation or makes an action safe to run.

The renderer checks these classes independently. Prefer one clear purpose
for a turn rather than assuming that combinations define another layout.
Unknown classes do not acquire behavior merely by being sent.

Progress elements render as a phase list. The first unfinished progress
element is active, and a log can use the remaining height. This follows the
element types and state; it does not require a `progress` turn class.

## Element presentation

| Element information | Terminal behavior |
|---|---|
| An action's `primary` state | Accent color and bold text |
| An action's `destructive` state | Destructive coloring, including focus |
| A string's `secret` flag | Masked entered text |
| A string's `placeholder` state | Dim text while the empty field is unfocused |
| A table column's `align: right` | Right-aligned cells |
| A table row's `enabled: false` | Dimmed row, skipped during selection |
| A table row's `note` | A remark after its cells |
| A table's `empty` state | Text shown when it has no rows |
| An element's `help` | Context explaining the element or why it is unavailable |
| An element's `error` | An error below the element until cleared |

Masking is a display property. It does not make logging, debug formatting,
or retaining a submitted secret safe. Follow the
[MSIP library's secret handling guidance](https://learn.peios.org/peios/developing-for-peios/setup-conversations/rust-library/)
and keep secrets out of turn updates and diagnostics.

A primary action's appearance does not authorize unattended execution.
Choose explicit actions in automation. A destructive action is not given
extra validation by its color; the daemon owns confirmation and authorization.

## Focus and narrow terminals

Unavailable input elements can take focus, allowing a person to discover
why they cannot answer them. Supply useful `help`, such as the missing
prerequisite. Initial focus favors the first answerable element, with the
primary action used where appropriate.

Fields reserve space for their note so moving focus does not shift the page.
Errors remain visible. Help appears when the element is focused or
unavailable.
Table selection skips unavailable rows, and the answer validator also rejects
them; dimming alone is not the enforcement mechanism.

Tables drop columns from the right when space is limited. Put the column
needed to identify a row first, and test narrow terminal widths.

## Leaving and automation

Esc opens the surface's leave question. Its wording depends on the frontend:
leaving an ongoing installation differs from leaving initial account setup.
A daemon page cannot override that lifecycle meaning with a presentation
class.

`msip-drive` does not draw the terminal layout. Its scripted answers rely on
refs, values, and actions rather than color or class hints. Use
[Installer Conversation Rehearsals](~peios/setup-conversations/rehearsing-conversations)
to check that the same conversation remains usable through automation.
