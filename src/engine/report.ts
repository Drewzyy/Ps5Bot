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
