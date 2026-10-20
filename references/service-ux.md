# Service and UX assessment

Establish what the service is for before assessing hierarchy or proposing structural changes. An existing government service has approved identity and user needs, and those constrain every recommendation. Never assign a new visual direction to an existing service.

The profiles and rules in this file are product review guidance, not legal obligations. A separate authority mapping can make a particular outcome mandatory. Findings from this file never enter the regulatory gate unless they are separately mapped to an applicable binding obligation.

Diligent 0.2.1 has no automated UX check. Craft and clarity signals from the companion binary are advisory and are not rules. Every `UX-*` rule is a human or model-assisted review question that an agent can route and record.

## 1. Service brief

Capture the brief in `SERVICE-BRIEF.json` before assessing structure. Every task and assumption carries provenance: `owner_confirmed`, `research_supported`, `operational_data`, or `inferred`. An inferred primary task cannot justify deleting important content or blocking a release on subjective UX grounds.

| Field | Required content | If missing |
| --- | --- | --- |
| `service_id`, `service_name`, `owner` | Program and accountable product owner | Record unknown ownership |
| `subject` | What the service explains or provides; topic boundaries | Draft a hypothesis from supplied evidence |
| `audiences` | Primary and secondary users, representatives, assisted users, staff | Do not invent demographics |
| `jobs` | User goal, entry condition, desired outcome, task priority | Propose task hypotheses for confirmation |
| `critical_questions` | Eligibility, preparation, cost, timing, consequences, next steps | Flag missing answers in the content audit |
| `journeys` | Entry, decision, action, confirmation, follow-up, help | Inventory visible paths; mark inaccessible steps |
| `failure_consequences` | Lost access, missed deadline, wrong submission, cost, disclosure | Owner review required before prioritizing on high impact |
| `constraints` | Legal text, required fields, assurance, design system, backend limits | Protect known constraints; mark unknowns |
| `contexts` | Mobile, bandwidth, language, disability, literacy, stress, shared devices | Treat possible contexts as research questions |
| `evidence` | Research, analytics, support and search logs, content authority | Label heuristic judgments as unvalidated |
| `success_measures` | Definition, numerator, denominator, baseline, target, window | Recommend measures; do not invent targets |
| `non_goals` | Explicitly unsupported transactions or audiences | Avoid recommending unapproved scope expansion |

## 2. Service-type review profiles

Activate more than one profile when needed. Program-specific notice, appeal, eligibility, and timing requirements must come from an approved service pack, not from these generic profiles.

| Profile | Key user questions | Required review scenarios |
| --- | --- | --- |
| Public information or policy | Does this apply to me? Is it current? What should I do? | Search entry, topic scan, authority and date check, related action and contact |
| Application, benefit, or permit | Am I eligible? What do I need? What happens after submission? | Eligibility, preparation, valid and invalid input, save and resume, review, receipt, status, correction |
| Payment | What am I paying, how much, and was it received? | Fee explanation, payment method, failure, ambiguous timeout, duplicate prevention, receipt |
| Appointment | What service, location, and time do I need? Can I change it? | Availability, time zone, access needs, confirmation, cancellation and rescheduling |
| Search or directory | Can I find the right service or person? | Common query, typo and synonym, filters, zero results, stale or duplicate entries, mobile results |
| Account or status | What is happening and what action is needed? | Sign-in and recovery, empty state, pending and rejected status, next action, representative access |
| Dashboard or data | What does this number mean and what decision can I make? | Units, period, definitions, uncertainty, missing data, filters, export, nonvisual alternative |
| Emergency or time-sensitive | Is this relevant now and what must I do first? | Location and time relevance, urgent action, low bandwidth, accessible alternatives, expired alert |

## 3. UX rule family, UX-001 to UX-018

Each rule has an output state: `supported`, `barrier_observed`, `hypothesis`, `not_assessed`, or `not_applicable`. These are UX states, not legal conformance states. A `hypothesis` needs a research question and a validation method. Do not fail something on reviewer taste alone. Link every finding to a confirmed task or an explicit hypothesis.

| Rule | Review question and evidence | Repair direction |
| --- | --- | --- |
| UX-001 | Is page/service purpose clear from title, heading and opening content? Compare with the confirmed task. | State service and relevant audience plainly. |
| UX-002 | Does visual and reading order prioritize the user's next decision? Inspect desktop/mobile and semantic order. | Adjust grouping/emphasis while preserving required content. |
| UX-003 | Can users answer eligibility and preparation questions before investing effort? Check approved program content. | Explain conditions, documents, fees and expected time early. |
| UX-004 | Do navigation and labels use audience language? Compare search/support terminology and task tests. | Rename ambiguous labels without changing legal meaning. |
| UX-005 | Are next actions distinguishable and accurately named? Review action sets and outcomes. | Clarify labels and primary/secondary/destructive hierarchy. |
| UX-006 | Does every collected field have a documented purpose? Compare field inventory with approved collection. | Remove only with authorized collection/product review. |
| UX-007 | Can users prevent and recover from errors? Exercise validation, back, retry and corrections. | Explain correction, preserve safe valid input, offer recovery. |
| UX-008 | Can people understand status and next steps? Inspect submission, pending and rejected states. | Provide receipt, expected timing and action/help paths. |
| UX-009 | Is help available at the point of difficulty? Check content, contact and handoff behavior. | Add contextual guidance and a usable support route. |
| UX-010 | Is avoidable cognitive burden reduced? Identify memorization, unexplained terms and repeated steps. | Reduce repetition and provide contextual explanations. |
| UX-011 | Can users work with realistic interruptions and devices? Exercise resume, orientation, mobile and poor network. | Make interruption/recovery behavior explicit and reliable. |
| UX-012 | Are language and alternative access paths usable? Follow the path through the task, not only a selector. | Align translated content, routing and support. |
| UX-013 | Is content accurate, owned and timely? Compare with approved program source and update record. | Correct with program owner review and remove stale conflicts. |
| UX-014 | Is search effective for top tasks? Test an owner-approved query set and zero-result handling. | Improve indexing, naming, result context and recovery. |
| UX-015 | Does typography/spacing support scanning and comprehension? Inspect actual content and user tasks. | Improve hierarchy and grouping within approved tokens. |
| UX-016 | Do charts/tables support correct interpretation? Check definitions, units, uncertainty and alternatives. | Add context, accessible equivalent and useful summaries. |
| UX-017 | Are people informed about consequences and choices? Check consent, fees, destructive and final actions. | Explain consequence before commitment; preserve real choice. |
| UX-018 | Is the complete outcome achievable across channels/vendors? Follow handoffs and return paths. | Fix breaks in context, state, accountability and confirmation. |

Supplemental cognitive-accessibility guidance informs review. It goes beyond WCAG conformance and is not automatically mandatory. Task-oriented implementation patterns in a design system are references; choosing a component does not establish that the assembled journey is accessible or usable.

## 4. Research and measurement protocol

Create a research plan with audience segments, recruitment rationale, task scenarios, assistive technology needs, accessible participation arrangements, observation questions, consent and data handling, and a synthesis method. Participant counts depend on the question and design. Do not claim statistical certainty from a few sessions. Model-generated personas and simulated sessions are hypotheses, not user research.

- **Task success** is correct outcome divided by eligible attempts. Report assistance separately.
- **Error rate** is defined by opportunity, not by arbitrary page count.
- **Comprehension** of eligibility, consequence, and next steps is measured with scenario questions.
- **Abandonment** is recorded by step with known causes where possible. An informed decision not to apply, for example after correctly learning ineligibility, is a successful outcome, not a failure.
- **Time** is measured alongside accuracy and accessibility, never as the sole success signal.
- For comparative quantitative studies, predefine the outcome, the meaningful effect, sample and power assumptions, the uncertainty method, and stopping rules. For qualitative studies, document observed barriers, contradictory evidence, and remaining audience gaps.
- Do not collect new sensitive demographic data merely to populate a dashboard. Use approved research and data practices.

Use actual users alongside expert accessibility review. Connect feedback to service improvement with federal customer-experience resources, but verify the applicable current circular provisions before treating a collection or reporting duty as mandatory.

## 5. Translating agency requirements into constraints

A component can satisfy its keyboard and naming requirements while its use in a form violates an approved collection or notice requirement. Conversely, an agency branding preference does not excuse inaccessible contrast or focus behavior. Record each program obligation's consequence for the task flow in the service brief, link it to pattern rules and repair acceptance tests, and update them together when a requirement changes.
