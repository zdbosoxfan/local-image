# Original Local Image reproduction helper. SPDX-License-Identifier: GPL-3.0-or-later
import pathlib, xml.etree.ElementTree as E

def prepare():
    directory = pathlib.Path('/tmp/vector-inkscape-profile')
    directory.mkdir(parents=True, exist_ok=True)
    preferences = directory / 'preferences.xml'
    tree = E.parse(preferences) if preferences.exists() else E.ElementTree(E.Element('inkscape', {'version': '1'}))
    output = next((g for g in tree.getroot().iter('group') if g.get('id') == 'svgoutput'), None)
    if output is None:
        output = E.SubElement(tree.getroot(), 'group', {'id': 'svgoutput'})
    output.set('numericprecision', '16')
    output.set('minimumexponent', '-16')
    tree.write(preferences, encoding='utf-8', xml_declaration=True)
