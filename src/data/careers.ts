// Career path catalog. RIASEC codes are approximate, hand-assigned, loosely
// informed by Holland's occupational themes. Work-preference profiles (0..100)
// describe typical demands of the role family, not any specific job, and vary a lot
// between employers. This is a way to explore options, not a verdict.

export interface WorkProfile {
  autonomy: number // how much self-direction the role usually allows
  security: number // typical stability of employment
  prestige: number // social status attached
  helping: number // direct benefit to other people
  leadership: number // how much leading others is involved
  workload: number // typical intensity and hours (high = demanding)
  balance: number // typical ease of keeping work and life in balance
  income: number // typical income potential
}

export interface CareerDef {
  id: string
  title: string
  riasec: string
  profile: WorkProfile
  tradeoff: string
}

const c = (
  id: string,
  title: string,
  riasec: string,
  p: [number, number, number, number, number, number, number, number],
  tradeoff: string,
): CareerDef => ({
  id,
  title,
  riasec,
  profile: { autonomy: p[0], security: p[1], prestige: p[2], helping: p[3], leadership: p[4], workload: p[5], balance: p[6], income: p[7] },
  tradeoff,
})

// order: autonomy, security, prestige, helping, leadership, workload, balance, income
export const CAREERS: CareerDef[] = [
  c('software-eng', 'Software engineer', 'IRC', [65, 70, 60, 30, 25, 55, 60, 80], 'Good pay and flexibility, but long stretches of abstract work and fast-changing tools.'),
  c('data-scientist', 'Data scientist or analyst', 'ICR', [60, 70, 62, 35, 25, 55, 62, 78], 'Intellectually rich, though much of the day can be data cleaning and stakeholder alignment.'),
  c('researcher', 'Academic or research scientist', 'IAR', [75, 45, 70, 40, 35, 65, 45, 50], 'High intellectual freedom, but competitive funding, slow careers and modest pay early on.'),
  c('medical-doctor', 'Physician', 'ISR', [50, 90, 90, 85, 50, 90, 25, 90], 'Strong security and meaning, with long training and demanding hours.'),
  c('nurse', 'Nurse', 'SIR', [40, 90, 60, 95, 35, 80, 40, 60], 'High meaning and demand, but emotionally and physically taxing shifts.'),
  c('therapist', 'Counsellor or therapist', 'SAI', [65, 65, 55, 95, 25, 55, 60, 50], 'Deeply meaningful, though emotionally heavy with limited income ceilings.'),
  c('teacher', 'School teacher', 'SAE', [40, 80, 50, 90, 45, 65, 55, 40], 'Stable and meaningful, with workload spillover and constrained pay.'),
  c('professor', 'University lecturer', 'ISA', [75, 60, 75, 65, 45, 65, 50, 60], 'Autonomy and ideas, balanced against publish-or-perish pressure.'),
  c('social-worker', 'Social worker', 'SEC', [40, 75, 40, 95, 30, 75, 35, 40], 'High purpose, high caseload and emotional strain.'),
  c('ux-designer', 'UX or product designer', 'AIE', [60, 60, 55, 50, 35, 55, 60, 70], 'Creative and collaborative, with constant negotiation over taste and constraints.'),
  c('graphic-designer', 'Graphic designer', 'AER', [60, 50, 40, 25, 20, 55, 55, 50], 'Visual craft day to day, with client revisions and uneven income.'),
  c('writer', 'Writer or journalist', 'AIE', [75, 35, 50, 40, 20, 60, 45, 40], 'Creative autonomy, with unstable income and public criticism.'),
  c('filmmaker', 'Filmmaker or video producer', 'AES', [70, 30, 55, 35, 55, 80, 25, 45], 'High creative ownership, irregular work and heavy project crunches.'),
  c('musician', 'Professional musician', 'ASE', [80, 20, 55, 35, 35, 65, 35, 30], 'Rare chance to live in your craft, with high income uncertainty.'),
  c('architect', 'Architect', 'AIR', [55, 60, 70, 40, 40, 75, 35, 65], 'Creative and tangible, with long hours and slow routes to authorship.'),
  c('chef', 'Chef', 'AER', [50, 45, 45, 35, 55, 90, 15, 40], 'Craft and teamwork, but long, late hours.'),
  c('entrepreneur', 'Founder or small business owner', 'ESA', [95, 20, 65, 40, 85, 90, 20, 70], 'Maximum autonomy with high uncertainty and heavy workload, and income can be zero.'),
  c('sales-leader', 'Sales or business development', 'ECS', [60, 55, 55, 30, 50, 70, 40, 85], 'Performance-linked pay, with pressure on targets and rejection.'),
  c('marketing', 'Marketing or brand manager', 'EAC', [55, 60, 55, 30, 50, 60, 55, 65], 'Creative and commercial, though results can be hard to attribute.'),
  c('consultant', 'Management consultant', 'ECI', [45, 55, 80, 30, 55, 90, 15, 90], 'Fast learning and prestige, with intense hours and travel.'),
  c('finance-analyst', 'Investment or finance analyst', 'CEI', [45, 60, 75, 15, 35, 85, 20, 90], 'Strong pay and prestige at a high cost in hours and stress.'),
  c('accountant', 'Accountant or auditor', 'CEI', [40, 85, 55, 20, 25, 60, 55, 60], 'Stable and structured, with seasonal peaks and repetitive tasks.'),
  c('project-manager', 'Project or programme manager', 'ECS', [50, 70, 55, 35, 70, 70, 45, 70], 'Leadership and variety, though you carry accountability without full control.'),
  c('hr', 'HR or people operations', 'SEC', [45, 70, 45, 70, 50, 55, 60, 55], 'People-focused, with policy work and difficult conversations.'),
  c('lawyer', 'Lawyer', 'EIC', [45, 70, 85, 50, 45, 90, 20, 85], 'Prestige and intellectual challenge, with demanding hours and adversarial settings.'),
  c('policy', 'Policy analyst or civil servant', 'ICS', [40, 90, 55, 65, 30, 50, 75, 55], 'Stable and mission-oriented, with slow change and bureaucracy.'),
  c('nonprofit', 'Nonprofit programme manager', 'SEA', [55, 45, 40, 90, 55, 70, 40, 35], 'Mission-driven, with limited resources and lower pay.'),
  c('engineer-mech', 'Mechanical or civil engineer', 'RIC', [45, 75, 60, 35, 35, 60, 60, 70], 'Tangible problems and steady demand, with regulation-heavy practice.'),
  c('electrician', 'Electrician or skilled tradesperson', 'RCE', [60, 80, 40, 40, 30, 65, 55, 60], 'Hands-on work with reliable demand, and physical wear over time.'),
  c('mechanic', 'Automotive or aviation technician', 'RCI', [45, 80, 40, 35, 20, 60, 60, 55], 'Practical problem-solving, with physical strain and repetitive jobs.'),
  c('pilot', 'Pilot', 'REI', [45, 60, 75, 25, 55, 70, 35, 85], 'Prestige and adventure, with long time away and strict regulation.'),
  c('farmer', 'Farmer or land manager', 'RIE', [85, 45, 35, 35, 40, 85, 25, 40], 'Strong autonomy and nature, with weather, market and debt risk.'),
  c('forestry', 'Conservation or ecology work', 'RIS', [60, 55, 40, 70, 30, 60, 55, 40], 'Purposeful outdoor work, with funding-dependent roles and low pay.'),
  c('cybersecurity', 'Cybersecurity specialist', 'IRC', [55, 80, 60, 40, 25, 65, 50, 80], 'Puzzle-solving with strong demand, and on-call stress.'),
  c('lab-tech', 'Laboratory scientist or technician', 'IRC', [45, 70, 50, 45, 25, 55, 65, 55], 'Methodical work with clear procedure, and limited autonomy.'),
  c('pharmacist', 'Pharmacist', 'ICS', [40, 85, 65, 75, 30, 60, 60, 80], 'Stable and respected, with repetitive tasks in many settings.'),
  c('physio', 'Physiotherapist or occupational therapist', 'SRI', [55, 80, 55, 90, 30, 60, 60, 60], 'Visible patient progress, with physical workload.'),
  c('coach', 'Coach or personal trainer', 'SEA', [75, 40, 40, 85, 45, 70, 35, 45], 'Direct impact on people, with variable income and unsociable hours.'),
  c('event-planner', 'Event or experience producer', 'ECA', [55, 40, 45, 40, 65, 80, 25, 50], 'Variety and visible results, with deadline stress and long event days.'),
  c('realestate', 'Real estate agent or developer', 'ECR', [80, 30, 55, 30, 45, 70, 30, 75], 'Autonomy and uncapped income, with unpredictable commissions.'),
  c('librarian', 'Librarian or archivist', 'CIS', [50, 80, 40, 70, 20, 35, 85, 40], 'Calm and meaningful curation, with limited pay and funding.'),
  c('translator', 'Translator or interpreter', 'ASC', [80, 40, 45, 40, 15, 55, 60, 45], 'Language mastery and flexibility, with automation pressure and uneven work.'),
  c('game-designer', 'Game or interactive designer', 'AIR', [55, 45, 50, 25, 35, 75, 35, 60], 'Creative and technical blend, with crunch cycles and volatile studios.'),
  c('operations', 'Operations or logistics manager', 'ECR', [50, 75, 50, 30, 65, 70, 45, 65], 'Leadership and systems thinking, with on-call and firefighting.'),
  c('ux-research', 'User researcher', 'ISA', [60, 60, 50, 60, 30, 50, 65, 65], 'People-focused inquiry, though it can depend on influence rather than authority.'),
  c('physician-assistant', 'Emergency or paramedic services', 'SRE', [35, 75, 55, 95, 35, 90, 25, 55], 'Intense, meaningful work with trauma exposure and shift patterns.'),
]
