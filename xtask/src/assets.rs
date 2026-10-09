//! Audit attributed assets using the path column of the repository's two attribution tables.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

fn code_spans(text: &str) -> impl Iterator<Item = &str> {
    text.split('`').enumerate().filter_map(|(i, s)| (i % 2 == 1).then_some(s))
}

/// `*` stays within a path component; a directory pattern covers its whole subtree.
fn matches(pattern: &str, path: &str) -> bool {
    if pattern.ends_with('/') {
        return path.starts_with(pattern);
    }
    if let Some(start) = pattern.find('{')
        && let Some(end) = pattern[start..].find('}').map(|i| i + start)
    {
        return pattern[start + 1..end].split(',').any(|p| matches(&format!("{}{}{}", &pattern[..start], p, &pattern[end + 1..]), path));
    }
    let (p, s) = (pattern.as_bytes(), path.as_bytes());
    let mut dp = vec![false; s.len() + 1];
    dp[0] = true;
    for &b in p {
        let mut next = vec![false; s.len() + 1];
        for j in 1..=s.len() {
            next[j] = if b == b'*' { dp[j] || (next[j - 1] && s[j - 1] != b'/') } else { dp[j - 1] && b == s[j - 1] };
        }
        if b == b'*' {
            next[0] = dp[0];
        }
        dp = next;
    }
    dp[s.len()]
}

/// Paths in the inherited tables predate the merged workspace. Preserve those notices and
/// resolve their documented source paths to the corresponding relocated files.
fn relocated(path: &str, lightcraft: bool) -> String {
    let origin = if lightcraft { "lightcraft" } else { "photocraft" };
    if let Some(rest) = path.strip_prefix("docs/brand/") {
        return format!("docs/upstream/{origin}/brand/{rest}");
    }
    if let Some(rest) = path.strip_prefix("docs/images/") {
        return format!("docs/upstream/{origin}/images/{rest}");
    }
    if let Some(rest) = path.strip_prefix("crates/cms/") {
        return format!("crates/pc-cms/{rest}");
    }
    if let Some(rest) = path.strip_prefix("crates/ui-egui/") {
        return format!("crates/{}-ui-egui/{rest}", if lightcraft { "lc" } else { "pc" });
    }
    path.to_owned()
}

fn asset(path: &str) -> bool {
    if path.starts_with("assets/camera-profiles/") && path.ends_with(".json") {
        return true;
    }
    let ext = Path::new(path).extension().and_then(|s| s.to_str()).unwrap_or("").to_ascii_lowercase();
    matches!(
        ext.as_str(),
        "svg"
            | "png"
            | "jpg"
            | "jpeg"
            | "gif"
            | "webp"
            | "avif"
            | "ico"
            | "icns"
            | "ttf"
            | "otf"
            | "wav"
            | "mp3"
            | "ogg"
            | "mp4"
            | "webm"
            | "tiff"
            | "tif"
            | "icc"
            | "icm"
            | "dng"
            | "arw"
            | "cr2"
            | "cr3"
            | "nef"
            | "exr"
    )
}

fn licence_paths(column: &str) -> Vec<&str> {
    let mut paths: Vec<_> = code_spans(column).filter(|s| (s.contains('/') || *s == "LICENSE") && !s.contains("://")).collect();
    for part in column.split("](").skip(1) {
        if let Some(path) = part.split(')').next()
            && path.contains('/')
            && !path.contains("://")
        {
            paths.push(path);
        }
    }
    paths
}

pub fn run(root: &Path) -> Result<()> {
    let mut patterns = Vec::new();
    for (table, lightcraft) in [("assets/ATTRIBUTION-lightcraft.md", true), ("licenses/photocraft-ATTRIBUTION.md", false)] {
        let contents = std::fs::read_to_string(root.join(table)).with_context(|| table.to_string())?;
        for row in contents.lines().filter(|l| l.starts_with('|')) {
            let columns: Vec<_> = row.split('|').collect();
            if columns.len() < 7 {
                continue;
            }
            let paths: Vec<_> = code_spans(columns[1]).filter(|s| s.contains('/')).collect();
            if paths.is_empty() {
                continue;
            }
            patterns.extend(paths.iter().flat_map(|p| [p.to_string(), relocated(p, lightcraft)]));
            let licence = columns.get(5);
            for path in licence_paths(licence.copied().unwrap_or("")) {
                if !root.join(path).is_file() && !root.join(relocated(path, lightcraft)).is_file() {
                    bail!("{table}: missing licence file {path}");
                }
            }
        }
    }
    // Git's inventory includes new assets and respects ignored build/reference outputs.
    let output = Command::new("git").current_dir(root).args(["ls-files", "--cached", "--others", "--exclude-standard", "-z"]).output()?;
    if !output.status.success() {
        bail!("git ls-files failed");
    }
    let files = String::from_utf8(output.stdout)?;
    let mut missing = Vec::new();
    let mut count = 0;
    for path in files.split('\0').filter(|p| asset(p)) {
        count += 1;
        if !patterns.iter().any(|p| matches(p, path)) {
            missing.push(PathBuf::from(path));
        }
    }
    if !missing.is_empty() {
        bail!("{} assets have no attribution row:\n{}", missing.len(), missing.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join("\n"));
    }
    println!("assets: {count} assets attributed; referenced licence files exist");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patterns_do_not_hide_unattributed_subdirectories() {
        assert!(matches("assets/icons/*.svg", "assets/icons/brush.svg"));
        assert!(!matches("assets/icons/*.svg", "assets/icons/color/brush.svg"));
        assert!(matches("assets/icons/color/", "assets/icons/color/brush.svg"));
        assert!(matches("a/{x,y}.svg", "a/y.svg"));
        assert!(!matches("a/{x,y}.svg", "a/z.svg"));
    }

    #[test]
    fn licences_are_read_from_both_table_formats() {
        assert_eq!(licence_paths("GPL (`LICENSE`), ISC (`assets/icons/LICENSE.txt`)"), ["LICENSE", "assets/icons/LICENSE.txt"]);
        assert_eq!(licence_paths("ISC, [`notice`](assets/icons/LICENSE.txt)"), ["assets/icons/LICENSE.txt"]);
    }
}
