#!/usr/bin/env python3
"""Run the new existing-API regressions against HEAD, then restore all builder sources.

New-only Library preferences reader is supplied with HEAD's exact whole-struct parsing
boundary. New-only disk-delete helpers are omitted; the context-menu regression uses the
pre-existing API. No existing tests are changed in the baseline run.
"""
from pathlib import Path
import os
import subprocess

ROOT = Path(__file__).resolve().parents[3]
os.chdir(ROOT)
assert 'li-rustc-gate' in os.environ.get('RUSTC_WRAPPER', '')
paths = subprocess.check_output(['git', 'diff', '--name-only'], text=True).splitlines()
paths = [p for p in paths if p.endswith('.rs')]
saved = {p: Path(p).read_bytes() for p in paths}
texts = {p: b.decode() for p, b in saved.items()}
test_only = {
    'crates/pc-engine/src/float_cmds/tests.rs',
    'crates/pc-engine/src/prefs/tests.rs',
    'crates/lc-engine/src/tests_import.rs',
    'crates/lc-engine/src/tests_xmp.rs',
    'crates/pc-io/tests/vector_fallback/roundtrip.rs',
}

def function(source, name):
    pos = source.index('fn ' + name)
    start = source.rfind('#[test]', 0, pos)
    opening = source.index('{', pos)
    depth = 1
    end = opening + 1
    # These extracted test functions contain only balanced-brace format strings.
    while depth:
        depth += (source[end] == '{') - (source[end] == '}')
        end += 1
    return source[start:end]

def test_in_module(path, name):
    text = Path(path).read_text()
    snippet = function(texts[path], name)
    assert 'mod tests {' in text
    text = text.replace('mod tests {', 'mod tests {\n' + snippet, 1)
    Path(path).write_text(text)

def appended_module(path):
    mark = '\n#[cfg(test)]\nmod bugfix_save_tests {'
    if 'import.rs' in path:
        mark = '\n#[cfg(all(test, not(target_arch = "wasm32")))]\nmod bugfix_save_tests {'
    text = texts[path]
    assert mark in text
    with Path(path).open('a') as f:
        f.write(text[text.index(mark):])

try:
    for p in paths:
        if p not in test_only:
            Path(p).write_bytes(subprocess.check_output(['git', 'show', 'HEAD:' + p]))
    with Path('crates/pc-ui-egui/src/lib.rs').open('a') as f:
        f.write('\n#[cfg(test)]\nmod bugfix_save_tests;\n')
    p = 'crates/pc-engine/src/vector_integration.rs'
    with Path(p).open('a') as f:
        f.write('\n#[cfg(test)]\n' + function(texts[p], 'bugfix_save_o031_locked_swatch_links_survive_unlock_and_prune_deleted_targets') + '\n')
    test_in_module('crates/lc-engine/src/enhance/mod.rs', 'bugfix_save_o047_cancel_during_saving_discards_successful_outcome')
    test_in_module('crates/lc-engine/src/export.rs', 'bugfix_save_o029_overwrite_never_reuses_a_batch_output_or_sidecar')
    test_in_module('crates/pc-ui-egui/src/filmstrip_ui.rs', 'bugfix_save_o007_filmstrip_waits_for_tiff_then_steps_from_original_file')
    test_in_module('crates/lc-ui-egui/src/panels/grid.rs', 'bugfix_save_u006_context_delete_uses_the_same_confirmation')
    test_in_module('apps/local-image/src/library_host.rs', 'bugfix_save_o009_overwrite_resets_master_virtual_copy_and_its_develop_link')
    appended_module('crates/lc-ui-egui/src/import.rs')
    appended_module('crates/pc-ui-egui/src/prefs_ui.rs')
    p = 'apps/local-image/src/library_host.rs'
    appended_module(p)
    text = Path(p).read_text().replace('mod bugfix_save_tests {', '''mod bugfix_save_tests {
        fn read_library_prefs(path: &std::path::Path) -> Result<(UiState, Vec<String>), String> {
            let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
            serde_json::from_slice::<UiState>(&bytes).map(|ui| (ui.sanitized(), Vec::new())).map_err(|e| e.to_string())
        }
''', 1)
    Path(p).write_text(text)
    command = ['cargo', 'test', '--offline', '-p', 'photocraft-engine', '-p', 'photocraft-ui-egui', '-p', 'photocraft-io', '-p', 'lightcraft-engine', '-p', 'lightcraft-ui-egui', '-p', 'local-image', '--no-fail-fast', 'bugfix_save_']
    print('BASELINE HEAD=' + subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip(), flush=True)
    print('COMMAND: ' + ' '.join(command), flush=True)
    result = subprocess.run(command)
    print('BASELINE EXIT: ' + str(result.returncode), flush=True)
finally:
    for p, data in saved.items():
        Path(p).write_bytes(data)
    assert all(Path(p).read_bytes() == data for p, data in saved.items())
    print('All final sources restored byte-for-byte.', flush=True)
