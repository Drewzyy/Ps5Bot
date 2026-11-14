// Registry of every psychological dimension the engine tracks.
// `validated: true` means the dimension is scored from a published public-domain
// or free-to-use instrument. Everything else is a custom exploratory score.

export type DimGroup =
  | 'bigfive'
  | 'needs'
  | 'ambition'
  | 'wellbeing'
  | 'confidence'
  | 'relationships'
  | 'motives'
  | 'riasec'
  | 'work'
  | 'values'
  | 'habits'
  | 'future'
  | 'domains'

interface DimDef {
  label: string
  group: DimGroup
  framework: string
  validated?: boolean
  /** true when a high score is generally the less comfortable pole (stress, loneliness, fear...) */
  inverse?: boolean
}

const d = (label: string, group: DimGroup, framework: string, extra: Partial<DimDef> = {}): DimDef => ({
  label,
  group,
  framework,
  ...extra,
})

export const DIMENSIONS = {
  // Big Five (Mini-IPIP)
  extraversion: d('Extraversion', 'bigfive', 'Big Five (Mini-IPIP)', { validated: true }),
  agreeableness: d('Agreeableness', 'bigfive', 'Big Five (Mini-IPIP)', { validated: true }),
  conscientiousness: d('Conscientiousness', 'bigfive', 'Big Five (Mini-IPIP)', { validated: true }),
  emotionalStability: d('Emotional stability', 'bigfive', 'Big Five (Mini-IPIP)', { validated: true }),
  openness: d('Openness', 'bigfive', 'Big Five (Mini-IPIP)', { validated: true }),
  // Personality facets (custom)
  riskTolerance: d('Risk tolerance', 'bigfive', 'Custom facet'),
  impulsivity: d('Impulsivity', 'bigfive', 'Custom facet', { inverse: true }),
  adaptability: d('Adaptability', 'bigfive', 'Custom facet'),
  structurePreference: d('Preference for structure', 'bigfive', 'Custom facet'),
  assertiveness: d('Assertiveness', 'bigfive', 'Custom facet'),
  curiosity: d('Curiosity', 'bigfive', 'Custom facet'),
  creativity: d('Creativity', 'bigfive', 'Custom facet'),

  // Self-Determination Theory basic needs (satisfaction)
  autonomy: d('Autonomy', 'needs', 'Self-Determination Theory'),
  competence: d('Competence', 'needs', 'Self-Determination Theory'),
  relatedness: d('Relatedness', 'needs', 'Self-Determination Theory'),
  intrinsicMotivation: d('Intrinsic motivation', 'needs', 'Self-Determination Theory'),
  extrinsicMotivation: d('Extrinsic motivation', 'needs', 'Self-Determination Theory'),

  // Ambition drivers
  achievement: d('Achievement', 'ambition', 'Ambition profile'),
  financialDrive: d('Financial drive', 'ambition', 'Ambition profile'),
  recognitionNeed: d('Recognition', 'ambition', 'Ambition profile'),
  influence: d('Influence', 'ambition', 'Ambition profile'),
  contribution: d('Contribution', 'ambition', 'Ambition profile'),
  adventure: d('Adventure', 'ambition', 'Ambition profile'),
  stabilityNeed: d('Stability', 'ambition', 'Ambition profile'),
  freedomNeed: d('Freedom', 'ambition', 'Ambition profile'),
  familyOrientation: d('Family', 'ambition', 'Ambition profile'),
  growthDrive: d('Personal growth', 'ambition', 'Ambition profile'),
  intellectualDrive: d('Intellectual fulfilment', 'ambition', 'Ambition profile'),
  societalExpectation: d('Pull of others’ expectations', 'ambition', 'Ambition profile', { inverse: true }),

  // Well-being
  lifeSatisfaction: d('Life satisfaction', 'wellbeing', 'SWLS', { validated: true }),
  positiveEmotion: d('Positive emotion', 'wellbeing', 'PERMA'),
  engagement: d('Engagement', 'wellbeing', 'PERMA'),
  relationshipQuality: d('Relationships', 'wellbeing', 'PERMA'),
  meaning: d('Meaning', 'wellbeing', 'PERMA'),
  accomplishment: d('Accomplishment', 'wellbeing', 'PERMA'),
  stress: d('Stress', 'wellbeing', 'Well-being', { inverse: true }),
  loneliness: d('Loneliness', 'wellbeing', 'Well-being', { inverse: true }),
  optimism: d('Optimism', 'wellbeing', 'Well-being'),
  gratitude: d('Gratitude', 'wellbeing', 'Well-being'),
  recentMood: d('Recent mood', 'wellbeing', 'Well-being (state)'),
  workLifeBalance: d('Work-life balance', 'wellbeing', 'Well-being'),

  // Confidence and self-perception
  selfEsteem: d('Self-esteem', 'confidence', 'Self-perception'),
  selfEfficacy: d('Self-efficacy', 'confidence', 'Self-efficacy theory'),
  socialConfidence: d('Social confidence', 'confidence', 'Self-perception'),
  professionalConfidence: d('Professional confidence', 'confidence', 'Self-perception'),
  bodyImage: d('Physical self-image', 'confidence', 'Self-perception'),
  fearOfFailure: d('Fear of failure', 'confidence', 'Self-perception', { inverse: true }),
  fearOfJudgment: d('Fear of judgment', 'confidence', 'Self-perception', { inverse: true }),
  fearOfRejection: d('Fear of rejection', 'confidence', 'Self-perception', { inverse: true }),
  perfectionism: d('Perfectionism', 'confidence', 'Self-perception', { inverse: true }),
  validationNeed: d('Need for validation', 'confidence', 'Self-perception', { inverse: true }),
  selfAdvocacy: d('Self-advocacy', 'confidence', 'Self-perception'),

  // Relationships
  intimacyComfort: d('Comfort with closeness', 'relationships', 'Relationship tendencies'),
  trust: d('Trust', 'relationships', 'Relationship tendencies'),
  empathy: d('Empathy', 'relationships', 'Relationship tendencies'),
  boundaries: d('Boundaries', 'relationships', 'Relationship tendencies'),
  communication: d('Open communication', 'relationships', 'Relationship tendencies'),
  conflictApproach: d('Engaging with conflict', 'relationships', 'Relationship tendencies'),
  companionshipNeed: d('Need for companionship', 'relationships', 'Relationship tendencies'),
  relationalIndependence: d('Independence in relationships', 'relationships', 'Relationship tendencies'),
  attachmentAnxiety: d('Worry about closeness', 'relationships', 'Attachment-related tendency', { inverse: true }),
  attachmentAvoidance: d('Distance from closeness', 'relationships', 'Attachment-related tendency', { inverse: true }),
  feelingUnderstood: d('Feeling understood', 'relationships', 'Relationship tendencies'),

  // Why activities are enjoyable
  motCompetition: d('Competition', 'motives', 'Enjoyment motives'),
  motCreativity: d('Creating', 'motives', 'Enjoyment motives'),
  motMastery: d('Mastery', 'motives', 'Enjoyment motives'),
  motSocial: d('Social connection', 'motives', 'Enjoyment motives'),
  motCollecting: d('Collecting', 'motives', 'Enjoyment motives'),
  motProblemSolving: d('Problem-solving', 'motives', 'Enjoyment motives'),
  motRecognition: d('Being recognised', 'motives', 'Enjoyment motives'),
  motRelaxation: d('Relaxation', 'motives', 'Enjoyment motives'),
  motProgress: d('Visible progress', 'motives', 'Enjoyment motives'),
  motEscapism: d('Escape', 'motives', 'Enjoyment motives'),
  motPhysical: d('Physical challenge', 'motives', 'Enjoyment motives'),
  motNature: d('Being outdoors', 'motives', 'Enjoyment motives'),

  // Holland RIASEC interests
  realistic: d('Realistic', 'riasec', 'Holland RIASEC'),
  investigative: d('Investigative', 'riasec', 'Holland RIASEC'),
  artistic: d('Artistic', 'riasec', 'Holland RIASEC'),
  social: d('Social', 'riasec', 'Holland RIASEC'),
  enterprising: d('Enterprising', 'riasec', 'Holland RIASEC'),
  conventional: d('Conventional', 'riasec', 'Holland RIASEC'),

  // Work preferences
  careerSatisfaction: d('Satisfaction with current direction', 'work', 'Career'),
  careerClarity: d('Career clarity', 'work', 'Career'),
  workAutonomyNeed: d('Autonomy at work', 'work', 'Career'),
  prestigeNeed: d('Prestige', 'work', 'Career'),
  securityNeed: d('Job security', 'work', 'Career'),
  leadershipDrive: d('Leadership', 'work', 'Career'),
  helpingDrive: d('Helping others', 'work', 'Career'),
  workloadTolerance: d('Workload tolerance', 'work', 'Career'),
  balancePriority: d('Priority on balance', 'work', 'Career'),
  incomeExpectation: d('Income expectation', 'work', 'Career'),
  careerFit: d('Perceived fit of current path', 'work', 'Career'),

  // Values (Schwartz-informed, custom list)
  valHonesty: d('Honesty', 'values', 'Values'),
  valLoyalty: d('Loyalty', 'values', 'Values'),
  valFamily: d('Family', 'values', 'Values'),
  valFreedom: d('Freedom', 'values', 'Values'),
  valAchievement: d('Achievement', 'values', 'Values'),
  valCompassion: d('Compassion', 'values', 'Values'),
  valWealth: d('Wealth', 'values', 'Values'),
  valKnowledge: d('Knowledge', 'values', 'Values'),
  valJustice: d('Justice', 'values', 'Values'),
  valSecurity: d('Security', 'values', 'Values'),
  valCreativity: d('Creativity', 'values', 'Values'),
  valCommunity: d('Community', 'values', 'Values'),
  valIndependence: d('Independence', 'values', 'Values'),
  valResponsibility: d('Responsibility', 'values', 'Values'),
  valRecognition: d('Recognition', 'values', 'Values'),

  // Habits and behaviour
  procrastination: d('Procrastination', 'habits', 'Habits', { inverse: true }),
  consistency: d('Consistency', 'habits', 'Habits'),
  followThrough: d('Follow-through', 'habits', 'Habits'),
  impulseControl: d('Impulse control', 'habits', 'Habits'),
  planning: d('Long-term planning', 'habits', 'Habits'),
  goalClarity: d('Goal clarity', 'habits', 'Goal-setting'),
  timeManagement: d('Time management', 'habits', 'Habits'),
  avoidance: d('Avoidance', 'habits', 'Habits', { inverse: true }),
  environmentObstacles: d('Environmental obstacles', 'habits', 'Habits', { inverse: true }),
  competingPriorities: d('Competing priorities', 'habits', 'Habits', { inverse: true }),
  masteryOrientation: d('Learning orientation', 'habits', 'Goal orientation theory'),
  performanceOrientation: d('Performance orientation', 'habits', 'Goal orientation theory'),

  // Future self
  futureClarity: d('Clarity about the future', 'future', 'Future self'),
  futureOptimism: d('Confidence in the future', 'future', 'Future self'),
  trajectoryFit: d('Current path leads where I want', 'future', 'Future self'),
  regretRisk: d('Anticipated regret', 'future', 'Future self', { inverse: true }),

  // Life domains: how things are now vs how much they matter
  curCareer: d('Career now', 'domains', 'Current vs ideal'),
  curRelationships: d('Relationships now', 'domains', 'Current vs ideal'),
  curHealth: d('Health now', 'domains', 'Current vs ideal'),
  curFinances: d('Finances now', 'domains', 'Current vs ideal'),
  curGrowth: d('Growth now', 'domains', 'Current vs ideal'),
  curLeisure: d('Leisure now', 'domains', 'Current vs ideal'),
  idealCareer: d('Career importance', 'domains', 'Current vs ideal'),
  idealRelationships: d('Relationships importance', 'domains', 'Current vs ideal'),
  idealHealth: d('Health importance', 'domains', 'Current vs ideal'),
  idealFinances: d('Finances importance', 'domains', 'Current vs ideal'),
  idealGrowth: d('Growth importance', 'domains', 'Current vs ideal'),
  idealLeisure: d('Leisure importance', 'domains', 'Current vs ideal'),
} as const satisfies Record<string, DimDef>

export type DimId = keyof typeof DIMENSIONS
export const DIM_IDS = Object.keys(DIMENSIONS) as DimId[]

export const dimLabel = (id: DimId): string => DIMENSIONS[id].label
export const dimsInGroup = (g: DimGroup): DimId[] => DIM_IDS.filter((id) => DIMENSIONS[id].group === g)
export const isValidated = (id: DimId): boolean => Boolean((DIMENSIONS[id] as DimDef).validated)
export const isInverse = (id: DimId): boolean => Boolean((DIMENSIONS[id] as DimDef).inverse)
