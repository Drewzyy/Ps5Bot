// Exploratory "what if" lifestyle simulator. Takes the eight priority sliders and
// the measured scores and describes the lifestyle shapes the combination resembles.
// It is a thinking aid built from hand-written archetypes, not a prediction.

import type { LifePriorities, Scores } from './types'

export interface ArchetypeMatch {
  id: string
  name: string
  percent: number
  description: string
}

export interface LifestyleProfile {
  headline: string
  matches: ArchetypeMatch[]
  description: string
  tradeoffs: string[]
  tensions: string[]
  environment: string
  caveat: string
}

type Vec = Record<keyof LifePriorities, number>
interface Archetype {
  id: string
  name: string
  v: Vec
  description: string
  tradeoffs: string[]
  environment: string
}

const A = (
  id: string,
  name: string,
  v: [number, number, number, number, number, number, number, number],
  description: string,
  tradeoffs: string[],
  environment: string,
): Archetype => ({
  id,
  name,
  v: { wealth: v[0], freedom: v[1], relationships: v[2], achievement: v[3], creativity: v[4], stability: v[5], adventure: v[6], contribution: v[7] },
  description,
  tradeoffs,
  environment,
})
// order: wealth, freedom, relationships, achievement, creativity, stability, adventure, contribution

const ARCHETYPES: Archetype[] = [
  A('builder', 'The Builder', [0.7, 0.8, 0.35, 0.9, 0.5, 0.15, 0.5, 0.3],
    'A life organised around making something of your own and pushing it forward.',
    ['Income and sleep can be uneven in the early years.', 'Relationships need protected time or they quietly lose out to the project.'],
    'Small teams, startups or independent practice where you own outcomes and can move fast.'),
  A('maker', 'The Maker', [0.3, 0.75, 0.4, 0.5, 0.95, 0.25, 0.5, 0.4],
    'A life where creative work is the centre and money is a means to protect it.',
    ['Creative freedom often trades against predictable income.', 'Finishing and sharing work takes a different muscle than making it.'],
    'Flexible, project-based work with studio space, collaborators and long uninterrupted blocks.'),
  A('anchor', 'The Anchor', [0.45, 0.35, 0.95, 0.35, 0.3, 0.8, 0.15, 0.55],
    'A life built around close people, home and dependable routines.',
    ['Career moves that need relocation or long hours will feel expensive.', 'Comfort can slowly narrow your range of experiences if nothing nudges you out.'],
    'Stable work near people you love, with predictable hours and a strong local community.'),
  A('steward', 'The Steady Professional', [0.65, 0.35, 0.5, 0.6, 0.25, 0.95, 0.15, 0.4],
    'A life built on solid footing: reliable income, clear progression and few surprises.',
    ['Lower volatility often means lower upside.', 'Long stretches without novelty can feel flat unless you plan some on purpose.'],
    'Established organisations with clear structure, defined paths and good benefits.'),
  A('explorer', 'The Explorer', [0.35, 0.95, 0.4, 0.4, 0.55, 0.1, 0.95, 0.3],
    'A life that stays mobile and curious, trading permanence for experience.',
    ['Deep roots and long-term relationships take extra effort when you move often.', 'Savings and career ladders are harder to build in a patchwork life.'],
    'Remote or seasonal work, short contracts, and a base you can leave easily.'),
  A('caregiver', 'The Caregiver', [0.2, 0.4, 0.75, 0.4, 0.3, 0.6, 0.2, 0.95],
    'A life where usefulness to other people is the main source of meaning.',
    ['Giving without limits is a common route to burnout.', 'Mission-driven work often pays less than comparable roles elsewhere.'],
    'Mission-led organisations, healthcare, education or community roles with real contact with the people served.'),
  A('specialist', 'The Deep Specialist', [0.55, 0.5, 0.3, 0.85, 0.4, 0.55, 0.2, 0.3],
    'A life of focused mastery, where being very good at something is the organising idea.',
    ['Narrow focus can crowd out friendships and rest.', 'Your worth can become tied to performance.'],
    'Expert-track roles with respect for depth, protected focus time and a strong peer group.'),
  A('connector', 'The Community Connector', [0.3, 0.45, 0.85, 0.4, 0.45, 0.45, 0.4, 0.8],
    'A life spent gathering people, hosting, organising and weaving networks.',
    ['You may be the person everyone leans on, with little left for yourself.', 'Social energy is finite even for people who love it.'],
    'Roles with collaboration and visibility inside a community, such as organising, teaching, hospitality or local business.'),
  A('impact', 'The Impact Entrepreneur', [0.4, 0.75, 0.4, 0.75, 0.5, 0.25, 0.4, 0.85],
    'A life that tries to combine ownership and independence with a cause.',
    ['The mission can make it hard to say no and to charge fairly.', 'Funding and income are often the slowest parts to solve.'],
    'Founder-style or social-enterprise settings where you set direction and see the effects on people.'),
  A('homesteader', 'The Quiet Homesteader', [0.3, 0.7, 0.4, 0.3, 0.55, 0.8, 0.1, 0.35],
    'A life of modest, self-directed routines close to home, with space for slow projects.',
    ['Low ambition on paper can clash with outside expectations.', 'Isolation can build up unnoticed.'],
    'Home-based or rural-friendly work with low commute, low noise and a lot of autonomy.'),
  A('balanced', 'The Balanced Generalist', [0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5],
    'A life that keeps many things in moderate balance without giving any one of them everything.',
    ['No single area gets the depth that comes from sustained focus.', 'Without a clear lean, decisions can stall.'],
    'Flexible environments that let you adjust the dial between work, people and exploration as seasons change.'),
  A('striver', 'The Wealth Strategist', [0.95, 0.5, 0.3, 0.8, 0.3, 0.5, 0.3, 0.2],
    'A life that treats financial strength as the base for everything else.',
    ['Chasing money past a comfortable level has weak links to happiness.', 'Time with people and rest can become the hidden cost.'],
    'High-earning sectors or equity-linked roles with clear metrics for reward.'),
]

const norm = (p: LifePriorities): Vec => {
  const vals = Object.values(p)
  const max = Math.max(...vals, 0)
  const div = max <= 10 ? 10 : 100
  const o = {} as Vec
  for (const k of Object.keys(p) as (keyof LifePriorities)[]) o[k] = Math.min(1, Math.max(0, p[k] / div))
  return o
}

const known = (s: Scores, d: keyof Scores) => s[d].n > 0 && s[d].confidence >= 0.35

export function simulateLifestyle(p: LifePriorities, scores: Scores): LifestyleProfile {
  const u = norm(p)
  const keys = Object.keys(u) as (keyof Vec)[]
  const fits = ARCHETYPES.map((a) => {
    const rmse = Math.sqrt(keys.reduce((s, k) => s + (u[k] - a.v[k]) ** 2, 0) / keys.length)
    return { a, fit: Math.exp(-5 * rmse) }
  }).sort((x, y) => y.fit - x.fit)
  const top = fits.slice(0, 4)
  const total = top.reduce((s, x) => s + x.fit, 0) || 1
  let pct = top.map((x) => Math.round((x.fit / total) * 100))
  pct[0] += 100 - pct.reduce((s, v) => s + v, 0)
  const matches: ArchetypeMatch[] = top.map((x, i) => ({ id: x.a.id, name: x.a.name, percent: pct[i], description: x.a.description }))

  const main = top[0].a
  const second = top[1].a
  const tensions: string[] = []
  const tradeoffs = [...main.tradeoffs]
  if (top[1].fit / top[0].fit > 0.85) {
    tradeoffs.push(`Your sliders sit close to a second pattern, ${second.name}. ${second.tradeoffs[0]}`)
  }

  const sorted = [...keys].sort((a, b) => u[b] - u[a])
  const highs = sorted.filter((k) => u[k] >= 0.7)
  if (highs.length >= 5) tradeoffs.push('Rating five or more priorities as very high usually means something has to give. Which one would you let slide first?')

  if (u.freedom >= 0.7 && u.stability >= 0.7) tensions.push('Freedom and stability both rank high. They can coexist but usually need a deliberate design, such as savings runway plus flexible work.')
  if (u.wealth >= 0.7 && u.contribution >= 0.7) tensions.push('Wealth and contribution both rank high. Worth asking which one you would protect if you could not have both.')
  if (u.adventure >= 0.7 && u.relationships >= 0.7) tensions.push('Adventure and relationships both rank high. Shared adventures or a movable partner or circle may matter here.')
  if (u.achievement >= 0.7 && u.relationships >= 0.7 && known(scores, 'balancePriority') && scores.balancePriority.score >= 60) {
    tensions.push('You rate achievement and relationships highly, and your answers also lean toward balance. Time is the likely constraint to design around.')
  }
  if (u.freedom >= 0.7 && known(scores, 'securityNeed') && scores.securityNeed.score >= 62) {
    tensions.push('You put freedom high, while your earlier answers suggest a real need for security. This may be worth testing with a small experiment before a big change.')
  }
  if (u.wealth >= 0.7 && known(scores, 'riskTolerance') && scores.riskTolerance.score <= 38) {
    tensions.push('A high wealth priority sits beside low comfort with risk. Steady-compounding paths may suit this better than high-variance ones.')
  }
  if (u.relationships >= 0.7 && known(scores, 'companionshipNeed') && scores.companionshipNeed.score <= 35) {
    tensions.push('You rank relationships highly, though your answers about needing company were low. It may be about quality or a specific relationship more than quantity.')
  }
  if (u.adventure >= 0.7 && known(scores, 'stabilityNeed') && scores.stabilityNeed.score >= 65) {
    tensions.push('High adventure beside a strong stability need suggests bounded adventure, such as short trips from a stable base, may fit better than a fully mobile life.')
  }

  let env = main.environment
  if (known(scores, 'extraversion') && scores.extraversion.score <= 38) env += ' Given your answers, build in quieter, lower-stimulation stretches.'
  else if (known(scores, 'extraversion') && scores.extraversion.score >= 65) env += ' Given your answers, a setting with regular contact with other people will likely help.'
  if (known(scores, 'structurePreference') && scores.structurePreference.score >= 65) env += ' You may want clear routines even inside a flexible set-up.'

  const description = `Taking your sliders as they are, the closest pattern is ${main.name} (${matches[0].percent}%). ${main.description}${
    matches[1] ? ` The next closest is ${second.name} (${matches[1].percent}%).` : ''
  }`

  return {
    headline: `Closest lifestyle pattern: ${main.name}`,
    matches,
    description,
    tradeoffs,
    tensions,
    environment: env,
    caveat:
      'This is an exploratory sketch built from eight sliders and a small set of hand-written patterns. It does not predict outcomes or recommend a life. Move the sliders and notice what changes.',
  }
}
