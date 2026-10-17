/**
 * Scope fact catalog: the field groups of blueprint section 2, expressed as
 * facts that start in state `unknown`. Each carries the question to ask, who
 * can answer it, which obligations it gates, and what an unanswered fact does
 * to the final assessment.
 */

const GROUPS = {
  government_owner: {
    owner_role: 'program owner or agency counsel',
    affected: ['authority-selection', 'all-packs'],
    effect: 'Without this fact the jurisdiction and the authority set cannot be selected, so every dependent obligation stays unknown and the assessment stays incomplete.'
  },
  audience_and_channel: {
    owner_role: 'product owner',
    affected: ['federal-public-web', 'a11y-wcag21-aa'],
    effect: 'Public-only requirements must not be applied to internal tools, so applicability of those obligations stays unknown.'
  },
  lifecycle: {
    owner_role: 'product owner',
    affected: ['a11y-wcag21-aa', 'a11y-wcag22-aa', 'federal-public-web'],
    effect: 'Launch and alteration dates decide transition treatment and evidence validity, so date-gated obligations stay unknown or pending.'
  },
  system: {
    owner_role: 'system owner or information security officer',
    affected: ['infrastructure'],
    effect: 'Security requirements are tailored at system level, so infrastructure obligations stay unknown.'
  },
  data: {
    owner_role: 'privacy officer',
    affected: ['privacy-collection'],
    effect: 'Data types activate privacy and sector rules, so privacy obligations stay unknown.'
  },
  service: {
    owner_role: 'product owner',
    affected: ['ux-service', 'privacy-collection'],
    effect: 'The service type decides identity, form and transaction obligations and which service review profile applies.'
  },
  reach: {
    owner_role: 'program owner',
    affected: ['authority-selection', 'federal-public-web'],
    effect: 'Reach drives newer federal service policies and state and local ADA dates, so those obligations stay unknown.'
  },
  dependencies: {
    owner_role: 'technical lead',
    affected: ['infrastructure', 'privacy-collection', 'a11y-wcag21-aa'],
    effect: 'Vendors and embeds extend review beyond the first-party code, so third-party coverage stays unresolved.'
  },
  adopted_standards: {
    owner_role: 'technical lead or contracting officer representative',
    affected: ['a11y-wcag20-aa', 'a11y-wcag21-aa', 'a11y-wcag22-aa'],
    effect: 'A modern product target must not be confused with the legal minimum, so the WCAG version to assess stays undecided.'
  },
  governance: {
    owner_role: 'contracting officer representative or compliance lead',
    affected: ['all-packs'],
    effect: 'Contracts, waivers and records schedules make organizational obligations auditable, so they remain unverified.'
  },
  permission_and_evidence: {
    owner_role: 'program owner',
    affected: ['all-packs'],
    effect: 'Without permitted origins, roles and fixtures no live collection is allowed, so runtime obligations remain untested.'
  }
};

/** [fact_id, label, question] per group. */
const FACTS = {
  government_owner: [
    ['country', 'Country', 'Which country governs this service?'],
    ['jurisdiction', 'Jurisdiction', 'Is this a federal, state, local, tribal, territorial, or District of Columbia service?'],
    ['branch', 'Branch of government', 'Which branch operates the service (executive, legislative, judicial)?'],
    ['agency', 'Agency', 'Which agency or department owns the service?'],
    ['sub_agency', 'Sub-agency', 'Which sub-agency or office operates it?'],
    ['program', 'Program', 'Which program or statute does the service deliver?'],
    ['contractor_relationship', 'Contractor relationship', 'Is the service built or operated by a contractor, and under which vehicle?']
  ],
  audience_and_channel: [
    ['audience', 'Audience', 'Is the audience public, employee, contractor, or mixed?'],
    ['channel', 'Delivery channel', 'Is this a website, intranet, mobile app, PDF, API, or kiosk?']
  ],
  lifecycle: [
    ['launch_or_redesign_dates', 'Launch and redesign dates', 'When did the service launch and when was it last redesigned?'],
    ['legacy_components', 'Legacy components', 'Which components predate the current standards and have not been altered?'],
    ['material_alterations', 'Material alterations', 'What has been materially altered since the relevant effective dates?'],
    ['environments', 'Environments', 'Which environments exist (production, staging, test) and which is in scope?']
  ],
  system: [
    ['system_boundary', 'System boundary', 'What is the authorization boundary of the system?'],
    ['authorizing_official', 'Authorizing official', 'Who is the authorizing official?'],
    ['fips199_category', 'FIPS 199 category', 'What is the FIPS 199 security category?'],
    ['control_baseline', 'Control baseline', 'Which control baseline applies?'],
    ['inherited_controls', 'Inherited controls', 'Which controls are inherited from a provider?'],
    ['ato_status', 'ATO status', 'What is the authorization to operate status?']
  ],
  data: [
    ['pii', 'Personally identifiable information', 'Does the service collect or display personally identifiable information?'],
    ['privacy_act_records', 'Privacy Act records', 'Do records retrieved by personal identifier form a Privacy Act system of records?'],
    ['cui', 'Controlled unclassified information', 'Does the service handle controlled unclassified information?'],
    ['sector_data', 'Sector data', 'Does it handle health, education, tax, payment, or children\'s data?'],
    ['retrieval_identifiers', 'Retrieval identifiers', 'Which identifiers are used to retrieve records?']
  ],
  service: [
    ['service_kind', 'Service kind', 'Is the service information-only, an application, benefits, payment, authentication, identity proofing, or a legal attestation?']
  ],
  reach: [
    ['annual_users', 'Annual users', 'How many annual users does the service have?'],
    ['hisp_designation', 'High impact service provider designation', 'Is the agency or service designated a high impact service provider?'],
    ['population_special_district_status', 'Population and special district status', 'For state and local services, what is the population served and any special district status?']
  ],
  dependencies: [
    ['hosting', 'Hosting', 'Where is the service hosted?'],
    ['cdn', 'Content delivery network', 'Which content delivery network is used?'],
    ['analytics', 'Analytics', 'Which analytics services are loaded?'],
    ['fonts', 'Font providers', 'Which font providers are loaded?'],
    ['embeds', 'Embeds', 'Which third-party embeds appear on in-scope pages?'],
    ['identity_provider', 'Identity provider', 'Which identity provider handles sign-in?'],
    ['captcha', 'CAPTCHA', 'Is a CAPTCHA or bot challenge used?'],
    ['ai_services', 'AI services', 'Which AI services process user content?'],
    ['vendors', 'Other vendors', 'Which other vendors have a role in the service?']
  ],
  adopted_standards: [
    ['wcag_version_level', 'WCAG version and level', 'Which WCAG version and conformance level does the contract or policy adopt?'],
    ['uswds_version', 'USWDS version', 'Which U.S. Web Design System version and maturity is used, if any?'],
    ['agency_design_system', 'Agency design system', 'Which agency design system applies?'],
    ['supported_browsers', 'Supported browsers', 'Which browsers and assistive technologies must be supported?']
  ],
  governance: [
    ['contracts_acquisition_clauses', 'Contract and acquisition clauses', 'Which accessibility clauses appear in the contract or acquisition?'],
    ['waiver_exception_records', 'Waiver and exception records', 'Are there approved waivers or exception records?'],
    ['content_owners', 'Content owners', 'Who owns the content on each in-scope page?'],
    ['records_schedules', 'Records schedules', 'Which records schedules apply?']
  ],
  permission_and_evidence: [
    ['allowed_origins', 'Allowed origins', 'Which origins may be tested?'],
    ['roles', 'Roles', 'Which user roles may be exercised?'],
    ['routes', 'Routes', 'Which routes are in scope?'],
    ['test_accounts', 'Test accounts', 'Which test accounts may be used?'],
    ['safe_fixtures', 'Safe fixtures', 'Which synthetic fixtures may be submitted?'],
    ['prohibited_actions', 'Prohibited actions', 'Which actions are prohibited (live submissions, payments, record changes)?']
  ]
};

/** Build the initial fact list: every fact unknown, with its question and routing. */
export function buildInitialFacts() {
  const facts = [];
  for (const [group, entries] of Object.entries(FACTS)) {
    const defaults = GROUPS[group];
    for (const [factId, label, question] of entries) {
      facts.push({
        fact_id: factId,
        group,
        label,
        value: null,
        state: 'unknown',
        evidence_refs: [],
        provided_by: null,
        observed_at: null,
        valid_until: null,
        question,
        owner_role: defaults.owner_role,
        affected_obligations: [...defaults.affected],
        effect_on_assessment: defaults.effect
      });
    }
  }
  return facts;
}

export const FACT_GROUPS = Object.keys(FACTS);
