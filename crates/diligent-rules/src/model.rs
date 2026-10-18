//! Typed models for the three shipped data files.
//!
//! Every field present in `schemas/authority-registry.schema.json`,
//! `schemas/rule-pack.schema.json` and `schemas/profile.schema.json` is
//! modelled. Where a schema has `additionalProperties: false` and an
//! `extensions` object, the struct keeps `extensions` as raw JSON. Everywhere
//! else unknown fields are rejected so drift between the schemas and these
//! types is caught at load time.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

// ---------------------------------------------------------------------------
// Authority registry
// ---------------------------------------------------------------------------

/// `authorities/registry.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthorityRegistry {
    pub schema_version: String,
    pub registry_version: String,
    pub authorities: Vec<Authority>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extensions: Option<Value>,
}

/// One legal or technical authority.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Authority {
    pub authority_id: String,
    pub title: String,
    pub issuer: String,
    pub kind: AuthorityKind,
    pub official_url: Option<String>,
    pub version_url: Option<String>,
    pub publication_date: Option<String>,
    pub effective_date: Option<String>,
    pub transition_dates: Vec<TransitionDate>,
    pub status: AuthorityStatus,
    pub supersedes: Option<Vec<String>>,
    pub superseded_by: Option<Vec<String>>,
    pub scope_note: String,
    pub source_verified_at: Option<String>,
    pub source_review_status: SourceReviewStatus,
    pub content_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extensions: Option<Value>,
}

/// A labelled date, used by authorities and by rule temporal blocks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransitionDate {
    pub label: String,
    pub date: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthorityKind {
    Statute,
    Regulation,
    ExecutiveOrder,
    BindingPolicy,
    Contract,
    AgencyPolicy,
    TechnicalStandard,
    Guidance,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthorityStatus {
    Active,
    Pending,
    Draft,
    Superseded,
    Withdrawn,
    Disputed,
    Unverified,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceReviewStatus {
    Research,
    InReview,
    Reviewed,
}

// ---------------------------------------------------------------------------
// Rule packs
// ---------------------------------------------------------------------------

/// `rules/packs/<pack_id>.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RulePack {
    pub schema_version: String,
    pub pack_id: String,
    pub pack_version: String,
    pub profile_ids: Vec<String>,
    /// `false` marks product engineering rules that are not legal obligations.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub normative: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    pub rules: Vec<Rule>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extensions: Option<Value>,
}

/// One atomic rule.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    pub rule_id: String,
    pub obligation_id: String,
    pub check_ids: Vec<String>,
    pub version: String,
    pub lifecycle: Lifecycle,
    pub obligation: String,
    pub authority: RuleAuthority,
    pub jurisdiction: String,
    pub applicability: Applicability,
    pub temporal: Temporal,
    pub evaluation: Evaluation,
    pub channel_support: ChannelSupport,
    pub blind_spots: Vec<String>,
    pub fixtures: Fixtures,
    pub remediation_constraints: Vec<String>,
    pub owner_role: String,
    pub severity_default: Severity,
    pub source_verified_at: Option<String>,
    pub source_review_status: SourceReviewStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extensions: Option<Value>,
    /// `false` marks a product engineering rule that is not a legal obligation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub normative: Option<bool>,

    // Forward-compatible fields (research doc 4.2). The shipped data does not
    // carry them yet, so they deserialize as `None`.
    /// ICT Testing Baseline web test ID, for example `"08"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub baseline_test_id: Option<String>,
    /// Which WCAG success criteria this rule maps to, and in which role.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wcag_mappings: Option<Vec<WcagMapping>>,
    /// Where the underlying standard sits in its adoption path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub standards_status: Option<StandardsStatus>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Lifecycle {
    Research,
    Draft,
    Validated,
    Published,
    Retired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Critical,
    High,
    Medium,
    Low,
}

/// The authority a rule cites. `authority_id` may be null for non-normative rules.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuleAuthority {
    pub authority_id: Option<String>,
    pub section: Option<String>,
    pub modality: Option<Modality>,
    pub incorporated_standard: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Modality {
    Shall,
    Should,
    May,
    Must,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Applicability {
    pub predicate: Predicate,
    pub required_facts: Vec<String>,
}

/// A predicate is either a named predicate or a structured object.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Predicate {
    Named(String),
    Structured(Map<String, Value>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Temporal {
    pub effective: Option<String>,
    pub transition: Option<Transition>,
    pub pending_effective_date: bool,
}

/// A transition is free text or a list of labelled dates.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Transition {
    Text(String),
    Dates(Vec<TransitionDate>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Evaluation {
    pub channels: Vec<String>,
    pub required_states: Vec<String>,
    pub automatic_pass_scope: String,
}

/// Support level per test channel (audit specification section 3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChannelSupport {
    pub source: Support,
    pub browser: Support,
    pub network: Support,
    pub model: Support,
    pub human: Support,
    pub documentary: Support,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Support {
    Automated,
    Assisted,
    Human,
    Documentary,
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Fixtures {
    pub positive: Vec<String>,
    pub negative: Vec<String>,
    pub exception: Vec<String>,
    pub insufficient: Vec<String>,
    pub regression: Vec<String>,
}

/// One WCAG success-criterion mapping on a rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WcagMapping {
    pub version: WcagVersion,
    pub level: WcagLevel,
    /// Success criterion number, for example `"1.4.3"`.
    pub criterion: String,
    pub role: WcagRole,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WcagVersion {
    #[serde(rename = "2.0")]
    V2_0,
    #[serde(rename = "2.1")]
    V2_1,
    #[serde(rename = "2.2")]
    V2_2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WcagLevel {
    A,
    AA,
    AAA,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WcagRole {
    LegalFloor,
    TitleIiTarget,
    Recommended,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StandardsStatus {
    Research,
    Draft,
    Pending,
    Required,
}

// ---------------------------------------------------------------------------
// Profiles
// ---------------------------------------------------------------------------

/// `profiles/<profile_id>.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub schema_version: String,
    pub profile_id: String,
    pub title: String,
    pub jurisdiction: String,
    pub status: ProfileStatus,
    pub selected_packs: Vec<String>,
    pub required_facts: Vec<String>,
    pub claim_boundary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub corroboration: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extensions: Option<Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProfileStatus {
    Draft,
    Research,
}
