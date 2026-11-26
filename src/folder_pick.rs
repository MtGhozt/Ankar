//! Footer cd helper (the in-app picker lives in `dir_picker`).

use std::path::Path;

/// The line typed into the shell for the footer's "cd": quoted, so spaces survive.
pub fn cd_command(path: &Path) -> String {
    format!("cd \"{}\"\r", path.display())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cd_quotes_the_path_and_presses_enter() {
        assert_eq!(cd_command(Path::new(r"C:\Program Files\x")), "cd \"C:\\Program Files\\x\"\r");
    }
}