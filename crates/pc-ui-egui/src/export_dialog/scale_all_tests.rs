//! Export As's Scale All rows. + adds sizes in the measured order with `@<size>` suffixes, up to
//! 10 rows; each row's trash removes it (greyed with one row left). One row exports through the
//! save dialog; several ask for a folder and write `<name><suffix>.<ext>` per row at its size.
//! The rows are remembered after Export and not after Cancel.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use egui_kittest::{
    Harness,
    kittest::{NodeT, Queryable},
};
use photocraft_doc::{Color, ColorMode, SampleType, Size};
use std::sync::Mutex;

/// The written files, each as its path and the pixel size and resolution it was encoded at.
type Writes = Arc<Mutex<Vec<(String, Vec<u8>)>>>;
/// The file dialogs asked for.
type Asked = Arc<Mutex<Vec<crate::FileDialogRequest>>>;

fn rows(list: &[(&str, &str)]) -> Vec<(String, String)> {
    list.iter().map(|(a, b)| ((*a).to_string(), (*b).to_string())).collect()
}

#[test]
fn plus_adds_sizes_in_the_measured_order_up_to_ten_rows() {
    let mut list = rows(&[("1x", "")]);
    while let Some(row) = next_scale_row(&list) {
        list.push(row);
    }
    let sizes: Vec<&str> = list.iter().map(|(s, _)| s.as_str()).collect();
    assert_eq!(sizes, ["1x", "0.5x", "0.75x", "0.25x", "2x", "0.33x", "1.5x", "0.1x", "3x", "1.25x"]);
    assert_eq!(list[1].1, "@0.5x");
    assert_eq!(list[9].1, "@1.25x");
    assert!(!sizes.contains(&"0.3x"), "0.3x is never added");
    assert_eq!(SCALE_SIZES.len(), 11);
    // A changed first row: + still skips the sizes already in use.
    assert_eq!(next_scale_row(&rows(&[("0.5x", "")])), Some(("0.75x".into(), "@0.75x".into())));
}

#[test]
fn rows_validate_scale_the_size_and_name_files() {
    assert!(valid_scale_rows(&json!([{"size": "1x", "suffix": ""}, {"size": "0.3x", "suffix": "@small"}])));
    assert!(!valid_scale_rows(&json!([])), "at least one row");
    assert!(!valid_scale_rows(&json!([{"size": "4x", "suffix": ""}])), "a listed size");
    assert!(!valid_scale_rows(&Value::Array(vec![json!({"size": "1x", "suffix": ""}); 11])), "at most 10 rows");
    let f = json!({"scale": 100}).as_object().unwrap().clone();
    assert_eq!(n(&scaled_fields(&f, scale_factor("0.5x")), "scale", 0.0), 50.0);
    assert_eq!(scaled_fields(&f, scale_factor("1x")), f);
    assert_eq!(file_suffix("@2x/../a:b"), "@2x_.._a_b");
}

/// Export As over a 200 × 150 "harbor" document at 144 ppi; each file dialog is answered with
/// `answer`, and the exporter writes the pixel size and resolution it was given.
fn harness(answer: crate::FileDialogAnswer) -> (Harness<'static, PhotocraftApp>, Writes, Asked) {
    let (writes, asked) = (Writes::default(), Asked::default());
    let (sink, picked) = (writes.clone(), asked.clone());
    let services = crate::Services {
        file_dialog: Some(crate::file_dialog::answering(move |request| {
            picked.lock().unwrap().push(request);
            Some(answer.clone())
        })),
        export: Some(Arc::new(|doc, _, _| Ok((format!("{} {} {}", doc.size.width, doc.size.height, doc.resolution_dpi).into_bytes(), vec![])))),
        write: Some(Box::new(move |path, bytes| {
            sink.lock().unwrap().push((path.to_string(), bytes.to_vec()));
            Ok(())
        })),
        ..Default::default()
    };
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), services);
    let mut doc = Document::with_background("harbor", Size::new(200, 150), ColorMode::Rgb, SampleType::U8, Color::WHITE);
    doc.resolution_dpi = 144.0;
    app.session.add_document(doc, None);
    let mut h = Harness::builder().with_size(egui::vec2(1100.0, 900.0)).build_ui_state(
        |ui, app| {
            let ctx = ui.ctx().clone();
            if !ctx.fonts(|fonts| fonts.families().contains(&egui::FontFamily::Name("semibold".into()))) {
                return;
            }
            crate::dialogs::show(app, &ctx);
        },
        app,
    );
    PhotocraftApp::setup_context(&h.ctx, crate::theme::ThemeKind::Studio);
    h.run_steps(3);
    (h, writes, asked)
}

fn open_dialog(h: &mut Harness<'_, PhotocraftApp>) {
    open(h.state_mut()).unwrap();
    h.run_steps(6);
}

fn dialog_rows(h: &Harness<'_, PhotocraftApp>) -> Vec<(String, String)> {
    scale_rows(&h.state().ui.dialogs.last().expect("Export As is open").fields)
}

fn click(h: &mut Harness<'_, PhotocraftApp>, label: &str) {
    h.get_by_label(label).click();
    h.run_steps(4);
}

fn disabled(h: &Harness<'_, PhotocraftApp>, label: &str) -> bool {
    h.get_by_label(label).accesskit_node().is_disabled()
}

/// The pixel size and resolution a written file was encoded at.
fn size_of(bytes: &[u8]) -> (u32, u32, f32) {
    let text = String::from_utf8(bytes.to_vec()).unwrap();
    let mut parts = text.split(' ');
    let mut next = || parts.next().unwrap().to_string();
    (next().parse().unwrap(), next().parse().unwrap(), next().parse().unwrap())
}

#[test]
fn rows_are_added_up_to_ten_and_trash_removes_one_but_never_the_last() {
    let (mut h, _, _) = harness(crate::FileDialogAnswer::SaveTo("x.png".into()));
    open_dialog(&mut h);
    assert_eq!(dialog_rows(&h), rows(&[("1x", "")]));
    assert!(disabled(&h, "Remove size 1"), "greyed with one row");
    for _ in 0..12 {
        click(&mut h, "Add a size");
    }
    let list = dialog_rows(&h);
    assert_eq!(list.len(), 10, "at most 10 rows");
    assert_eq!(list[1], ("0.5x".to_string(), "@0.5x".to_string()));
    assert!(!disabled(&h, "Remove size 1"));
    click(&mut h, "Remove size 2");
    let list = dialog_rows(&h);
    assert_eq!(list.len(), 9);
    assert_eq!(list[1].0, "0.75x", "the 0.5x row is gone");
}

#[test]
fn several_rows_ask_for_a_folder_and_write_one_file_per_size() {
    let dir = std::env::temp_dir().join(format!("li-scale-all-{}", std::process::id()));
    let (mut h, writes, asked) = harness(crate::FileDialogAnswer::Paths(vec![dir.to_string_lossy().into_owned()]));
    open_dialog(&mut h);
    click(&mut h, "Add a size");
    click(&mut h, "Export");
    assert!(matches!(asked.lock().unwrap().first(), Some(crate::FileDialogRequest::Folder)), "a folder choice");
    let writes = writes.lock().unwrap();
    let paths: Vec<&str> = writes.iter().map(|(p, _)| p.as_str()).collect();
    let want = [dir.join("harbor.png"), dir.join("harbor@0.5x.png")];
    assert_eq!(paths, want.iter().map(|p| p.to_str().unwrap()).collect::<Vec<_>>());
    let (full, half) = (size_of(&writes[0].1), size_of(&writes[1].1));
    assert_eq!((full.0, full.1), (200, 150));
    assert_eq!((half.0, half.1), (100, 75), "half the size");
    assert_eq!(half.2, full.2, "the same ppi");
}

#[test]
fn one_row_uses_the_save_dialog_at_its_size() {
    let (mut h, writes, asked) = harness(crate::FileDialogAnswer::SaveTo("out.png".into()));
    open_dialog(&mut h);
    h.state_mut().ui.dialogs.last_mut().unwrap().fields.insert("scaleAll".into(), json!([{"size": "2x", "suffix": "@2x"}]));
    h.run_steps(2);
    click(&mut h, "Export");
    assert!(matches!(asked.lock().unwrap().first(), Some(crate::FileDialogRequest::Save { .. })), "today's save dialog");
    let (w, h, _) = size_of(&writes.lock().unwrap()[0].1);
    assert_eq!((w, h), (400, 300));
}

#[test]
fn rows_are_remembered_after_export_not_after_cancel() {
    let dir = std::env::temp_dir().join(format!("li-scale-all-memory-{}", std::process::id()));
    let (mut h, _, _) = harness(crate::FileDialogAnswer::Paths(vec![dir.to_string_lossy().into_owned()]));
    open_dialog(&mut h);
    click(&mut h, "Add a size");
    click(&mut h, "Cancel");
    open_dialog(&mut h);
    assert_eq!(dialog_rows(&h).len(), 1, "Cancel forgets the rows");
    click(&mut h, "Add a size");
    click(&mut h, "Add a size");
    click(&mut h, "Export");
    open_dialog(&mut h);
    assert_eq!(dialog_rows(&h), rows(&[("1x", ""), ("0.5x", "@0.5x"), ("0.75x", "@0.75x")]), "Export remembers them");
}
