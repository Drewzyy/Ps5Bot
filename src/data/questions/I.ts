import type { Question } from '../../engine/types'
import { choice, forced, likert, textQ } from '../helpers'

const M = 'I' as const

export const I: Question[] = [
  likert('I_proc1', M, 'Most people put things off sometimes. I often delay starting tasks I know matter.', { procrastination: 1, timeManagement: -0.6, impulseControl: -0.4, consistency: -0.5, avoidance: 0.4 }),
  likert('I_fol1', M, 'When I start something, I usually finish it.', { followThrough: 1, accomplishment: 0.4, consistency: 0.6, conscientiousness: 0.2, selfEfficacy: 0.4 }, { pairWith: 'I_fol2' }),
  likert('I_fol2', M, 'Lots of people have unfinished projects lying around. I have more than my share.', { followThrough: -1 }),
  likert('I_plan1', M, 'I think several years ahead and make plans that reflect it.', { planning: 1, futureClarity: 0.3, goalClarity: 0.6, careerClarity: 0.3 }),
  likert('I_env1', M, 'My surroundings, such as my phone, home or the people around me, make it hard to work on what matters.', { environmentObstacles: 1, competingPriorities: 0.4 }),
  forced(
    'I_orient',
    M,
    'Which would you rather do?',
    ['learn', 'Take on a hard task where I might fail but would learn a lot', { masteryOrientation: 0.9, performanceOrientation: 0.2, growthDrive: 0.7, fearOfFailure: 0.25, riskTolerance: 0.65 }],
    ['ace', 'Do an easier task I know I can do well in front of others', { masteryOrientation: 0.2, performanceOrientation: 0.9, fearOfFailure: 0.7, riskTolerance: 0.3 }],
  ),
  choice('I_bottle', M, 'When you fall short of a goal, what is usually the main reason?', [
    ['motivation', 'I lost interest or the drive faded', { intrinsicMotivation: 0.2, consistency: 0.35, procrastination: 0.65 }],
    ['clarity', 'I was never clear on what I actually wanted', { goalClarity: 0.15, procrastination: 0.6 }],
    ['environment', 'My surroundings and distractions got in the way', { environmentObstacles: 0.85, timeManagement: 0.35 }],
    ['priorities', 'Other things always took priority', { competingPriorities: 0.85, timeManagement: 0.3 }],
    ['rare', 'I rarely fall short of goals I set', { followThrough: 0.9, goalClarity: 0.8, procrastination: 0.1, timeManagement: 0.85 }],
  ]),

  // Adaptive follow-ups
  choice(
    'I_proc_why',
    M,
    'When you put something off, what is usually going on just before?',
    [
      ['overwhelm', 'It feels too big or too much', { avoidance: 0.8, stress: 0.7 }],
      ['perfect', 'I worry I will not do it well enough', { perfectionism: 0.85, fearOfFailure: 0.8 }],
      ['bored', 'It feels dull and pointless', { intrinsicMotivation: 0.2 }],
      ['unclear', 'I am not sure where to start', { goalClarity: 0.2, planning: 0.25 }],
      ['tired', 'I am too tired or drained', { stress: 0.75, workLifeBalance: 0.2 }],
    ],
    { showIf: { dim: 'procrastination', op: 'gte', value: 65 } },
  ),
]
