# Supported checks

**The installable Diligent 0.1.1 package ships zero automated website checks.** Every rule below is at the `research` lifecycle and none is published. A rule listed here is a cataloged obligation, not an executable test. The "automated" and "assisted" Network cells and the counts of 5 (federal-public-web), 5 (infrastructure), and 10 (total) in this document refer to the `diligent-rs` native companion engine. That engine is built from source in the development repository and is not yet distributed, so none of those checks runs from the 0.1.1 package.

What 0.1.1 does ship: the router, the artifact chain and schemas, the authority registry, the rule-pack format and seven research packs, and a deterministic gate that computes an overall status from results an agent or human supplies. The gate can only report `fail` when supplied results show a confirmed failure of an applicable mandatory obligation. It cannot report a pass for anything that was not observed and recorded.

## Channel support values

| Value | Meaning in this matrix |
| --- | --- |
| `automated` | A shipped check produces the result. None in the 0.1.1 package |
| `assisted` | A shipped check produces a lead that a person or model must confirm. None in the 0.1.1 package |
| `human` | The rule routes the question to a qualified person through the review queue |
| `documentary` | The rule routes the question to organizational evidence the owner supplies |
| `unsupported` | No supported path in the 0.1.1 package. A required unsupported check keeps the assessment incomplete |

Channels are source, browser, network, model, human, and documentary. A cell shows `unsupported` for a channel the audit specification proposes but the 0.1.1 package does not implement. A Network cell marked `automated` or `assisted` is implemented only by the `diligent-rs` companion engine.

## Summary

| Pack | Rules | Human route | Documentary route | Automated or assisted (`diligent-rs` only) |
| --- | --- | --- | --- | --- |
| a11y-wcag20-aa | 38 | 29 | 3 | 0 |
| a11y-wcag21-aa | 12 | 7 | 0 | 0 |
| a11y-wcag22-aa | 6 | 3 | 0 | 0 |
| federal-public-web | 42 | 20 | 27 | 5 |
| privacy-collection | 17 | 0 | 16 | 0 |
| infrastructure | 26 | 2 | 18 | 5 |
| ux-service | 18 | 18 | 0 | 0 |
| total | 159 |  |  | 10 |

## Matrix

Every rule in every pack, with its support in each channel.

| Pack | Rule | Outcome | Source | Browser | Network | Model | Human | Documentary |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| a11y-wcag20-aa | A11Y-1.1.1 | Meet WCAG 2.0 success criterion 1.1.1 Non-text content at level A where the... | unsupported | unsupported | unsupported | unsupported | human | unsupported |
| a11y-wcag20-aa | A11Y-1.2.1 | Meet WCAG 2.0 success criterion 1.2.1 Audio/video only at level A where the... | unsupported | unsupported | unsupported | unsupported | human | unsupported |
| a11y-wcag20-aa | A11Y-1.2.2 | Meet WCAG 2.0 success criterion 1.2.2 Prerecorded captions at level A where the... | unsupported | unsupported | unsupported | unsupported | human | unsupported |
| a11y-wcag20-aa | A11Y-1.2.3 | Meet WCAG 2.0 success criterion 1.2.3 Description or media alternative at level A... | unsupported | unsupported | unsupported | unsupported | human | unsupported |
| a11y-wcag20-aa | A11Y-1.2.4 | Meet WCAG 2.0 success criterion 1.2.4 Live captions at level AA where the incorporated... | unsupported | unsupported | unsupported | unsupported | human | documentary |
| a11y-wcag20-aa | A11Y-1.2.5 | Meet WCAG 2.0 success criterion 1.2.5 Prerecorded audio description at level AA where... | unsupported | unsupported | unsupported | unsupported | human | unsupported |
| a11y-wcag20-aa | A11Y-1.3.1 | Meet WCAG 2.0 success criterion 1.3.1 Relationships at level A where the incorporated... | unsupported | unsupported | unsupported | unsupported | human | unsupported |
| a11y-wcag20-aa | A11Y-1.3.2 | Meet WCAG 2.0 success criterion 1.3.2 Meaningful sequence at level A where the... | unsupported | unsupported | unsupported | unsupported | human | unsupported |
| a11y-wcag20-aa | A11Y-1.3.3 | Meet WCAG 2.0 success criterion 1.3.3 Sensory characteristics at level A where the... | unsupported | unsupported | unsupported | unsupported | human | unsupported |
| a11y-wcag20-aa | A11Y-1.4.1 | Meet WCAG 2.0 success criterion 1.4.1 Color at level A where the incorporated standard... | unsupported | unsupported | unsupported | unsupported | human | unsupported |
| a11y-wcag20-aa | A11Y-1.4.2 | Meet WCAG 2.0 success criterion 1.4.2 Audio control at level A where the incorporated... | unsupported | unsupported | unsupported | unsupported | unsupported | unsupported |
| a11y-wcag20-aa | A11Y-1.4.3 | Meet WCAG 2.0 success criterion 1.4.3 Text contrast at level AA where the incorporated... | unsupported | unsupported | unsupported | unsupported | unsupported | unsupported |
| a11y-wcag20-aa | A11Y-1.4.4 | Meet WCAG 2.0 success criterion 1.4.4 Resize text at level AA where the incorporated... | unsupported | unsupported | unsupported | unsupported | human | unsupported |
| a11y-wcag20-aa | A11Y-1.4.5 | Meet WCAG 2.0 success criterion 1.4.5 Images of text at level AA where the... | unsupported | unsupported | unsupported | unsupported | human | unsupported |
| a11y-wcag20-aa | A11Y-2.1.1 | Meet WCAG 2.0 success criterion 2.1.1 Keyboard at level A where the incorporated... | unsupported | unsupported | unsupported | unsupported | human | unsupported |
| a11y-wcag20-aa | A11Y-2.1.2 | Meet WCAG 2.0 success criterion 2.1.2 No keyboard trap at level A where the... | unsupported | unsupported | unsupported | unsupported | human | unsupported |
| a11y-wcag20-aa | A11Y-2.2.1 | Meet WCAG 2.0 success criterion 2.2.1 Timing at level A where the incorporated... | unsupported | unsupported | unsupported | unsupported | unsupported | documentary |
| a11y-wcag20-aa | A11Y-2.2.2 | Meet WCAG 2.0 success criterion 2.2.2 Moving/updating content at level A where the... | unsupported | unsupported | unsupported | unsupported | human | unsupported |
| a11y-wcag20-aa | A11Y-2.3.1 | Meet WCAG 2.0 success criterion 2.3.1 Flashes at level A where the incorporated... | unsupported | unsupported | unsupported | unsupported | human | unsupported |
| a11y-wcag20-aa | A11Y-2.4.1 | Meet WCAG 2.0 success criterion 2.4.1 Bypass blocks at level A where the incorporated... | unsupported | unsupported | unsupported | unsupported | human | unsupported |
| a11y-wcag20-aa | A11Y-2.4.2 | Meet WCAG 2.0 success criterion 2.4.2 Page title at level A where the incorporated... | unsupported | unsupported | unsupported | unsupported | unsupported | unsupported |
| a11y-wcag20-aa | A11Y-2.4.3 | Meet WCAG 2.0 success criterion 2.4.3 Focus order at level A where the incorporated... | unsupported | unsupported | unsupported | unsupported | human | unsupported |
| a11y-wcag20-aa | A11Y-2.4.4 | Meet WCAG 2.0 success criterion 2.4.4 Link purpose at level A where the incorporated... | unsupported | unsupported | unsupported | unsupported | human | unsupported |
| a11y-wcag20-aa | A11Y-2.4.5 | Meet WCAG 2.0 success criterion 2.4.5 Multiple ways at level AA where the incorporated... | unsupported | unsupported | unsupported | unsupported | human | unsupported |
| a11y-wcag20-aa | A11Y-2.4.6 | Meet WCAG 2.0 success criterion 2.4.6 Headings/labels at level AA where the... | unsupported | unsupported | unsupported | unsupported | human | unsupported |
| a11y-wcag20-aa | A11Y-2.4.7 | Meet WCAG 2.0 success criterion 2.4.7 Visible focus at level AA where the incorporated... | unsupported | unsupported | unsupported | unsupported | human | unsupported |
| a11y-wcag20-aa | A11Y-3.1.1 | Meet WCAG 2.0 success criterion 3.1.1 Page language at level A where the incorporated... | unsupported | unsupported | unsupported | unsupported | unsupported | unsupported |
| a11y-wcag20-aa | A11Y-3.1.2 | Meet WCAG 2.0 success criterion 3.1.2 Language changes at level AA where the... | unsupported | unsupported | unsupported | unsupported | human | unsupported |
| a11y-wcag20-aa | A11Y-3.2.1 | Meet WCAG 2.0 success criterion 3.2.1 Focus behavior at level A where the incorporated... | unsupported | unsupported | unsupported | unsupported | unsupported | unsupported |
| a11y-wcag20-aa | A11Y-3.2.2 | Meet WCAG 2.0 success criterion 3.2.2 Input behavior at level A where the incorporated... | unsupported | unsupported | unsupported | unsupported | human | unsupported |
| a11y-wcag20-aa | A11Y-3.2.3 | Meet WCAG 2.0 success criterion 3.2.3 Consistent navigation at level AA where the... | unsupported | unsupported | unsupported | unsupported | unsupported | unsupported |
| a11y-wcag20-aa | A11Y-3.2.4 | Meet WCAG 2.0 success criterion 3.2.4 Consistent identification at level AA where the... | unsupported | unsupported | unsupported | unsupported | unsupported | unsupported |
| a11y-wcag20-aa | A11Y-3.3.1 | Meet WCAG 2.0 success criterion 3.3.1 Error identification at level A where the... | unsupported | unsupported | unsupported | unsupported | human | unsupported |
| a11y-wcag20-aa | A11Y-3.3.2 | Meet WCAG 2.0 success criterion 3.3.2 Labels/instructions at level A where the... | unsupported | unsupported | unsupported | unsupported | human | unsupported |
| a11y-wcag20-aa | A11Y-3.3.3 | Meet WCAG 2.0 success criterion 3.3.3 Error suggestions at level AA where the... | unsupported | unsupported | unsupported | unsupported | human | unsupported |
| a11y-wcag20-aa | A11Y-3.3.4 | Meet WCAG 2.0 success criterion 3.3.4 Error prevention at level AA where the... | unsupported | unsupported | unsupported | unsupported | human | documentary |
| a11y-wcag20-aa | A11Y-4.1.1 | Meet WCAG 2.0 success criterion 4.1.1 Parsing at level A where the incorporated... | unsupported | unsupported | unsupported | unsupported | unsupported | unsupported |
| a11y-wcag20-aa | A11Y-4.1.2 | Meet WCAG 2.0 success criterion 4.1.2 Name/role/value at level A where the... | unsupported | unsupported | unsupported | unsupported | human | unsupported |
| a11y-wcag21-aa | A11Y-1.3.4 | Meet WCAG 2.1 success criterion 1.3.4 Orientation at level AA where WCAG 2.1 is the... | unsupported | unsupported | unsupported | unsupported | human | unsupported |
| a11y-wcag21-aa | A11Y-1.3.5 | Meet WCAG 2.1 success criterion 1.3.5 Input purpose at level AA where WCAG 2.1 is the... | unsupported | unsupported | unsupported | unsupported | unsupported | unsupported |
| a11y-wcag21-aa | A11Y-1.4.10 | Meet WCAG 2.1 success criterion 1.4.10 Reflow at level AA where WCAG 2.1 is the... | unsupported | unsupported | unsupported | unsupported | human | unsupported |
| a11y-wcag21-aa | A11Y-1.4.11 | Meet WCAG 2.1 success criterion 1.4.11 Non-text contrast at level AA where WCAG 2.1 is... | unsupported | unsupported | unsupported | unsupported | human | unsupported |
| a11y-wcag21-aa | A11Y-1.4.12 | Meet WCAG 2.1 success criterion 1.4.12 Text spacing at level AA where WCAG 2.1 is the... | unsupported | unsupported | unsupported | unsupported | unsupported | unsupported |
| a11y-wcag21-aa | A11Y-1.4.13 | Meet WCAG 2.1 success criterion 1.4.13 Hover/focus content at level AA where WCAG 2.1... | unsupported | unsupported | unsupported | unsupported | human | unsupported |
| a11y-wcag21-aa | A11Y-2.1.4 | Meet WCAG 2.1 success criterion 2.1.4 Character shortcuts at level A where WCAG 2.1 is... | unsupported | unsupported | unsupported | unsupported | unsupported | unsupported |
| a11y-wcag21-aa | A11Y-2.5.1 | Meet WCAG 2.1 success criterion 2.5.1 Pointer gestures at level A where WCAG 2.1 is... | unsupported | unsupported | unsupported | unsupported | human | unsupported |
| a11y-wcag21-aa | A11Y-2.5.2 | Meet WCAG 2.1 success criterion 2.5.2 Pointer cancellation at level A where WCAG 2.1... | unsupported | unsupported | unsupported | unsupported | unsupported | unsupported |
| a11y-wcag21-aa | A11Y-2.5.3 | Meet WCAG 2.1 success criterion 2.5.3 Label in name at level A where WCAG 2.1 is the... | unsupported | unsupported | unsupported | unsupported | unsupported | unsupported |
| a11y-wcag21-aa | A11Y-2.5.4 | Meet WCAG 2.1 success criterion 2.5.4 Motion actuation at level A where WCAG 2.1 is... | unsupported | unsupported | unsupported | unsupported | human | unsupported |
| a11y-wcag21-aa | A11Y-4.1.3 | Meet WCAG 2.1 success criterion 4.1.3 Status messages at level AA where WCAG 2.1 is... | unsupported | unsupported | unsupported | unsupported | human | unsupported |
| a11y-wcag22-aa | A11Y-2.4.11 | Meet WCAG 2.2 success criterion 2.4.11 Focus not obscured, minimum at level AA as a... | unsupported | unsupported | unsupported | unsupported | unsupported | unsupported |
| a11y-wcag22-aa | A11Y-2.5.7 | Meet WCAG 2.2 success criterion 2.5.7 Dragging at level AA as a product target. | unsupported | unsupported | unsupported | unsupported | human | unsupported |
| a11y-wcag22-aa | A11Y-2.5.8 | Meet WCAG 2.2 success criterion 2.5.8 Target size, minimum at level AA as a product... | unsupported | unsupported | unsupported | unsupported | unsupported | unsupported |
| a11y-wcag22-aa | A11Y-3.2.6 | Meet WCAG 2.2 success criterion 3.2.6 Consistent help at level A as a product target. | unsupported | unsupported | unsupported | unsupported | unsupported | unsupported |
| a11y-wcag22-aa | A11Y-3.3.7 | Meet WCAG 2.2 success criterion 3.3.7 Redundant entry at level A as a product target. | unsupported | unsupported | unsupported | unsupported | human | unsupported |
| a11y-wcag22-aa | A11Y-3.3.8 | Meet WCAG 2.2 success criterion 3.3.8 Accessible authentication at level AA as a... | unsupported | unsupported | unsupported | unsupported | human | unsupported |
| federal-public-web | FED-01 | Appropriate government domain and domain governance. | unsupported | unsupported | automated | unsupported | unsupported | documentary |
| federal-public-web | FED-02 | Clear official government identity. | unsupported | unsupported | unsupported | unsupported | human | unsupported |
| federal-public-web | FED-03 | Clear parent agency attribution. | unsupported | unsupported | unsupported | unsupported | unsupported | unsupported |
| federal-public-web | FED-04 | About information and applicable organizational disclosures. | unsupported | unsupported | unsupported | unsupported | unsupported | documentary |
| federal-public-web | FED-05 | Accessible accessibility statement and feedback path. | unsupported | unsupported | unsupported | unsupported | unsupported | documentary |
| federal-public-web | FED-06 | Privacy policy and collection-context notices. | unsupported | unsupported | assisted | unsupported | unsupported | documentary |
| federal-public-web | FED-07 | FOIA access and electronic reading-room duties. | unsupported | unsupported | unsupported | unsupported | unsupported | documentary |
| federal-public-web | FED-08 | No FEAR Act data link/content. | unsupported | unsupported | unsupported | unsupported | unsupported | documentary |
| federal-public-web | FED-09 | Budget/performance information. | unsupported | unsupported | unsupported | unsupported | unsupported | documentary |
| federal-public-web | FED-10 | Inspector General reporting access. | unsupported | unsupported | unsupported | unsupported | unsupported | documentary |
| federal-public-web | FED-11 | Public vulnerability reporting policy. | unsupported | unsupported | assisted | unsupported | unsupported | documentary |
| federal-public-web | FED-12 | Contact/help and policy access. | unsupported | unsupported | unsupported | unsupported | human | unsupported |
| federal-public-web | FED-13 | Appropriate government-wide support link. | unsupported | unsupported | unsupported | unsupported | unsupported | unsupported |
| federal-public-web | FED-14 | Consistent appearance across a service. | unsupported | unsupported | unsupported | unsupported | human | unsupported |
| federal-public-web | FED-15 | Appropriate federal design-system/website-standard adoption. | unsupported | unsupported | unsupported | unsupported | unsupported | documentary |
| federal-public-web | FED-16 | User-centered design informed by evidence. | unsupported | unsupported | unsupported | unsupported | human | documentary |
| federal-public-web | FED-17 | Public content can be searched. | unsupported | unsupported | unsupported | unsupported | unsupported | unsupported |
| federal-public-web | FED-18 | Public information is discoverable externally. | unsupported | unsupported | assisted | unsupported | unsupported | unsupported |
| federal-public-web | FED-19 | Search results are usable. | unsupported | unsupported | unsupported | unsupported | human | unsupported |
| federal-public-web | FED-20 | Authoritative, current, nonduplicative content. | unsupported | unsupported | unsupported | unsupported | unsupported | documentary |
| federal-public-web | FED-21 | Plain writing in covered public material. | unsupported | unsupported | unsupported | unsupported | human | unsupported |
| federal-public-web | FED-22 | Accurate policies, claims, eligibility and deadlines. | unsupported | unsupported | unsupported | unsupported | unsupported | documentary |
| federal-public-web | FED-23 | Functional and usable on common mobile devices. | unsupported | unsupported | unsupported | unsupported | human | unsupported |
| federal-public-web | FED-24 | Digital forms/services and signature alternatives. | unsupported | unsupported | unsupported | unsupported | unsupported | documentary |
| federal-public-web | FED-25 | Access for people unable to use digital services. | unsupported | unsupported | unsupported | unsupported | unsupported | documentary |
| federal-public-web | FED-26 | Shared services and appropriate customization. | unsupported | unsupported | unsupported | unsupported | human | documentary |
| federal-public-web | FED-27 | Digital Analytics Program use where applicable. | unsupported | unsupported | assisted | unsupported | unsupported | documentary |
| federal-public-web | FED-28 | Experience measurement and service improvement. | unsupported | unsupported | unsupported | unsupported | unsupported | documentary |
| federal-public-web | FED-29 | Language access under current applicable authority. | unsupported | unsupported | unsupported | unsupported | human | documentary |
| federal-public-web | FED-30 | Complete task states and recovery. | unsupported | unsupported | unsupported | unsupported | human | unsupported |
| federal-public-web | FED-31 | Honest visual priority and task flow. | unsupported | unsupported | unsupported | unsupported | human | unsupported |
| federal-public-web | FED-32 | Federal government banner (pending GSA standard). Readiness finding unless... | unsupported | unsupported | unsupported | unsupported | human | unsupported |
| federal-public-web | FED-33 | HTML page title (pending GSA standard). Readiness finding unless independently... | unsupported | unsupported | unsupported | unsupported | unsupported | unsupported |
| federal-public-web | FED-34 | Meta page description (pending GSA standard). Readiness finding unless independently... | unsupported | unsupported | unsupported | unsupported | unsupported | unsupported |
| federal-public-web | FED-35 | Correctly scope public and covered non-public electronic content. | unsupported | unsupported | unsupported | unsupported | human | documentary |
| federal-public-web | FED-36 | Evaluate applicable functional performance criteria and software requirements. | unsupported | unsupported | unsupported | unsupported | human | documentary |
| federal-public-web | FED-37 | Accessible documentation/support. | unsupported | unsupported | unsupported | unsupported | human | documentary |
| federal-public-web | FED-38 | Exceptions require authorized basis and alternative access where required. | unsupported | unsupported | unsupported | unsupported | human | documentary |
| federal-public-web | FED-39 | Prioritize HTML; use PDFs when necessary and accessible. | unsupported | unsupported | unsupported | unsupported | human | documentary |
| federal-public-web | FED-40 | Acquisition requirements must address applicable accessibility standards. | unsupported | unsupported | unsupported | unsupported | human | documentary |
| federal-public-web | FED-41 | Maintain accessibility program, testing/remediation evidence, and required reporting. | unsupported | unsupported | unsupported | unsupported | human | documentary |
| federal-public-web | FED-42 | ACR/VPAT is evidence, not certification of the deployed service. | unsupported | unsupported | unsupported | unsupported | human | documentary |
| privacy-collection | PRIV-01 | Identify lawful collection and data flows. | unsupported | unsupported | unsupported | unsupported | unsupported | documentary |
| privacy-collection | PRIV-02 | Privacy Act statements when applicable. | unsupported | unsupported | unsupported | unsupported | unsupported | documentary |
| privacy-collection | PRIV-03 | System of Records Notice where triggered. | unsupported | unsupported | unsupported | unsupported | unsupported | documentary |
| privacy-collection | PRIV-04 | Privacy impact assessment where triggered. | unsupported | unsupported | unsupported | unsupported | unsupported | documentary |
| privacy-collection | PRIV-05 | Tracking classification, notice and controls. | unsupported | unsupported | unsupported | unsupported | unsupported | documentary |
| privacy-collection | PRIV-06 | Third-party practices match disclosed/approved handling. | unsupported | unsupported | unsupported | unsupported | unsupported | documentary |
| privacy-collection | PRIV-07 | Prevent unnecessary disclosure of sensitive data. | unsupported | unsupported | unsupported | unsupported | unsupported | unsupported |
| privacy-collection | PRIV-08 | Rights, sharing and retention match authority. | unsupported | unsupported | unsupported | unsupported | unsupported | documentary |
| privacy-collection | PRIV-09 | PRA applicability determination. | unsupported | unsupported | unsupported | unsupported | unsupported | documentary |
| privacy-collection | PRIV-10 | Valid clearance and required display. | unsupported | unsupported | unsupported | unsupported | unsupported | documentary |
| privacy-collection | PRIV-11 | Collection changes reviewed before release. | unsupported | unsupported | unsupported | unsupported | unsupported | documentary |
| privacy-collection | PRIV-12 | E-signature/attestation treatment matches program authority. | unsupported | unsupported | unsupported | unsupported | unsupported | documentary |
| privacy-collection | PRIV-13 | Identify and schedule web records. | unsupported | unsupported | unsupported | unsupported | unsupported | documentary |
| privacy-collection | PRIV-14 | Preserve before removal or redesign. | unsupported | unsupported | unsupported | unsupported | unsupported | documentary |
| privacy-collection | PRIV-15 | FOIA proactive electronic disclosure. | unsupported | unsupported | unsupported | unsupported | unsupported | documentary |
| privacy-collection | PRIV-16 | Open-data/data-catalog obligations when applicable. | unsupported | unsupported | unsupported | unsupported | unsupported | documentary |
| privacy-collection | PRIV-17 | Information-quality correction process. | unsupported | unsupported | unsupported | unsupported | unsupported | documentary |
| infrastructure | INFRA-01 | HTTPS across public websites/services. | unsupported | unsupported | automated | unsupported | unsupported | unsupported |
| infrastructure | INFRA-02 | HSTS in applicable scope. | unsupported | unsupported | automated | unsupported | unsupported | documentary |
| infrastructure | INFRA-03 | Protect mail-enabled domains. | unsupported | unsupported | automated | unsupported | unsupported | documentary |
| infrastructure | INFRA-04 | System categorization, selected controls, authorization. | unsupported | unsupported | unsupported | unsupported | unsupported | documentary |
| infrastructure | INFRA-05 | Appropriate cloud-service assurance. | unsupported | unsupported | unsupported | unsupported | unsupported | documentary |
| infrastructure | INFRA-06 | Asset/software/hardware inventory. | unsupported | unsupported | unsupported | unsupported | unsupported | documentary |
| infrastructure | INFRA-07 | Known exploited vulnerabilities and vulnerability cadence. | unsupported | unsupported | unsupported | unsupported | unsupported | documentary |
| infrastructure | INFRA-08 | Vulnerability disclosure intake and response. | unsupported | unsupported | assisted | unsupported | unsupported | documentary |
| infrastructure | INFRA-09 | Server-side authorization. | unsupported | unsupported | unsupported | unsupported | unsupported | unsupported |
| infrastructure | INFRA-10 | Session and credential protection. | unsupported | unsupported | unsupported | unsupported | unsupported | unsupported |
| infrastructure | INFRA-11 | Injection, unsafe rendering and request handling. | unsupported | unsupported | unsupported | unsupported | unsupported | unsupported |
| infrastructure | INFRA-12 | Secure configuration and browser protections. | unsupported | unsupported | assisted | unsupported | unsupported | unsupported |
| infrastructure | INFRA-13 | Safe upload processing. | unsupported | unsupported | unsupported | unsupported | unsupported | unsupported |
| infrastructure | INFRA-14 | Secret and key management. | unsupported | unsupported | unsupported | unsupported | unsupported | documentary |
| infrastructure | INFRA-15 | Cryptography meets applicable standards. | unsupported | unsupported | unsupported | unsupported | unsupported | documentary |
| infrastructure | INFRA-16 | Supply-chain assurance based on current policy. | unsupported | unsupported | unsupported | unsupported | unsupported | documentary |
| infrastructure | INFRA-17 | Monitoring, incident response and audit logs. | unsupported | unsupported | unsupported | unsupported | unsupported | documentary |
| infrastructure | INFRA-18 | Backups, recovery and continuity. | unsupported | unsupported | unsupported | unsupported | unsupported | documentary |
| infrastructure | INFRA-19 | Workforce zero-trust/enterprise identity controls. | unsupported | unsupported | unsupported | unsupported | unsupported | documentary |
| infrastructure | INFRA-20 | Identity proofing/authentication levels fit service risk. | unsupported | unsupported | unsupported | unsupported | human | documentary |
| infrastructure | INFRA-21 | Login.gov for in-scope services, with dates/exceptions. | unsupported | unsupported | unsupported | unsupported | unsupported | documentary |
| infrastructure | INFRA-22 | Password behavior meets adopted NIST profile. | unsupported | unsupported | unsupported | unsupported | unsupported | unsupported |
| infrastructure | INFRA-23 | MFA/phishing resistance and recovery meet assurance requirements. | unsupported | unsupported | unsupported | unsupported | unsupported | documentary |
| infrastructure | INFRA-24 | Federation protects assertions and redirects. | unsupported | unsupported | unsupported | unsupported | unsupported | unsupported |
| infrastructure | INFRA-25 | Identity UX is monitored for exclusion and friction. | unsupported | unsupported | unsupported | unsupported | human | documentary |
| infrastructure | INFRA-26 | America.gov covered-service planning and execution. | unsupported | unsupported | unsupported | unsupported | unsupported | documentary |
| ux-service | UX-001 | Is page/service purpose clear from title, heading and opening content? Compare with... | unsupported | unsupported | unsupported | unsupported | human | unsupported |
| ux-service | UX-002 | Does visual and reading order prioritize the user's next decision? Inspect... | unsupported | unsupported | unsupported | unsupported | human | unsupported |
| ux-service | UX-003 | Can users answer eligibility and preparation questions before investing effort? Check... | unsupported | unsupported | unsupported | unsupported | human | unsupported |
| ux-service | UX-004 | Do navigation and labels use audience language? Compare search/support terminology and... | unsupported | unsupported | unsupported | unsupported | human | unsupported |
| ux-service | UX-005 | Are next actions distinguishable and accurately named? Review action sets and outcomes. | unsupported | unsupported | unsupported | unsupported | human | unsupported |
| ux-service | UX-006 | Does every collected field have a documented purpose? Compare field inventory with... | unsupported | unsupported | unsupported | unsupported | human | unsupported |
| ux-service | UX-007 | Can users prevent and recover from errors? Exercise validation, back, retry and... | unsupported | unsupported | unsupported | unsupported | human | unsupported |
| ux-service | UX-008 | Can people understand status and next steps? Inspect submission, pending and rejected... | unsupported | unsupported | unsupported | unsupported | human | unsupported |
| ux-service | UX-009 | Is help available at the point of difficulty? Check content, contact and handoff behavior. | unsupported | unsupported | unsupported | unsupported | human | unsupported |
| ux-service | UX-010 | Is avoidable cognitive burden reduced? Identify memorization, unexplained terms and... | unsupported | unsupported | unsupported | unsupported | human | unsupported |
| ux-service | UX-011 | Can users work with realistic interruptions and devices? Exercise resume, orientation,... | unsupported | unsupported | unsupported | unsupported | human | unsupported |
| ux-service | UX-012 | Are language and alternative access paths usable? Follow the path through the task,... | unsupported | unsupported | unsupported | unsupported | human | unsupported |
| ux-service | UX-013 | Is content accurate, owned and timely? Compare with approved program source and update... | unsupported | unsupported | unsupported | unsupported | human | unsupported |
| ux-service | UX-014 | Is search effective for top tasks? Test an owner-approved query set and zero-result... | unsupported | unsupported | unsupported | unsupported | human | unsupported |
| ux-service | UX-015 | Does typography/spacing support scanning and comprehension? Inspect actual content and... | unsupported | unsupported | unsupported | unsupported | human | unsupported |
| ux-service | UX-016 | Do charts/tables support correct interpretation? Check definitions, units, uncertainty... | unsupported | unsupported | unsupported | unsupported | human | unsupported |
| ux-service | UX-017 | Are people informed about consequences and choices? Check consent, fees, destructive... | unsupported | unsupported | unsupported | unsupported | human | unsupported |
| ux-service | UX-018 | Is the complete outcome achievable across channels/vendors? Follow handoffs and return... | unsupported | unsupported | unsupported | unsupported | human | unsupported |

## Baseline coverage

Every WCAG 2.0 rule in `a11y-wcag20-aa` carries a `baseline_test_id` for the ICT Testing Baseline for Web v3.1. This is a mapping for reporting, not a check. Baseline tests 03 Non-Interference and 20 Conforming Alternate Version are aggregate tests and are not a primary mapping for any rule; related tests appear in `extensions.baseline_related_test_ids`.

No baseline test exists for these 18 rules, because the Baseline covers WCAG 2.0 only. They have no `baseline_test_id`.

| Pack | Rules without a baseline test |
| --- | --- |
| a11y-wcag21-aa | A11Y-1.3.4, 1.3.5, 1.4.10, 1.4.11, 1.4.12, 1.4.13, 2.1.4, 2.5.1, 2.5.2, 2.5.3, 2.5.4, 4.1.3 |
| a11y-wcag22-aa | A11Y-2.4.11, 2.5.7, 2.5.8, 3.2.6, 3.3.7, 3.3.8 |

## How this matrix changes

A rule moves off `unsupported` for a channel only when a check for it ships with passing, failing, exception, insufficient-evidence, and regression fixtures, and the rule has completed source review and implementation validation. The rule's lifecycle then moves from `research` through `draft` and `validated` to `published`. Publish the updated matrix and known blind spots with every release. See `known-limitations.md`.
