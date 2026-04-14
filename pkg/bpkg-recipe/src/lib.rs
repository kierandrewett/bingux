pub mod error;
pub mod parser;
pub mod recipe;
mod tests;
pub mod validate;

pub use error::RecipeError;
pub use recipe::Recipe;

/// Parse and validate a BPKGBUILD recipe from source text.
pub fn parse_recipe(input: &str) -> error::Result<Recipe> {
    let recipe = parser::parse(input)?;
    validate::validate(&recipe)?;
    Ok(recipe)
}
