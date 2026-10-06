use std::path::Path;

pub(crate) fn shell_quote(value: &str) -> String {
    if !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"_./-:=@".contains(&byte))
    {
        value.to_owned()
    } else {
        format!("'{}'", value.replace('\'', "'\\''"))
    }
}

pub(crate) fn command_prefix(root: Option<&Path>) -> String {
    match root {
        Some(root) => format!(
            "orchestrate --root {}",
            shell_quote(&root.display().to_string())
        ),
        None => "orchestrate".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_quote_keeps_simple_paths_short_and_quotes_spaces_and_apostrophes() {
        assert_eq!(shell_quote("/tmp/store"), "/tmp/store");
        assert_eq!(shell_quote("/tmp/my store"), "'/tmp/my store'");
        assert_eq!(shell_quote("/tmp/user's store"), "'/tmp/user'\\''s store'");
    }
}
