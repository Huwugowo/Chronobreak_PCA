# Chronobreak product direction

## Product definition

Chronobreak is a League of Legends-specific desktop application centered on:

**Record -> Index -> Navigate -> Extract**

It automatically records matches, synchronizes recordings with League game data, makes the video library behave like a playable match history, and makes useful moments fast to find and clip.

## Core product promises

### Record
- recording is automatic;
- capture is reliable;
- capture should have negligible measurable impact on League;
- recording should stay out of the player's way.

### Index
- recordings are matches, not anonymous video files;
- League metadata and timestamped events should be durable and queryable;
- clips retain their relationship to source matches and contained events.

### Navigate
- the replay timeline is League-native;
- users can jump to meaningful events;
- descriptive match state can follow the playhead;
- search should use League concepts, not filenames alone.

### Extract
- event-to-clip should be fast;
- smart boundaries should reduce manual trimming;
- export should avoid unnecessary work;
- montage/recap features should be deterministic and built from the same indexed event model.

## Product boundary: not coaching

In scope:
- jump to deaths/kills/objectives;
- show score/items/state recorded at a timestamp;
- gold-difference or other descriptive graphs;
- search for pentakills, Baron steals, champions, opponents, queues, sessions;
- create clips or montages from recorded events.

Out of scope unless explicitly reconsidered:
- telling players a decision was good/bad;
- "you should have recalled here";
- coaching recommendations;
- build or matchup recommendations;
- decision grading;
- an AI coach;
- a competitive-information overlay unrelated to recording/replay.

## Product restraint

Chronobreak should not become:
- a generic all-games recorder;
- a social feed/network;
- a full professional video editor;
- an ad-heavy overlay platform;
- a coaching marketplace.

Prefer depth for League replay workflows over broad unrelated functionality.

## Data ownership

Prefer local-first behavior.

Recordings should remain ordinary user-owned media wherever practical. Library/index design should favor recoverability, portability, and re-indexing rather than making an opaque application database the only authoritative copy of user media relationships.

## Performance and reliability

Performance and reliability are product features, not implementation trivia.

Claims must be measurable.

Broad objectives such as "negligible performance impact" or "high reliability" are epics. They must be translated into measurable budgets, failure-mode coverage, and concrete child features before implementation is considered complete.
