// Interpretation engine. Turns dimension scores into a full Report using
// transparent, hand-written rules. No AI services, no network. Every claim is a
// hypothesis tied to the dimensions it came from, and rules that depend on weakly
// measured dimensions (confidence < 0.35 or no answers) are skipped.

import { DIMENSIONS, dimLabel, dimsInGroup, type DimId } from './dimensions'
import { rawSum, scoreAll } from './scoring'
import type {
  AlignmentComponent,
  Answers,
  CareerMatch,
  Contradiction,
  DomainGap,
  HobbyMatch,
  Insight,
  PopulationComparison,
  Question,
  Report,
  Scores,
} from './types'
import { HOBBIES, MOTIVE_DIMS, type MotiveDim } from '../data/hobbies'
import { CAREERS, type WorkProfile } from '../data/careers'
import normsRaw from '../data/knowledge/norms.json'

// ---------------------------------------------------------------- helpers

type Dir = 1 | -1
type Cond = [DimId, Dir]
const H: Dir = 1
const L: Dir = -1
const MIN_CONF = 0.35

interface Ctx {
  s: Scores
  sc: (d: DimId) => number
  ok: (...d: DimId[]) => boolean
  /** -1..1 movement from the midpoint in the requested direction. */
  lvl: (d: DimId, dir: Dir) => number
  avg: (ds: DimId[], inv?: DimId[]) => number | null
  band: (d: DimId) => string
}

const clamp = (v: number, lo = 0, hi = 100) => Math.min(hi, Math.max(lo, v))
const round = (v: number) => Math.round(v)
const cap = (t: string) => t.charAt(0).toUpperCase() + t.slice(1)
const lc = (t: string) => t.charAt(0).toLowerCase() + t.slice(1)
const para = (...parts: (string | false | null | undefined)[]) => parts.filter(Boolean).join(' ')

function makeCtx(s: Scores): Ctx {
  const ok = (...d: DimId[]) => d.every((x) => s[x].n > 0 && s[x].confidence >= MIN_CONF)
  const lvl = (d: DimId, dir: Dir) => (dir * (s[d].score - 50)) / 50
  const avg = (ds: DimId[], inv: DimId[] = []) => {
    const vals = ds.filter((d) => s[d].n > 0).map((d) => s[d].score).concat(inv.filter((d) => s[d].n > 0).map((d) => 100 - s[d].score))
    return vals.length ? vals.reduce((a, b) => a + b, 0) / vals.length : null
  }
  const band = (d: DimId) => {
    const v = s[d].score
    return v < 20 ? 'very low' : v < 40 ? 'low' : v < 60 ? 'moderate' : v < 80 ? 'high' : 'very high'
  }
  return { s, sc: (d) => s[d].score, ok, lvl, avg, band }
}

interface Rule {
  c: Cond[]
  t: string
  b: string
}
const R = (c: Cond[], t: string, b: string): Rule => ({ c, t, b })

/** Fire rules whose every condition is >= threshold (default: score 60+ or 40-) and well measured. Strongest first. */
function fire(x: Ctx, rules: Rule[], max: number, th = 0.2): Insight[] {
  const hits: { w: number; i: Insight }[] = []
  for (const r of rules) {
    const ds = r.c.map(([d]) => d)
    if (!x.ok(...ds)) continue
    const ws = r.c.map(([d, dir]) => x.lvl(d, dir))
    if (ws.some((w) => w < th)) continue
    hits.push({ w: ws.reduce((a, b) => a + b, 0) / ws.length, i: { title: r.t, body: r.b, evidence: ds } })
  }
  return hits.sort((a, b) => b.w - a.w).slice(0, max).map((h) => h.i)
}

const top = (x: Ctx, ds: DimId[], n: number, minConf = MIN_CONF) =>
  ds
    .filter((d) => x.s[d].n > 0 && x.s[d].confidence >= minConf)
    .sort((a, b) => x.sc(b) - x.sc(a))
    .slice(0, n)

const prof = (x: Ctx, ds: DimId[]) => ds.filter((d) => x.s[d].n > 0).map((d) => ({ dim: d, label: dimLabel(d), score: round(x.sc(d)) }))

// ---------------------------------------------------------------- traits

const TRAIT_TEXT: Partial<Record<DimId, { hi: string; lo: string }>> = {
  extraversion: {
    hi: 'Your answers lean toward gaining energy from people and activity. This often shows up as thinking out loud and building momentum in groups, and it is worth checking whether your current week gives you enough contact.',
    lo: 'Your answers lean toward gaining energy from quieter settings and fewer, deeper interactions. This often means needing recovery after crowded days, which is a preference rather than a flaw.',
  },
  agreeableness: {
    hi: 'You describe yourself as considerate and cooperative. This pattern often makes people feel safe around you, and it can also mean your own needs wait their turn.',
    lo: 'You describe yourself as direct and independent-minded. This often means you challenge ideas readily and are less swayed by social pressure, though some people may read that as hard.',
  },
  conscientiousness: {
    hi: 'You report being organised and reliable. This often means people trust you with commitments, and it can be tiring if the standard is applied to every area.',
    lo: 'You report a flexible, spontaneous approach. This often brings adaptability and ease, and may make routine upkeep feel heavier than it does for others.',
  },
  emotionalStability: {
    hi: 'You report staying fairly even when things get hard. This pattern often helps in pressured roles, and it may hide how much you are carrying if you do not show it.',
    lo: 'You report feeling pressure and mood shifts strongly. This often goes with real attunement to people and situations, and it makes rest and predictable support more valuable.',
  },
  openness: {
    hi: 'You are drawn to ideas, novelty and imagination. This often fuels learning and creativity, and can make settling on one path feel like losing the others.',
    lo: 'You prefer the practical and familiar. This often brings reliability and focus, and may mean unusual options are filtered out early. Worth checking whether that filter is deliberate.',
  },
  riskTolerance: {
    hi: 'You are comfortable with uncertainty and the chance of a loss. This often supports bold moves, and works best when paired with a safety net.',
    lo: 'You lean toward protecting what you have. This often prevents expensive mistakes, and may mean some opportunities go unexamined because the downside looks larger than the upside.',
  },
  impulsivity: {
    hi: 'You report acting on impulse fairly often. This can bring spontaneity and speed, and may cost you when decisions are expensive to reverse.',
    lo: 'You report pausing before acting. This often supports good judgement, and could occasionally slow decisions that would benefit from speed.',
  },
  adaptability: {
    hi: 'You report adjusting easily when plans change. This often makes new situations and transitions less stressful for you.',
    lo: 'You report being unsettled when plans change. This often points to a need for notice and a clear picture before shifts.',
  },
  structurePreference: {
    hi: 'You prefer clear plans, schedules and expectations. This often brings calm and efficiency, and may feel constraining in open-ended roles.',
    lo: 'You prefer open, flexible time and loose structure. This often brings creativity and ease, and may feel uncomfortable in rule-heavy environments.',
  },
  assertiveness: {
    hi: 'You say what you want and push for it. This often brings clarity to groups, and works best when paired with listening.',
    lo: 'You tend to hold back your own position. This pattern often keeps the peace, and worth checking whether it leaves needs unspoken.',
  },
  curiosity: {
    hi: 'You are pulled toward finding out how things work. This often drives self-directed learning and unusual connections.',
    lo: 'You prefer to stick to what you know. This often brings depth in a few areas and fewer detours.',
  },
  creativity: {
    hi: 'You report generating original ideas and enjoying making things. This often needs time and permission to show up, so the environment matters.',
    lo: 'You prefer proven methods to inventing your own. This often means you are practical and efficient, and may be underusing a creative side that has never been invited out.',
  },
}

function coreTraits(x: Ctx): Insight[] {
  const ids = Object.keys(TRAIT_TEXT) as DimId[]
  return ids
    .filter((d) => x.ok(d) && Math.abs(x.sc(d) - 50) >= 12)
    .sort((a, b) => Math.abs(x.sc(b) - 50) * x.s[b].confidence - Math.abs(x.sc(a) - 50) * x.s[a].confidence)
    .slice(0, 5)
    .map((d) => {
      const hi = x.sc(d) >= 50
      const m = TRAIT_TEXT[d]!
      const bandWord = x.sc(d) >= 80 || x.sc(d) <= 20 ? 'Very ' + (hi ? 'high' : 'low') : cap(hi ? 'high' : 'low')
      return { title: `${bandWord} ${lc(dimLabel(d))}`, body: hi ? m.hi : m.lo, evidence: [d] }
    })
}

// ---------------------------------------------------------------- headline

const HEADLINES: { t: string; c: Cond[] }[] = [
  { t: 'A driven builder who measures life in progress', c: [['achievement', H], ['conscientiousness', H], ['motProgress', H], ['performanceOrientation', H]] },
  { t: 'A self-directed creator who needs room to make things', c: [['creativity', H], ['artistic', H], ['freedomNeed', H], ['structurePreference', L]] },
  { t: 'A relationship-first person whose energy runs through other people', c: [['companionshipNeed', H], ['empathy', H], ['agreeableness', H], ['social', H]] },
  { t: 'A steady planner who builds on firm ground', c: [['securityNeed', H], ['structurePreference', H], ['riskTolerance', L], ['conventional', H]] },
  { t: 'A curious explorer still deciding where to commit', c: [['curiosity', H], ['openness', H], ['goalClarity', L], ['careerClarity', L]] },
  { t: 'A purposeful helper looking for work that matters', c: [['helpingDrive', H], ['contribution', H], ['valCompassion', H], ['empathy', H]] },
  { t: 'An independent risk-taker with an owner mindset', c: [['enterprising', H], ['riskTolerance', H], ['workAutonomyNeed', H], ['financialDrive', H]] },
  { t: 'A thoughtful analyst who comes alive on hard problems', c: [['investigative', H], ['intellectualDrive', H], ['motProblemSolving', H], ['curiosity', H]] },
  { t: 'A restless adventurer who needs open horizons', c: [['adventure', H], ['freedomNeed', H], ['riskTolerance', H], ['adaptability', H]] },
  { t: 'A grounded person who guards time, calm and balance', c: [['balancePriority', H], ['stress', L], ['emotionalStability', H], ['relationalIndependence', H]] },
  { t: 'A capable perfectionist with a demanding inner critic', c: [['perfectionism', H], ['fearOfFailure', H], ['selfEsteem', L], ['conscientiousness', H]] },
]

function headline(x: Ctx): string {
  let best: { t: string; w: number } | null = null
  for (const h of HEADLINES) {
    const known = h.c.filter(([d]) => x.ok(d))
    if (known.length < 3) continue
    const w = known.reduce((s, [d, dir]) => s + x.lvl(d, dir), 0) / known.length
    if (!best || w > best.w) best = { t: h.t, w }
  }
  if (best && best.w >= 0.2) return best.t
  const traits = top(x, ['extraversion', 'agreeableness', 'conscientiousness', 'emotionalStability', 'openness'], 5).sort(
    (a, b) => Math.abs(x.sc(b) - 50) - Math.abs(x.sc(a) - 50),
  )
  if (traits.length >= 2) {
    const d = (k: DimId) => `${x.sc(k) >= 50 ? 'higher' : 'lower'} ${lc(dimLabel(k))}`
    return `A profile defined by ${d(traits[0])} and ${d(traits[1])}, with room still to discover`
  }
  return 'A profile still taking shape: more answers will sharpen it'
}

// ---------------------------------------------------------------- type sketch

function typeSketch(x: Ctx): Report['typeSketch'] {
  const spec: [DimId, string, string, string][] = [
    ['extraversion', 'E', 'I', 'Energy: outward vs inward'],
    ['openness', 'N', 'S', 'Information: abstract vs concrete'],
    ['agreeableness', 'F', 'T', 'Decisions: people-first vs logic-first'],
    ['conscientiousness', 'J', 'P', 'Lifestyle: planned vs flexible'],
  ]
  const axes = spec.map(([d, hi, lo, axis]) => ({ axis, letter: x.sc(d) >= 50 ? hi : lo, strength: x.s[d].n ? Math.abs(x.sc(d) - 50) / 50 : 0 }))
  const code = axes.map((a) => a.letter).join('')
  const L4: Record<string, string> = {
    E: 'gain energy from contact and activity',
    I: 'recharge through quiet and depth',
    N: 'lean toward ideas, patterns and possibilities',
    S: 'lean toward the concrete, practical and proven',
    F: 'weigh how decisions land on people',
    T: 'weigh decisions by logic and consistency first',
    J: 'like plans, closure and clear commitments',
    P: 'like keeping options open and adapting as you go',
  }
  const weak = axes.filter((a) => a.strength < 0.15).map((a) => a.axis.split(':')[0].toLowerCase())
  const none = axes.every((a) => a.strength === 0)
  return {
    code,
    axes,
    description: none
      ? 'There were not enough Big Five answers to sketch a pattern.'
      : para(
          `This sketch suggests you ${L4[axes[0].letter]}, ${L4[axes[1].letter]}, ${L4[axes[2].letter]}, and ${L4[axes[3].letter]}.`,
          weak.length ? `Your ${weak.join(' and ')} ${weak.length > 1 ? 'axes sit' : 'axis sits'} close to the middle, so that letter could easily flip on another day.` : '',
        ),
    caveat:
      'This is not a validated MBTI measure. The four letters are a loose translation of your Big Five scores (extraversion, openness, agreeableness, conscientiousness), which are the better-researched traits. Types are not fixed boxes, and most people sit near the middle of at least one axis.',
  }
}

// ---------------------------------------------------------------- hobbies

const RIASEC: { dim: DimId; letter: string }[] = [
  { dim: 'realistic', letter: 'R' },
  { dim: 'investigative', letter: 'I' },
  { dim: 'artistic', letter: 'A' },
  { dim: 'social', letter: 'S' },
  { dim: 'enterprising', letter: 'E' },
  { dim: 'conventional', letter: 'C' },
]
const RIASEC_DIM: Record<string, DimId> = Object.fromEntries(RIASEC.map((r) => [r.letter, r.dim]))
const TRAIT_DIM: Record<string, DimId> = {
  openness: 'openness',
  extraversion: 'extraversion',
  realistic: 'realistic',
  investigative: 'investigative',
  artistic: 'artistic',
  social: 'social',
  enterprising: 'enterprising',
  conventional: 'conventional',
}

function matchHobbies(x: Ctx): HobbyMatch[] {
  const known = MOTIVE_DIMS.filter((d) => x.s[d].n > 0)
  const mean = known.length ? known.reduce((s, d) => s + x.sc(d), 0) / known.length : 50
  const u: Record<string, number> = {}
  for (const d of MOTIVE_DIMS) u[d] = x.s[d].n > 0 ? (x.sc(d) - mean) / 50 : 0
  const un = Math.sqrt(MOTIVE_DIMS.reduce((s, d) => s + u[d] ** 2, 0))

  if (!known.length) return []
  const out = HOBBIES.map((hb) => {
    const w = MOTIVE_DIMS.map((d) => hb.motives[d as MotiveDim] ?? 0)
    const wm = w.reduce((a, b) => a + b, 0) / w.length
    const wc = w.map((v) => v - wm)
    const wn = Math.sqrt(wc.reduce((s, v) => s + v * v, 0))
    let r = 0
    const contrib: { d: DimId; c: number }[] = []
    MOTIVE_DIMS.forEach((d, i) => {
      contrib.push({ d, c: u[d] * wc[i] })
    })
    if (un > 0 && wn > 0) r = contrib.reduce((s, c) => s + c.c, 0) / (un * wn)
    let tNum = 0
    let tDen = 0
    const traitHits: { d: DimId; v: number }[] = []
    for (const [k, wt] of Object.entries(hb.traits ?? {})) {
      const dim = TRAIT_DIM[k]
      if (!dim || !x.ok(dim)) continue
      const v = (wt * (x.sc(dim) - 50)) / 50
      tNum += v
      tDen += Math.abs(wt)
      if (v > 0.15) traitHits.push({ d: dim, v })
    }
    const t = tDen ? tNum / tDen : 0
    const combined = r * 0.82 + t * 0.18
    const matchScore = round(clamp(50 + 50 * combined * 1.2, 5, 98))

    const lead = contrib
      .filter((c) => c.c > 0 && x.sc(c.d) >= 55)
      .sort((a, b) => b.c - a.c)
      .slice(0, 3)
    const names = lead.map((c) => `${lc(dimLabel(c.d))} (${round(x.sc(c.d))})`)
    const traitName = traitHits.sort((a, b) => b.v - a.v)[0]
    let why: string
    if (!known.length) why = 'Not enough enjoyment answers to explain this match.'
    else if (!lead.length) why = 'A moderate fit. No single enjoyment motive stands out, so treat it as a low-risk one to try.'
    else {
      const list = names.length > 1 ? `${names.slice(0, -1).join(', ')} and ${names[names.length - 1]}` : names[0]
      why = `It speaks to your enjoyment of ${list}, which this hobby feeds directly.${
        traitName ? ` It also fits your ${lc(dimLabel(traitName.d))} lean.` : ''
      }`
    }
    return {
      id: hb.id,
      name: hb.name,
      matchScore,
      why,
      mightEnjoy: hb.mightEnjoy,
      mightDislike: hb.mightDislike,
      difficulty: hb.difficulty,
      cost: hb.cost,
      firstStep: hb.firstStep,
      motives: lead.map((c) => c.d),
    } as HobbyMatch
  })
  return out.sort((a, b) => b.matchScore - a.matchScore).slice(0, 15)
}

// ---------------------------------------------------------------- careers

const PREF: { key: keyof WorkProfile; dim: DimId; label: string; kind: 'need' | 'match' | 'load' }[] = [
  { key: 'autonomy', dim: 'workAutonomyNeed', label: 'autonomy', kind: 'need' },
  { key: 'security', dim: 'securityNeed', label: 'job security', kind: 'need' },
  { key: 'prestige', dim: 'prestigeNeed', label: 'prestige', kind: 'need' },
  { key: 'helping', dim: 'helpingDrive', label: 'helping others', kind: 'need' },
  { key: 'leadership', dim: 'leadershipDrive', label: 'leading others', kind: 'match' },
  { key: 'workload', dim: 'workloadTolerance', label: 'workload', kind: 'load' },
  { key: 'balance', dim: 'balancePriority', label: 'work-life balance', kind: 'need' },
  { key: 'income', dim: 'incomeExpectation', label: 'income', kind: 'need' },
]

const prefPenalty = (kind: 'need' | 'match' | 'load', user: number, job: number) => {
  const diff = job - user
  if (kind === 'match') return Math.abs(diff)
  if (kind === 'need') return diff < 0 ? -diff : diff * 0.3
  return diff > 0 ? diff : -diff * 0.3
}

function riasecCode(x: Ctx): string {
  return [...RIASEC].sort((a, b) => x.sc(b.dim) - x.sc(a.dim)).slice(0, 3).map((r) => r.letter).join('')
}

function matchCareers(x: Ctx): CareerMatch[] {
  const rs = RIASEC.filter((r) => x.s[r.dim].n > 0)
  if (!rs.length && !PREF.some((p) => x.ok(p.dim))) return []
  const meanR = rs.length ? rs.reduce((s, r) => s + x.sc(r.dim), 0) / rs.length : 50
  const res = CAREERS.map((c) => {
    const wts = [3, 2, 1]
    let num = 0
    let den = 0
    const letters = c.riasec.split('')
    letters.forEach((l, i) => {
      const dim = RIASEC_DIM[l]
      if (x.s[dim].n > 0) {
        num += wts[i] * x.sc(dim)
        den += wts[i]
      }
    })
    const fit = den ? num / den : 50
    const riasecScore = den ? clamp(0.5 * fit + 0.5 * (50 + (fit - meanR) * 1.5)) : 50

    let pn = 0
    let pd = 0
    const good: { l: string; v: number }[] = []
    const bad: { l: string; v: number }[] = []
    for (const p of PREF) {
      if (!x.ok(p.dim)) continue
      const u = x.sc(p.dim)
      const job = c.profile[p.key]
      const pen = prefPenalty(p.kind, u, job)
      const wt = 0.5 + Math.abs(u - 50) / 100
      pn += pen * wt
      pd += wt
      if (Math.abs(u - 50) >= 12) {
        if (pen <= 12) good.push({ l: p.label, v: Math.abs(u - 50) })
        else if (pen >= 28) bad.push({ l: p.label, v: pen })
      }
    }
    const prefScore = pd ? clamp(100 - pn / pd) : riasecScore
    const matchScore = round(clamp(0.5 * riasecScore + 0.5 * prefScore))

    const shared = letters.filter((l) => x.sc(RIASEC_DIM[l]) >= 58 && x.s[RIASEC_DIM[l]].n > 0).map((l) => lc(dimLabel(RIASEC_DIM[l])))
    const goodL = good.sort((a, b) => b.v - a.v).slice(0, 2).map((g) => g.l)
    const why = para(
      shared.length ? `Your interests lean ${shared.join(' and ')}, which are central to this path (${c.riasec}).` : `Its interest code (${c.riasec}) is a moderate overlap with yours rather than a strong one.`,
      goodL.length ? `Its usual conditions also line up with what you said about ${goodL.join(' and ')}.` : '',
    )
    const badL = bad.sort((a, b) => b.v - a.v)[0]
    const tradeoffs = para(
      c.tradeoff,
      badL ? `Your answers suggest you may want more ${badL.l} than this field typically offers, so check that in real roles.` : '',
    )
    return { id: c.id, title: c.title, matchScore, riasec: c.riasec, why, tradeoffs } as CareerMatch
  })
  return res.sort((a, b) => b.matchScore - a.matchScore).slice(0, 6)
}

// ---------------------------------------------------------------- contradictions

interface Side {
  dim: DimId
  dir: Dir
  label: string
  alt?: Cond
}
interface CRule {
  id: string
  title: string
  a: Side
  b: Side
  explanation: string
  question: string
}

const CONTRADICTIONS: CRule[] = [
  {
    id: 'money-risk',
    title: 'Wants financial freedom, avoids financial risk',
    a: { dim: 'financialDrive', dir: H, label: 'High financial drive' },
    b: { dim: 'riskTolerance', dir: L, label: 'Low risk tolerance' },
    explanation: 'Larger financial outcomes usually come with more variability, so this pairing often creates a bind. It may point to steady-compounding routes (saving, skills, equity over time) fitting better than all-or-nothing bets.',
    question: 'What level of loss could you absorb without losing sleep, and what is the biggest financial upside that fits inside it?',
  },
  {
    id: 'company-confidence',
    title: 'Wants close company, feels uneasy reaching for it',
    a: { dim: 'companionshipNeed', dir: H, label: 'High need for companionship' },
    b: { dim: 'socialConfidence', dir: L, label: 'Low social confidence', alt: ['extraversion', L] },
    explanation: 'A strong wish for connection beside low social ease can leave people lonely despite wanting the opposite. One hypothesis is that small, repeated, low-stakes contact helps more than big social leaps.',
    question: 'Which one or two settings make connecting feel easiest, and how could you show up there regularly?',
  },
  {
    id: 'ambition-balance',
    title: 'Ambitious, and also wants a balanced life',
    a: { dim: 'achievement', dir: H, label: 'High achievement drive' },
    b: { dim: 'balancePriority', dir: H, label: 'High priority on balance' },
    explanation: 'Both can be true, but they compete for the same hours. This pattern often shows up as swinging between pushing hard and feeling guilty about it. Worth checking which one you are shortchanging in a typical week.',
    question: 'If you could only fully protect one for the next year, which would it be, and what would that decision cost?',
  },
  {
    id: 'independent-validation',
    title: 'Values independence, wants outside approval',
    a: { dim: 'valIndependence', dir: H, label: 'Values independence', alt: ['autonomy', H] },
    b: { dim: 'validationNeed', dir: H, label: 'High need for validation' },
    explanation: 'Wanting to be self-directed while also needing others to confirm the choice is very common and usually a hint that approval is quietly steering decisions. It may be worth noticing whose approval matters most.',
    question: 'If nobody ever saw or praised this decision, would you still make it?',
  },
  {
    id: 'growth-procrastination',
    title: 'Strong drive to grow, but putting things off',
    a: { dim: 'growthDrive', dir: H, label: 'High personal growth drive' },
    b: { dim: 'procrastination', dir: H, label: 'High procrastination' },
    explanation: 'When growth matters this much and action stalls, the cause is often not laziness. It can be fear of doing it imperfectly, unclear next steps or too many competing goals. Your habit answers may help narrow which.',
    question: 'What is the smallest version of the thing you keep delaying, and what makes even that feel heavy?',
  },
  {
    id: 'health-gap',
    title: 'Health matters a lot, current health habits do not match',
    a: { dim: 'idealHealth', dir: H, label: 'Health is highly important' },
    b: { dim: 'curHealth', dir: L, label: 'Health currently rated low' },
    explanation: 'A big gap between importance and current state is a useful signal rather than a judgement. It often responds better to one specific, easy change than to a full overhaul.',
    question: 'What single change would make the biggest difference to how you feel, and what is stopping it?',
  },
  {
    id: 'free-secure',
    title: 'Craves freedom and security at once',
    a: { dim: 'freedomNeed', dir: H, label: 'High need for freedom' },
    b: { dim: 'securityNeed', dir: H, label: 'High need for job security' },
    explanation: 'These often pull in opposite directions in conventional careers. Hybrid designs, such as a stable base with a protected side project or a savings runway, sometimes bridge them.',
    question: 'How much security is truly required, and how much freedom would you trade for it?',
  },
  {
    id: 'recognition-judgment',
    title: 'Wants recognition, dreads being judged',
    a: { dim: 'recognitionNeed', dir: H, label: 'High recognition need' },
    b: { dim: 'fearOfJudgment', dir: H, label: 'High fear of judgment' },
    explanation: 'Recognition requires being seen, and being seen invites evaluation. This often leaves people holding back work they are proud of. Worth checking whether sharing in smaller, safer circles first would help.',
    question: 'Who is one safe person who could see your work first this month?',
  },
  {
    id: 'closeness-avoid',
    title: 'Wants closeness, keeps some distance',
    a: { dim: 'companionshipNeed', dir: H, label: 'High need for companionship' },
    b: { dim: 'attachmentAvoidance', dir: H, label: 'Tendency to keep distance' },
    explanation: 'Wanting connection while protecting yourself from being too close can look like pushing and pulling. This often comes from past experience and does not mean anything is wrong with you. It may be easier to explore with someone supportive.',
    question: 'What does closeness cost you, and what would you want a safe version of it to look like?',
  },
  {
    id: 'ambition-clarity',
    title: 'High ambition without a clear target',
    a: { dim: 'achievement', dir: H, label: 'High achievement drive' },
    b: { dim: 'goalClarity', dir: L, label: 'Low goal clarity' },
    explanation: 'Drive without a destination often burns energy across many directions. It may be worth defining what achieving would actually look like in a year.',
    question: 'If you reached the top of your current path, would you want to be there?',
  },
  {
    id: 'adventure-stability',
    title: 'Wants adventure, needs stability',
    a: { dim: 'adventure', dir: H, label: 'High appetite for adventure' },
    b: { dim: 'stabilityNeed', dir: H, label: 'High need for stability' },
    explanation: 'Many people who feel this tension find that bounded adventure (trips, projects, seasons) works better than all or nothing. It may also say that one of these is an expectation rather than a want.',
    question: 'What would a stable base with a recurring dose of novelty look like for you?',
  },
  {
    id: 'standards-follow',
    title: 'Very high standards, little getting finished',
    a: { dim: 'perfectionism', dir: H, label: 'High perfectionism' },
    b: { dim: 'followThrough', dir: L, label: 'Low follow-through' },
    explanation: 'High standards can make finishing feel unsafe, because finishing means judgement. This often responds to explicit definitions of "good enough" and short deadlines.',
    question: 'What would a 70 percent version of your current project look like, and who could see it?',
  },
  {
    id: 'give-boundaries',
    title: 'Gives a lot to others, struggles to set limits',
    a: { dim: 'contribution', dir: H, label: 'High drive to contribute', alt: ['helpingDrive', H] },
    b: { dim: 'boundaries', dir: L, label: 'Weaker boundaries' },
    explanation: 'Generosity without limits is a common route to exhaustion and quiet resentment. Worth checking where a firm no would give you more to offer elsewhere.',
    question: 'Where do you say yes when you mean no, and what would happen if you did not?',
  },
  {
    id: 'career-gap',
    title: 'Career matters a lot, current career feels off',
    a: { dim: 'idealCareer', dir: H, label: 'Career is highly important' },
    b: { dim: 'curCareer', dir: L, label: 'Career currently rated low' },
    explanation: 'When the stakes are high and satisfaction is low, even small mismatches can feel heavy. It may help to separate whether the problem is the role, the environment or the direction.',
    question: 'If one thing about your work changed, which change would matter most?',
  },
  {
    id: 'lead-voice',
    title: 'Wants to lead, finds it hard to speak up for yourself',
    a: { dim: 'leadershipDrive', dir: H, label: 'High leadership drive' },
    b: { dim: 'selfAdvocacy', dir: L, label: 'Low self-advocacy', alt: ['assertiveness', L] },
    explanation: 'Leadership usually requires asking for scope and visibility. This pattern often appears in capable people waiting to be noticed. Worth practising small, specific asks.',
    question: 'What is one clear ask you could make this month?',
  },
  {
    id: 'external-drive',
    title: 'Strong achievement drive that may come from outside',
    a: { dim: 'achievement', dir: H, label: 'High achievement drive' },
    b: { dim: 'intrinsicMotivation', dir: L, label: 'Low intrinsic motivation', alt: ['extrinsicMotivation', H] },
    explanation: 'Achievement fuelled mostly by rewards, status or other people\'s expectations can feel hollow even when it succeeds. It may be worth asking which goals you would still want if nobody knew about them.',
    question: 'Which of your goals would you keep if nobody could see or reward them?',
  },
  {
    id: 'autonomy-structure',
    title: 'Wants autonomy at work, also wants clear structure',
    a: { dim: 'workAutonomyNeed', dir: H, label: 'High need for autonomy at work' },
    b: { dim: 'structurePreference', dir: H, label: 'Strong preference for structure' },
    explanation: 'Freedom without guidance can feel anxious, and guidance without freedom can feel stifling. A role with clear outcomes but open methods is one design that sometimes fits both.',
    question: 'Which parts of your work do you want free, and which would you rather have defined?',
  },
  {
    id: 'creative-secure',
    title: 'Creative pull, and a strong need for security',
    a: { dim: 'creativity', dir: H, label: 'High creativity', alt: ['artistic', H] },
    b: { dim: 'securityNeed', dir: H, label: 'High need for security' },
    explanation: 'Creative work is often unpredictable in income and approval. People in this spot often do well with a protected creative practice alongside steady work, at least at first.',
    question: 'How much creative time per week would feel meaningful without risking what you need to feel safe?',
  },
  {
    id: 'independence-company',
    title: 'Values independence in relationships, also wants company',
    a: { dim: 'relationalIndependence', dir: H, label: 'High independence in relationships' },
    b: { dim: 'companionshipNeed', dir: H, label: 'High need for companionship' },
    explanation: 'Wanting both space and company is a real design question, not a flaw. Many people need explicit agreements on rhythm, like how often together and how often apart.',
    question: 'What does a good week of togetherness and space look like in numbers?',
  },
]

function contradictions(x: Ctx): Contradiction[] {
  const out: Contradiction[] = []
  for (const r of CONTRADICTIONS) {
    const sides = [r.a, r.b]
    const ds = sides.flatMap((s) => [s.dim, ...(s.alt ? [s.alt[0]] : [])])
    // the primary dim of each side must be well measured; an alt dim is used only if it is too
    if (!x.ok(r.a.dim, r.b.dim)) continue
    const d = sides.map((s) => {
      const main = x.lvl(s.dim, s.dir)
      if (s.alt && x.ok(s.alt[0])) return (main + x.lvl(s.alt[0], s.alt[1])) / 2
      return main
    })
    if (d[0] < 0.2 || d[1] < 0.2) continue
    const conf = Math.sqrt(x.s[r.a.dim].confidence * x.s[r.b.dim].confidence)
    const strength = clamp(Math.sqrt(d[0] * d[1]) * (0.5 + 0.5 * conf) * 1.15, 0, 1)
    void ds
    out.push({
      id: r.id,
      title: r.title,
      a: { dim: r.a.dim, label: r.a.label },
      b: { dim: r.b.dim, label: r.b.label },
      explanation: r.explanation,
      question: r.question,
      strength: Math.round(strength * 100) / 100,
    })
  }
  return out.sort((a, b) => b.strength - a.strength).slice(0, 10)
}

// ---------------------------------------------------------------- insight rule sets

const STRENGTHS: Rule[] = [
  R([['conscientiousness', H], ['consistency', H]], 'Dependable follow-through', 'A conscientious temperament plus consistent habits. This pairing often makes others comfortable handing you responsibility, and it compounds quietly over years.'),
  R([['curiosity', H], ['openness', H]], 'A strong learning appetite', 'Curiosity combined with openness often means you pick up new fields quickly and connect ideas across them. It tends to be an asset in changing environments.'),
  R([['empathy', H], ['communication', H]], 'Reading and reaching people', 'Empathy plus open communication often makes people feel understood around you. This may be useful in teams, teaching, care or negotiation.'),
  R([['emotionalStability', H], ['optimism', H]], 'Steadiness under pressure', 'A stable mood and a hopeful outlook tend to calm other people too. It may make you a natural anchor when situations are uncertain.'),
  R([['adaptability', H], ['openness', H]], 'Comfort with change', 'You report adjusting easily and welcoming novelty. This often helps in transitions, new roles and unfamiliar places.'),
  R([['creativity', H], ['openness', H]], 'Original connections', 'Creativity with openness often shows up as noticing links others miss. It usually needs unstructured time to surface.'),
  R([['selfEfficacy', H], ['followThrough', H]], 'Belief that turns into action', 'You trust your ability to handle things and you finish what you start. This is a strong combination for ambitious projects.'),
  R([['planning', H], ['timeManagement', H]], 'Looking ahead and using time well', 'Planning plus time management often means fewer last-minute scrambles and more room for the things you care about.'),
  R([['assertiveness', H], ['leadershipDrive', H]], 'Natural initiative', 'You say what you want and are willing to lead. Groups often benefit from someone who is willing to propose a direction.'),
  R([['trust', H], ['boundaries', H]], 'Open but protected', 'Trust plus clear boundaries is a healthy mix. It often means relationships can be warm without being draining.'),
  R([['gratitude', H], ['optimism', H]], 'An appreciative outlook', 'Gratitude and optimism are both linked in research with better well-being over time. They may help buffer you through hard stretches.'),
  R([['masteryOrientation', H], ['motMastery', H]], 'A craft orientation', 'You are motivated by getting better rather than just looking good. This often leads to deep skill when you stick with something.'),
  R([['empathy', H], ['agreeableness', H]], 'Warmth people feel', 'Empathy and agreeableness often show up as being easy to talk to. People may confide in you more than you realise.'),
  R([['selfAdvocacy', H], ['selfEsteem', H]], 'Speaking up for yourself', 'Self-regard plus willingness to ask for things often leads to better opportunities and clearer relationships.'),
  R([['intellectualDrive', H], ['investigative', H]], 'Analytical depth', 'You enjoy figuring things out for their own sake. That tends to support careers and hobbies where problems are open-ended.'),
  R([['impulseControl', H], ['conscientiousness', H]], 'Staying power', 'Good impulse control with conscientiousness often means you can pass up short-term pulls for longer-term goals.'),
  R([['workLifeBalance', H], ['boundaries', H]], 'A sustainable pace', 'You report keeping work and life in decent balance and holding your limits. That often protects energy over the long run.'),
  R([['socialConfidence', H], ['extraversion', H]], 'Ease in groups', 'Social confidence plus outgoingness tends to open doors, since people often meet you halfway.'),
]

const BLIND: Rule[] = [
  R([['perfectionism', H], ['procrastination', H]], 'High standards that may delay starting', 'When both are high, waiting for the right moment or perfect plan may be doing some of the work of avoiding a risk of falling short. Worth checking whether lowering the bar for a first draft helps.'),
  R([['agreeableness', H], ['conflictApproach', L]], 'Keeping the peace at your own expense', 'Wanting harmony while avoiding conflict can mean problems are managed internally rather than discussed. It might be worth noticing what goes unsaid.'),
  R([['assertiveness', H], ['empathy', L]], 'Directness that may land harder than intended', 'Strong assertiveness with lower empathy can come across as blunt. It may be worth asking trusted people how your tone lands.'),
  R([['achievement', H], ['workLifeBalance', L]], 'Drive that outruns recovery', 'High ambition with strained balance often means rest is treated as optional. Sustained effort usually needs deliberate recovery to last.'),
  R([['openness', H], ['followThrough', L]], 'Many starts, fewer finishes', 'Ideas are plentiful, and finishing may feel less rewarding than beginning. A rule such as finishing one before starting another may help.'),
  R([['structurePreference', H], ['adaptability', L]], 'Discomfort when plans shift', 'A strong need for structure with lower adaptability may make unexpected change feel disproportionately stressful. Building in buffers may help.'),
  R([['fearOfJudgment', H], ['selfAdvocacy', L]], 'Staying quiet to stay safe', 'Worry about being judged with hesitancy to speak up may mean good ideas and needs often remain unshared. A small, low-stakes share is a way to test the fear.'),
  R([['validationNeed', H], ['selfEsteem', L]], 'Outside approval carrying inner weight', 'When self-regard is shaky, other people\'s reactions can feel like a verdict. Worth checking whether praise and criticism are given more power than they deserve.'),
  R([['riskTolerance', H], ['impulseControl', L]], 'Speed over safeguards', 'Comfort with risk and weaker impulse control may lead to fast moves that are hard to reverse. A short cooling-off rule for big decisions may help.'),
  R([['relationalIndependence', H], ['communication', L]], 'Self-reliance others may read as distance', 'Valuing independence while saying little about your needs may leave people guessing. Naming what you need can feel risky but often brings people closer.'),
  R([['avoidance', H], ['fearOfFailure', H]], 'Sidestepping what matters most', 'Avoidance with a fear of failing can steer you away from exactly the things you care about. Worth checking which tasks you avoid and what you fear would happen.'),
  R([['stress', H], ['boundaries', L]], 'Absorbing more than you can hold', 'High stress with weaker boundaries often means taking on other people\'s load. Noticing the first moment you could say no may help.'),
  R([['emotionalStability', L], ['perfectionism', H]], 'A loud inner critic', 'Emotional sensitivity with perfectionism may make mistakes feel larger than they are. Self-compassion practices have some research support here.'),
  R([['socialConfidence', L], ['extraversion', H]], 'Wanting contact, hesitating to start it', 'Outgoing energy with low social confidence can look like wanting company while holding back. Rehearsed openers and small-group settings sometimes help.'),
  R([['impulsivity', H], ['planning', L]], 'Decisions made in the moment', 'Impulsive tendencies and limited planning may lead to frequent course corrections. A simple weekly review could catch drift early.'),
  R([['competingPriorities', H], ['goalClarity', L]], 'Too many things pulling at once', 'Competing demands with vague goals may leave you busy but unsure whether you are progressing. Ranking what matters most could reduce the pull.'),
]

const STATED_VS_REVEALED: Rule[] = [
  R([['valAchievement', H], ['followThrough', L]], 'Achievement is valued, follow-through lags', 'You say achievement matters, while your follow-through answers are low. This may mean the goals are not yet the right ones, or that something other than motivation is getting in the way.'),
  R([['valFamily', H], ['balancePriority', L]], 'Family-first values, work-first schedule', 'Family ranks high among your values, while balance at work ranks low. Worth checking whether the day-to-day allocation of time reflects what you say matters.'),
  R([['achievement', H], ['goalClarity', L]], 'Ambition without a target', 'High ambition together with low goal clarity may mean effort is spread widely. Defining what success looks like could focus it.'),
  R([['societalExpectation', H], ['intrinsicMotivation', L]], 'Following other people\'s script', 'A strong pull from expectations with weaker intrinsic motivation may mean some goals belong to others. Worth asking which goals feel like yours.'),
  R([['valCreativity', H], ['curLeisure', L]], 'Creativity valued, little room for it', 'You value creating, though your leisure life is rated low. This might be a scheduling issue more than a desire issue.'),
  R([['valKnowledge', H], ['curGrowth', L]], 'Wants to learn, is not currently learning', 'Knowledge ranks high and growth now ranks low. A small, regular learning habit may close this faster than a big plan.'),
  R([['growthDrive', H], ['consistency', L]], 'Growth desire, thin practice', 'A strong wish to grow with inconsistent habits might mean the method rather than the motivation needs work.'),
  R([['valFreedom', H], ['societalExpectation', H]], 'Freedom claimed, expectations felt', 'You value freedom while still feeling the pull of others\' expectations. It might be that part of the freedom you want is permission.'),
]

const DRIVE_CONFLICTS: Rule[] = [
  R([['achievement', H], ['intrinsicMotivation', L], ['extrinsicMotivation', H]], 'Goals that may be externally driven', 'High achievement with more extrinsic than intrinsic motivation suggests rewards, status or other people\'s approval may be powering the push. That can work well and can also feel empty. Worth checking which goals you would keep without an audience.'),
  R([['financialDrive', H], ['contribution', H]], 'Money and meaning both pulling', 'Strong financial drive and strong wish to contribute can pull career choices in different directions. Some people find roles that combine both, and others sequence them.'),
  R([['freedomNeed', H], ['stabilityNeed', H]], 'Freedom and stability together', 'Both rank high, and traditional paths usually deliver one more than the other. It may help to define a minimum of each.'),
  R([['adventure', H], ['familyOrientation', H]], 'Adventure and family', 'A pull toward novelty and a strong family orientation may need creative compromise on time, place and risk.'),
  R([['influence', H], ['fearOfJudgment', H]], 'Wants influence, fears evaluation', 'Influence means being visible, and visibility invites judgement. This might show as reluctance to take the lead role despite wanting it.'),
  R([['recognitionNeed', H], ['valIndependence', H]], 'Recognition versus independence', 'Wanting to be recognised and wanting to answer to no one may pull against each other, because recognition usually comes from others.'),
]

const OVERLOOKED: Rule[] = [
  R([['contribution', H], ['financialDrive', L]], 'Purpose may outrank payoff', 'Contribution ranks high and money ranks low. This may be an under-acknowledged driver worth designing around.'),
  R([['growthDrive', H], ['recognitionNeed', L]], 'A quiet self-improver', 'You want to grow without needing applause. Private, long-run projects may be where you thrive.'),
  R([['influence', H], ['assertiveness', L]], 'Influence without volume', 'You want to shape outcomes without being loud about it. Writing, mentoring and behind-the-scenes roles may fit.'),
  R([['adventure', H], ['curLeisure', L]], 'An appetite for novelty not being fed', 'Adventure ranks high while leisure now ranks low. Even small experiments may matter more than you expect.'),
  R([['familyOrientation', H], ['achievement', L]], 'Home may be the main project', 'Family matters a lot and ambition elsewhere is modest. That may be a deliberate choice worth protecting against outside pressure.'),
  R([['creativity', H], ['professionalConfidence', L]], 'Creative capacity under-claimed', 'You report a creative streak and little confidence at work. Your ideas may be better than you assume.'),
  R([['intellectualDrive', H], ['curCareer', L]], 'A mind that may be underused at work', 'You want intellectual fulfilment, and work is rated low. This might be a sign to look for more complexity in your role.'),
  R([['stabilityNeed', H], ['riskTolerance', H]], 'Calm base, bold edges', 'You want stability and tolerate risk. A stable core with bounded experiments might be the mix that suits you.'),
]

const LIFTS: Rule[] = [
  R([['relationshipQuality', H]], 'Relationships are carrying weight', 'Close relationships are among the most reliable predictors of well-being, and yours seem to be going well. Protecting time for them is probably high value.'),
  R([['meaning', H]], 'A sense of purpose', 'Feeling that life matters is a core part of well-being and often buffers stress.'),
  R([['engagement', H]], 'Absorbing activity', 'You describe regularly getting lost in what you do. That absorption is a hallmark of flourishing, so it is worth noticing where it happens.'),
  R([['accomplishment', H]], 'A sense of progress', 'Feeling you are achieving things you care about supports confidence and mood.'),
  R([['positiveEmotion', H]], 'Everyday positive feeling', 'Frequent positive emotion tends to build resources over time, such as relationships and resilience.'),
  R([['gratitude', H]], 'Noticing the good', 'Gratitude is linked with better mood and relationships and costs nothing to keep practising.'),
  R([['optimism', H]], 'Expecting things to go reasonably well', 'Optimism tends to support persistence and recovery, as long as it stays paired with realistic planning.'),
  R([['workLifeBalance', H]], 'Work that leaves room for life', 'Reasonable balance is a protective factor for both well-being and relationships.'),
  R([['feelingUnderstood', H]], 'Feeling understood', 'Feeling seen by others is a quiet source of well-being that is easy to underrate.'),
  R([['autonomy', H]], 'Directing your own life', 'Experiencing choice in what you do is one of the strongest drivers of intrinsic motivation.'),
  R([['emotionalStability', H]], 'Emotional steadiness', 'A steady baseline reduces the amount of energy spent recovering from ups and downs.'),
]

const DRAGS: Rule[] = [
  R([['stress', H]], 'High stress load', 'Elevated stress can quietly lower everything else, including mood, sleep and patience. If this has lasted a while, talking with a doctor or counsellor is reasonable.'),
  R([['loneliness', H]], 'Feeling lonely', 'Loneliness is a common and under-discussed drag on well-being. It often responds to small, regular contact more than occasional big efforts.'),
  R([['workLifeBalance', L]], 'Work may be crowding out life', 'Strained balance is associated with lower well-being and relationship quality. Even one protected evening a week can be a start.'),
  R([['meaning', L]], 'A thin sense of purpose', 'Low meaning often shows up as feeling flat even when things are fine. Small, concrete acts of contribution sometimes help.'),
  R([['engagement', L]], 'Little absorption', 'Rarely losing yourself in what you do may suggest your days lack challenge that matches your skill. Worth looking for one activity that does.'),
  R([['relationshipQuality', L]], 'Relationships not feeling nourishing', 'Strained or thin relationships weigh heavily on well-being. This might be worth gentle attention, perhaps with support.'),
  R([['accomplishment', L]], 'Little sense of progress', 'Feeling you are not getting anywhere often drains motivation. Tracking small wins may help rebuild it.'),
  R([['positiveEmotion', L]], 'Fewer positive moments', 'Few positive moments can be a sign of overload or low recovery. If it has lasted weeks, professional input is a reasonable step.'),
  R([['careerSatisfaction', L]], 'Dissatisfaction with direction', 'Feeling unhappy with where work is heading can seep into the rest of life.'),
  R([['feelingUnderstood', L]], 'Not feeling understood', 'Feeling unseen can be isolating even among people. It may help to find at least one relationship where you can be more fully honest.'),
  R([['optimism', L]], 'A pessimistic default', 'Expecting things to go badly may feel realistic and still shape what you attempt. Worth testing predictions against what actually happens.'),
]

const CONF_HYP: Rule[] = [
  R([['selfEsteem', L], ['professionalConfidence', H]], 'Capable at work, less sure of your worth', 'Skill confidence with weaker self-esteem often means performance is where self-worth is tested. Worth checking whether a bad day at work feels like a verdict on you.'),
  R([['selfEfficacy', H], ['fearOfFailure', H]], 'Confident, and pressure-sensitive', 'Believing you can do things while fearing failure is typical in high performers. The fear may be tied to stakes rather than ability.'),
  R([['socialConfidence', L], ['fearOfJudgment', H]], 'Group settings may feel like an exam', 'Low social confidence with fear of judgement often goes together, and it can ease with experience and the right environment.'),
  R([['fearOfRejection', H], ['selfAdvocacy', L]], 'Asking less than you want', 'Fear of rejection with low self-advocacy may mean many requests never get made. Practise small asks where a no costs little.'),
  R([['validationNeed', H], ['selfEsteem', L]], 'Self-worth borrowed from approval', 'A strong need for validation with lower self-esteem suggests others\' views hold much weight. Building evidence of your own standards may help.'),
  R([['selfEfficacy', L], ['masteryOrientation', H]], 'A learner who underrates their capacity', 'You like learning but doubt you can do things well. Small wins on a ladder of difficulty are often the best evidence.'),
  R([['perfectionism', H], ['selfEsteem', L]], 'Self-worth tied to flawlessness', 'High perfectionism with lower self-esteem can make ordinary mistakes feel threatening. Practising good-enough on low-stakes items is a gentle test.'),
  R([['selfEsteem', H], ['selfAdvocacy', L]], 'Inner security not yet voiced', 'Solid self-regard with little advocacy may mean you feel okay inside and still do not ask for what you want.'),
  R([['bodyImage', L], ['socialConfidence', L]], 'Self-image and social ease may be linked', 'Discomfort with how you look may be feeding social hesitancy. This often shifts with attention away from appearance and toward activity.'),
  R([['professionalConfidence', L], ['competence', H]], 'Skills that exceed self-view at work', 'You feel competent in general and less so professionally. It might help to gather outside feedback on your actual output.'),
]

const REL_STRENGTHS: Rule[] = [
  R([['empathy', H], ['trust', H]], 'Warm and willing to trust', 'Empathy and trust together often make people feel safe with you, which tends to deepen relationships quickly.'),
  R([['communication', H], ['conflictApproach', H]], 'Willing to have hard conversations', 'Open communication and willingness to engage conflict help issues get solved early rather than festering.'),
  R([['boundaries', H], ['intimacyComfort', H]], 'Close without losing yourself', 'Comfort with closeness and healthy boundaries are a strong base for lasting relationships.'),
  R([['feelingUnderstood', H], ['relationshipQuality', H]], 'Relationships that fit', 'You report feeling understood and satisfied. That is worth protecting through regular, deliberate time.'),
  R([['empathy', H], ['agreeableness', H]], 'People feel looked after', 'Caring and cooperation are often noticed and appreciated, especially in tough times.'),
]
const REL_NEEDS: Rule[] = [
  R([['companionshipNeed', H]], 'Regular company matters to you', 'You report a strong need for being with others. Long solitary stretches may leave you more flat than you realise.'),
  R([['relationalIndependence', H]], 'Space inside close relationships', 'You value room to be yourself. Saying this out loud early may prevent it being read as distance.'),
  R([['feelingUnderstood', L], ['communication', H]], 'Saying it, but not feeling heard', 'You communicate openly and still do not feel understood. It might be about who you share with more than how.'),
  R([['loneliness', H]], 'Connection that goes deeper', 'Loneliness is not just being alone. It often means wanting more depth or a fitting circle.'),
  R([['attachmentAnxiety', H]], 'Reassurance', 'Worry about closeness often shows up as wanting reassurance more often. Naming it directly with someone safe may reduce its grip.'),
  R([['attachmentAvoidance', H]], 'Room to open up at your own pace', 'A tendency to keep some distance usually serves a protective purpose. It tends to ease with relationships that do not push.'),
]
const REL_GROWTH: Rule[] = [
  R([['conflictApproach', L]], 'Practising conflict in small doses', 'Avoiding conflict may keep the peace short term. Raising small issues early can prevent big ones later.'),
  R([['communication', L]], 'Saying what you need', 'Others cannot meet needs they cannot see. One sentence of "I need..." can go a long way.'),
  R([['trust', L]], 'Testing trust gradually', 'Low trust is often earned, and it can also keep out people who would be reliable. Small, safe disclosures work as experiments.'),
  R([['empathy', L]], 'Curiosity about other people\'s inner world', 'Asking one more question than feels natural often builds understanding quickly.'),
  R([['boundaries', L]], 'Setting limits kindly', 'Clear limits tend to protect relationships rather than harm them.'),
  R([['intimacyComfort', L]], 'Closeness at a comfortable speed', 'Discomfort with closeness is common and workable. Small shared routines often feel safer than intense conversations.'),
  R([['attachmentAvoidance', H], ['companionshipNeed', H]], 'Reconciling wanting and avoiding closeness', 'You want company and also step back from closeness. It may help to explore, ideally with support, when you pull away and what you fear.'),
]

const CAREER_CONFLICTS: Rule[] = [
  R([['workAutonomyNeed', H], ['structurePreference', H]], 'Autonomy and structure', 'You want freedom in how you work and clarity in what is expected. Roles with clear outcomes but open methods may fit best.'),
  R([['prestigeNeed', H], ['balancePriority', H]], 'Prestige and balance', 'High-status paths often demand long hours. You may need to decide which matters more in a given decade.'),
  R([['securityNeed', H], ['careerFit', L]], 'Secure, but not a good fit', 'Security is important to you and your current path feels off. Staying could be a deliberate choice, and worth weighing against what staying costs.'),
  R([['leadershipDrive', H], ['selfAdvocacy', L]], 'Wants to lead, hesitates to ask', 'You want leadership and find it hard to ask for scope. Small, specific asks are a way to test the response.'),
  R([['helpingDrive', H], ['incomeExpectation', H]], 'Helping and income', 'Helping roles often pay less than other fields. Sectors such as health, education technology or social enterprise may offer more of both.'),
  R([['workloadTolerance', L], ['achievement', H]], 'Ambition with limited stamina', 'You want a lot and tire easily under load. Choosing fewer, higher-value goals may be more sustainable.'),
  R([['careerClarity', L], ['achievement', H]], 'Driven without a direction', 'High drive without a clear path can lead to chasing the next ladder rung. Short experiments in several areas may help.'),
  R([['incomeExpectation', H], ['riskTolerance', L]], 'High income hopes, low risk appetite', 'Income potential in many fields rises with risk. Steady-growth fields with specialised skills may help bridge it.'),
]

const VALUE_CONFLICTS: Rule[] = [
  R([['valFreedom', H], ['valSecurity', H]], 'Freedom and security', 'Both rank high, and they often pull in opposite directions. Worth deciding what the minimum of each is.'),
  R([['valAchievement', H], ['valFamily', H]], 'Achievement and family', 'Both are time-hungry. Clarifying which wins when they clash may spare future guilt.'),
  R([['valWealth', H], ['valCompassion', H]], 'Wealth and compassion', 'Chasing money and caring for others can conflict in some careers and complement each other in others.'),
  R([['valHonesty', H], ['valLoyalty', H]], 'Honesty and loyalty', 'These can collide when a friend does something you disagree with. It may help to think through your line in advance.'),
  R([['valIndependence', H], ['valCommunity', H]], 'Independence and community', 'Valuing both self-direction and belonging suggests looking for communities that welcome individuals.'),
  R([['valCreativity', H], ['valSecurity', H]], 'Creativity and security', 'Creative paths are often irregular in income. A protected creative practice beside stable work is a common compromise.'),
  R([['valResponsibility', H], ['valFreedom', H]], 'Responsibility and freedom', 'Being dependable and being unconstrained often conflict. It may help to choose commitments carefully.'),
  R([['valJustice', H], ['valLoyalty', H]], 'Fairness and loyalty', 'When someone close breaks a rule, these two values can tug in opposite directions.'),
]

const TRAJECTORY: Rule[] = [
  R([['futureClarity', L], ['futureOptimism', H]], 'Hopeful, but undefined', 'You expect good things without a clear picture. A few concrete "what would good look like" sketches may help turn hope into direction.'),
  R([['futureClarity', H], ['trajectoryFit', L]], 'A clear destination, an off-course path', 'You know where you want to go, and your current path does not lead there. This is often the clearest signal for a concrete change.'),
  R([['regretRisk', H], ['trajectoryFit', L]], 'Regret may grow if nothing changes', 'You anticipate regret and feel off-track. This tends to be the combination most worth acting on.'),
  R([['futureOptimism', L]], 'A cautious outlook on the future', 'Low confidence in what is ahead can come from past experience. Small, successful experiments can shift it more than reassurance.'),
  R([['trajectoryFit', H], ['futureClarity', H]], 'Path and picture line up', 'Your current direction fits where you want to end up. The task may be pace and protecting what works.'),
  R([['futureOptimism', H], ['goalClarity', H]], 'Optimism with a plan', 'Hope with clear goals is a strong base for sustained progress.'),
  R([['regretRisk', H]], 'Regret is a live concern', 'Anticipated regret can motivate or paralyse. Asking "what would I regret not trying?" can turn it into direction.'),
]

const HIDDEN: Rule[] = [
  R([['creativity', H], ['professionalConfidence', L]], 'Creative capacity that may exceed your self-view', 'High creativity with lower professional confidence may mean the ideas are stronger than you assume. A low-stakes way to share them could test this.'),
  R([['empathy', H], ['selfAdvocacy', L]], 'Quiet leadership', 'Empathy without loudness is a recognised leadership style. You might do well where listening matters more than speaking first.'),
  R([['curiosity', H], ['careerClarity', L]], 'Range as an asset', 'Wide curiosity with an unclear path may point toward hybrid fields that combine several interests.'),
  R([['motMastery', H], ['selfEfficacy', L]], 'Mastery you may be undercounting', 'You love getting good at things and doubt your capacity. Your track record in past skills could be better evidence than your feelings.'),
  R([['openness', H], ['riskTolerance', L]], 'Cautious creative', 'Creative interests with a cautious style often do well with small prototypes rather than big leaps.'),
  R([['intellectualDrive', H], ['curGrowth', L]], 'Unused intellectual energy', 'You want to learn and are not learning now. A single structured course or reading project might release a lot of energy.'),
  R([['leadershipDrive', H], ['assertiveness', L]], 'Leadership you have not tried yet', 'You want to lead and speak softly. Facilitation and mentoring styles may fit better than command styles.'),
  R([['investigative', H], ['artistic', H]], 'A bridge between analysis and imagination', 'Combining investigative and artistic interests points toward fields like design research, data visualisation, architecture or game design.'),
  R([['empathy', H], ['investigative', H]], 'Understanding people with evidence', 'Empathy and analytical interest together suit research that touches human problems, such as UX research, public health or psychology.'),
  R([['socialConfidence', L], ['empathy', H]], 'A one-to-one connector', 'Lower group confidence with high empathy often means you are strongest in one-to-one settings, which many roles value highly.'),
  R([['contribution', H], ['enterprising', H]], 'Mission-led building', 'A drive to contribute with an enterprising streak suggests social enterprise or community-building roles.'),
  R([['adaptability', H], ['curiosity', H]], 'A fast starter in new fields', 'Curiosity and adaptability often make people productive quickly in unfamiliar environments, which is valuable in changing industries.'),
]

// ---------------------------------------------------------------- narrative sections

const DRIVER_WHY: Partial<Record<DimId, string>> = {
  achievement: 'Wanting to reach hard goals and see measurable results. It often needs a clear target to stay healthy.',
  financialDrive: 'Valuing financial strength, as security, freedom or status. Worth separating which of these you are really after.',
  recognitionNeed: 'Wanting others to see and value your work. It tends to motivate effort and can make you sensitive to feedback.',
  influence: 'Wanting to shape decisions and direction. It may point to roles with scope to steer.',
  contribution: 'Wanting your work to benefit others. It often supports persistence when things are hard.',
  adventure: 'Craving novelty, challenge and new places. A routine without any of this may feel flat.',
  stabilityNeed: 'Valuing predictability and a secure base. It frees energy for other things when it is met.',
  freedomNeed: 'Valuing control over time, place and choices. It tends to rise when you feel constrained.',
  familyOrientation: 'Putting family and close ties at the centre of life plans.',
  growthDrive: 'Wanting to keep developing as a person. It tends to favour environments that stretch you.',
  intellectualDrive: 'Wanting to understand things deeply and use your mind fully.',
}

function behavioral(x: Ctx): string {
  const bits: string[] = []
  if (x.ok('consistency', 'followThrough')) {
    const a = (x.sc('consistency') + x.sc('followThrough')) / 2
    bits.push(a >= 62 ? 'Your habit answers suggest you tend to keep going once you start, and finish most of what you commit to.' : a <= 38 ? 'Your habit answers suggest routines and finishing are a struggle, and this may come from conditions more than character.' : 'Your habit answers suggest a mixed picture: steady in some areas, patchy in others.')
  }
  if (x.ok('procrastination')) {
    if (x.sc('procrastination') >= 62) bits.push('You report delaying tasks fairly often, which can be about fear, unclear next steps or low energy rather than laziness.')
    else if (x.sc('procrastination') <= 38) bits.push('You rarely put things off, and early action tends to reduce stress.')
  }
  if (x.ok('planning', 'timeManagement')) {
    const a = (x.sc('planning') + x.sc('timeManagement')) / 2
    if (a >= 62) bits.push('Planning and time management seem like real strengths.')
    else if (a <= 38) bits.push('Planning and time management are not your natural mode, so external structure such as calendars, deadlines and accountability partners may help more than willpower.')
  }
  if (x.ok('impulseControl') && x.sc('impulseControl') <= 38) bits.push('Impulse control may be a pressure point, particularly when tired or stressed.')
  return bits.length ? bits.join(' ') : 'There were not enough habit answers to describe your day-to-day behaviour with confidence.'
}

function social(x: Ctx): string {
  const bits: string[] = []
  if (x.ok('extraversion')) bits.push(x.sc('extraversion') >= 62 ? 'You tend to gain energy from other people and activity.' : x.sc('extraversion') <= 38 ? 'You tend to prefer smaller groups and need time alone to recover.' : 'You sit in the middle on social energy, and context probably matters more than a fixed preference.')
  if (x.ok('socialConfidence')) {
    if (x.sc('socialConfidence') <= 38) bits.push('Social situations may take more effort than they appear from outside.')
    else if (x.sc('socialConfidence') >= 62) bits.push('You feel reasonably at ease starting and joining conversations.')
  }
  if (x.ok('empathy', 'agreeableness') && (x.sc('empathy') + x.sc('agreeableness')) / 2 >= 62) bits.push('People are likely to experience you as warm and considerate.')
  if (x.ok('assertiveness') && x.sc('assertiveness') <= 38) bits.push('You may hold back your view in groups, so others may not know what you want.')
  if (x.ok('assertiveness') && x.sc('assertiveness') >= 62) bits.push('You usually say what you think and push for what you want.')
  return bits.length ? bits.join(' ') : 'There were not enough social answers to describe your social style with confidence.'
}

function decisionStyle(x: Ctx): string {
  const bits: string[] = []
  if (x.ok('riskTolerance')) bits.push(x.sc('riskTolerance') >= 62 ? 'You are comfortable deciding with incomplete information and accepting some risk.' : x.sc('riskTolerance') <= 38 ? 'You tend to weigh downsides heavily and prefer to protect what you have.' : 'Your risk appetite is moderate, so the stakes and the details probably decide.')
  if (x.ok('impulsivity')) bits.push(x.sc('impulsivity') >= 62 ? 'Decisions can come quickly and by feel.' : x.sc('impulsivity') <= 38 ? 'You tend to take time before deciding.' : '')
  if (x.ok('planning') && x.sc('planning') >= 62) bits.push('You like to plan several steps ahead.')
  if (x.ok('perfectionism', 'fearOfFailure') && (x.sc('perfectionism') + x.sc('fearOfFailure')) / 2 >= 62) bits.push('High standards and fear of failure may slow decisions, since every option feels as if it must be right.')
  if (x.ok('curiosity', 'openness') && (x.sc('curiosity') + x.sc('openness')) / 2 >= 62) bits.push('You are open to unusual options and may be drawn to the less obvious one.')
  if (x.ok('structurePreference') && x.sc('structurePreference') >= 62) bits.push('Clear criteria and a framework help you decide.')
  return bits.filter(Boolean).join(' ') || 'There were not enough answers to describe your decision style with confidence.'
}

function identityOverview(x: Ctx, quality: Report['quality'], ts: Report['typeSketch']): string[] {
  const out: string[] = []
  const lines = (...ds: DimId[]) => top(x, ds, 2).map((d) => `${lc(dimLabel(d))} (${round(x.sc(d))})`)
  const p1 = lines('extraversion', 'agreeableness', 'conscientiousness', 'emotionalStability', 'openness')
  out.push(
    para(
      p1.length ? `Your clearest personality signals are ${p1.join(' and ')}.` : 'Your personality scores sit near the middle on most traits.',
      ts.axes.every((a) => a.strength === 0) ? '' : `A loose type sketch gives ${ts.code}, which is only a starting point.`,
      'The sections below try to explain how these combine instead of listing them one at a time. Treat each as a hypothesis, not a verdict.',
    ),
  )
  const drv = top(x, ['achievement', 'financialDrive', 'recognitionNeed', 'influence', 'contribution', 'adventure', 'stabilityNeed', 'freedomNeed', 'familyOrientation', 'growthDrive', 'intellectualDrive'], 3)
  if (drv.length) out.push(`What seems to move you most is ${drv.map((d) => lc(dimLabel(d))).join(', ')}. ${x.ok('intrinsicMotivation', 'extrinsicMotivation') ? (x.sc('intrinsicMotivation') >= x.sc('extrinsicMotivation') ? 'Your motivation leans more toward interest and meaning than rewards.' : 'Your motivation leans more toward outcomes, rewards and recognition than interest for its own sake.') : ''}`.trim())
  const swb = x.avg(['lifeSatisfaction', 'meaning', 'engagement', 'positiveEmotion'])
  if (swb !== null && x.ok('lifeSatisfaction')) out.push(swb >= 62 ? 'Overall you describe life in fairly good terms, so the aim may be refinement more than repair.' : swb <= 40 ? 'You describe life as less satisfying than you would like. This could be a temporary phase, and it is worth treating as information, not a flaw.' : 'You describe life as mixed: some things work well, others less so.')
  if (quality.overallConfidence < 0.5 || quality.answered < 40) out.push('Many of these readings rest on a limited number of answers. Answering more questions, or retaking later, will make them more reliable.')
  for (const f of quality.flags) {
    if (f.id === 'straightlining') out.push('Some answers followed a very regular pattern, which can mean the questions blurred together. Confidence is lowered accordingly.')
    if (f.id === 'speeding') out.push('Answers were given very quickly, which can reduce how much they reflect considered views.')
    if (f.id === 'inconsistent-pairs') out.push('Some paired questions were answered in conflicting ways. That can reflect real ambivalence or a mixed mood, so some readings are held loosely.')
  }
  return out
}

function ambitions(x: Ctx, cons: Contradiction[]): Report['ambitions'] {
  const ids = (Object.keys(DRIVER_WHY) as DimId[])
  const drivers = top(x, ids, 5).map((d) => ({ dim: d, label: dimLabel(d), score: round(x.sc(d)), why: DRIVER_WHY[d]! }))
  const intr = x.avg(['intrinsicMotivation', 'masteryOrientation'])
  const extr = x.avg(['extrinsicMotivation', 'societalExpectation', 'performanceOrientation'])
  let reading = 'There were not enough answers to compare intrinsic and extrinsic motivation.'
  if (intr !== null && extr !== null) {
    const diff = intr - extr
    reading = diff >= 12
      ? 'You lean toward doing things for their own sake. Goals that feel meaningful probably keep you going better than rewards do.'
      : diff <= -12
        ? 'You lean toward outcomes, status and others\' expectations as fuel. That can be effective, and it may be worth checking that the goals are also yours.'
        : 'Interest and outcomes both matter to you. The balance may shift with the project, so ask of each goal which one is driving it.'
  }
  const ovr: string[] = []
  if (drivers.length) {
    ovr.push(`Your strongest drivers are ${drivers.slice(0, 3).map((d) => `${lc(d.label)} (${d.score})`).join(', ')}. ${DRIVER_WHY[drivers[0].dim]}`)
    const weak = top(x, ids, 11).slice(-1)[0]
    if (weak && weak !== drivers[0].dim && x.sc(weak) <= 40) ovr.push(`At the other end, ${lc(dimLabel(weak))} ranks low (${round(x.sc(weak))}). That is not a gap, only a place you may be less motivated, which is worth remembering when comparing yourself to people for whom it is central.`)
  } else ovr.push('There were not enough answers on ambition to say what drives you.')
  if (cons[0]) ovr.push(`One tension worth noticing: ${lc(cons[0].title)}.`)
  return {
    drivers,
    overview: ovr,
    intrinsicVsExtrinsic: { intrinsic: round(intr ?? 50), extrinsic: round(extr ?? 50), reading },
    statedVsRevealed: fire(x, STATED_VS_REVEALED, 4),
    conflicts: fire(x, DRIVE_CONFLICTS, 4),
    overlooked: fire(x, OVERLOOKED, 4),
  }
}

function happiness(x: Ctx): Report['happiness'] {
  const parts: [DimId, number, boolean][] = [
    ['lifeSatisfaction', 0.3, false],
    ['positiveEmotion', 0.1, false],
    ['engagement', 0.1, false],
    ['relationshipQuality', 0.1, false],
    ['meaning', 0.1, false],
    ['accomplishment', 0.1, false],
    ['stress', 0.1, true],
    ['loneliness', 0.1, true],
  ]
  let num = 0
  let den = 0
  for (const [d, w, inv] of parts) {
    if (x.s[d].n === 0) continue
    num += w * (inv ? 100 - x.sc(d) : x.sc(d))
    den += w
  }
  const overall = den ? round(num / den) : 50
  const perma = prof(x, ['positiveEmotion', 'engagement', 'relationshipQuality', 'meaning', 'accomplishment'])
  let stateVsTrait = 'There were not enough answers on recent mood and overall life satisfaction to compare them.'
  if (x.s.recentMood.n > 0 && x.s.lifeSatisfaction.n > 0) {
    const diff = x.sc('recentMood') - x.sc('lifeSatisfaction')
    stateVsTrait = diff >= 15
      ? `Your recent mood (${round(x.sc('recentMood'))}) is running higher than your longer-run satisfaction (${round(x.sc('lifeSatisfaction'))}). That could mean a good patch now, or that your overall appraisal is more critical than your day-to-day feelings.`
      : diff <= -15
        ? `Your recent mood (${round(x.sc('recentMood'))}) is lower than your overall life satisfaction (${round(x.sc('lifeSatisfaction'))}). That pattern often reflects a rough patch rather than a lasting state, though if it persists, speaking to someone is reasonable.`
        : `Your recent mood (${round(x.sc('recentMood'))}) and life satisfaction (${round(x.sc('lifeSatisfaction'))}) are close, so how you feel lately looks like a fair guide to how you feel overall.`
  }
  const overview: string[] = []
  const tone = overall >= 65 ? 'fairly high' : overall <= 40 ? 'on the lower side' : 'in the middle'
  overview.push(`Your well-being composite is ${overall} out of 100, which is ${tone}. It combines life satisfaction, the five PERMA elements and subtracts stress and loneliness. It is a reflection of how you described recent life, not a diagnosis.`)
  const pk = [...perma].sort((a, b) => b.score - a.score)
  if (pk.length >= 2) overview.push(`Of the PERMA elements, ${lc(pk[0].label)} is your strongest (${pk[0].score}) and ${lc(pk[pk.length - 1].label)} the lowest (${pk[pk.length - 1].score}). Raising the lowest one a little tends to give more than polishing the highest.`)
  if (overall <= 40 || (x.ok('stress') && x.sc('stress') >= 75) || (x.ok('loneliness') && x.sc('loneliness') >= 75)) overview.push('If this reflects how things have felt for a while, talking with a doctor, counsellor or someone you trust is a reasonable next step. This tool cannot assess that.')
  return { overall, perma, lifts: fire(x, LIFTS, 4), drags: fire(x, DRAGS, 4), stateVsTrait, overview }
}

function confidence(x: Ctx): Report['confidence'] {
  const group = ['selfEsteem', 'selfEfficacy', 'socialConfidence', 'professionalConfidence', 'bodyImage', 'selfAdvocacy', 'fearOfFailure', 'fearOfJudgment', 'fearOfRejection', 'perfectionism', 'validationNeed'] as DimId[]
  const positive = x.avg(['selfEsteem', 'selfEfficacy', 'socialConfidence', 'professionalConfidence', 'selfAdvocacy'])
  const fears = x.avg(['fearOfFailure', 'fearOfJudgment', 'fearOfRejection'])
  const overview: string[] = []
  if (positive !== null) overview.push(positive >= 62 ? 'Your self-confidence answers are mostly solid. Look at which areas fall below that level, since they are probably where it matters.' : positive <= 40 ? 'Your confidence answers lean low. Confidence is often domain-specific and changeable, and small, well-chosen wins tend to move it more than reassurance.' : 'Your confidence is mixed, strong in some areas and lower in others.')
  else overview.push('There were not enough answers to describe your confidence.')
  if (fears !== null) overview.push(fears >= 62 ? 'Fears of failing, being judged or rejected are fairly prominent. These can sit beside real competence and are often about stakes, not ability.' : fears <= 38 ? 'Fears of failure, judgement and rejection seem to have little hold on you, which makes risk-taking easier.' : 'Fears of failure, judgement and rejection are present but not dominant.')
  const social = x.ok('socialConfidence'), work = x.ok('professionalConfidence')
  if (social && work && Math.abs(x.sc('socialConfidence') - x.sc('professionalConfidence')) >= 20) overview.push(`Confidence differs by setting: social ${round(x.sc('socialConfidence'))} versus professional ${round(x.sc('professionalConfidence'))}. That suggests confidence is a skill of context rather than a fixed trait.`)
  return { overview, profile: prof(x, group), hypotheses: fire(x, CONF_HYP, 5) }
}

function relationships(x: Ctx): Report['relationships'] {
  const group = dimsInGroup('relationships')
  const overview: string[] = []
  if (x.ok('companionshipNeed', 'relationalIndependence')) overview.push(x.sc('companionshipNeed') > x.sc('relationalIndependence') + 12 ? 'You lean toward togetherness more than independence in close relationships. Time with people probably feeds you.' : x.sc('relationalIndependence') > x.sc('companionshipNeed') + 12 ? 'You lean toward independence more than constant togetherness. Space is probably part of how you stay close.' : 'You balance togetherness and independence fairly evenly.')
  if (x.ok('empathy', 'communication')) overview.push(`Your empathy (${round(x.sc('empathy'))}) and open communication (${round(x.sc('communication'))}) set the tone for how people experience you in relationships.`)
  if (x.ok('attachmentAnxiety') || x.ok('attachmentAvoidance')) {
    const bits: string[] = []
    if (x.ok('attachmentAnxiety') && x.sc('attachmentAnxiety') >= 62) bits.push('worrying about closeness and reassurance')
    if (x.ok('attachmentAvoidance') && x.sc('attachmentAvoidance') >= 62) bits.push('keeping some distance when things get close')
    if (bits.length) overview.push(`Some answers point toward ${bits.join(' and ')}. These are common styles, shaped by experience, and they can shift with supportive relationships. They are not labels or diagnoses.`)
  }
  if (!overview.length) overview.push('There were not enough answers on relationships to describe them.')
  let currentVsIdeal = 'Not enough answers to compare how relationships are now with how much they matter.'
  if (x.s.curRelationships.n > 0 && x.s.idealRelationships.n > 0) {
    const g = x.sc('idealRelationships') - x.sc('curRelationships')
    currentVsIdeal = g >= 15 ? `Relationships matter to you (${round(x.sc('idealRelationships'))}) more than they currently feel satisfying (${round(x.sc('curRelationships'))}). This gap is a good place to put effort.` : g <= -15 ? `Your relationships (${round(x.sc('curRelationships'))}) are doing better than you rate their importance (${round(x.sc('idealRelationships'))}). You might have room to invest elsewhere without losing much here.` : 'Your relationships and how much they matter to you are well matched right now.'
  }
  return { overview, profile: prof(x, group), strengths: fire(x, REL_STRENGTHS, 4), needs: fire(x, REL_NEEDS, 4), growth: fire(x, REL_GROWTH, 4), currentVsIdeal }
}

function environmentText(x: Ctx): string {
  const bits: string[] = []
  if (x.ok('workAutonomyNeed')) bits.push(x.sc('workAutonomyNeed') >= 62 ? 'a role with real say in how and when you work' : x.sc('workAutonomyNeed') <= 38 ? 'clear direction and well-defined expectations' : '')
  if (x.ok('structurePreference')) bits.push(x.sc('structurePreference') >= 62 ? 'predictable routines and defined processes' : x.sc('structurePreference') <= 38 ? 'room to improvise and change plans' : '')
  if (x.ok('riskTolerance') && x.sc('riskTolerance') >= 62) bits.push('tolerance for uncertainty, such as small, fast-moving teams')
  if (x.ok('securityNeed') && x.sc('securityNeed') >= 62) bits.push('stable employment and clear progression')
  if (x.ok('leadershipDrive') && x.sc('leadershipDrive') >= 62) bits.push('scope to lead people or projects')
  if (x.ok('helpingDrive') && x.sc('helpingDrive') >= 62) bits.push('visible benefit to the people you serve')
  if (x.ok('extraversion')) bits.push(x.sc('extraversion') >= 62 ? 'regular contact with colleagues' : x.sc('extraversion') <= 38 ? 'protected focus time and quieter spaces' : '')
  if (x.ok('balancePriority') && x.sc('balancePriority') >= 62) bits.push('reasonable hours and limits on after-hours demands')
  if (x.ok('creativity') && x.sc('creativity') >= 62) bits.push('space for original ideas')
  const b = bits.filter(Boolean)
  if (!b.length) return 'There were not enough work-preference answers to describe a best-fit environment. Add more answers to sharpen it.'
  const list = b.length > 1 ? `${b.slice(0, -1).join(', ')} and ${b[b.length - 1]}` : b[0]
  return `Your answers suggest you may do best in settings offering ${list}. This is a hypothesis about conditions, not about specific jobs, so test it against real roles.`
}

function career(x: Ctx, alts: CareerMatch[]): Report['career'] {
  const m = (ds: DimId[], inv: DimId[] = []) => x.avg(ds, inv)
  const comps: { label: string; w: number; v: number | null; ex: (v: number) => string }[] = [
    {
      label: 'Motivational fit',
      w: 0.3,
      v: m(['careerSatisfaction', 'engagement', 'accomplishment', 'autonomy', 'competence']),
      ex: (v) => (v >= 62 ? 'Your answers suggest your current direction feeds your motivation: you feel engaged, capable and able to choose.' : v <= 40 ? 'Your answers suggest work is not feeding your motivation well. Engagement, sense of progress or choice may be low.' : 'Your motivation at work is mixed. Some needs for engagement, progress and choice are met, others less.'),
    },
    {
      label: 'Values fit',
      w: 0.25,
      v: m(['careerFit', 'meaning', 'trajectoryFit']),
      ex: (v) => (v >= 62 ? 'Your work seems to connect with what you find meaningful and where you want to go.' : v <= 40 ? 'Your work may be out of step with your sense of meaning or direction. Worth identifying which values go unserved.' : 'Your work partly reflects your values and partly does not.'),
    },
    {
      label: 'Personality and interests fit',
      w: 0.25,
      v: m(['careerFit', 'competence', 'professionalConfidence', 'curCareer']),
      ex: (v) => (v >= 62 ? 'You feel competent and well suited to your current path, which is a good sign for fit with your personality and interests.' : v <= 40 ? 'You report low fit and low confidence in your current path. It may be about the particular role rather than your ability.' : 'The fit between your personality, interests and current path seems partial.'),
    },
    {
      label: 'Lifestyle fit',
      w: 0.2,
      v: m(['workLifeBalance', 'curFinances'], ['stress']),
      ex: (v) => (v >= 62 ? 'Work seems to fit your wider life: workable balance, manageable stress and reasonable finances.' : v <= 40 ? 'Work may be costing you in time, stress or money. That cost can be as important as interest in the work itself.' : 'Work fits your life only partly, with some balance or stress issues.'),
    },
  ]
  const components: AlignmentComponent[] = comps.map((c) => ({
    label: c.label,
    score: round(c.v ?? 50),
    explanation: c.v === null ? 'Not enough answers to score this component, so it is left at the midpoint.' : c.ex(c.v),
  }))
  const known = comps.filter((c) => c.v !== null)
  const align = known.length ? round(known.reduce((s, c) => s + c.w * c.v!, 0) / known.reduce((s, c) => s + c.w, 0)) : 50
  const rc = riasecCode(x)
  const rs = RIASEC.filter((r) => x.s[r.dim].n > 0)
  const sorted = [...rs].sort((a, b) => x.sc(b.dim) - x.sc(a.dim))
  const overview: string[] = []
  overview.push(`Your career alignment score is ${align}. This is about how well your current direction seems to fit you, as you perceive it. It says nothing about what you ought to do, and it is built from motivation, values, personality and lifestyle.`)
  if (sorted.length >= 3) {
    const spread = x.sc(sorted[0].dim) - x.sc(sorted[5 > sorted.length - 1 ? sorted.length - 1 : 5].dim)
    overview.push(`Your top interest themes are ${sorted.slice(0, 3).map((r) => `${lc(dimLabel(r.dim))} (${round(x.sc(r.dim))})`).join(', ')}, giving the Holland-style code ${rc}. ${spread < 20 ? 'Your interests are fairly even across themes, so many kinds of work might hold your interest and fit will depend more on conditions.' : 'Your interests are clearly differentiated, which tends to make narrowing options easier.'}`)
  }
  if (alts.length) overview.push(`The paths below are ideas to explore, not recommendations. The best match here is ${alts[0].title} (${alts[0].matchScore}), and real roles vary widely within any title.`)
  return {
    alignmentScore: align,
    components,
    riasecCode: rc,
    riasec: RIASEC.filter((r) => x.s[r.dim].n > 0).map((r) => ({ dim: r.dim, label: dimLabel(r.dim), score: round(x.sc(r.dim)) })),
    environment: environmentText(x),
    conflicts: fire(x, CAREER_CONFLICTS, 4),
    alternatives: alts,
    overview,
  }
}

function values(x: Ctx): Report['values'] {
  const ids = dimsInGroup('values')
  const ranked = ids.filter((d) => x.s[d].n > 0).sort((a, b) => x.sc(b) - x.sc(a)).map((d) => ({ dim: d, label: dimLabel(d), score: round(x.sc(d)) }))
  let overview = 'There were not enough answers about values to rank them.'
  if (ranked.length >= 3) {
    const t = ranked.slice(0, 3).map((r) => lc(r.label))
    overview = `Your three most important values appear to be ${t[0]}, ${t[1]} and ${t[2]}. Values show up most clearly in what you protect when time or money is short, so it is worth asking whether your calendar and spending reflect them.`
  }
  return { ranked, conflicts: fire(x, VALUE_CONFLICTS, 4), overview }
}

function habits(x: Ctx): Report['habits'] {
  const ids: DimId[] = ['procrastination', 'consistency', 'followThrough', 'impulseControl', 'planning', 'goalClarity', 'timeManagement', 'avoidance', 'environmentObstacles', 'competingPriorities']
  const sc: Record<'motivation' | 'clarity' | 'environment' | 'priorities', number | null> = {
    motivation: x.avg(['procrastination', 'avoidance'], ['followThrough', 'consistency']),
    clarity: x.avg([], ['goalClarity', 'planning']),
    environment: x.avg(['environmentObstacles']),
    priorities: x.avg(['competingPriorities']),
  }
  const entries = (Object.entries(sc) as [keyof typeof sc, number | null][]).filter((e): e is [keyof typeof sc, number] => e[1] !== null).sort((a, b) => b[1] - a[1])
  const readings: Record<string, string> = {
    motivation: 'Your answers fit a motivation or avoidance pattern best: delaying, dropping routines and trailing off. This often means tasks feel aversive or pointless, or there is a fear underneath. Making the first step very small and tying it to something you care about tends to help more than pressure.',
    clarity: 'Your answers fit unclear goals best: goal clarity and planning are low, so effort may be scattered. Writing down what done looks like for one goal and a first step may help more than adding discipline.',
    environment: 'Your answers fit an environment pattern best: obstacles around you, such as interruptions, access or other people, seem to be getting in the way. Changing the setting often beats trying harder.',
    priorities: 'Your answers fit competing priorities best: too many demands are pulling at once, so something has to give. Choosing what to pause is likely to help more than better time management.',
  }
  let bottleneck: Report['habits']['bottleneck']
  if (!entries.length) bottleneck = { kind: 'none', reading: 'There were not enough habit answers to say what gets in the way of follow-through.' }
  else if (entries[0][1] < 52) bottleneck = { kind: 'none', reading: 'No single factor stands out as a bottleneck. Your habit answers suggest follow-through is not heavily blocked by motivation, unclear goals, environment or competing demands.' }
  else {
    const [k, v] = entries[0]
    const next = entries[1] && entries[1][1] >= 55 ? ` A second factor, ${entries[1][0] === 'motivation' ? 'motivation' : entries[1][0] === 'clarity' ? 'goal clarity' : entries[1][0]}, also seems present, so it may be a combination.` : ''
    bottleneck = { kind: k, reading: readings[k] + next + ` (Fit score ${round(v)}.)` }
  }
  const overview: string[] = [behavioral(x)]
  if (x.ok('masteryOrientation', 'performanceOrientation')) overview.push(x.sc('masteryOrientation') >= x.sc('performanceOrientation') + 10 ? 'You are more oriented toward learning than proving yourself, which tends to support persistence after setbacks.' : x.sc('performanceOrientation') >= x.sc('masteryOrientation') + 10 ? 'You are more oriented toward proving yourself than learning, which can raise pressure and make mistakes feel costlier.' : 'You balance learning and proving yourself.')
  return { overview, profile: prof(x, ids), bottleneck }
}

const DOMAINS: { name: string; cur: DimId; ideal: DimId }[] = [
  { name: 'Career', cur: 'curCareer', ideal: 'idealCareer' },
  { name: 'Relationships', cur: 'curRelationships', ideal: 'idealRelationships' },
  { name: 'Health', cur: 'curHealth', ideal: 'idealHealth' },
  { name: 'Finances', cur: 'curFinances', ideal: 'idealFinances' },
  { name: 'Growth', cur: 'curGrowth', ideal: 'idealGrowth' },
  { name: 'Leisure', cur: 'curLeisure', ideal: 'idealLeisure' },
]

function future(x: Ctx): { f: Report['future']; gaps: DomainGap[] } {
  const gaps: DomainGap[] = DOMAINS.filter((d) => x.s[d.cur].n > 0 && x.s[d.ideal].n > 0).map((d) => ({ domain: d.name, current: round(x.sc(d.cur)), ideal: round(x.sc(d.ideal)) }))
  const worst = [...gaps].sort((a, b) => b.ideal - b.current - (a.ideal - a.current))[0]
  const overview: string[] = []
  if (worst && worst.ideal - worst.current >= 15) {
    overview.push(`The largest gap between how much something matters and how it is going is ${worst.domain.toLowerCase()} (importance ${worst.ideal}, now ${worst.current}). Gaps are most useful as a place to start, since small changes there tend to feel biggest.`)
    const align = [...gaps].filter((g) => g.current >= g.ideal - 5 && g.ideal >= 60)
    if (align.length) overview.push(`In contrast, ${align.map((g) => g.domain.toLowerCase()).join(' and ')} ${align.length > 1 ? 'are' : 'is'} going about as well as it matters to you.`)
  } else if (gaps.length) overview.push('Most areas of life are going about as well as they matter to you, so the future question may be about what to add rather than what to fix.')
  else overview.push('There were not enough answers on how areas of life are going compared with how much they matter.')
  if (x.ok('futureClarity', 'futureOptimism')) overview.push(`You describe your future as ${x.band('futureClarity')} in clarity (${round(x.sc('futureClarity'))}) and ${x.band('futureOptimism')} in confidence (${round(x.sc('futureOptimism'))}).`)
  const trajectory = fire(x, TRAJECTORY, 4)
  if (worst && worst.ideal - worst.current >= 25) {
    const dom = DOMAINS.find((d) => d.name === worst.domain)!
    if (x.ok(dom.cur, dom.ideal)) trajectory.push({ title: `A large gap in ${worst.domain.toLowerCase()}`, body: `This area matters far more than it currently delivers. Without a change it may keep drawing down energy. Worth checking what the smallest real step would be.`, evidence: [dom.cur, dom.ideal] })
  }
  return { f: { overview, gaps, trajectory }, gaps }
}

// ---------------------------------------------------------------- roadmap

interface Cand {
  w: number
  t: string
  b: string
  ev: DimId[]
}
function pickCands(x: Ctx, cands: Cand[], n: number): Insight[] {
  return cands
    .filter((c) => c.ev.length > 0 && x.ok(...c.ev))
    .sort((a, b) => b.w - a.w)
    .slice(0, n)
    .map((c) => ({ title: c.t, body: c.b, evidence: c.ev }))
}
const pickText = (x: Ctx, cands: { w: number; t: string; ev: DimId[] }[], n: number): string[] =>
  cands
    .filter((c) => c.ev.length > 0 && x.ok(...c.ev))
    .sort((a, b) => b.w - a.w)
    .slice(0, n)
    .map((c) => c.t)

function roadmap(x: Ctx, hobbies: HobbyMatch[], alts: CareerMatch[], bn: Report['habits']['bottleneck'], gaps: DomainGap[], cons: Contradiction[], vals: Report['values']['ranked']): Report['roadmap'] {
  const hob = hobbies[0]
  const hob2 = hobbies[1]
  const hobEv = (h?: HobbyMatch): DimId[] => (h && h.motives.length ? h.motives.slice(0, 2) : [])
  const worst = [...gaps].sort((a, b) => b.ideal - b.current - (a.ideal - a.current))[0]
  const worstDom = worst ? DOMAINS.find((d) => d.name === worst.domain)! : undefined
  const bigGap = worst && worst.ideal - worst.current >= 15
  const gapEv: DimId[] = worstDom ? [worstDom.cur, worstDom.ideal] : []
  const valEv: DimId[] = vals.slice(0, 2).map((v) => v.dim)
  const riasecEv = [...RIASEC].sort((a, b) => x.sc(b.dim) - x.sc(a.dim)).slice(0, 2).map((r) => r.dim)
  const bnEv: Record<string, DimId[]> = {
    motivation: ['procrastination', 'avoidance'],
    clarity: ['goalClarity', 'planning'],
    environment: ['environmentObstacles'],
    priorities: ['competingPriorities'],
    none: [],
  }
  const bnShort: Record<string, [string, string]> = {
    motivation: ['Shrink one stalled task to ten minutes', 'Pick the task you have delayed longest and define a version you could do in ten minutes. Do only that today. Starting is usually the hard part.'],
    clarity: ['Write one page on what you want', 'Describe in plain words what a good year would look like, and pick one measurable goal from it. Clarity usually comes from writing, not waiting.'],
    environment: ['Remove one friction from your setting', 'Name the single biggest obstacle in your space or routine (a notification, a missing tool, a person) and change it this week.'],
    priorities: ['Pick one thing to pause', 'List what is competing for your time and choose one commitment to pause or drop for a month. Relief is information about what to protect.'],
  }
  const stressHi = x.ok('stress') && x.sc('stress') >= 62
  const lonely = x.ok('loneliness') && x.sc('loneliness') >= 62

  const short: Cand[] = [
    { w: 0.6, t: hob ? `Try ${lc(hob.name)} for two weeks` : '', b: hob ? `${hob.firstStep} This is a low-cost test of what your enjoyment answers suggest. Treat it as an experiment, and drop it freely if it does not fit.` : '', ev: hobEv(hob) },
    ...(bn.kind !== 'none' ? [{ w: 0.9, t: bnShort[bn.kind][0], b: bnShort[bn.kind][1], ev: bnEv[bn.kind] }] : []),
    { w: 0.75, t: worst ? `One small move in ${worst.domain.toLowerCase()}` : '', b: worst ? `${worst.domain} matters more than it is delivering right now. Choose one action that takes under an hour this month and book it in your calendar.` : '', ev: bigGap ? gapEv : [] },
    { w: 0.8, t: 'Add one real recovery block', b: 'Your stress answers are high. Schedule one weekly block, such as a long walk or an unplugged evening, that is not negotiable. If stress stays high, speaking to a professional is reasonable.', ev: stressHi ? ['stress'] : [] },
    { w: 0.78, t: 'Reach out to two people', b: 'Message two people you would like to see more of and propose a specific time. Small regular contact tends to help loneliness more than occasional big gestures.', ev: lonely ? ['loneliness'] : [] },
    { w: 0.5, t: cons[0] ? 'Sit with one of your tensions' : '', b: cons[0] ? `Spend 20 minutes writing on this question: ${cons[0].question}` : '', ev: cons[0] ? [cons[0].a.dim, cons[0].b.dim] : [] },
    { w: 0.3, t: 'Check your calendar against your top values', b: 'Write your top two values and compare them with how your last week was spent. The gap is where to adjust.', ev: valEv },
  ]
  const medium: Cand[] = [
    { w: 0.6, t: hob2 ? `Run a 12-week project around ${lc(hob2.name)}` : '', b: hob2 ? 'Commit to a small, regular slot each week and track one measurable thing. Twelve weeks is long enough to tell whether it holds your interest.' : '', ev: hobEv(hob2) },
    { w: 0.7, t: alts[0] ? 'Talk to people in adjacent paths' : '', b: alts[0] ? `Have three short conversations with people working as ${lc(alts[0].title)}${alts[1] ? ` or ${lc(alts[1].title)}` : ''}. Ask what a normal week is like and what they would tell their younger self. This tests the match against reality.` : '', ev: riasecEv },
    { w: 0.7, t: 'Build a confidence ladder', b: 'List five situations that feel slightly scary and arrange them from easiest to hardest. Do one a week. Confidence usually follows evidence of coping, not preparation.', ev: x.ok('fearOfJudgment', 'fearOfFailure') && (x.sc('fearOfJudgment') + x.sc('fearOfFailure')) / 2 >= 60 ? ['fearOfJudgment', 'fearOfFailure'] : [] },
    { w: 0.6, t: bn.kind !== 'none' ? 'Install a simple weekly review' : '', b: 'Spend 20 minutes each week listing what worked, what stalled and the one thing you will do next. It addresses the bottleneck in your habit answers over time.', ev: bn.kind !== 'none' ? bnEv[bn.kind] : [] },
    { w: 0.55, t: worst ? `Redesign ${worst.domain.toLowerCase()}` : '', b: worst ? `Set a six-month target for ${worst.domain.toLowerCase()} that would close half the gap you reported, with a first milestone you can reach in a month.` : '', ev: bigGap ? gapEv : [] },
    { w: 0.4, t: 'Retake this questionnaire', b: 'In three months, answer again and compare. Changes are more informative than any single reading.', ev: valEv },
  ]
  const long: Cand[] = [
    { w: 0.7, t: 'Design toward your best-fit environment', b: environmentText(x), ev: x.ok('workAutonomyNeed') ? ['workAutonomyNeed'] : x.ok('structurePreference') ? ['structurePreference'] : [] },
    { w: 0.65, t: alts[0] ? 'Run a low-cost trial before any big switch' : '', b: alts[0] ? `If ${lc(alts[0].title)} or a nearby path stays interesting, test it through a course, a side project or shadowing. This lowers the cost of being wrong.` : '', ev: riasecEv },
    { w: 0.6, t: 'Build a financial buffer', b: 'Your answers suggest security matters and risk is uncomfortable. A buffer of several months of expenses makes later experiments feel far safer.', ev: x.ok('securityNeed', 'riskTolerance') && x.sc('securityNeed') >= 55 && x.sc('riskTolerance') <= 45 ? ['securityNeed', 'riskTolerance'] : [] },
    { w: 0.55, t: 'Invest steadily in relationships', b: 'Relationships rank high for you. Protect recurring time with the few people who matter most, since long-run well-being research points strongly here.', ev: x.ok('companionshipNeed', 'relationshipQuality') && x.sc('companionshipNeed') >= 60 ? ['companionshipNeed', 'relationshipQuality'] : [] },
    { w: 0.5, t: 'Make a creative practice permanent', b: 'Your creative pull is strong. A fixed weekly practice, with a shareable result each season, tends to matter more than occasional bursts.', ev: x.ok('creativity') && x.sc('creativity') >= 62 ? ['creativity'] : [] },
    { w: 0.4, t: 'Review your direction against your values yearly', b: 'Once a year, rank your values again and ask whether your path is moving toward or away from them.', ev: valEv },
  ]

  const exp: { w: number; t: string; ev: DimId[] }[] = [
    { w: 0.6, t: hob ? `${cap(lc(hob.name))}: ${hob.firstStep}` : '', ev: hobEv(hob) },
    { w: 0.5, t: alts[0] ? `Spend a day shadowing someone working as ${lc(alts[0].title)}.` : '', ev: riasecEv },
    { w: 0.55, t: 'Take a short trip with one unplanned day.', ev: x.ok('adventure') && x.sc('adventure') >= 60 ? ['adventure'] : [] },
    { w: 0.55, t: 'Volunteer once for something you care about, and notice how it feels afterwards.', ev: x.ok('contribution') && x.sc('contribution') >= 60 ? ['contribution'] : [] },
    { w: 0.5, t: 'Attend one small interest-based meetup instead of a large event.', ev: x.ok('extraversion') && x.sc('extraversion') <= 42 ? ['extraversion'] : [] },
    { w: 0.5, t: 'Host a small gathering around a shared activity.', ev: x.ok('motSocial') && x.sc('motSocial') >= 62 ? ['motSocial'] : [] },
    { w: 0.5, t: 'Take a one-off workshop in a medium you have never tried.', ev: x.ok('creativity') && x.sc('creativity') >= 60 ? ['creativity'] : [] },
    { w: 0.45, t: 'Attend a talk or course far outside your field.', ev: x.ok('curiosity') && x.sc('curiosity') >= 60 ? ['curiosity'] : [] },
    { w: 0.4, t: 'Try one new routine for a week, such as a different commute or a new weekly ritual.', ev: x.ok('stabilityNeed') && x.sc('stabilityNeed') >= 62 ? ['stabilityNeed'] : [] },
    { w: 0.3, t: 'Spend a morning somewhere you have never been in your own city.', ev: x.ok('openness') ? ['openness'] : [] },
  ]
  const hab: { w: number; t: string; ev: DimId[] }[] = [
    { w: 0.8, t: 'Two-minute start: begin any avoided task for just two minutes.', ev: x.ok('procrastination') && x.sc('procrastination') >= 58 ? ['procrastination'] : [] },
    { w: 0.7, t: 'Weekly 20 minute review of what worked, what stalled and next steps.', ev: x.ok('planning') && x.sc('planning') <= 50 ? ['planning'] : x.ok('goalClarity') ? ['goalClarity'] : [] },
    { w: 0.7, t: 'One planned no per week, practised in a low-stakes situation.', ev: x.ok('boundaries') && x.sc('boundaries') <= 45 ? ['boundaries'] : [] },
    { w: 0.6, t: 'Fixed wake time and a short wind-down, to protect sleep when stress is high.', ev: stressHi ? ['stress'] : [] },
    { w: 0.55, t: 'Three-line gratitude note each evening.', ev: x.ok('gratitude') && x.sc('gratitude') <= 50 ? ['gratitude'] : [] },
    { w: 0.55, t: 'Phone out of reach during your first focus block of the day.', ev: x.ok('impulseControl') && x.sc('impulseControl') <= 45 ? ['impulseControl'] : [] },
    { w: 0.5, t: 'A short daily movement block, such as a 20 minute walk.', ev: x.ok('curHealth') && x.sc('curHealth') <= 50 ? ['curHealth'] : [] },
    { w: 0.5, t: 'Keep a done list for a week to see progress that otherwise goes unnoticed.', ev: x.ok('accomplishment') && x.sc('accomplishment') <= 50 ? ['accomplishment'] : [] },
    { w: 0.4, t: 'Time-box work blocks and finish one thing before starting another.', ev: x.ok('followThrough') && x.sc('followThrough') <= 55 ? ['followThrough'] : [] },
    { w: 0.3, t: 'A short weekly check-in with someone you trust.', ev: x.ok('relationshipQuality') ? ['relationshipQuality'] : [] },
  ]
  const refl: { w: number; t: string; ev: DimId[] }[] = [
    ...cons.slice(0, 3).map((c, i) => ({ w: 0.9 - i * 0.1, t: c.question, ev: [c.a.dim, c.b.dim] as DimId[] })),
    { w: 0.5, t: 'What would you try this year if you knew you could not fail?', ev: x.ok('fearOfFailure') && x.sc('fearOfFailure') >= 55 ? ['fearOfFailure'] : [] },
    { w: 0.5, t: 'Which of your goals would you keep if nobody ever knew about them?', ev: x.ok('extrinsicMotivation') && x.sc('extrinsicMotivation') >= 55 ? ['extrinsicMotivation'] : [] },
    { w: 0.45, t: 'Whose approval are you still working for?', ev: x.ok('validationNeed') && x.sc('validationNeed') >= 55 ? ['validationNeed'] : [] },
    { w: 0.45, t: 'If your life five years from now looked exactly like today, how would you feel?', ev: x.ok('trajectoryFit') ? ['trajectoryFit'] : [] },
    { w: 0.4, t: 'What gives you energy, and what quietly drains it?', ev: x.ok('engagement') ? ['engagement'] : [] },
    { w: 0.4, t: 'What would you regret not trying?', ev: x.ok('regretRisk') ? ['regretRisk'] : [] },
  ]
  const sk: { w: number; t: string; ev: DimId[] }[] = [
    { w: 0.8, t: 'Naming needs and limits in one clear sentence.', ev: x.ok('communication') && x.sc('communication') <= 45 ? ['communication'] : x.ok('boundaries') && x.sc('boundaries') <= 45 ? ['boundaries'] : [] },
    { w: 0.75, t: 'Time-boxing and weekly planning.', ev: x.ok('timeManagement') && x.sc('timeManagement') <= 45 ? ['timeManagement'] : [] },
    { w: 0.75, t: 'Asking and negotiating, from small requests to pay and scope.', ev: x.ok('selfAdvocacy') && x.sc('selfAdvocacy') <= 45 ? ['selfAdvocacy'] : [] },
    { w: 0.6, t: 'Data literacy: reading charts, basic statistics and spreadsheets.', ev: x.ok('investigative') && x.sc('investigative') >= 60 ? ['investigative'] : [] },
    { w: 0.6, t: 'A craft skill in one medium, such as drawing, writing or music.', ev: x.ok('artistic') && x.sc('artistic') >= 60 ? ['artistic'] : [] },
    { w: 0.6, t: 'Persuasive writing and pitching ideas.', ev: x.ok('enterprising') && x.sc('enterprising') >= 60 ? ['enterprising'] : [] },
    { w: 0.6, t: 'Active listening and facilitation.', ev: x.ok('social') && x.sc('social') >= 60 ? ['social'] : [] },
    { w: 0.55, t: 'Hands-on repair or building with basic tools.', ev: x.ok('realistic') && x.sc('realistic') >= 60 ? ['realistic'] : [] },
    { w: 0.55, t: 'Personal finance basics: budgeting, buffers and investing fundamentals.', ev: x.ok('financialDrive') && x.sc('financialDrive') >= 55 ? ['financialDrive'] : x.ok('curFinances') && x.sc('curFinances') <= 50 ? ['curFinances'] : [] },
    { w: 0.5, t: 'Giving and receiving feedback without defensiveness.', ev: x.ok('fearOfJudgment') && x.sc('fearOfJudgment') >= 55 ? ['fearOfJudgment'] : [] },
    { w: 0.45, t: 'Systems for organising and tracking work: spreadsheets, checklists, simple tools.', ev: x.ok('conventional') && x.sc('conventional') >= 60 ? ['conventional'] : [] },
    { w: 0.3, t: 'Learning how to learn: spaced repetition and deliberate practice.', ev: x.ok('growthDrive') ? ['growthDrive'] : [] },
  ]
  return {
    short: pickCands(x, short, 3),
    medium: pickCands(x, medium, 3),
    long: pickCands(x, long, 3),
    experiences: pickText(x, exp, 5),
    habits: pickText(x, hab, 5),
    reflections: pickText(x, refl, 5),
    skills: pickText(x, sk, 5),
  }
}

// ---------------------------------------------------------------- population

interface SwlsBand {
  min: number
  max: number
  label: string
}
const SWLS_CITATION = 'Pavot, W., & Diener, E. (2008). The Satisfaction With Life Scale and the emerging construct of life satisfaction. The Journal of Positive Psychology, 3(2), 137-152.'
const DEFAULT_SWLS: SwlsBand[] = [
  { min: 31, max: 35, label: 'Extremely satisfied' },
  { min: 26, max: 30, label: 'Satisfied' },
  { min: 21, max: 25, label: 'Slightly satisfied' },
  { min: 20, max: 20, label: 'Neutral' },
  { min: 15, max: 19, label: 'Slightly dissatisfied' },
  { min: 10, max: 14, label: 'Dissatisfied' },
  { min: 5, max: 9, label: 'Extremely dissatisfied' },
]

interface NormEntry {
  id?: string
  kind?: string
  instrument?: string
  source?: string
  citation?: string
  sampleSize?: number | null
  population?: string
  limitations?: string
  bands?: SwlsBand[]
  statistics?: { mean?: number; sd?: number }
}

function normEntries(): NormEntry[] {
  try {
    const n = normsRaw as unknown as { norms?: unknown }
    return Array.isArray(n?.norms) ? (n.norms as NormEntry[]) : []
  } catch {
    return []
  }
}

const validBands = (b: unknown): b is SwlsBand[] =>
  Array.isArray(b) && b.length >= 3 && b.every((x) => x && typeof x.min === 'number' && typeof x.max === 'number' && typeof x.label === 'string')

function population(questions: Question[], answers: Answers): PopulationComparison[] {
  const out: PopulationComparison[] = []
  const r = rawSum(questions, answers, 'swls')
  const items = questions.filter((q) => q.instrument === 'swls')
  const sevenPoint = items.length > 0 && items.every((q) => q.type === 'likert' && (q.points ?? 5) === 7)
  if (!(r.of === 5 && r.n === 5 && sevenPoint)) return out
  const entries = normEntries()
  const bandsEntry = entries.find((e) => e.kind === 'interpretive-bands' && /satisfaction with life/i.test(e.instrument ?? '') && validBands(e.bands))
  const bands = bandsEntry ? (bandsEntry.bands as SwlsBand[]) : DEFAULT_SWLS
  const b = bands.find((x) => r.sum >= x.min && r.sum <= x.max)
  const instrument = 'Satisfaction With Life Scale (Diener, Emmons, Larsen & Griffin, 1985), 5 items, 7-point agreement.'
  if (b) {
    out.push({
      id: 'swls',
      label: 'Satisfaction with life',
      userValue: `${r.sum} out of 35`,
      band: `${b.label} (${b.min === b.max ? b.min : `${b.min}-${b.max}`})`,
      source: bandsEntry?.citation ?? SWLS_CITATION,
      sample: 'Descriptive score bands published with the scale. They are labels for ranges of the 5-35 total, not percentile ranks from a particular sample.',
      population: bandsEntry?.population ?? 'Interpretive guidance, not a population average.',
      instrument,
      limitations: bandsEntry?.limitations ?? 'Self-report, and a single day or mood can shift it. Bands are descriptive, not diagnostic.',
    })
  }
  const samp = entries.find((e) => e.kind === 'sample-statistic' && /satisfaction with life/i.test(e.instrument ?? '') && typeof e.statistics?.mean === 'number' && typeof e.statistics?.sd === 'number')
  if (samp && samp.statistics) {
    const mean = samp.statistics.mean!
    const sd = samp.statistics.sd!
    const diff = r.sum - mean
    const z = diff / sd
    out.push({
      id: 'swls-original-sample',
      label: 'Compared with the original SWLS validation sample',
      userValue: `${r.sum} out of 35`,
      band: Math.abs(z) < 0.25 ? `About the same as the sample mean of ${mean}` : `${Math.abs(diff).toFixed(1)} points ${diff > 0 ? 'above' : 'below'} the sample mean of ${mean} (about ${Math.abs(z).toFixed(1)} standard deviations)`,
      source: samp.citation ?? samp.source ?? SWLS_CITATION,
      sample: `${samp.sampleSize ?? 'A small'} participants. Mean ${mean}, SD ${sd}.`,
      population: samp.population ?? 'A single validation sample.',
      instrument,
      limitations: samp.limitations ?? 'One non-representative sample. Use only as a rough reference point.',
    })
  }
  return out
}

// ---------------------------------------------------------------- main

export function buildReport(questions: Question[], answers: Answers, timings: Record<string, number> = {}): Report {
  const { scores, quality } = scoreAll(questions, answers, timings)
  const x = makeCtx(scores)

  const ts = typeSketch(x)
  const cons = contradictions(x)
  const hobbies = matchHobbies(x)
  const alts = matchCareers(x)
  const amb = ambitions(x, cons)
  const hap = happiness(x)
  const conf = confidence(x)
  const rel = relationships(x)
  const car = career(x, alts)
  const val = values(x)
  const hab = habits(x)
  const fut = future(x)
  const head = headline(x)
  const strengths = fire(x, STRENGTHS, 5)
  const blind = fire(x, BLIND, 4)
  const hidden = fire(x, HIDDEN, 5)
  const motives = MOTIVE_DIMS.filter((d) => scores[d].n > 0)
    .sort((a, b) => scores[b].score - scores[a].score)
    .map((d) => ({ dim: d as DimId, label: dimLabel(d), score: round(scores[d].score) }))

  const rm = roadmap(x, hobbies, alts, hab.bottleneck, fut.gaps, cons, val.ranked)

  const summary: string[] = []
  summary.push(para(`${head}.`, strengths[0] ? `A pattern worth building on is ${lc(strengths[0].title)}.` : '', blind[0] ? `A possible blind spot to keep an eye on is ${lc(blind[0].title)}.` : ''))
  summary.push(
    para(
      cons[0] ? `The most interesting tension in your answers is ${lc(cons[0].title)}. ${cons[0].question}` : 'No strong tensions stood out in your answers, which may mean your priorities are fairly well aligned or that more answers are needed.',
      hab.bottleneck.kind !== 'none' ? `On follow-through, the best fit was ${hab.bottleneck.kind === 'motivation' ? 'a motivation or avoidance pattern' : hab.bottleneck.kind === 'clarity' ? 'unclear goals' : hab.bottleneck.kind === 'environment' ? 'environmental obstacles' : 'competing priorities'}.` : '',
    ),
  )
  summary.push(
    para(
      hobbies[0] && hobbies[0].motives.length ? `A low-cost place to start is ${lc(hobbies[0].name)}.` : '',
      alts[0] ? `For career ideas, ${lc(alts[0].title)} came out as the closest overlap, as one of several to explore.` : '',
      'Everything here is a hypothesis built from your own answers, so keep what rings true, challenge what does not, and retake this in a few months to see what moves.',
    ),
  )

  return {
    generatedAt: new Date().toISOString(),
    scores,
    quality,
    identity: {
      headline: head,
      overview: identityOverview(x, quality, ts),
      coreTraits: coreTraits(x),
      behavioral: behavioral(x),
      social: social(x),
      decisionStyle: decisionStyle(x),
      strengths,
      blindSpots: blind,
    },
    typeSketch: ts,
    ambitions: amb,
    happiness: hap,
    confidence: conf,
    relationships: rel,
    career: car,
    values: val,
    habits: hab,
    future: fut.f,
    hobbies,
    motives,
    contradictions: cons,
    hiddenPotential: hidden,
    roadmap: rm,
    summary,
    population: population(questions, answers),
  }
}
