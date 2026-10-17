# Changelog policy

The public `CHANGELOG.md` tells users what changed in the product. It is not a lab notebook, a research log, or a record of how the work got done.

Detailed development history, including methodology, evaluation internals, tooling, vendors, and dead ends, lives in the private work repository, not here.

## What a public entry is

One line per user-visible change, grouped under Keep a Changelog headings (`Added`, `Changed`, `Deprecated`, `Removed`, `Fixed`, `Security`). Write for someone deciding whether to upgrade.

A good entry answers: *what can I now do, or what will now behave differently?*

```
### Added
- `gate` exits 2 when any required check is untested, instead of reporting a
  pass for the checks that did run.

### Changed
- Install moves to the Agent Skills CLI: `npx skills@latest add <owner>/diligent`.

### Fixed
- `report` no longer omits the coverage denominators when no findings exist.
```

A public entry about capability must be honest about coverage. If a release adds a rule or a pack, say whether it ships a check or only a cataloged obligation. Never write an entry that implies an automated check exists where only a rule record does.

## What never goes in the public changelog

Each of these belongs in the private repository instead.

- **Named third-party services and data sources.** Reference libraries, scraping and search tools, model bridges, analytics vendors. Naming a service used to build or validate the product invites questions about licensing and terms of use, and tells competitors the supply chain.
- **Third-party brands and products used as comparisons or benchmarks.** Publishing a claim about someone else's work, made without their participation, is not allowed. Keep comparative results private, or publish only the aggregate with sources unnamed.
- **Evaluation and benchmarking internals.** Reviewer lineups, scoring thresholds, seeds, blinding procedures, panel composition, sample sizes.
- **Methodology defects and their fixes.** These are essential to record privately and corrosive to publish. Read in isolation they undermine confidence in results they were meant to strengthen.
- **Which models were used to build, review, or refine the work**, unless a model is the user-facing feature.
- **Internal process detail.** Agent orchestration, subagent limits, session mechanics, who reviewed what, how long something took.
- **Anything about a specific person**, including contributors.
- **Anything about a specific agency or site audited.** An audit target is never named in the public log, in a fixture, or in an example without the owner's written permission.

## Test before publishing an entry

Ask, in order:

1. **Does it change what a user can do?** If no, it does not belong here.
2. **Does it name a company, product, service, model, agency, or person?** If yes, remove the name or move the entry to the private log.
3. **Would I be comfortable with the named party reading this line?** If no, it is not publishable regardless of accuracy.
4. **Does it reveal how we measure ourselves?** Results may be summarized; the machinery stays private.
5. **Does it overstate coverage?** If a reader could conclude a check exists that does not, rewrite it.

## Where detail belongs

- **Private work repo.** Full changelog, `DEVLOG.md`, evaluation records, methodology notes, vendor and tooling decisions, the research blueprint and plans.
- **Public repo.** This changelog, the README, the skill itself, the supported-checks matrix, the known-limitations register, and any evidence deliberately prepared for publication.

When in doubt, write the detailed entry privately first, then decide what single sentence of it a user actually needs.
