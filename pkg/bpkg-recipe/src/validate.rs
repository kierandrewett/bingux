use crate::error::{RecipeError, Result};
use crate::recipe::Recipe;

/// Validate that a parsed recipe has all required fields.
pub fn validate(recipe: &Recipe) -> Result<()> {
    if recipe.pkgname.is_empty() {
        return Err(RecipeError::MissingField("pkgname".to_string()));
    }
    if recipe.pkgver.is_empty() {
        return Err(RecipeError::MissingField("pkgver".to_string()));
    }
    if recipe.pkgarch.is_empty() {
        return Err(RecipeError::MissingField("pkgarch".to_string()));
    }
    if recipe.package.is_none() {
        return Err(RecipeError::MissingField("package()".to_string()));
    }

    // pkgarch must be a known architecture.
    match recipe.pkgarch.as_str() {
        "x86_64-linux" | "aarch64-linux" => {}
        other => {
            return Err(RecipeError::ValidationError(format!(
                "unknown architecture: {other}"
            )));
        }
    }

    // If sources are specified, sha256sums count must match.
    if !recipe.source.is_empty() && recipe.source.len() != recipe.sha256sums.len() {
        return Err(RecipeError::ValidationError(format!(
            "source count ({}) does not match sha256sums count ({})",
            recipe.source.len(),
            recipe.sha256sums.len(),
        )));
    }

    Ok(())
}
