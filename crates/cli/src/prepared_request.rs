use anyhow::{Context, Result, bail, ensure};
use orchestrate_contracts::RequestKind;
use serde::Deserialize;
use std::{
    fs,
    path::{Path, PathBuf},
};

const TICKET_PLACEHOLDER: &str = "<!-- PASTE ORIGINAL TICKET VERBATIM HERE -->";

#[derive(Debug)]
pub struct PreparedRequest {
    pub root: PathBuf,
    pub project: PathBuf,
    pub effort: String,
    pub request_kind: RequestKind,
    pub constraints: Vec<String>,
    pub body: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Frontmatter {
    root: PathBuf,
    project: PathBuf,
    effort: String,
    request_kind: RequestKind,
    #[serde(default)]
    constraints: Vec<String>,
}

pub fn read(path: &Path) -> Result<PreparedRequest> {
    let bytes = fs::read(path)
        .with_context(|| format!("cannot read prepared request {}", path.display()))?;
    let document = std::str::from_utf8(&bytes)
        .with_context(|| format!("prepared request {} is not valid UTF-8", path.display()))?;
    let (frontmatter, body_offset) = split_frontmatter(&bytes)?;
    let frontmatter = std::str::from_utf8(frontmatter)
        .expect("frontmatter is a slice of a validated UTF-8 document");
    let parsed: Frontmatter =
        serde_yaml::from_str(frontmatter).context("prepared request frontmatter is invalid")?;
    ensure!(
        parsed.root.is_absolute(),
        "prepared request root must be an absolute path"
    );
    ensure!(
        parsed.project.is_absolute(),
        "prepared request project must be an absolute path"
    );
    ensure!(
        !parsed.effort.is_empty(),
        "prepared request effort must not be empty"
    );
    ensure!(
        parsed
            .effort
            .chars()
            .all(|character| character.is_ascii_alphanumeric()
                || matches!(character, '-' | '_' | '.')),
        "prepared request effort has an invalid label"
    );
    let body = document[body_offset..].to_owned();
    ensure!(
        !body.trim().is_empty(),
        "prepared request body must not be whitespace only"
    );
    if parsed.request_kind == RequestKind::Ticket {
        ensure!(
            !body.contains(TICKET_PLACEHOLDER),
            "replace the original ticket placeholder with the original ticket and retry"
        );
    }
    Ok(PreparedRequest {
        root: parsed.root,
        project: parsed.project,
        effort: parsed.effort,
        request_kind: parsed.request_kind,
        constraints: parsed.constraints,
        body,
    })
}

fn split_frontmatter(bytes: &[u8]) -> Result<(&[u8], usize)> {
    let (opening, first_end) = next_line(bytes, 0)
        .context("prepared request is missing an opening frontmatter delimiter")?;
    ensure!(opening == b"---", "prepared request must begin with `---`");
    let mut offset = first_end;
    while let Some((line, line_end)) = next_line(bytes, offset) {
        if line == b"---" {
            return Ok((&bytes[first_end..offset], line_end));
        }
        offset = line_end;
    }
    bail!("prepared request is missing a closing frontmatter delimiter")
}

fn next_line(bytes: &[u8], start: usize) -> Option<(&[u8], usize)> {
    if start >= bytes.len() {
        return None;
    }
    let newline = bytes[start..].iter().position(|byte| *byte == b'\n')? + start;
    let line_end = if newline > start && bytes[newline - 1] == b'\r' {
        newline - 1
    } else {
        newline
    };
    Some((&bytes[start..line_end], newline + 1))
}

#[cfg(test)]
mod tests {
    use super::split_frontmatter;

    #[test]
    fn split_preserves_the_body_offset_for_lf_and_crlf() {
        for (document, expected) in [
            (b"---\na: b\n---\nbody\n".as_slice(), b"body\n".as_slice()),
            (
                b"---\r\na: b\r\n---\r\nbody\r\n".as_slice(),
                b"body\r\n".as_slice(),
            ),
        ] {
            let (_, offset) = split_frontmatter(document).unwrap();
            assert_eq!(&document[offset..], expected);
        }
    }
}
