import type { Question } from '../../engine/types'
import { choice, forced, likert, multi, ranking, scale10, textQ } from '../helpers'

const M = 'G' as const

export const G: Question[] = [
  scale10('G_sat1', M, 'How satisfied are you with the direction of your working life right now?', { careerSatisfaction: 1, curCareer: 0.5, careerClarity: 0.3 }, 'Not at all', 'Completely'),
  likert('G_fit1', M, 'My current path makes good use of my strengths.', { careerFit: 1, professionalConfidence: 0.5, careerClarity: 0.6, competence: 0.5, autonomy: 0.3, meaning: 0.4, accomplishment: 0.4 }),
  choice('G_ria_role', M, 'On a group project, which role would you most like to take?', [
    ['build', 'Build or fix the physical thing', { realistic: 0.9, investigative: 0.35, conventional: 0.5 }],
    ['research', 'Research the problem and work out the answer', { investigative: 0.9, conventional: 0.4, realistic: 0.3 }],
    ['design', 'Design the look, idea or story', { artistic: 0.9, enterprising: 0.4, social: 0.4 }],
    ['people', 'Look after the team and help everyone do well', { social: 0.9, artistic: 0.3, enterprising: 0.4 }],
    ['pitch', 'Pitch it, persuade others and drive it forward', { enterprising: 0.9, social: 0.45, realistic: 0.2 }],
    ['organise', 'Plan the schedule, budget and details', { conventional: 0.9, enterprising: 0.35, investigative: 0.3 }],
  ]),
  likert('G_stress1', M, 'My work or study leaves me drained for most of the week.', { stress: 1, workLifeBalance: -0.7, workloadTolerance: -0.5, engagement: -0.3 }, { pairWith: 'C_stress1' }),
  // Adaptive: unhappy with direction
  choice(
    'G_unsat1',
    M,
    'You said your working life is not feeling right. What is the main reason?',
    [
      ['bored', 'It is boring or does not challenge me', { careerFit: 0.2, intellectualDrive: 0.7, engagement: 0.2 }],
      ['stuck', 'I am not growing or progressing', { growthDrive: 0.8, careerClarity: 0.3 }],
      ['pay', 'The pay does not match what I need', { financialDrive: 0.7, incomeExpectation: 0.7 }],
      ['people', 'The people or culture', { relationshipQuality: 0.3, careerFit: 0.35 }],
      ['values', 'It does not fit my values', { meaning: 0.2, careerFit: 0.15, valJustice: 0.6 }],
      ['load', 'Too much pressure or too many hours', { stress: 0.85, workLifeBalance: 0.15, balancePriority: 0.8 }],
      ['field', 'It is the wrong field for me', { careerFit: 0.1, careerClarity: 0.3 }],
    ],
    { showIf: { q: 'G_sat1', op: 'lte', value: 4 } },
  ),
  forced(
    'G_f1',
    M,
    'Which job would suit you better?',
    ['free', 'Freedom to decide how I work, with less certainty', { workAutonomyNeed: 0.9, autonomy: 0.8, securityNeed: 0.2, riskTolerance: 0.7 }],
    ['secure', 'A steady, secure role with clear expectations', { workAutonomyNeed: 0.3, securityNeed: 0.9, structurePreference: 0.75 }],
  ),
  forced(
    'G_f2',
    M,
    'Which would you rather have?',
    ['prestige', 'A high-status job at a well-known organisation', { prestigeNeed: 0.9, recognitionNeed: 0.7, helpingDrive: 0.3 }],
    ['help', 'A modest job where I directly help people', { prestigeNeed: 0.15, helpingDrive: 0.9, contribution: 0.75 }],
  ),
  forced(
    'G_f3',
    M,
    'Which working life sounds better?',
    ['intense', 'Long, intense stretches for big rewards', { workloadTolerance: 0.9, balancePriority: 0.15, achievement: 0.7, financialDrive: 0.65 }],
    ['balanced', 'Predictable hours and a modest income', { workloadTolerance: 0.25, balancePriority: 0.9, workLifeBalance: 0.8 }],
  ),
  forced(
    'G_f4',
    M,
    'Which role feels more natural?',
    ['lead', 'Leading a team and setting direction', { leadershipDrive: 0.9, influence: 0.8, enterprising: 0.7 }],
    ['expert', 'Being the deep expert people come to', { leadershipDrive: 0.2, investigative: 0.65, motMastery: 0.7 }],
  ),
  // RIASEC interests
  multi('G_ria_mc', M, 'Which of these would you enjoy doing in a job? Pick up to six.', 6, [
    ['r1', 'Repair or build something with your hands', { realistic: 0.9 }],
    ['r2', 'Work with tools, machines or equipment', { realistic: 0.85 }],
    ['r3', 'Work outdoors with plants, animals or the land', { realistic: 0.8 }],
    ['i1', 'Analyse data to find out what is going on', { investigative: 0.9 }],
    ['i2', 'Run experiments or research a hard question', { investigative: 0.9 }],
    ['i3', 'Diagnose why something is not working', { investigative: 0.8 }],
    ['a1', 'Design, write or create original work', { artistic: 0.9 }],
    ['a2', 'Perform, play music or act', { artistic: 0.8 }],
    ['a3', 'Come up with new concepts and ideas', { artistic: 0.8 }],
    ['s1', 'Teach, coach or mentor people', { social: 0.9 }],
    ['s2', 'Care for or counsel people', { social: 0.9 }],
    ['s3', 'Bring a group together and help them cooperate', { social: 0.8 }],
    ['e1', 'Sell, persuade or negotiate', { enterprising: 0.9 }],
    ['e2', 'Start and run a venture', { enterprising: 0.9 }],
    ['e3', 'Lead a team toward a goal', { enterprising: 0.8 }],
    ['c1', 'Organise records, schedules or systems', { conventional: 0.9 }],
    ['c2', 'Keep accounts or work with numbers carefully', { conventional: 0.85 }],
    ['c3', 'Follow clear procedures and check details', { conventional: 0.8 }],
  ]),
  ranking('G_ria_rank', M, 'Rank these work settings from the one you would most like to the one you would least like.', [
    ['r', 'A workshop, site or outdoor setting', { realistic: 1 }],
    ['i', 'A lab, library or analysis desk', { investigative: 1 }],
    ['a', 'A studio or creative space', { artistic: 1 }],
    ['s', 'A school, clinic or community setting', { social: 1 }],
    ['e', 'A fast-moving business or sales floor', { enterprising: 1 }],
    ['c', 'A tidy office with clear systems', { conventional: 1 }],
  ]),
]
