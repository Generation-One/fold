//! Slug generation and hashing for memory IDs.
//!
//! Generates unique, human-readable slugs from titles and converts them
//! to hash-based IDs for file storage.

use sha2::{Digest, Sha256};

/// Generate a slug from a title.
///
/// Converts the title to lowercase, replaces non-alphanumeric characters
/// with hyphens, and trims leading/trailing hyphens.
///
/// # Example
/// ```
/// use fold_storage::slug::slugify;
/// assert_eq!(slugify("Hello World!"), "hello-world");
/// assert_eq!(slugify("API Design Decisions"), "api-design-decisions");
/// ```
pub fn slugify(title: &str) -> String {
    let slug: String = title
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect();

    // Collapse multiple hyphens and trim
    let mut result = String::new();
    let mut prev_hyphen = true; // Start true to skip leading hyphens
    for c in slug.chars() {
        if c == '-' {
            if !prev_hyphen {
                result.push(c);
            }
            prev_hyphen = true;
        } else {
            result.push(c);
            prev_hyphen = false;
        }
    }

    // Trim trailing hyphen
    if result.ends_with('-') {
        result.pop();
    }

    result
}

/// Generate a unique slug by appending a nonce.
///
/// The nonce is the current timestamp in milliseconds, encoded as hex
/// and truncated to 8 characters for brevity.
///
/// # Example
/// ```
/// use fold_storage::slug::slugify_unique;
/// let slug = slugify_unique("Hello World");
/// // Returns something like "hello-world-1a2b3c4d"
/// assert!(slug.starts_with("hello-world-"));
/// ```
pub fn slugify_unique(title: &str) -> String {
    let base = slugify(title);
    let nonce = generate_nonce();
    if base.is_empty() {
        nonce
    } else {
        format!("{}-{}", base, nonce)
    }
}

/// Generate a short nonce from current timestamp.
///
/// Uses milliseconds since epoch, hashed and truncated to 8 hex chars.
fn generate_nonce() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();

    // Hash the timestamp to get a more uniform distribution
    let mut hasher = Sha256::new();
    hasher.update(now.to_le_bytes());
    let hash = hasher.finalize();

    // Take first 8 hex characters
    hex::encode(&hash[..4])
}

/// Hash a slug to create a memory ID.
///
/// The ID is a 16-character hex string derived from SHA-256 hash of the slug.
/// This provides a consistent, filesystem-safe identifier.
///
/// # Example
/// ```
/// use fold_storage::slug::slug_to_id;
/// let id = slug_to_id("hello-world-1a2b3c4d");
/// assert_eq!(id.len(), 16);
/// ```
pub fn slug_to_id(slug: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(slug.as_bytes());
    let hash = hasher.finalize();

    // Take first 8 bytes (16 hex chars) for a compact but unique ID
    hex::encode(&hash[..8])
}

/// Generate a memory ID from a title.
///
/// Convenience function that combines slugify_unique and slug_to_id.
/// Returns both the unique slug and the derived ID.
///
/// # Example
/// ```
/// use fold_storage::slug::generate_memory_id;
/// let (slug, id) = generate_memory_id("API Design Decisions");
/// // slug: "api-design-decisions-1a2b3c4d"
/// // id: "a1b2c3d4e5f6g7h8" (16 hex chars)
/// ```
pub fn generate_memory_id(title: &str) -> (String, String) {
    let slug = slugify_unique(title);
    let id = slug_to_id(&slug);
    (slug, id)
}

/// Generate a memory ID from a pre-made slug (without adding nonce).
///
/// Use this when the caller has already created a unique slug.
pub fn slug_to_memory_id(slug: &str) -> String {
    slug_to_id(slug)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slugify() {
        assert_eq!(slugify("Hello World"), "hello-world");
        assert_eq!(slugify("Hello World!"), "hello-world");
        assert_eq!(slugify("API Design Decisions"), "api-design-decisions");
        assert_eq!(slugify("  spaces  around  "), "spaces-around");
        assert_eq!(slugify("multiple---hyphens"), "multiple-hyphens");
        assert_eq!(slugify("123 Numbers"), "123-numbers");
        assert_eq!(slugify(""), "");
    }

    #[test]
    fn test_slugify_unique() {
        let slug1 = slugify_unique("Test");
        let slug2 = slugify_unique("Test");

        // Both should start with "test-"
        assert!(slug1.starts_with("test-"));
        assert!(slug2.starts_with("test-"));

        // Should have nonce appended (test-XXXXXXXX)
        assert!(slug1.len() > 5);
    }

    #[test]
    fn test_slug_to_id() {
        let id = slug_to_id("hello-world-1a2b3c4d");
        assert_eq!(id.len(), 16);

        // Same input should produce same output
        let id2 = slug_to_id("hello-world-1a2b3c4d");
        assert_eq!(id, id2);

        // Different input should produce different output
        let id3 = slug_to_id("different-slug");
        assert_ne!(id, id3);
    }

    #[test]
    fn test_generate_memory_id() {
        let (slug, id) = generate_memory_id("Test Memory");

        assert!(slug.starts_with("test-memory-"));
        assert_eq!(id.len(), 16);
    }
}
