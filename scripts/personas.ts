// Persona harness: generates synthetic answers from question metadata, runs the
// interpretation engine and checks that profiles are differentiated and that
// scoring edge cases behave. Run with: npm run personas

import { QUESTIONS } from '../src/data/questions'
import { buildReport } from '../src/engine/report'
import { scoreAll } from '../src/engine/scoring'
import { dimLabel, type DimId } from '../src/engine/dimensions'
import type { Answers, Question, Report } from '../src/engine/types'

declare const process: { exit(code: number): never }

type Targets = Partial<Record<DimId, number>> // 0..1 desired level per dimension

// small deterministic RNG so runs are repeatable
function rng(seed: number) {
  let s = seed >>> 0
  return () => {
    s = (s * 1664525 + 1013904223) >>> 0
    return s / 4294967296
  }
}

function answerFor(q: Question, t: Targets, rand: () => number, noise = 0.07): Answers[string] | undefined {
  const tg = (d: DimId) => t[d]
  const jitter = () => (rand() - 0.5) * 2 * noise
  if (q.type === 'text') return undefined
  if (q.type === 'likert' || q.type === 'scale10') {
    if (!q.dims) return undefined
    let num = 0
    let den = 0
    let any = false
    for (const [d, w] of Object.entries(q.dims) as [DimId, number][]) {
      const target = tg(d)
      if (target !== undefined) any = true
      const lv = target ?? 0.5
      num += Math.abs(w) * (w < 0 ? 1 - lv : lv)
      den += Math.abs(w)
    }
    const level = Math.min(1, Math.max(0, (den ? num / den : 0.5) + (any ? jitter() : jitter() * 4)))
    const max = q.type === 'scale10' ? 10 : q.points ?? 5
    return 1 + Math.round(level * (max - 1))
  }
  const opts = q.options ?? []
  if (!opts.length) return undefined
  const fit = (o: (typeof opts)[number]) => {
    let s = 0
    for (const [d, lvl] of Object.entries(o.effects ?? {}) as [DimId, number][]) {
      const target = tg(d)
      s -= target !== undefined ? Math.abs(lvl - target) : Math.abs(lvl - 0.5) * 0.3
    }
    return s + (rand() - 0.5) * 0.15
  }
  if (q.type === 'choice' || q.type === 'forced') return [...opts].sort((a, b) => fit(b) - fit(a))[0].id
  if (q.type === 'multi') {
    const ranked = [...opts].sort((a, b) => fit(b) - fit(a))
    const n = Math.max(1, Math.min(q.maxSelect ?? 3, 3))
    return ranked.slice(0, n).map((o) => o.id)
  }
  if (q.type === 'ranking') {
    const val = (o: (typeof opts)[number]) => {
      let s = 0
      for (const [d, w] of Object.entries(o.effects ?? {}) as [DimId, number][]) {
        const target = tg(d)
        s += w * ((target ?? 0.5) - 0.5)
      }
      return s + (rand() - 0.5) * 0.05
    }
    return [...opts].sort((a, b) => val(b) - val(a)).map((o) => o.id)
  }
  return undefined
}

export function generateAnswers(questions: Question[], t: Targets, seed = 1): Answers {
  const rand = rng(seed)
  const out: Answers = {}
  for (const q of questions) {
    if (q.probeFor) continue
    const a = answerFor(q, t, rand)
    if (a !== undefined) out[q.id] = a
  }
  return out
}

const PERSONAS: { name: string; targets: Targets }[] = [
  {
    name: 'Achiever',
    targets: {
      conscientiousness: 0.88, extraversion: 0.62, emotionalStability: 0.55, openness: 0.55, agreeableness: 0.4, riskTolerance: 0.6, structurePreference: 0.7, assertiveness: 0.85,
      achievement: 0.95, financialDrive: 0.8, recognitionNeed: 0.75, influence: 0.8, stabilityNeed: 0.5, growthDrive: 0.7,
      extrinsicMotivation: 0.75, intrinsicMotivation: 0.45, performanceOrientation: 0.85, masteryOrientation: 0.6,
      motCompetition: 0.9, motProgress: 0.92, motMastery: 0.75, motRecognition: 0.7, motPhysical: 0.65, motSocial: 0.45, motCreativity: 0.3, motNature: 0.3, motRelaxation: 0.25,
      realistic: 0.35, investigative: 0.55, artistic: 0.2, social: 0.4, enterprising: 0.95, conventional: 0.65,
      workAutonomyNeed: 0.6, prestigeNeed: 0.85, securityNeed: 0.45, leadershipDrive: 0.9, helpingDrive: 0.3, workloadTolerance: 0.9, balancePriority: 0.15, incomeExpectation: 0.9,
      goalClarity: 0.9, planning: 0.85, timeManagement: 0.8, consistency: 0.85, followThrough: 0.9, procrastination: 0.15, impulseControl: 0.75,
      careerClarity: 0.85, careerFit: 0.7, careerSatisfaction: 0.65, professionalConfidence: 0.8, selfEfficacy: 0.8, selfEsteem: 0.6, fearOfFailure: 0.65, perfectionism: 0.8,
      stress: 0.75, workLifeBalance: 0.25, lifeSatisfaction: 0.55, curCareer: 0.7, idealCareer: 0.95, curHealth: 0.4, idealHealth: 0.65, curRelationships: 0.4, idealRelationships: 0.6, curFinances: 0.6, idealFinances: 0.85,
      companionshipNeed: 0.4, relationalIndependence: 0.7, valAchievement: 0.95, valWealth: 0.75, valRecognition: 0.7, valFamily: 0.4, valFreedom: 0.5,
    },
  },
  {
    name: 'Creative free spirit',
    targets: {
      openness: 0.96, creativity: 0.96, curiosity: 0.9, structurePreference: 0.08, conscientiousness: 0.3, impulsivity: 0.7, adaptability: 0.85, riskTolerance: 0.75, extraversion: 0.5, agreeableness: 0.6, emotionalStability: 0.45, assertiveness: 0.5,
      autonomy: 0.9, freedomNeed: 0.95, adventure: 0.85, stabilityNeed: 0.12, achievement: 0.4, financialDrive: 0.2, recognitionNeed: 0.45, growthDrive: 0.8, intellectualDrive: 0.65, societalExpectation: 0.1,
      intrinsicMotivation: 0.92, extrinsicMotivation: 0.15, masteryOrientation: 0.8, performanceOrientation: 0.25,
      motCreativity: 0.97, motEscapism: 0.65, motNature: 0.65, motRelaxation: 0.5, motMastery: 0.55, motCompetition: 0.1, motRecognition: 0.35, motSocial: 0.4, motProgress: 0.3, motPhysical: 0.4, motCollecting: 0.3,
      realistic: 0.3, investigative: 0.55, artistic: 0.97, social: 0.5, enterprising: 0.4, conventional: 0.05,
      workAutonomyNeed: 0.97, prestigeNeed: 0.15, securityNeed: 0.12, leadershipDrive: 0.3, helpingDrive: 0.45, workloadTolerance: 0.45, balancePriority: 0.7, incomeExpectation: 0.25,
      goalClarity: 0.3, planning: 0.15, timeManagement: 0.25, consistency: 0.25, followThrough: 0.35, procrastination: 0.75, impulseControl: 0.3,
      careerClarity: 0.35, careerFit: 0.4, selfEsteem: 0.55, selfEfficacy: 0.6, professionalConfidence: 0.4, fearOfJudgment: 0.6, perfectionism: 0.5, validationNeed: 0.4,
      valFreedom: 0.97, valCreativity: 0.97, valIndependence: 0.92, valSecurity: 0.1, valWealth: 0.15, valKnowledge: 0.7,
      companionshipNeed: 0.5, relationalIndependence: 0.75, workLifeBalance: 0.6, stress: 0.5, lifeSatisfaction: 0.6, curLeisure: 0.7, idealLeisure: 0.8, curFinances: 0.3, idealFinances: 0.5,
    },
  },
  {
    name: 'Relationship-oriented',
    targets: {
      agreeableness: 0.92, extraversion: 0.72, empathy: 0.93, emotionalStability: 0.6, conscientiousness: 0.6, openness: 0.55, assertiveness: 0.35, riskTolerance: 0.3, structurePreference: 0.5,
      companionshipNeed: 0.95, relatedness: 0.95, relationalIndependence: 0.2, intimacyComfort: 0.88, trust: 0.85, attachmentAvoidance: 0.12, attachmentAnxiety: 0.45, communication: 0.85, boundaries: 0.35, conflictApproach: 0.45, feelingUnderstood: 0.85,
      relationshipQuality: 0.9, loneliness: 0.15, meaning: 0.8, positiveEmotion: 0.75, gratitude: 0.85, optimism: 0.7, lifeSatisfaction: 0.75,
      familyOrientation: 0.95, contribution: 0.85, achievement: 0.35, financialDrive: 0.3, influence: 0.3, recognitionNeed: 0.4, stabilityNeed: 0.65, freedomNeed: 0.4, adventure: 0.35, growthDrive: 0.6,
      motSocial: 0.97, motCompetition: 0.15, motRelaxation: 0.65, motCreativity: 0.5, motMastery: 0.35, motProgress: 0.3, motPhysical: 0.4, motRecognition: 0.3, motNature: 0.5,
      realistic: 0.25, investigative: 0.35, artistic: 0.5, social: 0.97, enterprising: 0.3, conventional: 0.55,
      helpingDrive: 0.95, balancePriority: 0.9, workloadTolerance: 0.35, incomeExpectation: 0.35, prestigeNeed: 0.2, leadershipDrive: 0.35, securityNeed: 0.6, workAutonomyNeed: 0.4, workLifeBalance: 0.8, stress: 0.35,
      valFamily: 0.97, valCompassion: 0.93, valCommunity: 0.9, valLoyalty: 0.9, valAchievement: 0.3, valWealth: 0.25, valHonesty: 0.8,
      selfEsteem: 0.6, socialConfidence: 0.75, validationNeed: 0.6, fearOfRejection: 0.6, selfAdvocacy: 0.35,
      curRelationships: 0.85, idealRelationships: 0.97, curCareer: 0.5, idealCareer: 0.5, curHealth: 0.55, idealHealth: 0.7,
      goalClarity: 0.5, careerClarity: 0.5,
    },
  },
  {
    name: 'Security-focused',
    targets: {
      conscientiousness: 0.85, extraversion: 0.35, emotionalStability: 0.5, openness: 0.25, agreeableness: 0.6, riskTolerance: 0.08, impulsivity: 0.08, adaptability: 0.3, structurePreference: 0.95, assertiveness: 0.35, curiosity: 0.35, creativity: 0.25,
      securityNeed: 0.97, stabilityNeed: 0.97, freedomNeed: 0.3, adventure: 0.08, achievement: 0.55, financialDrive: 0.65, recognitionNeed: 0.3, influence: 0.3, familyOrientation: 0.7, growthDrive: 0.4, societalExpectation: 0.65,
      motCollecting: 0.85, motRelaxation: 0.75, motProblemSolving: 0.6, motCreativity: 0.2, motCompetition: 0.2, motSocial: 0.35, motPhysical: 0.25, motEscapism: 0.4, motProgress: 0.5, motMastery: 0.5, motNature: 0.45, motRecognition: 0.15,
      realistic: 0.7, investigative: 0.45, artistic: 0.1, social: 0.35, enterprising: 0.25, conventional: 0.97,
      workAutonomyNeed: 0.25, prestigeNeed: 0.4, leadershipDrive: 0.25, helpingDrive: 0.4, workloadTolerance: 0.5, balancePriority: 0.65, incomeExpectation: 0.6,
      planning: 0.9, timeManagement: 0.85, consistency: 0.9, followThrough: 0.85, procrastination: 0.15, impulseControl: 0.92, goalClarity: 0.7,
      fearOfFailure: 0.75, fearOfJudgment: 0.6, perfectionism: 0.6, selfEfficacy: 0.55, selfEsteem: 0.5, socialConfidence: 0.4,
      valSecurity: 0.97, valResponsibility: 0.92, valLoyalty: 0.8, valFamily: 0.75, valFreedom: 0.25, valCreativity: 0.15, valIndependence: 0.3, valHonesty: 0.8,
      curFinances: 0.5, idealFinances: 0.92, curCareer: 0.6, idealCareer: 0.7, curHealth: 0.55, idealHealth: 0.75, stress: 0.6, workLifeBalance: 0.55, lifeSatisfaction: 0.55,
      companionshipNeed: 0.55, relationalIndependence: 0.5, careerFit: 0.55,
    },
  },
  {
    name: 'Uncertain explorer',
    targets: {
      curiosity: 0.88, openness: 0.82, conscientiousness: 0.4, extraversion: 0.45, agreeableness: 0.55, emotionalStability: 0.38, adaptability: 0.65, riskTolerance: 0.5, structurePreference: 0.4, creativity: 0.6, impulsivity: 0.55,
      goalClarity: 0.08, careerClarity: 0.06, futureClarity: 0.1, careerSatisfaction: 0.25, careerFit: 0.25, trajectoryFit: 0.2, futureOptimism: 0.5, regretRisk: 0.82,
      selfEfficacy: 0.4, selfEsteem: 0.4, professionalConfidence: 0.3, fearOfFailure: 0.7, avoidance: 0.65, procrastination: 0.65, followThrough: 0.35, consistency: 0.35, planning: 0.2,
      competingPriorities: 0.6, environmentObstacles: 0.4, meaning: 0.3, engagement: 0.35, positiveEmotion: 0.4, lifeSatisfaction: 0.35, stress: 0.65, recentMood: 0.4, optimism: 0.45,
      investigative: 0.72, artistic: 0.6, social: 0.5, enterprising: 0.3, conventional: 0.2, realistic: 0.3,
      motEscapism: 0.65, motProblemSolving: 0.6, motMastery: 0.45, motCompetition: 0.2, motProgress: 0.3, motSocial: 0.4, motCreativity: 0.6, motNature: 0.55, motRelaxation: 0.5,
      achievement: 0.5, financialDrive: 0.35, growthDrive: 0.85, intellectualDrive: 0.8, adventure: 0.65, stabilityNeed: 0.4, freedomNeed: 0.65, contribution: 0.5,
      workAutonomyNeed: 0.6, securityNeed: 0.45, prestigeNeed: 0.3, leadershipDrive: 0.35, helpingDrive: 0.5, workloadTolerance: 0.45, balancePriority: 0.6, incomeExpectation: 0.45,
      valKnowledge: 0.8, valFreedom: 0.7, valCreativity: 0.65, valSecurity: 0.35, valAchievement: 0.4,
      curCareer: 0.3, idealCareer: 0.8, curGrowth: 0.3, idealGrowth: 0.9, curLeisure: 0.4, idealLeisure: 0.6, curHealth: 0.5, idealHealth: 0.6, curRelationships: 0.5, idealRelationships: 0.6, curFinances: 0.35, idealFinances: 0.6,
      companionshipNeed: 0.5, relationalIndependence: 0.5,
    },
  },
]

// ---------------------------------------------------------------- run

let failures = 0
const check = (ok: boolean, msg: string) => {
  console.log(`${ok ? 'PASS' : 'FAIL'}  ${msg}`)
  if (!ok) failures++
}
const sep = '-'.repeat(72)

const reports: { name: string; r: Report }[] = []
for (const [i, p] of PERSONAS.entries()) {
  const answers = generateAnswers(QUESTIONS, p.targets, 101 + i)
  const r = buildReport(QUESTIONS, answers)
  reports.push({ name: p.name, r })
  const bf = (['extraversion', 'agreeableness', 'conscientiousness', 'emotionalStability', 'openness'] as DimId[])
    .map((d) => [d, r.scores[d].score] as [DimId, number])
    .sort((a, b) => b[1] - a[1])
    .slice(0, 3)
    .map(([d, s]) => `${dimLabel(d)} ${Math.round(s)}`)
    .join(', ')
  console.log(sep)
  console.log(`${p.name.toUpperCase()}  (${Object.keys(answers).length} answers)`)
  console.log(`  Headline:      ${r.identity.headline}`)
  console.log(`  Big Five top:  ${bf}`)
  console.log(`  Type sketch:   ${r.typeSketch.code}`)
  console.log(`  Top drivers:   ${r.ambitions.drivers.slice(0, 3).map((d) => `${d.label} ${d.score}`).join(', ')}`)
  console.log(`  RIASEC:        ${r.career.riasecCode}   alignment ${r.career.alignmentScore}   happiness ${r.happiness.overall}`)
  console.log(`  Top hobbies:   ${r.hobbies.slice(0, 3).map((h) => `${h.name} (${h.matchScore})`).join('; ')}`)
  console.log(`  Top careers:   ${r.career.alternatives.slice(0, 3).map((c) => `${c.title} (${c.matchScore})`).join('; ')}`)
  console.log(`  Contradictions:${r.contradictions.slice(0, 3).map((c) => ` ${c.title} [${c.strength}]`).join(' |') || ' none'}`)
  console.log(`  Bottleneck:    ${r.habits.bottleneck.kind}`)
  console.log(`  Population:    ${r.population.map((x) => `${x.label}: ${x.band}`).join(' | ') || 'none'}`)
  console.log(`  Roadmap items: short ${r.roadmap.short.length}, medium ${r.roadmap.medium.length}, long ${r.roadmap.long.length}; strengths ${r.identity.strengths.length}, blind spots ${r.identity.blindSpots.length}`)
  console.log(`  Confidence:    ${r.quality.overallConfidence.toFixed(2)}  flags: ${r.quality.flags.map((f) => f.id).join(', ') || 'none'}`)
}

console.log(sep)
console.log('CHECKS')
const uniq = (xs: string[]) => new Set(xs).size
check(uniq(reports.map((x) => x.r.identity.headline)) === reports.length, 'every persona has a distinct headline')
check(uniq(reports.map((x) => x.r.career.riasecCode)) >= 4, `at least 4 distinct RIASEC codes (${reports.map((x) => x.r.career.riasecCode).join(', ')})`)
check(uniq(reports.map((x) => x.r.hobbies[0].id)) >= 4, `at least 4 distinct top hobbies (${reports.map((x) => x.r.hobbies[0].id).join(', ')})`)
check(uniq(reports.map((x) => x.r.career.alternatives[0].id)) >= 4, 'at least 4 distinct top careers')
check(uniq(reports.map((x) => x.r.typeSketch.code)) >= 3, `at least 3 distinct type sketch codes (${reports.map((x) => x.r.typeSketch.code).join(', ')})`)
check(reports.every((x) => x.r.hobbies.length >= 12 && x.r.career.alternatives.length === 6), 'each report has >=12 hobbies and 6 career alternatives')
check(reports.every((x) => x.r.contradictions.length >= 0 && x.r.roadmap.short.length === 3 && x.r.roadmap.medium.length === 3 && x.r.roadmap.long.length === 3), 'roadmap has 3 items per horizon')
check(reports.every((x) => x.r.quality.overallConfidence > 0.4), 'full-answer personas have overall confidence above 0.4')
check(reports.every((x) => x.r.population.length >= 1), 'population comparison present for SWLS when all items answered')
const allInsights = (r: Report): { evidence: DimId[] }[] => [
  ...r.identity.coreTraits, ...r.identity.strengths, ...r.identity.blindSpots, ...r.ambitions.statedVsRevealed, ...r.ambitions.conflicts, ...r.ambitions.overlooked,
  ...r.happiness.lifts, ...r.happiness.drags, ...r.confidence.hypotheses, ...r.relationships.strengths, ...r.relationships.needs, ...r.relationships.growth,
  ...r.career.conflicts, ...r.values.conflicts, ...r.future.trajectory, ...r.hiddenPotential, ...r.roadmap.short, ...r.roadmap.medium, ...r.roadmap.long,
]
check(
  reports.every((x) => allInsights(x.r).every((i) => i.evidence.length > 0 && i.evidence.every((d) => x.r.scores[d].n > 0 && x.r.scores[d].confidence >= 0.35))),
  'every insight has evidence dims that are measured with confidence >= 0.35',
)
const textBlob = JSON.stringify(reports.map((x) => x.r))
check(!/—|–/.test(textBlob), 'no em or en dashes in generated text')

// reverse scoring: toggling a reverse-keyed item moves the score the right way
{
  const q = QUESTIONS.find((q) => q.type === 'likert' && q.dims && Object.values(q.dims).some((w) => w < 0) && Object.keys(q.dims).length === 1)
  if (!q) check(false, 'found a single-dimension reverse-keyed likert question')
  else {
    const [dim, w] = Object.entries(q.dims!)[0] as [DimId, number]
    const max = q.points ?? 5
    const hi = scoreAll([q], { [q.id]: max }).scores[dim].score
    const lo = scoreAll([q], { [q.id]: 1 }).scores[dim].score
    check(w < 0 && hi < lo, `reverse-scored item ${q.id} (${dim}): agree=${hi} < disagree=${lo}`)
    const pos = QUESTIONS.find((p) => p.type === 'likert' && p.dims && Object.keys(p.dims).length === 1 && Object.values(p.dims)[0] > 0)!
    const pd = Object.keys(pos.dims!)[0] as DimId
    const pmax = pos.points ?? 5
    check(scoreAll([pos], { [pos.id]: pmax }).scores[pd].score > scoreAll([pos], { [pos.id]: 1 }).scores[pd].score, `forward-scored item ${pos.id} (${pd}) rises with agreement`)
    // and the effect survives through the full report
    const base = generateAnswers(QUESTIONS, PERSONAS[0].targets, 5)
    const a1 = buildReport(QUESTIONS, { ...base, [q.id]: max }).scores[dim].score
    const a2 = buildReport(QUESTIONS, { ...base, [q.id]: 1 }).scores[dim].score
    check(a1 < a2, `toggling ${q.id} inside a full answer set moves ${dim} the right way (${a1} vs ${a2})`)
  }
}

// missing answers => neutral score and zero confidence
{
  const empty = buildReport(QUESTIONS, {})
  const dims = Object.keys(empty.scores) as DimId[]
  check(dims.every((d) => empty.scores[d].score === 50 && empty.scores[d].confidence === 0 && empty.scores[d].n === 0), 'no answers: every dimension is 50 with confidence 0')
  check(empty.quality.overallConfidence === 0, 'no answers: overall confidence 0')
  check(empty.population.length === 0, 'no answers: no population comparison fabricated')
  check(empty.identity.strengths.length === 0 && empty.identity.blindSpots.length === 0 && empty.contradictions.length === 0, 'no answers: no insights or contradictions generated')
  const partial = buildReport(QUESTIONS, { [QUESTIONS[0].id]: QUESTIONS[0].type === 'likert' ? 3 : 'x' })
  check(Object.keys(partial.scores).length === dims.length, 'a single answer still produces a full report without errors')
}

console.log(sep)
if (failures) {
  console.log(`${failures} check(s) failed`)
  process.exit(1)
}
console.log('All checks passed')
