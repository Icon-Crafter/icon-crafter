//! Small, dependency-free input validators shared by the HTTP handlers.
//!
//! Each function returns `Result<(), String>` so handlers can collect several
//! field errors into one 422 response, or an `ApiError` directly through the
//! `validate_*` helpers.

/// Maximum length of a user/team/project/file display name, in characters.
pub const MAX_NAME_CHARS: usize = 200;
/// Maximum length of a file version label, in characters.
pub const MAX_LABEL_CHARS: usize = 200;
/// Maximum length of a comment body, in characters.
pub const MAX_COMMENT_CHARS: usize = 10_000;
/// Maximum e-mail length (RFC 5321 practical limit).
pub const MAX_EMAIL_CHARS: usize = 254;
/// Minimum password length, in characters.
pub const MIN_PASSWORD_CHARS: usize = 8;
/// Maximum accepted password length, in characters.
pub const MAX_PASSWORD_CHARS: usize = 1024;
/// Maximum slug length.
pub const MAX_SLUG_CHARS: usize = 63;

/// Normalize an e-mail address for storage and lookup (trimmed + lowercase).
pub fn normalize_email(raw: &str) -> String {
    raw.trim().to_lowercase()
}

/// Validate a display name, returning the trimmed value.
pub fn validate_name(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("must not be empty".to_owned());
    }
    if trimmed.chars().count() > MAX_NAME_CHARS {
        return Err(format!("must be at most {MAX_NAME_CHARS} characters"));
    }
    if trimmed.chars().any(char::is_control) {
        return Err("must not contain control characters".to_owned());
    }
    Ok(trimmed.to_owned())
}

/// Validate an e-mail address (already normalized or not).
pub fn validate_email(raw: &str) -> Result<String, String> {
    let email = normalize_email(raw);
    if email.is_empty() {
        return Err("must not be empty".to_owned());
    }
    if email.chars().count() > MAX_EMAIL_CHARS {
        return Err(format!("must be at most {MAX_EMAIL_CHARS} characters"));
    }
    if email.chars().any(char::is_whitespace) || email.chars().any(char::is_control) {
        return Err("must not contain whitespace or control characters".to_owned());
    }
    let mut parts = email.split('@');
    let (Some(local), Some(domain), None) = (parts.next(), parts.next(), parts.next()) else {
        return Err("must contain exactly one `@`".to_owned());
    };
    if local.is_empty() || domain.is_empty() {
        return Err("must have a non-empty local part and domain".to_owned());
    }
    if local.len() > 64 {
        return Err("local part is too long".to_owned());
    }
    if local.starts_with('.') || local.ends_with('.') || local.contains("..") {
        return Err("local part must not start/end with a dot or contain consecutive dots".to_owned());
    }
    if !domain.contains('.') || domain.starts_with('.') || domain.ends_with('.') {
        return Err("domain must contain a dot and must not start/end with one".to_owned());
    }
    if domain.split('.').any(|label| {
        label.is_empty()
            || label.starts_with('-')
            || label.ends_with('-')
            || !label
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-')
    }) {
        return Err("domain contains an invalid label".to_owned());
    }
    // A real TLD is at least two characters.
    let tld = domain.rsplit('.').next().unwrap_or_default();
    if tld.len() < 2 || !tld.chars().all(|c| c.is_ascii_alphabetic()) {
        return Err("domain must end with an alphabetic top level domain".to_owned());
    }
    Ok(email)
}

/// Validate a new password.
pub fn validate_password(password: &str) -> Result<(), String> {
    let length = password.chars().count();
    if length < MIN_PASSWORD_CHARS {
        return Err(format!("must be at least {MIN_PASSWORD_CHARS} characters"));
    }
    if length > MAX_PASSWORD_CHARS {
        return Err(format!("must be at most {MAX_PASSWORD_CHARS} characters"));
    }
    if password.chars().any(|c| c == '\0') {
        return Err("must not contain NUL bytes".to_owned());
    }
    Ok(())
}

/// Validate a version label or arbitrary short label.
pub fn validate_label(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    if trimmed.chars().count() > MAX_LABEL_CHARS {
        return Err(format!("must be at most {MAX_LABEL_CHARS} characters"));
    }
    if trimmed.chars().any(char::is_control) {
        return Err("must not contain control characters".to_owned());
    }
    Ok(trimmed.to_owned())
}

/// Validate a comment body.
pub fn validate_comment_body(raw: &str) -> Result<String, String> {
    if raw.trim().is_empty() {
        return Err("must not be empty".to_owned());
    }
    if raw.chars().count() > MAX_COMMENT_CHARS {
        return Err(format!(
            "must be at most {MAX_COMMENT_CHARS} characters"
        ));
    }
    if raw.contains('\0') {
        return Err("must not contain NUL bytes".to_owned());
    }
    Ok(raw.to_owned())
}

/// Validate a URL-safe slug (`^[a-z0-9]([a-z0-9-]{0,62})$`, no trailing hyphen).
pub fn validate_slug(raw: &str) -> Result<String, String> {
    let slug = raw.trim().to_ascii_lowercase();
    if slug.is_empty() {
        return Err("must not be empty".to_owned());
    }
    if slug.len() > MAX_SLUG_CHARS {
        return Err(format!("must be at most {MAX_SLUG_CHARS} characters"));
    }
    if slug.starts_with('-') || slug.ends_with('-') {
        return Err("must not start or end with a hyphen".to_owned());
    }
    if !slug
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err("may only contain lowercase letters, digits and hyphens".to_owned());
    }
    Ok(slug)
}

/// Derive a slug from a free-form name.
pub fn slugify(raw: &str) -> String {
    let mut slug = String::with_capacity(raw.len());
    let mut last_was_dash = false;
    for character in raw.chars().flat_map(char::to_lowercase) {
        if character.is_ascii_alphanumeric() {
            slug.push(character);
            last_was_dash = false;
        } else if !last_was_dash && !slug.is_empty() {
            slug.push('-');
            last_was_dash = true;
        }
        if slug.len() >= MAX_SLUG_CHARS {
            break;
        }
    }
    while slug.ends_with('-') {
        slug.pop();
    }
    if slug.is_empty() {
        "site".to_owned()
    } else {
        slug
    }
}

/// Validate a display name and map failures into a 422 error.
pub fn require_name(raw: &str, field: &str) -> Result<String, crate::error::ApiError> {
    validate_name(raw).map_err(|message| {
        crate::error::ApiError::validation_failed(serde_json::json!([{ "field": field, "message": message }]))
    })
}

/// Validate an e-mail address and map failures into a 422 error.
pub fn require_email(raw: &str) -> Result<String, crate::error::ApiError> {
    validate_email(raw).map_err(|message| {
        crate::error::ApiError::validation_failed(
            serde_json::json!([{ "field": "email", "message": message }]),
        )
    })
}

/// Validate a password and map failures into a 422 error.
pub fn require_password(raw: &str) -> Result<(), crate::error::ApiError> {
    validate_password(raw).map_err(|message| {
        crate::error::ApiError::validation_failed(
            serde_json::json!([{ "field": "password", "message": message }]),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emails_are_normalized_and_validated() {
        assert_eq!(
            validate_email("  Ada@Example.COM ").unwrap(),
            "ada@example.com"
        );
        assert_eq!(normalize_email(" A@B.CO "), "a@b.co");
    }

    #[test]
    fn invalid_emails_are_rejected() {
        for email in [
            "",
            "plainaddress",
            "@example.com",
            "user@",
            "user@@example.com",
            "user@example",
            "user@.example.com",
            "user@example..com",
            ".user@example.com",
            "user.@example.com",
            "us er@example.com",
            "user@exa mple.com",
            "user@example.c",
            "user@example.123",
            "user@-example.com",
        ] {
            assert!(validate_email(email).is_err(), "{email} should be invalid");
        }
    }

    #[test]
    fn valid_emails_are_accepted() {
        for email in [
            "a@b.co",
            "ada.lovelace+figma@example.co.uk",
            "x_y-z@sub.domain.example.com",
        ] {
            assert!(validate_email(email).is_ok(), "{email} should be valid");
        }
    }

    #[test]
    fn names_are_trimmed_and_length_checked() {
        assert_eq!(validate_name("  Icon Crafter  ").unwrap(), "Icon Crafter");
        assert!(validate_name("   ").is_err());
        assert!(validate_name(&"a".repeat(MAX_NAME_CHARS)).is_ok());
        assert!(validate_name(&"a".repeat(MAX_NAME_CHARS + 1)).is_err());
        assert!(validate_name("bad\u{0}name").is_err());
    }

    #[test]
    fn passwords_are_length_checked() {
        assert!(validate_password("short").is_err());
        assert!(validate_password("long-enough-password").is_ok());
        assert!(validate_password(&"a".repeat(MAX_PASSWORD_CHARS + 1)).is_err());
    }

    #[test]
    fn comment_bodies_are_checked() {
        assert_eq!(validate_comment_body(" nice ").unwrap(), " nice ");
        assert!(validate_comment_body("   ").is_err());
        assert!(validate_comment_body(&"a".repeat(MAX_COMMENT_CHARS + 1)).is_err());
    }

    #[test]
    fn slugs_are_validated() {
        assert_eq!(validate_slug("My-Site").unwrap(), "my-site");
        assert!(validate_slug("").is_err());
        assert!(validate_slug("-bad").is_err());
        assert!(validate_slug("bad-").is_err());
        assert!(validate_slug("bad_slug").is_err());
        assert!(validate_slug("bad slug").is_err());
        assert!(validate_slug(&"a".repeat(MAX_SLUG_CHARS + 1)).is_err());
    }

    #[test]
    fn slugify_produces_safe_slugs() {
        assert_eq!(slugify("My Landing Page!"), "my-landing-page");
        assert_eq!(slugify("   "), "site");
        assert_eq!(slugify("--a--b--"), "a-b");
        let long = slugify(&"word ".repeat(100));
        assert!(long.len() <= MAX_SLUG_CHARS);
        assert!(!long.ends_with('-'));
    }
}
