A couple grounding facts first (so we don’t build on sand):

TOON Rust: toon-format/toon-rust is a community-driven Rust implementation.

There’s also a reserved crate namespace on docs.rs (toon_format) that explicitly says the “official Rust implementation” is coming soon / not fully implemented yet.

Claude “official crates”: Anthropic’s “Client SDKs” page lists official SDKs for Python/TS/Java/Go/C#/Ruby/PHP — not Rust.
So in Rust we either:

use a reputable community SDK (there are several), or

call the HTTP API ourselves with reqwest.

That said: you can still use “blessed crates” on the Rust side and keep Claude integration clean.

What we’re building (Rust-first)
Project name (honey/hive-perfect)

NectarWarden
Because the warden’s job is to protect the hive’s nectar stores: keep the high-value stuff, prevent waste, and make sure the colony isn’t starving during winter (incidents).

Other very-fitting alternates:

WaggleSmith (crafts the waggle-dance = policy recommendations)

ApiaryPilot (control plane vibe)

CombSieve (sampling/filtering metaphor)

If you want one name that screams “Honeycomb + sampling + guidance”: NectarWarden.

The core problem we solve (in Honeycomb terms)

Tail sampling is only as good as the rules. Rules rot, systems change, and humans don’t have time to keep up. Refinery is powerful because it lets you decide what to keep, but the “rule authoring + validation” loop is still hard.

So we build a system that:

writes sampling intent in TOON (compact, model-friendly, diff-friendly)

compiles it into Refinery rules/config (deterministic)

proves it won’t blind you (simulation/replay tests)

optionally uses Claude as an advisor, never as the executor.

Architecture (Rust) — deterministic core, optional AI
1) TOON as the policy language

Policy files live in .toon and define:

what “must keep” means (errors, slow traces, rare customers, specific endpoints)

budgets (ingest caps)

keys (trace fields to group/sample by)

guardrails (“never drop checkout failures”)

Rust reads TOON → AST → typed IR.

Use toon-rust as the parser/formatter baseline.
(And we keep an abstraction layer in case the “official” crate matures later. )

2) Compiler: TOON policy → Refinery output

Output targets:

rules.yaml (sampling rules)

config.yaml (if needed)

plus a generated “why” report (“waggle notes”)

The compiler is pure + deterministic:

no network calls

no randomness

fully testable

3) Prover: “Would this have dropped last incident?”

Given:

a corpus of past trace summaries (or sampled exemplars)

candidate rules

The prover simulates:

keep/drop decisions

budget impact

coverage of “important” categories

It fails the build if:

a must-keep class would be dropped

budgets explode

keys are unstable / too-high-cardinality

4) Advisor (Claude) — optional, offline/control plane

Claude can be used to:

propose rule deltas

explain current rule behavior

recommend better keys

summarize “what changed this week”

But Claude only emits structured TOON patches, never direct prod changes.

“Official Claude crates” reality check

Since Anthropic doesn’t list an official Rust SDK today, we either:

implement HTTP client ourselves, or

adopt a community SDK that matches our needs.

Anthropic’s official list omits Rust.
(There are community Rust SDKs like anthropic_sdk etc, but they’re not “official” per that page. )

“Blessed crates” set (Rust)

If we keep it modern + idiomatic:

async runtime: tokio

http: reqwest (or hyper if you want max control)

serialization: serde, serde_json

config/schema validation: schemars (or jsonschema if you want runtime validation)

cli: clap

observability: tracing, tracing-subscriber

errors: thiserror, anyhow

testing: proptest (for policy compiler invariants), plus standard cargo test

This gives you: speed, correctness gates, and excellent DX.

What “TOON policy” could feel like (conceptually)

Think of a policy as a hive directive:

Queen’s law: never lose these signals

Foragers: everything else gets sampled

Scouts: detect new rare events and elevate them

Waggle dance: explain where the value is now

The artifacts are:

policy.toon (source of truth)

policy.lock (compiled + pinned output)

waggle.md (human explanation)

rules.yaml (Refinery-compatible output)

Repo layout (what you’d actually build)

crates/toon_policy/ — parse TOON → typed policy model

crates/nectar_compiler/ — policy → Refinery rules/config

crates/nectar_prover/ — replay/simulation + safety checks

crates/nectar_advisor/ — Claude integration + patch generation (optional)

cmd/nectarwarden/ — CLI (compile, prove, advise, explain)

Quick next step (so this becomes real fast)

If you tell me one thing: what is your intended integration point?

A) generate rules.yaml for Refinery

B) sit in front as a proxy

C) run as a CI tool that manages rules PRs (GitOps)

…I’ll lock the MVP to the smallest viable wedge and write the full spec for:

TOON policy schema (v0)

compiler IR

prover checks

and the Claude “advisor” contract (structured patch format)

Also: if you want “official Claude” integration without Rust SDK ambiguity, the cleanest approach is direct HTTP to Anthropic API with reqwest + strict typed request/response models—still “blessed,” still deterministic.