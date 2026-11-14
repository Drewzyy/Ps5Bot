import type { Question } from '../../engine/types'
import { choice, likert, scale10, textQ } from '../helpers'

const M = 'E' as const

export const E: Question[] = [
  likert('E_anx1', M, 'I sometimes worry that people I care about will lose interest in me.', { attachmentAnxiety: 1, fearOfRejection: 0.6 }),
  likert('E_anx2', M, 'I often need reassurance that I matter to the people close to me.', { attachmentAnxiety: 1, validationNeed: 0.3 }),
  likert('E_av2', M, 'I get uncomfortable when someone wants to get very emotionally close.', { attachmentAvoidance: 1, intimacyComfort: -0.8 }),
  likert('E_int1', M, 'I find it easy to open up about how I really feel with people I trust.', { intimacyComfort: 1, communication: 0.5 }),
  likert('E_trust1', M, 'I usually assume that people mean well.', { trust: 1 }),
  likert('E_emp1', M, 'I can usually tell how someone feels before they say it.', { empathy: 1 }),
  likert('E_bound1', M, 'I can say no to a request without feeling guilty for days.', { boundaries: 1 }),
  choice('E_conf1', M, 'A close friend says something that hurts you. What do you usually do?', [
    ['soon', 'Tell them soon and clearly', { conflictApproach: 0.9, communication: 0.85, assertiveness: 0.7 }],
    ['wait', 'Wait until I have cooled down, then raise it', { conflictApproach: 0.65, communication: 0.7 }],
    ['avoid', 'Let it go and say nothing', { conflictApproach: 0.15, communication: 0.25, avoidance: 0.75 }],
    ['pull', 'Pull back for a while without explaining', { conflictApproach: 0.1, attachmentAvoidance: 0.7, communication: 0.15 }],
  ]),
  scale10('E_comp1', M, 'In an ideal week, how much of your free time is spent with other people rather than alone?', { companionshipNeed: 1, extraversion: 0.4 }, 'Mostly alone', 'Mostly with others'),
  likert('E_und1', M, 'There is at least one person who really understands me.', { feelingUnderstood: 1, relationshipQuality: 0.4 }, { pairWith: 'E_und2' }),
  likert('E_und2', M, 'Most people only ever see a surface version of me.', { feelingUnderstood: -1 }),
  likert('E_lonely2', M, 'I have plenty of companionship in my life.', { loneliness: -1, relationshipQuality: 0.5, curRelationships: 0.5 }),
  choice('E_first', M, 'You get some bad news. Who do you tend to turn to first?', [
    ['partner', 'A partner or very close friend', { relatedness: 0.9, trust: 0.75, intimacyComfort: 0.8 }],
    ['family', 'A family member', { relatedness: 0.8, familyOrientation: 0.8, trust: 0.7 }],
    ['pro', 'A professional or a written plan', { relationalIndependence: 0.7, planning: 0.6 }],
    ['none', 'Nobody, I work through it alone', { relationalIndependence: 0.9, attachmentAvoidance: 0.7, intimacyComfort: 0.2, trust: 0.35 }],
  ]),
]
