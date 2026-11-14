// Scoring core: turns raw answers into normalised 0..100 dimension scores with
// per-dimension confidence, handles reverse keying, missing data, conditional
// questions and adaptive probe selection. Pure functions, no I/O.

import { DIM_IDS, type DimId } from './dimensions'
import type { Answers, AnswerValue, Condition, Contribution, DimScore, Quality, Question, Scores } from './types'

/** Reported confidence is capped. A self-report questionnaire cannot justify certainty. */
export const CONFIDENCE_CAP = 0.97

type Obs = Contribution

const isAnswered = (v: AnswerValue | undefined): boolean =>
  v !== undefined && v !== null && !(typeof v === 'string' && v.trim() === '') && !(Array.isArray(v) && v.length === 0)

/** All (dimension, level, weight) observations produced by one answer. */
export function observe(q: Question, v: AnswerValue | undefined): { dim: DimId; level: number; weight: number }[] {
  if (!isAnswered(v)) return []
  const w = q.weight ?? 1
  const out: { dim: DimId; level: number; weight: number }[] = []

  if (q.type === 'likert' || q.type === 'scale10') {
    if (typeof v !== 'number' || !q.dims) return out
    const max = q.type === 'scale10' ? 10 : q.points ?? 5
    const level = Math.min(1, Math.max(0, (v - 1) / (max - 1)))
    for (const [dim, dw] of Object.entries(q.dims) as [DimId, number][]) {
      if (!dw) continue
      out.push({ dim, level: dw < 0 ? 1 - level : level, weight: Math.abs(dw) * w })
    }
    return out
  }

  const opts = q.options ?? []
  if (q.type === 'choice' || q.type === 'forced') {
    const o = opts.find((x) => x.id === v)
    for (const [dim, level] of Object.entries(o?.effects ?? {}) as [DimId, number][]) out.push({ dim, level, weight: w })
    return out
  }
  if (q.type === 'multi' && Array.isArray(v)) {
    for (const o of opts) {
      const picked = v.includes(o.id)
      for (const [dim, level] of Object.entries(o.effects ?? {}) as [DimId, number][]) {
        // Not picking an option is weaker evidence than picking it.
        out.push(picked ? { dim, level, weight: w } : { dim, level: 1 - level, weight: w * 0.35 })
      }
    }
    return out
  }
  if (q.type === 'ranking' && Array.isArray(v) && v.length > 1) {
    v.forEach((id, i) => {
      const o = opts.find((x) => x.id === id)
      const level = 1 - i / (v.length - 1)
      for (const [dim, dw] of Object.entries(o?.effects ?? {}) as [DimId, number][]) out.push({ dim, level, weight: dw * w })
    })
  }
  return out
}

function collect(questions: Question[], answers: Answers): Record<DimId, Obs[]> {
  const by = Object.fromEntries(DIM_IDS.map((d) => [d, [] as Obs[]])) as Record<DimId, Obs[]>
  for (const q of questions) {
    for (const o of observe(q, answers[q.id])) {
      if (by[o.dim]) by[o.dim].push({ questionId: q.id, level: o.level, weight: o.weight })
    }
  }
  return by
}

const wmean = (obs: Obs[]): number => {
  const tw = obs.reduce((s, o) => s + o.weight, 0)
  return tw ? obs.reduce((s, o) => s + o.level * o.weight, 0) / tw : 0.5
}

/**
 * Confidence in one dimension score. Three ingredients:
 *  - evidence: more (weighted) observations => more confidence, with diminishing returns
 *  - agreement: observations that point the same way => more confidence
 *  - response quality: a global factor from straightlining / inconsistency checks
 */
export function dimConfidence(obs: Obs[], qualityFactor = 1): number {
  if (!obs.length) return 0
  const nEff = obs.reduce((s, o) => s + o.weight, 0)
  const evidence = nEff / (nEff + 1.2)
  const m = wmean(obs)
  const variance = obs.reduce((s, o) => s + o.weight * (o.level - m) ** 2, 0) / nEff
  const agreement = 1 - 0.4 * Math.min(1, Math.sqrt(variance) / 0.45)
  return Math.min(CONFIDENCE_CAP, evidence * agreement * qualityFactor)
}

export function assessQuality(questions: Question[], answers: Answers, timings: Record<string, number> = {}): Quality {
  const shown = visibleQuestions(questions, answers)
  const required = shown.filter((q) => q.type !== 'text' && !q.optional)
  const answered = shown.filter((q) => isAnswered(answers[q.id])).length
  const flags: Quality['flags'] = []

  // Straightlining: long runs of the identical likert answer.
  let run = 0
  let best = 0
  let last: AnswerValue | undefined
  for (const q of shown) {
    if (q.type !== 'likert') continue
    const v = answers[q.id]
    if (!isAnswered(v)) continue
    run = v === last ? run + 1 : 1
    last = v
    best = Math.max(best, run)
  }
  if (best >= 10) flags.push({ id: 'straightlining', detail: `${best} identical ratings in a row.` })

  // Consistency pairs: two items measuring the same thing should roughly agree.
  const byId = new Map(questions.map((q) => [q.id, q]))
  let pairs = 0
  let off = 0
  for (const q of shown) {
    if (!q.pairWith) continue
    const p = byId.get(q.pairWith)
    if (!p) continue
    const a = observe(q, answers[q.id])
    const b = observe(p, answers[p.id])
    if (!a.length || !b.length) continue
    const shared = a.find((x) => b.some((y) => y.dim === x.dim))
    if (!shared) continue
    const other = b.find((y) => y.dim === shared.dim)!
    pairs++
    if (Math.abs(shared.level - other.level) >= 0.6) off++
  }
  if (pairs >= 3 && off / pairs > 0.34) flags.push({ id: 'inconsistent-pairs', detail: `${off} of ${pairs} paired items disagreed strongly.` })

  const times = required.map((q) => timings[q.id]).filter((t): t is number => typeof t === 'number' && t > 0)
  if (times.length >= 20) {
    const sorted = [...times].sort((x, y) => x - y)
    if (sorted[Math.floor(sorted.length / 2)] < 1500) flags.push({ id: 'speeding', detail: 'Median response time under 1.5 seconds.' })
  }
  const missing = required.filter((q) => !isAnswered(answers[q.id])).length
  if (required.length && missing / required.length > 0.15) flags.push({ id: 'many-skipped', detail: `${missing} questions left unanswered.` })

  return { answered, shown: shown.length, overallConfidence: 0, flags }
}

export const qualityFactor = (q: Quality): number =>
  q.flags.reduce((f, flag) => f * (flag.id === 'many-skipped' ? 0.95 : 0.85), 1)

export function scoreAll(
  questions: Question[],
  answers: Answers,
  timings: Record<string, number> = {},
): { scores: Scores; quality: Quality } {
  const asked = questions.filter((q) => isAnswered(answers[q.id]))
  const by = collect(asked, answers)
  const quality = assessQuality(questions, answers, timings)
  const qf = qualityFactor(quality)
  const scores = {} as Scores
  let cSum = 0
  let cN = 0
  for (const dim of DIM_IDS) {
    const obs = by[dim]
    const s: DimScore = {
      score: Math.round(wmean(obs) * 1000) / 10, // missing data => neutral 50 with zero confidence
      confidence: dimConfidence(obs, qf),
      n: obs.length,
      contributions: obs,
    }
    scores[dim] = s
    if (obs.length) {
      cSum += s.confidence
      cN++
    }
  }
  quality.overallConfidence = cN ? cSum / cN : 0
  return { scores, quality }
}

// ---------- adaptive logic ----------

export function evalCondition(c: Condition, answers: Answers, provisional: (dim: DimId) => number): boolean {
  if ('all' in c) return c.all.every((x) => evalCondition(x, answers, provisional))
  if ('any' in c) return c.any.some((x) => evalCondition(x, answers, provisional))
  if ('dim' in c) {
    const s = provisional(c.dim)
    return c.op === 'gte' ? s >= c.value : s <= c.value
  }
  const v = answers[c.q]
  if (!isAnswered(v)) return false
  switch (c.op) {
    case 'eq':
      return v === c.value
    case 'neq':
      return v !== c.value
    case 'gte':
      return typeof v === 'number' && v >= (c.value as number)
    case 'lte':
      return typeof v === 'number' && v <= (c.value as number)
    case 'includes':
      return Array.isArray(v) ? v.includes(String(c.value)) : v === c.value
  }
}

/** Base-flow questions (no probes) whose conditions currently hold, in authoring order. */
export function visibleQuestions(questions: Question[], answers: Answers, probeIds: string[] = []): Question[] {
  const base = questions.filter((q) => !q.probeFor)
  const by = collect(base, answers)
  const provisional = (dim: DimId) => wmean(by[dim]) * 100
  const out = base.filter((q) => !q.showIf || evalCondition(q.showIf, answers, provisional))
  const probes = probeIds.map((id) => questions.find((q) => q.id === id)).filter((q): q is Question => Boolean(q))
  return [...out, ...probes]
}

/**
 * Choose follow-up probes once the base flow is done. A probe is selected when one of its
 * target dimensions is still weakly measured, or sits near the midpoint with conflicting
 * evidence. Lowest-confidence targets are served first.
 */
export function selectProbes(questions: Question[], answers: Answers, max = 12, threshold = 0.8): string[] {
  const { scores } = scoreAll(questions, answers)
  const pool = questions.filter((q) => q.probeFor?.length && !isAnswered(answers[q.id]))
  const need = (q: Question): number =>
    Math.max(
      ...q.probeFor!.map((dim) => {
        const s = scores[dim]
        const ambiguous = Math.abs(s.score - 50) < 12 ? 0.1 : 0
        return threshold - s.confidence + ambiguous
      }),
    )
  const chosen: string[] = []
  const covered = new Map<DimId, number>()
  for (const q of pool.map((q) => ({ q, n: need(q) })).filter((x) => x.n > 0).sort((a, b) => b.n - a.n)) {
    if (chosen.length >= max) break
    // At most two probes per dimension so the follow-up round stays short.
    if (q.q.probeFor!.every((dim) => (covered.get(dim) ?? 0) >= 2)) continue
    chosen.push(q.q.id)
    q.q.probeFor!.forEach((dim) => covered.set(dim, (covered.get(dim) ?? 0) + 1))
  }
  return chosen
}

/** Raw sum of numeric answers, for instruments that are interpreted on their original scale (e.g. SWLS). */
export function rawSum(questions: Question[], answers: Answers, instrument: Question['instrument']): { sum: number; n: number; of: number } {
  const items = questions.filter((q) => q.instrument === instrument)
  const vals = items.map((q) => answers[q.id]).filter((v): v is number => typeof v === 'number')
  return { sum: vals.reduce((s, v) => s + v, 0), n: vals.length, of: items.length }
}

export { isAnswered }
