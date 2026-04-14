use crate::output;

pub fn grant(package: &str, perms: &[String]) {
    output::status("grant", &format!("would grant {} to {package}", perms.join(", ")));
}

pub fn revoke(package: &str, perms: &[String]) {
    output::status("revoke", &format!("would revoke {} from {package}", perms.join(", ")));
}
