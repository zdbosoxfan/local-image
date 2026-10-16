use super::*;

#[test]
fn preview_origins_and_credentials_are_exact_not_prefix_matches() {
    let net = HttpNet::new(Config { hf_token: Some("synthetic-hf-key".into()), civitai_token: Some("synthetic-civitai-key".into()), ..Default::default() });
    for url in [
        "https://huggingface.co.attacker.invalid/preview.png",
        "https://huggingface.co@attacker.invalid/preview.png",
        "https://huggingface.co:443@attacker.invalid/preview.png",
        "https://civitai.com.attacker.invalid/preview.png",
        "https://huggingface.co:444/preview.png",
        "https://huggingface.co:invalid/preview.png",
        "http://huggingface.co/preview.png",
    ] {
        assert!(!net.allowed(url), "{url}");
        assert_eq!(net.token(url), None, "{url}");
    }
    assert!(net.allowed("https://huggingface.co:443/preview.png"));
    assert_eq!(net.token("https://huggingface.co:443/preview.png"), Some("synthetic-hf-key"));
    assert_eq!(net.token("https://civitai.com/api/v1/models"), Some("synthetic-civitai-key"));
    assert!(net.allowed("https://image.civitai.com/preview.png"));
    assert_eq!(net.token("https://image.civitai.com/preview.png"), None);
}

#[test]
fn attacker_catalogue_preview_stays_untrusted_after_parsing() {
    let response = serde_json::json!({"items":[{"id":1,"name":"Fixture","type":"LORA","modelVersions":[{
        "files":[{"name":"fixture.safetensors","downloadUrl":"https://huggingface.co/fixture/repo/resolve/main/fixture.safetensors"}],
        "images":[{"url":"https://huggingface.co.attacker.invalid/preview.png","nsfwLevel":1}]
    }]}]});
    let items = parse_civitai(crate::family::registry(), &response);
    assert_eq!(items.len(), 1);
    let preview = items[0].preview.as_deref().unwrap();
    let net = HttpNet::new(Config { hf_token: Some("synthetic-hf-key".into()), ..Default::default() });
    assert!(!net.allowed(preview));
    assert_eq!(net.token(preview), None);
}

#[test]
fn redirects_resolve_before_rechecking_origin_and_never_downgrade_tls() {
    let base = "https://huggingface.co/api/preview";
    let net = HttpNet::new(Config::default());
    let same = redirect_url(base, "/image.png").unwrap();
    assert!(same_origin(&same, base));
    let foreign = redirect_url(base, "//huggingface.co.attacker.invalid/image.png").unwrap();
    assert!(!net.allowed(&foreign));
    assert!(!same_origin(&foreign, base));
    assert!(redirect_url(base, "http://huggingface.co/image.png").is_err());
    assert!(redirect_url(base, "https://huggingface.co:443@attacker.invalid/image.png").is_err());
    let initial = origin(base).unwrap();
    assert_eq!(routed_token(Some("synthetic-key"), &initial, &same), Some("synthetic-key"));
    // An allowed CDN redirect still receives no provider credential.
    assert!(net.allowed("https://cdn-lfs.huggingface.co/image.png"));
    assert_eq!(routed_token(Some("synthetic-key"), &initial, "https://cdn-lfs.huggingface.co/image.png"), None);
    assert_eq!(routed_token(None, &initial, &same), None);
}
