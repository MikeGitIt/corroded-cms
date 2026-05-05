use thiserror::Error;

#[derive(Debug, Error, Eq, PartialEq)]
pub enum ValidationError {
    #[error("slug is empty")]
    EmptySlug,
    #[error("slug is too long")]
    SlugTooLong,
    #[error("slug contains invalid characters")]
    InvalidSlug,
}

pub fn slugify(input: &str) -> String {
    let mut slug = String::new();
    let mut previous_was_dash = false;

    for ch in input.trim().chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            previous_was_dash = false;
        } else if !previous_was_dash && !slug.is_empty() {
            slug.push('-');
            previous_was_dash = true;
        }
    }

    if slug.ends_with('-') {
        slug.pop();
    }

    slug
}

pub fn validate_slug(slug: &str, max_len: usize) -> Result<(), ValidationError> {
    if slug.is_empty() {
        return Err(ValidationError::EmptySlug);
    }

    if slug.len() > max_len {
        return Err(ValidationError::SlugTooLong);
    }

    let mut previous_was_dash = false;
    for (idx, ch) in slug.chars().enumerate() {
        let valid = ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-';
        if !valid {
            return Err(ValidationError::InvalidSlug);
        }

        if ch == '-' {
            if idx == 0 || previous_was_dash {
                return Err(ValidationError::InvalidSlug);
            }
            previous_was_dash = true;
        } else {
            previous_was_dash = false;
        }
    }

    if slug.ends_with('-') {
        return Err(ValidationError::InvalidSlug);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugify_normalizes_titles() {
        assert_eq!(
            slugify("  My First Rust CMS Post!  "),
            "my-first-rust-cms-post"
        );
    }

    #[test]
    fn validate_slug_rejects_bad_shapes() {
        assert_eq!(validate_slug("", 200), Err(ValidationError::EmptySlug));
        assert_eq!(
            validate_slug("-bad", 200),
            Err(ValidationError::InvalidSlug)
        );
        assert_eq!(
            validate_slug("bad-", 200),
            Err(ValidationError::InvalidSlug)
        );
        assert_eq!(
            validate_slug("bad--slug", 200),
            Err(ValidationError::InvalidSlug)
        );
        assert_eq!(validate_slug("Bad", 200), Err(ValidationError::InvalidSlug));
    }
}
