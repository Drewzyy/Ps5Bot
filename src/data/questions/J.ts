import type { Question } from '../../engine/types'
import { choice, likert, scale10, textQ } from '../helpers'

const M = 'J' as const

const areas = [
  ['Career', 'career or studies', 'curCareer', 'idealCareer'],
  ['Relationships', 'relationships with family, friends and partner', 'curRelationships', 'idealRelationships'],
  ['Health', 'physical and mental health', 'curHealth', 'idealHealth'],
  ['Finances', 'financial situation', 'curFinances', 'idealFinances'],
  ['Growth', 'personal growth and learning', 'curGrowth', 'idealGrowth'],
  ['Leisure', 'leisure and fun', 'curLeisure', 'idealLeisure'],
] as const

const cur: Question[] = areas.map(([name, phrase, c]) =>
  scale10(`J_cur_${name.toLowerCase()}`, M, `Right now, how well is your ${phrase} going?`, { [c]: 1 }, 'Not well', 'Very well', { help: name === 'Career' ? 'Rate where things stand today.' : undefined }),
)
const ideal: Question[] = areas.map(([name, phrase, , i]) =>
  scale10(`J_ideal_${name.toLowerCase()}`, M, `How much does it matter to you that your ${phrase} is in great shape over the next five years?`, { [i]: 1 }, 'Matters little', 'Matters hugely', {
    help: name === 'Career' ? 'This is about importance, not how it is going now.' : undefined,
  }),
)

export const J: Question[] = [
  ...cur,
  ...ideal,
  likert('J_clar1', M, 'I have a fairly clear picture of who I want to be in ten years.', { futureClarity: 1 }),
  likert('J_opt1', M, 'I feel hopeful about my future.', { futureOptimism: 1, optimism: 0.5, trajectoryFit: 0.6, regretRisk: -0.4 }),
  likert('J_reg1', M, 'I worry that I will one day wish I had taken more chances or spent my time on different things.', { regretRisk: 1, trajectoryFit: -0.5, futureClarity: -0.3 }),
  textQ('J_t1', M, 'When you are 80, what would you most regret not having done?', 'Optional. Take your time.'),
  textQ('J_t2', M, 'Describe your ideal ordinary Tuesday, a few years from now.', 'Optional. Where are you, who is there, what do you do?'),

  // Adaptive follow-ups
  choice(
    'J_fu1',
    M,
    'You feel your current path may not lead where you want. What would change it most?',
    [
      ['skills', 'New skills or qualifications', { growthDrive: 0.8 }],
      ['direction', 'A clearer idea of what I really want', { goalClarity: 0.2, futureClarity: 0.2 }],
      ['courage', 'The courage to make a bold move', { riskTolerance: 0.3, fearOfFailure: 0.7 }],
      ['support', 'Support from other people', { relatedness: 0.7 }],
      ['circumstance', 'A change in my circumstances', { environmentObstacles: 0.8 }],
    ],
    { showIf: { dim: 'trajectoryFit', op: 'lte', value: 40 } },
  ),
  textQ('J_fu2', M, 'Health matters a lot to you and is not where you want it. What is one small step you could take this month?', 'Optional. Keep it small and realistic.', {
    showIf: { all: [{ q: 'J_ideal_health', op: 'gte', value: 8 }, { q: 'J_cur_health', op: 'lte', value: 5 }] },
  }),
]
