//! The HTML autofill token grammar, vendored from the HTML Standard
//! (section "Autofill", `autocomplete` attribute).
//!
//! Token order: optional `section-*`, optional `shipping` or `billing`,
//! then either a field name or a contact modifier followed by a contact
//! field name, then an optional trailing `webauthn`.

/// Field names that may follow no contact modifier.
pub const NON_CONTACT_FIELDS: &[&str] = &[
    "name",
    "honorific-prefix",
    "given-name",
    "additional-name",
    "family-name",
    "honorific-suffix",
    "nickname",
    "username",
    "new-password",
    "current-password",
    "one-time-code",
    "organization-title",
    "organization",
    "street-address",
    "address-line1",
    "address-line2",
    "address-line3",
    "address-level4",
    "address-level3",
    "address-level2",
    "address-level1",
    "country",
    "country-name",
    "postal-code",
    "cc-name",
    "cc-given-name",
    "cc-additional-name",
    "cc-family-name",
    "cc-number",
    "cc-exp",
    "cc-exp-month",
    "cc-exp-year",
    "cc-csc",
    "cc-type",
    "transaction-currency",
    "transaction-amount",
    "language",
    "bday",
    "bday-day",
    "bday-month",
    "bday-year",
    "sex",
    "url",
    "photo",
];

/// Field names that accept a contact modifier.
pub const CONTACT_FIELDS: &[&str] = &[
    "tel",
    "tel-country-code",
    "tel-national",
    "tel-area-code",
    "tel-local",
    "tel-local-prefix",
    "tel-local-suffix",
    "tel-extension",
    "email",
    "impp",
];

/// Contact modifiers.
pub const CONTACT_MODIFIERS: &[&str] = &["home", "work", "mobile", "fax", "pager"];

/// Why an `autocomplete` value is not valid.
#[derive(Debug, PartialEq, Eq)]
pub enum Invalid {
    /// No tokens.
    Empty,
    /// A token is not part of the grammar, or tokens are out of order.
    Grammar(String),
}

/// Validates an `autocomplete` attribute value. `on` and `off` alone are valid.
pub fn validate(value: &str) -> Result<(), Invalid> {
    let tokens: Vec<String> = value
        .split_ascii_whitespace()
        .map(|t| t.to_ascii_lowercase())
        .collect();
    if tokens.is_empty() {
        return Err(Invalid::Empty);
    }
    if tokens.len() == 1 && (tokens[0] == "on" || tokens[0] == "off") {
        return Ok(());
    }
    let mut rest: &[String] = &tokens;
    if let Some(first) = rest.first() {
        if first.len() > "section-".len() && first.starts_with("section-") {
            rest = &rest[1..];
        }
    }
    if let Some(first) = rest.first() {
        if first == "shipping" || first == "billing" {
            rest = &rest[1..];
        }
    }
    if rest.len() > 1 && rest.last().map(String::as_str) == Some("webauthn") {
        rest = &rest[..rest.len() - 1];
    }
    let ok = match rest {
        [field] => {
            NON_CONTACT_FIELDS.contains(&field.as_str()) || CONTACT_FIELDS.contains(&field.as_str())
        }
        [modifier, field] => {
            CONTACT_MODIFIERS.contains(&modifier.as_str())
                && CONTACT_FIELDS.contains(&field.as_str())
        }
        _ => false,
    };
    if ok {
        Ok(())
    } else {
        Err(Invalid::Grammar(value.trim().to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::validate;

    #[test]
    fn accepts_valid_forms() {
        for v in [
            "username",
            "bday-month",
            "Street-Address",
            "work email",
            "section-partner email",
            "section-primary billing address-line1",
            "section-primary shipping work email",
            "off",
            "on",
            "username webauthn",
            "tel",
        ] {
            assert!(validate(v).is_ok(), "{v}");
        }
    }

    #[test]
    fn rejects_invalid_forms() {
        for v in [
            "badname",
            "work photo",
            "work shipping email",
            "work,email",
            "banner",
            "webauthn",
            "section-",
            "off username",
        ] {
            assert!(validate(v).is_err(), "{v}");
        }
        assert!(validate("  ").is_err());
    }
}
