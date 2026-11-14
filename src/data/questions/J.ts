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
  likert('J_clar1', M, 'I have a fairly clear picture of who I want to be in ten years.', { futureClarity: 1 }, { pairWith: 'J_clar2' }),
  likert('J_clar2', M, 'It is hard for me to picture my life in ten years.', { futureClarity: -1 }),
  likert('J_opt1', M, 'I feel hopeful about my future.', { futureOptimism: 1, optimism: 0.5 }),
  scale10('J_traj2', M, 'How closely does the life you are building match the life you want?', { trajectoryFit: 1 }, 'Not at all', 'Very closely'),
  likert('J_reg1', M, 'I worry that I will one day wish I had taken more chances or spent my time on different things.', { regretRisk: 1, trajectoryFit: -0.5 }),
  choice('J_reg2', M, 'Looking back at 80, which regret would sting more?', [
    ['tried', 'Trying something big and failing', { regretRisk: 0.3, riskTolerance: 0.8, fearOfFailure: 0.25 }],
    ['never', 'Never having tried', { regretRisk: 0.8, riskTolerance: 0.3, fearOfFailure: 0.7 }],
    ['neither', 'Neither. I am at peace with how I live', { regretRisk: 0.15, optimism: 0.7 }],
  ]),
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
    { showIf: { q: 'J_traj2', op: 'lte', value: 4 } },
  ),
  textQ('J_fu2', M, 'Health matters a lot to you and is not where you want it. What is one small step you could take this month?', 'Optional. Keep it small and realistic.', {
    showIf: { all: [{ q: 'J_ideal_health', op: 'gte', value: 8 }, { q: 'J_cur_health', op: 'lte', value: 5 }] },
  }),
  textQ('J_fu3', M, 'Your career matters a lot to you and is not where you want it. What is one change that would make the biggest difference?', 'Optional.', {
    showIf: { all: [{ q: 'J_ideal_career', op: 'gte', value: 8 }, { q: 'J_cur_career', op: 'lte', value: 5 }] },
  }),
  textQ('J_fu4', M, 'Relationships matter a lot to you and are not where you want them. Who would you like to reconnect or invest more in?', 'Optional.', {
    showIf: { all: [{ q: 'J_ideal_relationships', op: 'gte', value: 8 }, { q: 'J_cur_relationships', op: 'lte', value: 5 }] },
  }),
]
