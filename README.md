# The Human Index

A local-only questionnaire that maps personality, motivation, happiness, relationships, values, habits, career fit and future direction, then builds an interactive report.

Nothing leaves your browser. Answers and results live in localStorage and can be exported or deleted from Settings.

## Run

    npm install
    npm run dev        # local dev server
    npm run build      # production build in dist/
    npm run personas   # five test personas + scoring checks
    npx tsx scripts/checkQuestions.ts   # question bank integrity

## How it works

- `src/data/questions/` about 90 base questions plus adaptive follow-ups. Each has dimension weights, reverse keys and optional `showIf` conditions.
- `src/engine/scoring.ts` weighted, reverse-scored, normalised scores per dimension, with confidence from evidence volume, answer agreement and response quality checks (straightlining, inconsistent pairs, speeding, skipped items). After the base flow it picks follow-up probes for the weakest-measured dimensions.
- `src/engine/report.ts` rules-based interpretation: cross-dimension patterns, contradictions, hobby and career matching, roadmap and narrative.
- `src/data/knowledge/` sources, citations, licences and limitations for every instrument and every population figure.

## Scientific honesty

Only the Mini-IPIP (Big Five, Donnellan et al. 2006) and the SWLS (Diener et al. 1985) are validated instruments. Everything else is a custom exploratory score and is labelled that way in the app. Confidence is capped at 97 percent. A self-report questionnaire cannot support certainty about who someone is, and this tool is not a clinical assessment.
