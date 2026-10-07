# Qt and PySide6

Local Image uses unmodified PySide6 / Shiboken6 and Qt 6.11.2 under the GNU
Lesser General Public License version 3. Copyright belongs to The Qt Company
and the respective contributors. Complete LGPLv3 and GPLv3 texts accompany
this notice. Qt also contains components under separate permissive licenses.

The Qt shared libraries are installed as ordinary replaceable files under
`_internal/PySide6/Qt/lib`, with plugins under `_internal/PySide6/Qt/plugins`.
The PySide6 bindings and Shiboken shared libraries are under `_internal`.
You may replace them with compatible modified versions and run Local Image
with those versions; Local Image does not impose a signature check or license
restriction on modification, debugging, or reverse engineering for that purpose.
Preserve the folder layout and compatible Qt/PySide ABI. The complete application
source and build scripts are at https://github.com/zdbosoxfan/local-image.

The exact upstream corresponding source, including its build instructions,
is freely available from the Qt publisher:

- https://download.qt.io/official_releases/QtForPython/pyside6/PySide6-6.11.2-src/pyside-setup-everywhere-src-6.11.2.tar.xz
- https://download.qt.io/official_releases/qt/6.11/6.11.2/single/qt-everywhere-src-6.11.2.tar.xz
- https://download.qt.io/official_releases/qt/6.11/6.11.2/submodules/qtbase-everywhere-src-6.11.2.tar.xz
- https://download.qt.io/official_releases/qt/6.11/6.11.2/submodules/qtwebengine-everywhere-src-6.11.2.tar.xz
- https://download.qt.io/official_releases/qt/6.11/6.11.2/submodules/qtwebchannel-everywhere-src-6.11.2.tar.xz
- https://download.qt.io/official_releases/qt/6.11/6.11.2/submodules/qtdeclarative-everywhere-src-6.11.2.tar.xz
- https://download.qt.io/official_releases/qt/6.11/6.11.2/submodules/qtwayland-everywhere-src-6.11.2.tar.xz
- https://download.qt.io/official_releases/qt/6.11/6.11.2/submodules/qttranslations-everywhere-src-6.11.2.tar.xz

Complete Qt WebEngine / Chromium notices from the Qt 6.11.2 publisher are in
`Qt-WebEngine-THIRD-PARTY-NOTICES.txt` and the source archive above. Ubuntu
native-library copyright and license notices are retained separately under
`licenses/ubuntu-runtime-notices`. Python and pip package license notices remain
alongside their installed distribution metadata and in this licenses folder.
