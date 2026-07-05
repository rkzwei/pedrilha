# Ethos

Every function, feature, and change in this project must derive from, support, or incorporate this sentence:

> "My friend recommended me a movie another friend showed him, said it was great, and I watched it. It was great, the movie is called Sorcerer, somewhat old, not a lot of people watched it but critics loved it. It felt like a hidden gem."

This is a **hard gate**, not a guideline. A feature that cannot cite a clause below does not get built.

---

## The Clauses

| # | Clause | Fragment | What it demands |
|---|--------|----------|-----------------|
| C1 | **Word of mouth** | "My friend recommended me a movie another friend showed him" | Recommendations travel person → person through trust chains, not algorithmic feeds. Sharing, public ratings, and "pass it on" mechanics derive here. |
| C2 | **The promise must hold** | "said it was great... It was great" | A recommendation that disappoints breaks the trust chain. Precision beats recall: show fewer gems rather than wrong ones. Quality gates, exclusion filters, and data accuracy derive here. |
| C3 | **One named movie** | "the movie is called Sorcerer" | Specificity. A person recommends *a* movie, not a feed. Curation, detail pages, and focused presentation derive here. Infinite scroll does not. |
| C4 | **Somewhat old** | "somewhat old" | Age raises gem-ness. Year decay derives here. |
| C5 | **Few watched it** | "not a lot of people watched it" | Obscurity is the core signal. Vote-ratio scoring and the blockbuster-shadow factor derive here. |
| C6 | **Critics loved it** | "but critics loved it" | Critical endorsement separates hidden gems from informed avoidance. The RT quality gate and credibility multiplier derive here. |
| C7 | **It felt like a hidden gem** | "It felt like" | Discovery is a *feeling*. The UX must deliver serendipity and delight, not a spreadsheet. Design decisions derive here. |
| C8 | **There's a way to do it** | "and I watched it" | The recommendation is actionable: the product exists, runs, and is reachable. Foundation and infrastructure derive here. Deployment ease derives from C1 (passing it on just happens); reliability and hardening derive from C2 (the trust chain holds — it was great, and it stays great). |

---

### C5 is the pain point, not the goal

"Not a lot of people watched it" describes the world **before** the product.
The product exists to fix C5 — to get gems watched. Obscurity is a detection
signal measured on pre-discovery external data (IMDb/TMDB vote counts), never
a state to preserve:

- A gem gaining viewers *because the product surfaced it* is success, not a scoring bug.
- No feedback loop may demote a gem because the product's own community watched, rated, or shared it — that would punish the product for working. Community signals may only work in the gem's favor (e.g. Acclaimed candidacy, per Phase 8).
- External popularity growth over the years naturally retiring a former gem from the rankings is the pipeline working: found, spread, no longer hidden. Fresh gems take its place.

---

## The Sorcerer Test (feature gate)

Before building anything, answer: **which clause does this derive from?**

- Every new entry in `PHASES.md` must carry an `Ethos:` line citing one or more clauses (e.g. `Ethos: C1, C7`).
- No clause → no feature. Rework it or drop it.
- A feature that *contradicts* a clause is rejected even if it cites another.

### What this kills (non-exhaustive)

Trending charts, popularity feeds, "most watched this week", engagement mechanics, recommendation-by-similarity at scale ("users also watched"), anything that surfaces movies *because* many people saw them. These invert C5.

---

## Sorcerer's role: calibration only

Sorcerer (1977, `tt0076740`) is the founding case and the algorithm's calibration reference:

- **If Sorcerer doesn't score highly, that is a red flag to investigate** — either the algorithm or the data is wrong.
- **No test may hardcode Sorcerer's (or any seeded gem's) rank as a pass/fail condition.** An algorithm told where Sorcerer must land proves nothing. The algorithm must *find* it.
- **In tests, seeded-gem rank and score checks are diagnostics, never assertions.** A test prints a `🚩 RED FLAG` when a seeded gem scores 0.0 or no seeded gem ranks in the top 25% of the scored population — the build does not fail. A red flag means: investigate the algorithm or the data. Synthetic-profile property tests (e.g. an obscure old critic-loved profile must beat a blockbuster-adjacent profile) may assert freely — they test algorithm properties, not seeded outcomes.
- `SEEDED_GEMS` exists only to guarantee these movies are present in the database population. It must never feed the scoring path.

### The N=1 warning

The founding story is a single anecdote. The blockbuster-shadow window (6 weeks) was chosen specifically to capture Sorcerer opening 30 days after Star Wars. Calibrating constants against the founding case is acceptable; **fitting every constant to it is overfitting to a sample of one.** When tuning, validate against the other seeded gems and the population at large, not Sorcerer alone.

---

## Existing architecture, mapped

| Component | Clause |
|-----------|--------|
| Year decay (dominant weight) | C4 |
| Vote-ratio score | C5 |
| Blockbuster-shadow factor | C5 |
| RT critic gate (< 65% excluded) | C6, C2 |
| RT credibility multiplier on vote ratio | C6, C2 |
| Rating sweet spot (6.5–7.9) | C5, C2 |
| Phase 8: public user ratings, sharing | C1 |
| Movie detail page | C3 |
| Population-relative normalization (top gem = 100%) | C7 |
| Infrastructure (foundation, Docker deploy, hardening) | C8, C1, C2 |
