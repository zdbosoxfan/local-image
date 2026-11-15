//! The storytold upstreams (PhotoCraft, LightCraft, VectorCraft) and the tools that bring their
//! commits into Local Image.

/// A storytold upstream: `key` is its repository name and the folder of its clone.
#[derive(Debug, PartialEq)]
pub struct Upstream {
    pub key: &'static str,
    pub name: &'static str,
    pub url: &'static str,
    pub licence: &'static str,
}

pub const UPSTREAMS: [Upstream; 3] = [
    Upstream { key: "photocraft", name: "PhotoCraft", url: "https://github.com/storytold/photocraft", licence: "MIT OR Apache-2.0" },
    Upstream { key: "lightcraft", name: "LightCraft", url: "https://github.com/storytold/lightcraft", licence: "MIT OR Apache-2.0" },
    Upstream { key: "vectorcraft", name: "VectorCraft", url: "https://github.com/storytold/vectorcraft", licence: "MIT OR Apache-2.0" },
];

pub fn by_key(key: &str) -> Option<&'static Upstream> {
    UPSTREAMS.iter().find(|u| u.key == key)
}

#[cfg(test)]
pub(crate) mod test_support {
    use std::path::{Path, PathBuf};
    use std::process::Command;

    /// A fresh directory under the system temp dir.
    pub fn temp_dir(name: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir().join(format!("li-{name}-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Runs git in `dir` as a fixed fixture identity and returns its trimmed stdout.
    pub fn git(dir: &Path, args: &[&str]) -> String {
        let out = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid", "-c", "commit.gpgsign=false", "-c", "init.defaultBranch=main"])
            .args(args)
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
        String::from_utf8(out.stdout).unwrap().trim().to_string()
    }
}
