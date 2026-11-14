import type { Question } from '../../engine/types'
import { choice, forced, likert, scale10 } from '../helpers'

const M = 'D' as const

export const D: Question[] = [
  likert('D_se1', M, 'On the whole, I am satisfied with who I am.', { selfEsteem: 1 }),
  likert('D_se2', M, 'Many people sometimes feel they do not measure up. I often feel that I do not.', { selfEsteem: -1 }, { pairWith: 'D_se1' }),
  likert('D_eff1', M, 'When I set my mind to something difficult, I can usually find a way to do it.', { selfEfficacy: 1 }),
  likert('D_soc1', M, 'I feel comfortable starting a conversation with someone I do not know.', { socialConfidence: 1, extraversion: 0.3 }),
  likert('D_soc2', M, 'Walking into a room full of strangers makes me tense.', { socialConfidence: -1, fearOfJudgment: 0.5 }, { pairWith: 'D_soc1' }),
  likert('D_pro1', M, 'At work or study I trust my judgement and offer my ideas even when I am not sure they will land.', { professionalConfidence: 1, selfAdvocacy: 0.4, selfEfficacy: 0.3 }),
  likert('D_body1', M, 'I feel reasonably comfortable in my own body.', { bodyImage: 1 }),
  likert('D_fail1', M, 'Most people dread failing at things. How much does the fear of failing stop you from trying new things?', { fearOfFailure: 1 }, { help: 'Agree means it holds you back quite a lot.' }),
  choice('D_fail2', M, 'A big opportunity comes up, and you have a real chance of falling flat. What is closest to your reaction?', [
    ['go', 'I go for it. A failed try beats not knowing', { fearOfFailure: 0.15, riskTolerance: 0.8, selfEfficacy: 0.7 }],
    ['prepare', 'I go for it, but only after a lot of preparation', { fearOfFailure: 0.5, perfectionism: 0.7, planning: 0.7 }],
    ['hesitate', 'I hesitate and often talk myself out of it', { fearOfFailure: 0.85, riskTolerance: 0.25, avoidance: 0.7 }],
    ['pass', 'I pass. The downside feels too big', { fearOfFailure: 0.95, riskTolerance: 0.1, avoidance: 0.8 }],
  ]),
  likert('D_judge1', M, 'I often replay what other people might have thought about something I said or did.', { fearOfJudgment: 1 }),
  likert('D_rej1', M, 'I sometimes avoid asking for things, like a date, a job or help, because I might be turned down.', { fearOfRejection: 1, selfAdvocacy: -0.4 }),
  likert('D_perf1', M, 'I find it hard to call something finished unless it is exactly right.', { perfectionism: 1 }),
  forced(
    'D_valid2',
    M,
    'Which matters more to how you feel about something you made or did?',
    ['own', 'My own sense of whether it was good', { validationNeed: 0.15, selfEsteem: 0.65 }],
    ['others', 'What other people think of it', { validationNeed: 0.9, recognitionNeed: 0.7 }],
  ),
  likert('D_adv1', M, 'I ask for what I need or deserve, like a raise or a change, rather than hoping it gets noticed.', { selfAdvocacy: 1, assertiveness: 0.4, boundaries: 0.4 }),
]
