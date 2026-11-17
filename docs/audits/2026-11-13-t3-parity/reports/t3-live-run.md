# T3 live comparison run

The user wants Ferrite assessed against actual T3 behavior across the application. This run uses the existing Sol and Sonnet subscriptions in the installed T3 app, then traces the observed behavior to source. Application code is unchanged.

Throughput checkpoint: n/a, read-only investigation with disposable runtime fixtures.

## Completion contract

Each reachable screen and control family needs a recorded entry path, observed result, source implementation, and cleanup state. Model-dependent journeys need an actual model response. Source presence and partial screenshots cannot certify a journey. Each case is VERIFIED, NOT VERIFIED, or INCONCLUSIVE. The report separates ordinary success, failure, recovery, keyboard and pointer routes, provider differences, and visual states.

## Execution order

1. Establish installed version, existing provider subscriptions and isolated project.
2. Exercise the Sol conversation, live output, tools, citations, attachments, queue, stop, agents and lineage.
3. Repeat provider-sensitive paths with Sonnet.
4. Exercise browser, panels, navigation, files, changes, terminal, PR reading and settings.
5. Recheck surprising outcomes and map each observation to source.
6. Review evidence, stop owned runs and servers, restore test settings, and publish coverage.

## Evidence rules

The native app observation channel can return stale accessibility or screenshot state after an action. An action acknowledgment alone does not establish success or failure. Native screenshots displayed in the conversation are not saved PNG artifacts unless an export succeeds. External PR writes and destructive actions are inspected without executing them.

## Initial setup

The isolated fixture is recorded in `../evidence/t3-live/fixture.json`. It contains a small calculator defect, long Markdown, a test image and a local browser page. No remote repository is configured.

## Published outcome

The native run and source matching are published in [the conclusions](t3-live-conclusions.md), with [case-by-case coverage](t3-live-coverage-reconciliation.md), [bounded independent review](t3-live-independent-review.md) and [cleanup](../evidence/t3-live/cleanup.json). The completion contract above describes the desired coverage; it is not a claim that every control variant or integration was executed. The final matrix records the unexecuted remainder. Ferrite application code is unchanged and its existing failures remain open.
