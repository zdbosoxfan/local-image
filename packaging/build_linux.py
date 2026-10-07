"""Freeze, smoke-test and package the Linux desktop app (build-time Python only)."""
import argparse
import hashlib
import os
from pathlib import Path
import platform
import re
import shutil
import subprocess
import tarfile
import tempfile
from urllib.request import urlopen

ROOT = Path(__file__).resolve().parents[1]
TEXTURE_URL = ('https://github.com/EmbarkStudios/texture-synthesis/releases/download/0.8.2/'
               'texture-synthesis-0.8.2-x86_64-unknown-linux-musl.tar.gz')
TEXTURE_SHA256 = 'f97aba8e4f82d2971149159ccf73b441d0c4442855143aa43a77c54a3fcd9c6d'


def run(*command, env=None, **options):
    print('+ ' + ' '.join(str(value) for value in command), flush=True)
    return subprocess.run([str(value) for value in command], check=True, env=env, **options)


def texture_helper(output):
    folder = output / 'helpers'
    folder.mkdir(parents=True, exist_ok=True)
    archive = folder / 'texture-synthesis-0.8.2-linux.tar.gz'
    if not archive.is_file() or hashlib.sha256(archive.read_bytes()).hexdigest() != TEXTURE_SHA256:
        with urlopen(TEXTURE_URL, timeout=90) as response:
            body = response.read(32 * 1024 * 1024)
        if hashlib.sha256(body).hexdigest() != TEXTURE_SHA256:
            raise RuntimeError('The Texture helper does not match its pinned publisher checksum.')
        archive.write_bytes(body)
    with tarfile.open(archive) as source:
        for name in ('texture-synthesis', 'LICENSE-MIT', 'LICENSE-APACHE', 'README.md'):
            member = source.getmember('texture-synthesis-0.8.2-x86_64-unknown-linux-musl/' + name)
            if not member.isfile():
                raise RuntimeError('Unexpected Texture helper archive content.')
            with source.extractfile(member) as stream:
                (folder / name).write_bytes(stream.read())
    helper = folder / 'texture-synthesis'
    helper.chmod(0o755)
    run(helper, '--help', stdout=subprocess.DEVNULL)
    return helper


def smoke_test(package):
    environment = os.environ.copy()
    environment['QT_QPA_PLATFORM'] = 'offscreen'
    # Chromium refuses the build container's root account. This is test-only;
    # real launches keep Qt WebEngine's normal security settings.
    if os.geteuid() == 0:
        environment['QTWEBENGINE_DISABLE_SANDBOX'] = '1'
    result = subprocess.run([str(package / 'local-image'), '--smoke-test'], env=environment,
                            capture_output=True, text=True, timeout=100)
    print(result.stdout, flush=True)
    print(result.stderr, flush=True)
    result.check_returncode()
    if ('LOCAL_IMAGE_NATIVE_READY' not in result.stdout
            or 'LOCAL_IMAGE_PAGE_LOADED True' not in result.stdout):
        raise RuntimeError('The packaged native app did not load its UI and native bridge.')
    if shutil.which('xvfb-run'):
        environment['QT_QPA_PLATFORM'] = 'xcb'
        result = subprocess.run(['xvfb-run', '-a', str(package / 'local-image'), '--smoke-test'],
                                env=environment, capture_output=True, text=True, timeout=100)
        print(result.stdout, flush=True)
        print(result.stderr, flush=True)
        result.check_returncode()
        if 'LOCAL_IMAGE_NATIVE_READY' not in result.stdout or 'LOCAL_IMAGE_PAGE_LOADED True' not in result.stdout:
            raise RuntimeError('The packaged native app failed its X11 desktop smoke test.')


def audit_libraries(package):
    """Catch missing Qt plugins/codecs that an offscreen startup cannot exercise."""
    unresolved = []
    for directory in (package / '_internal', package / 'backend' / '_internal'):
        binaries = []
        for path in directory.rglob('*'):
            if path.is_file() and not path.is_symlink():
                with path.open('rb') as stream:
                    if stream.read(4) == b'\x7fELF':
                        binaries.append(path)
        environment = os.environ.copy()
        environment['LD_LIBRARY_PATH'] = os.pathsep.join(sorted({str(path.parent) for path in binaries}))
        for path in binaries:
            result = subprocess.run(['ldd', str(path)], env=environment, capture_output=True, text=True)
            for line in result.stdout.splitlines():
                if 'not found' in line:
                    unresolved.append(str(path.relative_to(package)) + ': ' + line.strip())
    if unresolved:
        raise RuntimeError('Missing bundled/system library dependencies:\n' + '\n'.join(unresolved))
    print('All bundled ELF shared-library dependencies resolved.', flush=True)


def make_debian(package, output, name, version):
    if not shutil.which('dpkg-deb'):
        raise RuntimeError('Install dpkg-deb or pass --skip-deb when building outside Ubuntu/Debian.')
    with tempfile.TemporaryDirectory(prefix='local-image-deb-', dir=output) as temporary:
        staging = Path(temporary)
        shutil.copytree(package, staging / 'opt' / 'local-image', symlinks=True)
        (staging / 'usr' / 'bin').mkdir(parents=True)
        launcher = staging / 'usr' / 'bin' / 'local-image'
        launcher.write_text('#!/bin/sh\nexec /opt/local-image/local-image "$@"\n')
        launcher.chmod(0o755)
        applications = staging / 'usr' / 'share' / 'applications'
        applications.mkdir(parents=True)
        template = (ROOT / 'packaging' / 'linux' / 'local-image.desktop').read_text()
        desktop = (template.replace('@EXEC@', '/usr/bin/local-image')
                   .replace('@ICON@', '/opt/local-image/icon.png')
                   .replace('@INSTALL_ROOT@', '/opt/local-image'))
        desktop = '\n'.join(line for line in desktop.splitlines()
                            if not line.startswith(('X-LocalImage-Managed=', 'X-LocalImage-InstallRoot='))) + '\n'
        (applications / 'local-image.desktop').write_text(desktop)
        control = staging / 'DEBIAN'
        control.mkdir()
        size = sum(path.stat().st_size for path in (staging / 'opt').rglob('*')
                   if path.is_file() and not path.is_symlink()) // 1024
        (control / 'control').write_text(
            f'Package: local-image\nVersion: {version}\nArchitecture: amd64\n'
            'Maintainer: Local Image <zdbosoxfan@users.noreply.github.com>\n'
            'Section: graphics\nPriority: optional\n'
            f'Installed-Size: {size}\n'
            'Depends: libc6 (>= 2.39), libgl1, libegl1, libdbus-1-3, libnss3, libnspr4, '
            'libxcb-cursor0, libxkbcommon0, fontconfig\n'
            'Recommends: fonts-dejavu-core\n'
            'Description: Local image editing and optional local AI tools\n'
            ' Native Qt desktop with a bundled Python server and React interface.\n')
        target = output / (name + '.deb')
        run('dpkg-deb', '--root-owner-group', '--threads-max=2', '-Zxz', '-z6', '--build', staging, target)
    return target


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--python', default='python3', help='Python environment with requirements-linux-build.txt')
    parser.add_argument('--version', required=True, help='Release version, for example 0.7.2-linux-preview')
    parser.add_argument('--output', default=str(ROOT / 'dist' / 'linux'))
    parser.add_argument('--skip-deb', action='store_true', help='Build the portable installer only')
    options = parser.parse_args()
    if platform.system() != 'Linux' or platform.machine() not in ('x86_64', 'amd64'):
        parser.error('Build this package on Linux x86_64.')
    if not re.fullmatch(r'[0-9][A-Za-z0-9.+~-]*', options.version):
        parser.error('Use a version suitable for a Debian package and release filename.')
    output = Path(options.output).resolve()
    output.mkdir(parents=True, exist_ok=True)
    name = f'Local-Image-{options.version}-linux-x86_64'
    package = output / name
    for filename in ('install.sh', 'uninstall.sh', 'local-image.desktop'):
        if not (ROOT / 'packaging' / 'linux' / filename).is_file():
            raise RuntimeError('The Linux installer templates are missing.')
    environment = os.environ.copy()
    environment['LOCAL_IMAGE_TEXTURE_HELPER'] = str(texture_helper(output))
    environment['PYINSTALLER_CONFIG_DIR'] = str(output / 'pyinstaller-cache')
    run(options.python, '-m', 'PyInstaller', '--noconfirm',
        '--distpath', output / 'frozen', '--workpath', output / 'build',
        ROOT / 'packaging' / 'local-image-linux.spec', env=environment, cwd=ROOT)
    if package.exists():
        shutil.rmtree(package)
    shutil.copytree(output / 'frozen' / 'desktop', package, symlinks=True)
    shutil.copytree(output / 'frozen' / 'backend', package / 'backend', symlinks=True)
    shutil.copy2(ROOT / 'backend' / 'frontend' / 'app-icon.png', package / 'icon.png')
    (package / 'VERSION').write_text(options.version + '\n')
    for filename in ('install.sh', 'uninstall.sh', 'local-image.desktop'):
        shutil.copy2(ROOT / 'packaging' / 'linux' / filename, package / filename)
    for filename in ('install.sh', 'uninstall.sh'):
        (package / filename).chmod(0o755)
    licenses = package / 'licenses'
    shutil.copytree(ROOT / 'backend' / 'licenses', licenses)
    shutil.copytree(ROOT / 'packaging' / 'linux' / 'licenses', licenses / 'Qt-PySide')
    shutil.copy2(ROOT / 'backend' / 'LICENSE', licenses / 'Local-Image-LICENSE.txt')
    shutil.copy2(ROOT / 'backend' / 'THIRD_PARTY_NOTICES.md', licenses / 'THIRD_PARTY_NOTICES.md')
    shutil.copy2(ROOT / 'backend' / 'frontend_dist' / 'THIRD_PARTY_NOTICES.txt', licenses / 'React-THIRD_PARTY_NOTICES.txt')
    python_license = Path('/usr/share/doc/python3.12/copyright')
    if python_license.is_file():
        shutil.copy2(python_license, licenses / 'Python-copyright.txt')
    # PyInstaller also carries unmodified native distro libraries. Preserve the
    # Ubuntu build image's copyright/license notices instead of dropping them.
    for notice in Path('/usr/share/doc').glob('*/copyright'):
        destination = licenses / 'ubuntu-runtime-notices' / notice.parent.name / 'copyright'
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(notice, destination)
    if Path('/usr/share/common-licenses').is_dir():
        shutil.copytree('/usr/share/common-licenses', licenses / 'common-licenses')
    for filename in ('LICENSE-MIT', 'LICENSE-APACHE'):
        shutil.copy2(output / 'helpers' / filename, licenses / ('texture-synthesis-' + filename + '.txt'))
    (package / 'INSTALL.txt').write_text(
        'Local Image for Linux x86_64\n\n'
        'Extract the complete folder and run ./install.sh to add Local Image to your applications.\n'
        'You can also run ./local-image directly without installation.\n'
        'The app includes its own Python and Qt runtime. No Node or Python setup is required.\n'
        'Qt and PySide6 6.11.2 are used under LGPLv3; their notices and source links are in licenses/Qt-PySide.\n'
        'Requires a normal Linux graphical desktop with glibc 2.39 or newer (Ubuntu 24.04 or newer).\n'
        'AI models and a compatible ComfyUI runtime are optional, separate downloads.\n'
        'Settings, models and recovery documents stay in your Local Image user data folder.\n'
        'The supplied uninstall.sh removes desktop integration without deleting documents or models.\n'
        'License notices are in licenses/ and the bundled *_internal/*dist-info/licenses folders.\n'
        'Updates and help: https://github.com/zdbosoxfan/local-image\n')
    audit_libraries(package)
    smoke_test(package)
    archive = output / (name + '.tar.gz')
    with tarfile.open(archive, 'w:gz', compresslevel=6, dereference=False) as target:
        target.add(package, arcname=name)
    artifacts = [archive]
    if not options.skip_deb:
        artifacts.append(make_debian(package, output, name, options.version))
    checksums = output / 'SHA256SUMS'
    checksums.write_text(''.join(hashlib.sha256(path.read_bytes()).hexdigest() + '  ' + path.name + '\n'
                                 for path in artifacts))
    for artifact in artifacts:
        print(artifact, flush=True)
    print(checksums, flush=True)


if __name__ == '__main__':
    main()
