//! Bounded XML subset for Krita presets/manifests. No DTD, entity definitions, or external I/O.
use std::collections::BTreeMap;
#[derive(Debug, Default)]
pub(crate) struct Node {
    pub name: String,
    pub attrs: BTreeMap<String, String>,
    pub text: String,
    pub children: Vec<usize>,
}
impl Node {
    pub fn attr(&self, k: &str) -> &str {
        self.attrs.get(k).map_or("", String::as_str)
    }
}
fn unescape(s: &str) -> Result<String, String> {
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i + 1..];
        let end = rest.find(';').filter(|i| *i < 16).ok_or("invalid XML entity")?;
        let e = &rest[..end];
        let c = match e {
            "amp" => '&',
            "lt" => '<',
            "gt" => '>',
            "quot" => '"',
            "apos" => '\'',
            _ => {
                let n = if let Some(n) = e.strip_prefix("#x") { u32::from_str_radix(n, 16).ok() } else { e.strip_prefix('#').and_then(|n| n.parse().ok()) };
                n.and_then(char::from_u32).filter(|c| *c != '\0').ok_or("unsupported XML entity")?
            }
        };
        out.push(c);
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    Ok(out)
}
fn tag(s: &str) -> Result<Node, String> {
    let name_end = s.find(char::is_whitespace).unwrap_or(s.len());
    let name = &s[..name_end];
    if name.is_empty() {
        return Err("empty XML tag".into());
    }
    let mut out = Node { name: name.into(), ..Default::default() };
    let mut rest = s[name_end..].trim_start();
    while !rest.is_empty() {
        if out.attrs.len() >= 64 {
            return Err("XML attribute count exceeds limit".into());
        }
        let (key, r) = rest.split_once('=').ok_or("invalid XML attribute")?;
        let key = key.trim();
        let r = r.trim_start();
        let q = r.chars().next().ok_or("missing XML attribute")?;
        if q != '\'' && q != '"' {
            return Err("unquoted XML attribute".into());
        }
        let end = r[1..].find(q).ok_or("unterminated XML attribute")? + 1;
        let v = unescape(&r[1..end])?;
        if out.attrs.insert(key.into(), v).is_some() {
            return Err("duplicate XML attribute".into());
        }
        rest = r[end + 1..].trim_start();
    }
    Ok(out)
}
pub(crate) fn parse(s: &str) -> Result<Vec<Node>, String> {
    if s.len() > 2 << 20 {
        return Err("preset XML exceeds 2 MiB".into());
    }
    let mut nodes = vec![Node::default()];
    let mut stack = vec![0usize];
    let mut p = 0usize;
    while p < s.len() {
        let rest = &s[p..];
        if let Some(rest) = rest.strip_prefix("<![CDATA[") {
            let n = rest.find("]]>").ok_or("unterminated XML CDATA")?;
            let id = *stack.last().ok_or("invalid XML stack")?;
            nodes[id].text.push_str(&rest[..n]);
            p += 9 + n + 3;
            continue;
        }
        if let Some(rest) = rest.strip_prefix("<!--") {
            let n = rest.find("-->").ok_or("unterminated XML comment")?;
            p += 4 + n + 3;
            continue;
        }
        if let Some(rest) = rest.strip_prefix("<?") {
            let n = rest.find("?>").ok_or("unterminated XML instruction")?;
            p += 2 + n + 2;
            continue;
        }
        if rest.starts_with("<!") {
            return Err("XML declarations and external entities are not supported".into());
        }
        if rest.starts_with('<') {
            // Find the closing bracket outside quoted attributes.
            let mut quote = None;
            let mut end = None;
            for (i, c) in rest.char_indices().skip(1) {
                if let Some(q) = quote {
                    if c == q {
                        quote = None;
                    }
                } else if c == '\'' || c == '"' {
                    quote = Some(c);
                } else if c == '>' {
                    end = Some(i);
                    break;
                }
            }
            let n = end.ok_or("unterminated XML tag")?;
            let content = rest[1..n].trim();
            if let Some(close) = content.strip_prefix('/') {
                let id = stack.pop().filter(|id| *id > 0).ok_or("unexpected XML closing tag")?;
                if nodes[id].name != close.trim() {
                    return Err("mismatched XML closing tag".into());
                }
            } else {
                if nodes.len() >= 16384 || stack.len() >= 64 {
                    return Err("XML node/depth limit exceeded".into());
                }
                let single = content.ends_with('/');
                let node = tag(content.trim_end_matches('/').trim())?;
                let id = nodes.len();
                nodes.push(node);
                let parent = *stack.last().ok_or("invalid XML stack")?;
                nodes[parent].children.push(id);
                if !single {
                    stack.push(id);
                }
            }
            p += n + 1;
        } else {
            let n = rest.find('<').unwrap_or(rest.len());
            let id = *stack.last().ok_or("invalid XML stack")?;
            nodes[id].text.push_str(&unescape(&rest[..n])?);
            p += n;
        }
    }
    if stack.len() != 1 || nodes[0].children.len() != 1 {
        return Err("invalid XML root".into());
    }
    Ok(nodes)
}
