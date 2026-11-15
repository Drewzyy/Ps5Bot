// Sanity checks for the question bank. Run: npx tsx scripts/checkQuestions.ts
import { DIM_IDS, DIMENSIONS, type DimId } from '../src/engine/dimensions'
import type { Condition, Question } from '../src/engine/types'
import { QUESTIONS } from '../src/data/questions'
import { SECTIONS } from '../src/data/sections'

const errors: string[] = []
const warnings: string[] = []
const err = (m: string) => errors.push(m)
const valid = new Set<string>(DIM_IDS)

// unique ids
const ids = new Set<string>()
for (const q of QUESTIONS) {
  if (ids.has(q.id)) err(`duplicate id ${q.id}`)
  ids.add(q.id)
}

// sections
const mods = 'ABCDEFGHIJ'.split('')
for (const m of mods) if (!SECTIONS.some((s) => s.module === m)) err(`missing section ${m}`)

const dimKeys = (q: Question): string[] => {
  const keys = Object.keys(q.dims ?? {})
  for (const o of q.options ?? []) keys.push(...Object.keys(o.effects ?? {}))
  return keys
}

const condRefs = (c: Condition, out: { q: string[]; dim: string[] }) => {
  if ('all' in c) c.all.forEach((x) => condRefs(x, out))
  else if ('any' in c) c.any.forEach((x) => condRefs(x, out))
  else if ('dim' in c) out.dim.push(c.dim)
  else out.q.push(c.q)
}

const byId = new Map(QUESTIONS.map((q) => [q.id, q]))
let showIfCount = 0
let pairCount = 0
for (const q of QUESTIONS) {
  for (const k of dimKeys(q)) if (!valid.has(k)) err(`${q.id}: invalid dim "${k}"`)
  for (const k of q.probeFor ?? []) if (!valid.has(k)) err(`${q.id}: invalid probeFor "${k}"`)
  if (q.showIf) {
    showIfCount++
    const r = { q: [] as string[], dim: [] as string[] }
    condRefs(q.showIf, r)
    for (const x of r.q) {
      const t = byId.get(x)
      if (!t) err(`${q.id}: showIf references missing question ${x}`)
      else if (t.probeFor) err(`${q.id}: showIf references a probe ${x}`)
      else if (QUESTIONS.indexOf(t) > QUESTIONS.indexOf(q)) err(`${q.id}: showIf references later question ${x}`)
    }
    for (const x of r.dim) if (!valid.has(x)) err(`${q.id}: showIf invalid dim ${x}`)
  }
  if (q.pairWith) {
    pairCount++
    if (!byId.has(q.pairWith)) err(`${q.id}: pairWith missing ${q.pairWith}`)
  }
  if (q.type === 'forced' && q.options?.length !== 2) err(`${q.id}: forced needs exactly 2 options`)
  if ((q.type === 'choice' || q.type === 'multi' || q.type === 'ranking') && (q.options?.length ?? 0) < 2) err(`${q.id}: needs options`)
  if (q.type === 'multi' && !q.maxSelect) err(`${q.id}: multi needs maxSelect`)
  if ((q.type === 'likert' || q.type === 'scale10') && !(q.dims && Object.keys(q.dims).length)) err(`${q.id}: ${q.type} needs dims`)
  if (q.type === 'text' && (q.dims || q.options)) err(`${q.id}: text must be unscored`)
  if (q.type === 'likert' && q.points === 7 && q.instrument !== 'swls') warnings.push(`${q.id}: 7-point likert outside SWLS`)
  if (/[—–]/.test(q.text + JSON.stringify(q.options ?? []) + (q.help ?? ''))) err(`${q.id}: contains a dash character (em/en)`)
  for (const o of q.options ?? []) {
    const ids2 = new Set<string>()
    if (ids2.has(o.id)) err(`${q.id}: duplicate option id ${o.id}`)
    ids2.add(o.id)
    for (const [d, v] of Object.entries(o.effects ?? {})) if (typeof v !== 'number' || v < 0 || v > 1) err(`${q.id}.${o.id}: effect ${d}=${v} out of 0..1`)
  }
}

// instruments
const ip = QUESTIONS.filter((q) => q.instrument === 'mini-ipip')
const sw = QUESTIONS.filter((q) => q.instrument === 'swls')
if (ip.length !== 20) err(`mini-ipip must have 20 items, has ${ip.length}`)
if (sw.length !== 5) err(`swls must have 5 items, has ${sw.length}`)
for (const q of sw) if (q.points !== 7 || q.dims?.lifeSatisfaction !== 1) err(`${q.id}: SWLS must be 7-point with lifeSatisfaction 1`)
const fiveEach = ['extraversion', 'agreeableness', 'conscientiousness', 'emotionalStability', 'openness']
for (const d of fiveEach) {
  const items = ip.filter((q) => q.dims && d in q.dims)
  const rev = items.filter((q) => (q.dims as Record<string, number>)[d] < 0).length
  if (items.length !== 4 || (d === 'openness' ? rev !== 3 : rev !== 2)) err(`mini-ipip ${d}: unexpected item/reverse count ${items.length}/${rev}`)
}

// coverage
const base = QUESTIONS.filter((q) => !q.probeFor)
const cover = (qs: Question[]) => {
  const m = new Map<string, Set<string>>()
  for (const q of qs) for (const k of dimKeys(q)) (m.get(k) ?? m.set(k, new Set()).get(k)!).add(q.id)
  return m
}
const covBase = cover(base)
const covAll = cover(QUESTIONS)
const KEY: DimId[] = [
  'extraversion', 'agreeableness', 'conscientiousness', 'emotionalStability', 'openness',
  'lifeSatisfaction', 'positiveEmotion', 'engagement', 'relationshipQuality', 'meaning', 'accomplishment',
  'realistic', 'investigative', 'artistic', 'social', 'enterprising', 'conventional',
  'valHonesty', 'valLoyalty', 'valFamily', 'valFreedom', 'valAchievement', 'valCompassion', 'valWealth', 'valKnowledge', 'valJustice', 'valSecurity', 'valCreativity', 'valCommunity', 'valIndependence', 'valResponsibility', 'valRecognition',
  'intrinsicMotivation', 'extrinsicMotivation', 'autonomy', 'competence', 'relatedness',
]
for (const d of DIM_IDS) {
  const n = covBase.get(d)?.size ?? 0
  const need = KEY.includes(d) ? 3 : 2
  if (n < 2) err(`dimension ${d} covered by only ${n} base questions`)
  else if (n < need) warnings.push(`key dimension ${d} covered by ${n} base questions (want ${need}+)`)
  if ((covAll.get(d)?.size ?? 0) < need && n >= 2) warnings.push(`${d} total coverage below ${need}`)
}

// counts
const types: Record<string, number> = {}
const perModule: Record<string, number> = {}
for (const q of base) {
  types[q.type] = (types[q.type] ?? 0) + 1
  perModule[q.module] = (perModule[q.module] ?? 0) + 1
}
const probes = QUESTIONS.filter((q) => q.probeFor)
if (showIfCount < 8) err(`need at least 8 showIf questions, have ${showIfCount}`)
if (pairCount < 6) err(`need about 6 pairWith, have ${pairCount}`)
if (!types.text || !types.ranking || !types.forced || !types.multi || !types.choice || !types.scale10 || !types.likert) err('missing a question type')

// rough timing: seconds per type
const secs: Record<string, number> = { likert: 8, scale10: 9, choice: 18, forced: 12, multi: 22, ranking: 45, text: 30 }
const hidden = base.filter((q) => q.showIf).length
const minutes = base.filter((q) => !q.showIf).reduce((s, q) => s + (secs[q.type] ?? 10), 0) / 60

console.log(`base questions: ${base.length} (${hidden} conditional), probes: ${probes.length}, total: ${QUESTIONS.length}`)
console.log('per module:', JSON.stringify(perModule))
console.log('per type:', JSON.stringify(types))
console.log(`showIf: ${showIfCount}, pairWith: ${pairCount}, est. unconditional time: ~${minutes.toFixed(0)} min`)
console.log(`dimensions: ${DIM_IDS.length}, all covered by >= 2 base questions: ${DIM_IDS.every((d) => (covBase.get(d)?.size ?? 0) >= 2)}`)
void DIMENSIONS
if (warnings.length) console.log('\nWarnings:\n' + warnings.map((w) => ' - ' + w).join('\n'))
if (errors.length) {
  console.error('\nErrors:\n' + errors.map((e) => ' - ' + e).join('\n'))
  throw new Error(`${errors.length} check(s) failed`)
}
console.log('\nOK')
