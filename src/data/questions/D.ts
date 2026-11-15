import type { Question } from '../../engine/types'
import { choice, forced, likert, scale10 } from '../helpers'

const M = 'D' as const

export const D: Question[] = [
  likert('D_se1', M, 'On the whole, I am satisfied with who I am.', { selfEsteem: 1 }),
  likert('D_se2', M, 'Many people sometimes feel they do not measure up. I often feel that I do not.', { selfEsteem: -1 }, { pairWith: 'D_se1' }),
  likert('D_eff1', M, 'When I set my mind to something difficult, I can usually find a way to do it.', { selfEfficacy: 1, competence: 0.5, selfEsteem: 0.3, professionalConfidence: 0.5, adaptability: 0.3 }),
  likert('D_soc1', M, 'I feel comfortable starting a conversation with someone I do not know.', { socialConfidence: 1, extraversion: 0.3, fearOfJudgment: -0.3 }),
  likert('D_body1', M, 'I feel reasonably comfortable in my own body.', { bodyImage: 1 }),
  likert('D_fail1', M, 'Most people dread failing at things. How much does the fear of failing stop you from trying new things?', { fearOfFailure: 1, perfectionism: 0.5, avoidance: 0.3 }, { help: 'Agree means it holds you back quite a lot.' }),
  likert('D_judge1', M, 'I often replay what other people might have thought about something I said or did.', { fearOfJudgment: 1, fearOfRejection: 0.6, validationNeed: 0.5, perfectionism: 0.4 }),
  forced(
    'D_valid2',
    M,
    'Which matters more to how you feel about something you made or did?',
    ['own', 'My own sense of whether it was good', { validationNeed: 0.15, selfEsteem: 0.65 }],
    ['others', 'What other people think of it', { validationNeed: 0.9, recognitionNeed: 0.7 }],
  ),
]
