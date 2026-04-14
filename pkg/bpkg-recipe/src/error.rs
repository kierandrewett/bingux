use std::fmt;

/// Errors that can occur when parsing or validating a BPKGBUILD recipe.
#[derive(Debug, Clone, PartialEq)]
pub enum RecipeError {
    /// A required field is missing from the recipe.
    MissingField(String),
    /// A syntax error was encountered during parsing.
    SyntaxError { line: usize, message: String },
    /// An undefined variable was referenced in an expansion.
    UndefinedVariable(String),
    /// Validation failed after parsing.
    ValidationError(String),
}

impl fmt::Display for RecipeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RecipeError::MissingField(field) => {
                write!(f, "missing required field: {field}")
            }
            RecipeError::SyntaxError { line, message } => {
                write!(f, "syntax error on line {line}: {message}")
            }
            RecipeError::UndefinedVariable(var) => {
                write!(f, "undefined variable: ${{{var}}}")
            }
            RecipeError::ValidationError(msg) => {
                write!(f, "validation error: {msg}")
            }
        }
    }
}

impl std::error::Error for RecipeError {}

pub type Result<T> = std::result::Result<T, RecipeError>;
