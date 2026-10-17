use photocraft_engine::Session;
use photocraft_engine::file_cmds::{self, OutputClaims};

// extension

#[test]
fn extension_returns_lowercase_after_last_dot() {
    assert_eq!(file_cmds::extension("dir/file.PNG"), Some("png".to_string()));
    assert_eq!(file_cmds::extension("archive.tar.gz"), Some("gz".to_string()));
    assert_eq!(file_cmds::extension("no_ext"), None);
    assert_eq!(file_cmds::extension(".gitignore"), None);
    assert_eq!(file_cmds::extension("trailing."), None);
    assert_eq!(file_cmds::extension("UPPER.PNG"), Some("png".to_string()));
    assert_eq!(file_cmds::extension(""), None);
}

// untitled_name / template_name

#[test]
fn untitled_name_skips_taken_names() {
    let taken = ["Untitled-1", "Untitled-3"];
    assert_eq!(file_cmds::untitled_name("Untitled", |n| taken.contains(&n)), "Untitled-2");

    let taken_many = ["Doc-1", "Doc-2", "Doc-3", "Doc-4"];
    assert_eq!(file_cmds::untitled_name("Doc", |n| taken_many.contains(&n)), "Doc-5");

    assert_eq!(file_cmds::untitled_name("File", |_| false), "File-1");
}

#[test]
fn template_name_returns_none_for_non_template() {
    let s = Session::new();
    assert_eq!(file_cmds::template_name(&s, "photo.png"), None);
}

// OutputClaims

#[test]
fn output_claims_detects_case_insensitive_collision() {
    let mut claims = OutputClaims::default();
    claims.record("out.PNG", "input1.png");

    let err = claims.check("out.png").unwrap_err();
    assert!(err.contains("input1.png"));
    assert!(err.contains("already holds"));

    assert!(claims.check("other.png").is_ok());

    claims.record("other.png", "input2.png");
    assert!(claims.check("OTHER.png").is_err());
}

// XMP read / write

#[test]
fn read_file_info_none_returns_defaults() {
    let v = lightcraft_meta::read_file_info(None);
    assert_eq!(v.get("title").and_then(|x| x.as_str()), Some(""));
    assert_eq!(v.get("author").and_then(|x| x.as_str()), Some(""));
    assert_eq!(v.get("authorTitle").and_then(|x| x.as_str()), Some(""));
    assert_eq!(v.get("description").and_then(|x| x.as_str()), Some(""));
    assert_eq!(v.get("copyright").and_then(|x| x.as_str()), Some(""));
    assert_eq!(v.get("copyrightUrl").and_then(|x| x.as_str()), Some(""));
    assert_eq!(v.get("copyrightStatus").and_then(|x| x.as_str()), Some("unknown"));
    assert!(v.get("keywords").and_then(|x| x.as_array()).unwrap().is_empty());
}

#[test]
fn read_file_info_parses_simple_properties() {
    let xmp = r#"<rdf:RDF
        xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
        xmlns:dc="http://purl.org/dc/elements/1.1/"
        xmlns:photoshop="http://ns.adobe.com/photoshop/1.0/"
        xmlns:xmpRights="http://ns.adobe.com/xap/1.0/rights/">
        <rdf:Description>
            <dc:title>Hello</dc:title>
            <dc:creator><rdf:Seq><rdf:li>Alice</rdf:li><rdf:li>Bob</rdf:li></rdf:Seq></dc:creator>
            <photoshop:AuthorsPosition>Engineer</photoshop:AuthorsPosition>
            <dc:description>A test</dc:description>
            <dc:subject><rdf:Bag><rdf:li>one</rdf:li><rdf:li>two</rdf:li></rdf:Bag></dc:subject>
            <dc:rights><rdf:Alt><rdf:li xml:lang="x-default">© 2024</rdf:li></rdf:Alt></dc:rights>
            <xmpRights:Marked>True</xmpRights:Marked>
            <xmpRights:WebStatement>https://example.com</xmpRights:WebStatement>
        </rdf:Description>
    </rdf:RDF>"#;

    let v = lightcraft_meta::read_file_info(Some(xmp));
    assert_eq!(v.get("title").and_then(|x| x.as_str()), Some("Hello"));
    assert_eq!(v.get("author").and_then(|x| x.as_str()), Some("Alice; Bob"));
    assert_eq!(v.get("authorTitle").and_then(|x| x.as_str()), Some("Engineer"));
    assert_eq!(v.get("description").and_then(|x| x.as_str()), Some("A test"));
    assert_eq!(v.get("copyright").and_then(|x| x.as_str()), Some("© 2024"));
    assert_eq!(v.get("copyrightStatus").and_then(|x| x.as_str()), Some("copyrighted"));
    assert_eq!(v.get("copyrightUrl").and_then(|x| x.as_str()), Some("https://example.com"));

    let kw = v.get("keywords").and_then(|x| x.as_array()).unwrap();
    let kws: Vec<&str> = kw.iter().filter_map(|x| x.as_str()).collect();
    assert_eq!(kws, vec!["one", "two"]);
}
