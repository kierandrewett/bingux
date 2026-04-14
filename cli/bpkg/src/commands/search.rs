use anyhow::Result;

use crate::output;

/// Search available packages in configured repositories.
pub fn run(query: &str) -> Result<()> {
    output::print_spinner(&format!("Searching for '{query}'..."));

    // TODO: search repo indexes for matching packages
    // For now, show stub results to demonstrate output formatting.

    let results: Vec<(String, String, String)> = vec![
        (
            query.to_string(),
            "0.0.0".to_string(),
            format!("(stub result for '{query}')"),
        ),
    ];

    output::print_search_results(&results);
    Ok(())
}
