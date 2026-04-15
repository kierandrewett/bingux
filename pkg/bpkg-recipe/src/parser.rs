use std::collections::HashMap;

use crate::error::{RecipeError, Result};
use crate::recipe::Recipe;

/// Parse a BPKGBUILD recipe from its source text.
pub fn parse(input: &str) -> Result<Recipe> {
    let mut recipe = Recipe::empty();
    let mut variables: HashMap<String, String> = HashMap::new();
    let lines: Vec<&str> = input.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim();

        // Skip blank lines and comments.
        if trimmed.is_empty() || trimmed.starts_with('#') {
            i += 1;
            continue;
        }

        // Try function definition: `name() {`
        if let Some(func_name) = try_parse_function_start(trimmed) {
            let (body, end_line) = parse_function_body(&lines, i)?;
            match func_name.as_str() {
                "build" => recipe.build = Some(body),
                "package" => recipe.package = Some(body),
                _ => {} // Ignore unknown functions.
            }
            i = end_line + 1;
            continue;
        }

        // Try array assignment: `name=("a" "b" "c")` (may be multiline).
        if let Some((name, rest)) = try_parse_array_start(trimmed) {
            let (values, end_line) = parse_array_values(rest, &lines, i, &variables)?;
            set_array_field(&mut recipe, &name, values);
            i = end_line + 1;
            continue;
        }

        // Try scalar assignment: `name="value"`
        if let Some((name, value)) = try_parse_scalar(trimmed) {
            let expanded = expand_variables(&value, &variables)?;
            variables.insert(name.clone(), expanded.clone());
            set_scalar_field(&mut recipe, &name, expanded);
            i += 1;
            continue;
        }

        // Unrecognised line — skip silently (could be a bare shell command, etc.).
        i += 1;
    }

    Ok(recipe)
}

/// Try to parse a scalar assignment like `pkgname="firefox"` or `pkgname=firefox`.
fn try_parse_scalar(line: &str) -> Option<(String, String)> {
    let eq_pos = line.find('=')?;
    let name = line[..eq_pos].trim();

    // Must be a valid identifier.
    if !is_valid_identifier(name) {
        return None;
    }

    // Must not be an array assignment.
    let rhs = line[eq_pos + 1..].trim();
    if rhs.starts_with('(') {
        return None;
    }

    let value = strip_quotes(rhs);
    Some((name.to_string(), value))
}

/// Try to detect the start of an array assignment. Returns the variable name
/// and the rest of the line after the opening `(`.
fn try_parse_array_start(line: &str) -> Option<(String, &str)> {
    let eq_pos = line.find('=')?;
    let name = line[..eq_pos].trim();

    if !is_valid_identifier(name) {
        return None;
    }

    let rhs = line[eq_pos + 1..].trim();
    if !rhs.starts_with('(') {
        return None;
    }

    Some((name.to_string(), &rhs[1..]))
}

/// Parse array values, handling multiline arrays. `rest` is the content after
/// the opening `(` on the starting line.
fn parse_array_values(
    rest: &str,
    lines: &[&str],
    start_line: usize,
    variables: &HashMap<String, String>,
) -> Result<(Vec<String>, usize)> {
    let mut values = Vec::new();
    let mut current = rest.to_string();
    let mut line_idx = start_line;

    loop {
        // Check if the closing `)` is on this chunk.
        if let Some(close_pos) = find_unquoted_close_paren(&current) {
            let before_close = &current[..close_pos];
            extract_quoted_values(before_close, &mut values);
            // Expand variables in collected values.
            let expanded: Result<Vec<String>> = values
                .into_iter()
                .map(|v| expand_variables(&v, variables))
                .collect();
            return Ok((expanded?, line_idx));
        }

        // No closing paren yet — consume the whole line fragment and continue.
        extract_quoted_values(&current, &mut values);

        line_idx += 1;
        if line_idx >= lines.len() {
            return Err(RecipeError::SyntaxError {
                line: start_line + 1,
                message: "unterminated array".to_string(),
            });
        }
        current = lines[line_idx].trim().to_string();
    }
}

/// Find the position of a `)` that is not inside quotes.
fn find_unquoted_close_paren(s: &str) -> Option<usize> {
    let mut in_quote = false;
    for (i, ch) in s.char_indices() {
        match ch {
            '"' => in_quote = !in_quote,
            ')' if !in_quote => return Some(i),
            _ => {}
        }
    }
    None
}

/// Extract quoted or unquoted whitespace-separated values from a string fragment.
fn extract_quoted_values(s: &str, out: &mut Vec<String>) {
    let s = s.trim();
    if s.is_empty() {
        return;
    }

    let mut chars = s.chars().peekable();
    while chars.peek().is_some() {
        // Skip whitespace.
        while chars.peek().is_some_and(|c| c.is_whitespace()) {
            chars.next();
        }
        if chars.peek().is_none() {
            break;
        }

        if chars.peek() == Some(&'#') {
            // Rest of fragment is a comment.
            break;
        }

        if chars.peek() == Some(&'"') {
            // Quoted value.
            chars.next(); // consume opening quote
            let mut val = String::new();
            loop {
                match chars.next() {
                    Some('"') | None => break,
                    Some(ch) => val.push(ch),
                }
            }
            out.push(val);
        } else {
            // Unquoted value.
            let mut val = String::new();
            while chars.peek().is_some_and(|c| !c.is_whitespace() && *c != ')') {
                val.push(chars.next().unwrap());
            }
            if !val.is_empty() {
                out.push(val);
            }
        }
    }
}

/// Try to detect a function definition line like `build() {` or `build(){`.
fn try_parse_function_start(line: &str) -> Option<String> {
    // Match patterns: `name() {`, `name () {`, `name(){`
    let paren_pos = line.find("()")?;
    let name = line[..paren_pos].trim();
    if !is_valid_identifier(name) {
        return None;
    }

    let after_parens = line[paren_pos + 2..].trim();
    if after_parens == "{" || after_parens.starts_with('{') {
        Some(name.to_string())
    } else {
        None
    }
}

/// Parse a function body using brace-counting.
/// `start_line` is the line containing `name() {`.
/// Returns the body (everything between the outer `{` and `}`) and the line index of the closing `}`.
fn parse_function_body(lines: &[&str], start_line: usize) -> Result<(String, usize)> {
    let mut depth: i32 = 0;
    let mut body_lines: Vec<&str> = Vec::new();
    let mut found_open = false;
    let mut i = start_line;

    while i < lines.len() {
        let line = lines[i];

        // Check for single-line compact function: `name() { cmd1; cmd2; }`
        if i == start_line {
            if let Some(open_brace) = line.find('{') {
                let after_brace = &line[open_brace + 1..];
                if let Some(close_brace) = after_brace.rfind('}') {
                    // Everything between { and } on the same line
                    let body = after_brace[..close_brace].trim().to_string();
                    if !body.is_empty() {
                        return Ok((body, i));
                    }
                }
            }
        }

        for ch in line.chars() {
            match ch {
                '{' => {
                    depth += 1;
                    found_open = true;
                }
                '}' if found_open => {
                    depth -= 1;
                    if depth == 0 {
                        let body = body_lines.join("\n");
                        return Ok((body, i));
                    }
                }
                _ => {}
            }
        }

        // After the first line (which has the `{`), collect body lines.
        if found_open && depth > 0 && i > start_line {
            body_lines.push(line);
        }

        i += 1;
    }

    Err(RecipeError::SyntaxError {
        line: start_line + 1,
        message: "unterminated function body".to_string(),
    })
}

/// Expand `${variable}` and `$VARIABLE` references in a string.
pub fn expand_variables(input: &str, vars: &HashMap<String, String>) -> Result<String> {
    let mut result = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '$' {
            if chars.peek() == Some(&'{') {
                // ${variable}
                chars.next(); // consume '{'
                let mut var_name = String::new();
                loop {
                    match chars.next() {
                        Some('}') => break,
                        Some(c) => var_name.push(c),
                        None => {
                            return Err(RecipeError::SyntaxError {
                                line: 0,
                                message: "unterminated ${...} expansion".to_string(),
                            });
                        }
                    }
                }
                if let Some(val) = vars.get(&var_name) {
                    result.push_str(val);
                } else {
                    // Keep the original reference for env-vars like $SRCDIR
                    // that are expanded at build time, not parse time.
                    result.push_str(&format!("${{{var_name}}}"));
                }
            } else if chars.peek().is_some_and(|c| c.is_ascii_alphabetic() || *c == '_') {
                // $VARIABLE
                let mut var_name = String::new();
                while chars
                    .peek()
                    .is_some_and(|c| c.is_ascii_alphanumeric() || *c == '_')
                {
                    var_name.push(chars.next().unwrap());
                }
                if let Some(val) = vars.get(&var_name) {
                    result.push_str(val);
                } else {
                    result.push('$');
                    result.push_str(&var_name);
                }
            } else {
                result.push('$');
            }
        } else {
            result.push(ch);
        }
    }

    Ok(result)
}

/// Set a scalar field on the Recipe by name.
fn set_scalar_field(recipe: &mut Recipe, name: &str, value: String) {
    match name {
        "pkgscope" => recipe.pkgscope = value,
        "pkgname" => recipe.pkgname = value,
        "pkgver" => recipe.pkgver = value,
        "pkgarch" => recipe.pkgarch = value,
        "pkgdesc" => recipe.pkgdesc = Some(value),
        "license" => recipe.license = Some(value),
        _ => {} // Ignore unknown scalars.
    }
}

/// Set an array field on the Recipe by name.
fn set_array_field(recipe: &mut Recipe, name: &str, values: Vec<String>) {
    match name {
        "depends" => recipe.depends = values,
        "makedepends" => recipe.makedepends = values,
        "exports" => recipe.exports = values,
        "source" => recipe.source = values,
        "sha256sums" => recipe.sha256sums = values,
        "dlopen_hints" => recipe.dlopen_hints = values,
        _ => {} // Ignore unknown arrays.
    }
}

/// Strip surrounding double-quotes from a string, if present.
fn strip_quotes(s: &str) -> String {
    if s.len() >= 2 && s.starts_with('"') && s.ends_with('"') {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}

fn is_valid_identifier(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
        && s.chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
}
