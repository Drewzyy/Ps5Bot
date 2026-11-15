import type { Question } from '../../engine/types'
import { choice, likert, multi, ranking } from '../helpers'

const M = 'H' as const

export const H: Question[] = [
  ranking('H_rank1', M, 'Which of these matter most to you in how you live? Rank them, most important first.', [
    ['honesty', 'Honesty', { valHonesty: 1 }],
    ['loyalty', 'Loyalty', { valLoyalty: 1 }],
    ['family', 'Family', { valFamily: 1 }],
    ['freedom', 'Freedom', { valFreedom: 1 }],
    ['achievement', 'Achievement', { valAchievement: 1 }],
    ['compassion', 'Compassion', { valCompassion: 1 }],
    ['wealth', 'Wealth', { valWealth: 1 }],
    ['knowledge', 'Knowledge', { valKnowledge: 1 }],
  ]),
  ranking('H_rank2', M, 'Now rank this second set the same way.', [
    ['justice', 'Justice and fairness', { valJustice: 1 }],
    ['security', 'Security', { valSecurity: 1 }],
    ['creativity', 'Creativity', { valCreativity: 1 }],
    ['community', 'Community', { valCommunity: 1 }],
    ['independence', 'Independence', { valIndependence: 1 }],
    ['responsibility', 'Responsibility', { valResponsibility: 1 }],
    ['recognition', 'Recognition', { valRecognition: 1 }],
  ]),
  multi('H_never', M, 'Which would you refuse to compromise on, even when it costs you something? Pick up to four.', 4, [
    ['honesty', 'Honesty', { valHonesty: 0.9 }],
    ['loyalty', 'Loyalty', { valLoyalty: 0.9 }],
    ['family', 'Family', { valFamily: 0.9 }],
    ['freedom', 'Freedom', { valFreedom: 0.9, valIndependence: 0.5 }],
    ['achievement', 'Achievement', { valAchievement: 0.9 }],
    ['compassion', 'Compassion', { valCompassion: 0.9 }],
    ['wealth', 'Wealth', { valWealth: 0.9 }],
    ['knowledge', 'Knowledge', { valKnowledge: 0.9 }],
    ['justice', 'Justice', { valJustice: 0.9 }],
    ['security', 'Security', { valSecurity: 0.9 }],
    ['creativity', 'Creativity', { valCreativity: 0.9 }],
    ['community', 'Community', { valCommunity: 0.9 }],
    ['independence', 'Independence', { valIndependence: 0.9, valFreedom: 0.5 }],
    ['responsibility', 'Responsibility', { valResponsibility: 0.9 }],
    ['recognition', 'Recognition', { valRecognition: 0.9 }],
  ]),
  choice('H_s1', M, 'A close friend asks you to cover for them with a small lie that would not hurt anyone. What do you do?', [
    ['lie', 'Cover for them. Loyalty comes first', { valLoyalty: 0.9, valHonesty: 0.25 }],
    ['refuse', 'Refuse, but stay a good friend otherwise', { valHonesty: 0.85, valLoyalty: 0.55 }],
    ['tell', 'Explain that I do not lie, even small lies', { valHonesty: 0.95, valLoyalty: 0.35, valJustice: 0.6 }],
  ]),
  choice('H_s2', M, 'You are offered a much better-paid job at a company whose practices you find questionable. You...', [
    ['take', 'Take it. I can do good with the money', { valWealth: 0.85, valSecurity: 0.7, valJustice: 0.2 }],
    ['think', 'Think hard, and probably take it if it is not illegal', { valWealth: 0.6, valJustice: 0.4 }],
    ['pass', 'Pass. I would not feel right', { valJustice: 0.9, valWealth: 0.25, valHonesty: 0.7 }],
  ]),
  choice('H_s3', M, 'A relative needs a year of your help. It would pause a promising opportunity. You...', [
    ['help', 'Help without hesitation', { valFamily: 0.95, valCompassion: 0.85, valResponsibility: 0.8, valAchievement: 0.25 }],
    ['balance', 'Find a way to do both, even if imperfectly', { valFamily: 0.65, valAchievement: 0.65, valResponsibility: 0.7 }],
    ['opp', 'Arrange other help and take the opportunity', { valAchievement: 0.9, valFamily: 0.3, valIndependence: 0.7 }],
  ]),
]
