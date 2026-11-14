import type { Question } from '../../engine/types'
import { choice, likert, textQ } from '../helpers'

const M = 'I' as const

export const I: Question[] = [
  likert('I_proc1', M, 'Most people put things off sometimes. I often delay starting tasks I know matter.', { procrastination: 1, timeManagement: -0.4 }),
  likert('I_cons1', M, 'Once I start a routine, I can usually stick with it for months.', { consistency: 1, conscientiousness: 0.3 }),
  likert('I_fol1', M, 'When I start something, I usually finish it.', { followThrough: 1, accomplishment: 0.4 }, { pairWith: 'I_fol2' }),
  likert('I_fol2', M, 'Lots of people have unfinished projects lying around. I have more than my share.', { followThrough: -1 }),
  likert('I_ic1', M, 'I can resist a tempting distraction when I need to focus.', { impulseControl: 1, impulsivity: -0.5 }),
  likert('I_plan1', M, 'I think several years ahead and make plans that reflect it.', { planning: 1, futureClarity: 0.3 }),
  likert('I_goal1', M, 'I could write down my top three goals for this year right now without much thought.', { goalClarity: 1, careerClarity: 0.2 }),
  likert('I_time1', M, 'I usually know where my time goes and I get things done when I said I would.', { timeManagement: 1, conscientiousness: 0.3 }),
  likert('I_avoid1', M, 'When something feels stressful or uncertain, I tend to avoid it until I have to deal with it.', { avoidance: 1, procrastination: 0.4 }),
  likert('I_env1', M, 'My surroundings, such as my phone, home or the people around me, make it hard to work on what matters.', { environmentObstacles: 1 }),
  likert('I_comp1', M, 'I have more important things on my plate than I can give proper attention to.', { competingPriorities: 1, stress: 0.4 }),
  likert('I_mast1', M, 'I would rather take on a hard task I might fail at than an easy one I will ace, if I could learn from it.', { masteryOrientation: 1, growthDrive: 0.4 }),
  likert('I_perf1', M, 'I care more about looking competent in front of others than about learning something new.', { performanceOrientation: 1, fearOfJudgment: 0.3, masteryOrientation: -0.4 }),
  likert('I_eff1', M, 'When I commit to a change, I can usually make it happen.', { selfEfficacy: 1, followThrough: 0.4 }),
  choice('I_bottle', M, 'When you fall short of a goal, what is usually the main reason?', [
    ['motivation', 'I lost interest or the drive faded', { intrinsicMotivation: 0.2, consistency: 0.35 }],
    ['clarity', 'I was never clear on what I actually wanted', { goalClarity: 0.15 }],
    ['environment', 'My surroundings and distractions got in the way', { environmentObstacles: 0.85 }],
    ['priorities', 'Other things always took priority', { competingPriorities: 0.85 }],
    ['rare', 'I rarely fall short of goals I set', { followThrough: 0.9, goalClarity: 0.8 }],
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
  choice(
    'I_fol_why',
    M,
    'You tend to drop things before they are done. What seems most true?',
    [
      ['novelty', 'I get excited by new things and the old ones fade', { curiosity: 0.8, impulsivity: 0.75 }],
      ['hard', 'It gets hard and I step away', { avoidance: 0.75, fearOfFailure: 0.65 }],
      ['life', 'Life gets in the way', { competingPriorities: 0.85 }],
      ['wrong', 'They stop feeling like what I really want', { goalClarity: 0.25, societalExpectation: 0.65 }],
    ],
    { showIf: { dim: 'followThrough', op: 'lte', value: 40 } },
  ),
]
