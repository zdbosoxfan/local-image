from pathlib import Path
import hashlib
import json
import zipfile

HERE = Path(__file__).resolve().parent
OUT = HERE.parent.parent/'outputs'
CONNECTOR = Path(r'C:\Users\Owner\Documents\RapidRAW-AI-Connector')
APP = Path(r'C:\Users\Owner\Documents\Local Remove')
load = lambda path: json.loads(path.read_text(encoding='utf-8-sig'))
digest = lambda path: hashlib.sha256(path.read_bytes()).hexdigest()
report = load(OUT/'Local Remove verification.json')
live = load(HERE/'live-project-verification.json')
native = load(HERE/'installed-native-self-test.json')
probe = load(HERE/'installed-project-probe.json')
matches = {name:digest(HERE/name)==digest(CONNECTOR/name) for name in ('local_remove.py','local_remove.html','local_remove_project.py')}
matches['Local Remove.exe'] = digest(HERE/'native-host/Local Remove.exe') == digest(APP/'Local Remove.exe')
assert all(matches.values()) and live['ok'] and native['ok'] and probe['ok']
assert not (CONNECTOR/'local-remove-data/sessions'/live['session']).exists()
assert Path(live['source']).exists() and Path(live['project']).exists()
with zipfile.ZipFile(OUT/'Local Remove source.zip') as archive:
    for name in ('local_remove.py','local_remove.html','local_remove_project.py'):
        assert archive.read(name) == (HERE/name).read_bytes()
    assert not any(name.endswith(('.safetensors','.ckpt','.gguf','.lremove')) or '/fixtures/' in name or 'launcher.key' in name for name in archive.namelist())
report['editable_projects_update'] = {
    'ok':True, 'backend_tests_passed':28, 'ui_regression_suites_passed':2,
    'live_project_roundtrip':{key:value for key,value in live.items() if isinstance(value,bool)},
    'toggle_json_roundtrips':live['toggle_roundtrips'],
    'performance_context':'Old full-photo preview transferred 5,441,860 bytes for the4000x2667 drone photo. New state writes in the separate2-layer fixture transferred1245–1247bytes; browser visibility changes use existing decoded images before the response. Different fixture sizes; no total speedup ratio asserted.',
    'browser_acceptance':{'layer_toggle':True,'close_dialog':True,'cancel_preserves_selection_and_layers':True,'pending_selection_warning':True,'discard_empties_editor_and_removes_session':True},
    'native_self_test':native,
    'installed_native_project_opens_in_webview':probe['ok'],
    'native_picker_and_window_close_clickthrough':'Not directly automated; native close correlation and UI multi-document save/cancel/batchclose covered by separate tests.',
    'installed_files_match_build':matches,
    'test_data_cleanup':load(HERE/'test-session-cleanup.json'),
    'existing_user_sessions_retained':True,
    'project_keeps':['original bytes','completed layer colors and masks','visibility and discard states','native16bit merged snapshots','ICC profile'],
    'project_excludes':['pending selections and pen paths','selection undo','zoom and pan','model weights','source overwrite authority'],
    'restart_from_desktop_shortcut_required_for_native_v2':True
}
(OUT/'Local Remove verification.json').write_text(json.dumps(report,indent=2),encoding='utf-8')
print(json.dumps({'ok':True,'installed_files_match':matches,'archive_verified':True,'own_live_session_removed':True}))
