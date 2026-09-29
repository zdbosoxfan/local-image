"""Bounded stock-image credits that remain portable and grant no file authority."""
from urllib.parse import urlsplit

KEYS = {'provider', 'asset_id', 'title', 'creator', 'creator_url', 'source_url', 'license', 'license_url', 'attribution'}


def validate_attribution(value):
    if not isinstance(value, dict) or set(value) != KEYS:
        raise ValueError('Stock image attribution is invalid.')
    for key, limit in {'provider': 32, 'asset_id': 1024, 'title': 1000, 'creator': 1000,
                       'creator_url': 4096, 'source_url': 4096, 'license': 255, 'license_url': 4096, 'attribution': 8000}.items():
        text = value[key]
        if not isinstance(text, str) or len(text) > limit or any(ord(char) < 32 and char not in '\n\t' for char in text):
            raise ValueError('Stock image attribution text is invalid.')
    if value['provider'] != 'openverse' or not value['asset_id'].strip() or not value['source_url'].strip() or not value['license'].strip():
        raise ValueError('Stock image attribution is missing its provider, source or license.')
    for key in ('creator_url', 'source_url', 'license_url'):
        if not value[key]:
            continue
        try:
            url = urlsplit(value[key])
            if (url.scheme not in ('http', 'https') or not url.hostname or url.username is not None
                    or url.password is not None or any(char.isspace() for char in value[key])):
                raise ValueError()
            url.port
        except ValueError:
            raise ValueError('Stock image attribution links must be ordinary web URLs.')
    return dict(value)


def validate_attributions(values):
    if not isinstance(values, list) or len(values) > 32:
        raise ValueError('The document contains too many stock image credits.')
    return [validate_attribution(value) for value in values]


def collect_attributions(data):
    """Collect visible-source provenance for a composited generation reference."""
    values = []
    if data.get('source_attribution') is not None:
        values.append(validate_attribution(data['source_attribution']))
    values.extend(validate_attributions(data.get('reference_attributions', [])))
    state = data.get('cutout', {})
    if state.get('enabled') and state.get('background', {}).get('mode') == 'image':
        background = state['background']
        if background.get('attribution') is not None:
            values.append(validate_attribution(background['attribution']))
        values.extend(validate_attributions(background.get('reference_attributions', [])))
    return unique_attributions(values)


def unique_attributions(values):
    result, seen = [], set()
    for value in values:
        value = validate_attribution(value)
        identity = (value['provider'], value['asset_id'], value['source_url'])
        if identity not in seen:
            seen.add(identity); result.append(value)
    if len(result) > 32:
        raise ValueError('Use references containing at most 32 distinct stock credits.')
    return result
