use crate::output;

pub fn run(recipe: &str) {
    output::status("build", &format!("would build recipe {recipe}"));
}
