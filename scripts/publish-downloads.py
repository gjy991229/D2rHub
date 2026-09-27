"""One build, two platforms. Immutable assets first, anonymous verification, index last.

Usage: python scripts/publish-downloads.py --spec release-spec.json --revision 2026092801
Only specify changed files in --spec; --previous preserves unchanged assets.
"""
import argparse
import copy
import json
import re
import sys
from pathlib import Path
from datetime import datetime, timezone
from release_platforms import CONFIG, Platform, credentials, anonymous_verify, anonymous_json, sha256, windows_file_version

def identity(a):
    return {k:a.get(k) for k in ('id','version','size','sha256','game_data_version','profile')}

def guard_asset(previous, asset):
    if previous and previous['version'] == asset['version'] and identity(previous) != identity(asset):
        raise RuntimeError(f"Refusing different content under the same version: {asset['id']} {asset['version']}")
    if asset['id'] in ('hub','processor'):
        def version(value):
            match=re.fullmatch(r'(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-([0-9A-Za-z.-]+))?',value)
            if not match: raise RuntimeError('Invalid semantic version')
            major,minor,patch,pre=match.groups()
            suffix=tuple((0,int(v)) if v.isdigit() else (1,v) for v in pre.split('.')) if pre else ()
            return (int(major),int(minor),int(patch),int(pre is None),suffix)
        next_version=version(asset['version'])
        if asset['id']=='hub' and '-' in asset['version']: raise RuntimeError('The software index requires a stable version')
        if previous and next_version < version(previous['version']): raise RuntimeError('Refusing software/processor downgrade')

def publish(spec, revision, output, config_path, previous=None):
    if hasattr(sys.stdout, 'reconfigure'):
        sys.stdout.reconfigure(encoding='utf-8')
    problems = []
    try: cfg, token = credentials(config_path)
    except Exception:
        cfg = json.loads(Path(config_path).read_text(encoding='utf-8-sig')) if Path(config_path).exists() else {'github_repo':'gjy991229/D2rHub','gitee_repo':'garyi7e/d2-rhub_-resource'}
        token = None; problems.append('gitee: local publishing credential unavailable')
    platforms = [Platform('github',cfg['github_repo'],'')]
    if token: platforms.append(Platform('gitee',cfg['gitee_repo'],token))
    kind = spec['kind']; tag = 'hub-update-index-v2' if kind == 'software' else 'mod-update-index-v2'
    if kind not in ('software','resources'): raise RuntimeError('Invalid release kind')
    # Read the current pointers before any upload, even when --previous is omitted.
    for platform in platforms:
        try:
            release = platform.release(tag, create=False)
            current = json.loads(release['body']) if release else None
        except Exception: current = None
        if current and current.get('schema') == 2:
            if previous and current['revision'] == previous['revision'] and current != previous:
                raise RuntimeError('The two platform indices disagree at the same revision')
            if not previous or current['revision'] > previous['revision']: previous = current
    manifest = copy.deepcopy(previous or {})
    manifest.update({k:v for k,v in spec.items() if k not in ('assets', 'skip_unchanged')})
    manifest.update(schema=2, kind=kind, revision=revision)
    manifest['assets'] = copy.deepcopy((previous or {}).get('assets', []))
    if previous and revision < previous['revision']: raise RuntimeError('Use a revision no lower than the current index')
    output.mkdir(parents=True,exist_ok=True)
    expected = {'hub'} if kind == 'software' else {'processor','LiteHub','BoHub','NullHub'}
    if {a['id'] for a in manifest['assets']} | {a['id'] for a in spec['assets']} != expected:
        raise RuntimeError('An initial publication must contain the complete catalog')
    for definition in spec['assets']:
        asset = {k:v for k,v in definition.items() if k not in ('file','release_tag')}
        path = Path(definition['file']); asset.update(size=path.stat().st_size, sha256=sha256(path), mirrors=[])
        old = next((a for a in manifest['assets'] if a['id'] == asset['id']), None)
        if (spec.get('skip_unchanged') and old and
                all(old.get(k) == asset.get(k) for k in ('size','sha256','game_data_version','profile')) and
                (asset['id'] in ('LiteHub','BoHub','NullHub') or old['version'] == asset['version']) and
                {m['platform'] for m in old.get('mirrors',[])} == {'github','gitee'}):
            print(f"{asset['id']}: unchanged; retaining published version {old['version']}",flush=True)
            continue
        guard_asset(old, asset)
        if asset['size'] <= 0: raise RuntimeError('Cannot publish an empty artifact')
        if asset['id']=='hub' and windows_file_version(path)!=asset['version']:
            raise RuntimeError('Installer embedded version differs from the declared version')
        asset.setdefault('sequence', old.get('sequence',0) if old and old['version']==asset['version'] else (old.get('sequence',0)+1 if old else 1))
        if old and asset['sequence'] < old.get('sequence',0): raise RuntimeError('Refusing resource sequence downgrade')
        for value in (asset['id'],asset['version']):
            if not value or not all(c.isascii() and (c.isalnum() or c in '._-') for c in value): raise RuntimeError('Invalid resource ID/version')
        reservation=output / f"{asset['id']}-{asset['version']}.identity.json"
        record=identity(asset)
        if reservation.exists() and json.loads(reservation.read_text(encoding='utf-8')) != record:
            raise RuntimeError('This local version was already built with different bytes')
        reservation.write_text(json.dumps(record,sort_keys=True),encoding='utf-8')
        for p in platforms:
            existing = None
            try:
                release = p.release(definition['release_tag'])
                attachments = p.assets(release)
                reserved = next((a for a in attachments if a['name'] == reservation.name), None)
                if reserved and anonymous_json(reserved['browser_download_url']) != record:
                    raise RuntimeError('Immutable version reservation mismatch')
                existing = next((a for a in attachments if a['name'] == path.name), None)
                if existing is not None:
                    # Existing name alone never proves a successful earlier upload.
                    if existing.get('size') != asset['size']: raise RuntimeError('Same asset name has different size')
                    attached = existing
                else: attached = p.upload(release, path)
                url = attached['browser_download_url']
                anonymous_verify(url, asset['size'], asset['sha256'])
                if not reserved:
                    reserved = p.upload(release,reservation)
                    anonymous_verify(reserved['browser_download_url'],reservation.stat().st_size,sha256(reservation))
                asset['mirrors'].append({'platform':p.name,'url':url})
                if p.name == 'github': asset['url'] = url
                print(f"{p.name}: {asset['id']} {'verified existing' if existing else 'uploaded and anonymously verified'}",flush=True)
            except Exception as e:
                # Different existing bytes are fatal; never publish a version split.
                if 'reservation mismatch' in str(e) or (existing is not None and ('mismatch' in str(e) or 'different size' in str(e))):
                    raise RuntimeError(f"{p.name}: existing immutable asset differs; choose a new version") from None
                problems.append(f"{p.name}: {asset['id']}: {type(e).__name__}: {e}")
                print(problems[-1],flush=True)
        if not asset['mirrors']: raise RuntimeError(f"No verified download for {asset['id']}; indices unchanged")
        asset['url'] = next((m['url'] for m in asset['mirrors'] if m['platform']=='github'),asset['mirrors'][0]['url'])
        manifest['assets'] = [a for a in manifest['assets'] if a['id'] != asset['id']] + [asset]
    output.mkdir(parents=True,exist_ok=True)
    snapshot = output / f'{kind}-v2-{revision}.json'
    body = json.dumps(manifest,ensure_ascii=False,indent=2)+'\n'
    if snapshot.exists() and snapshot.read_text(encoding='utf-8') != body:
        raise RuntimeError('This revision already has different content; use a new revision for mirror repair')
    snapshot.write_text(body,encoding='utf-8')
    # Two independent pointers, both written only after all assets in this manifest
    # are verified. A failed mirror is not advertised as a usable source.
    published = []
    for p in platforms:
        if any(not any(m['platform'] == p.name for m in a['mirrors']) for a in manifest['assets']):
            problems.append(f'{p.name}: mirror incomplete; index not advanced'); continue
        try:
            index = p.release(tag)
            try: current = json.loads(index.get('body',''))
            except ValueError: current = {}
            if current.get('revision',0) > revision: raise RuntimeError('Remote index is newer; refusing downgrade')
            if current.get('revision') == revision and current != manifest: raise RuntimeError('Same revision differs')
            existing = next((a for a in p.assets(index) if a['name'] == snapshot.name), None)
            attached = existing or p.upload(index,snapshot)
            anonymous_verify(attached['browser_download_url'],snapshot.stat().st_size,sha256(snapshot))
            p.publish_body(index,body)
            # Index itself must be anonymously readable as well as the binaries.
            r=anonymous_json(p.base+'/releases/tags/'+tag)
            if json.loads(r['body']) != manifest: raise RuntimeError('Anonymous index differs after publication')
            published.append(p.name); print(p.name+': index committed and anonymously verified',flush=True)
        except Exception as e: problems.append(f'{p.name}: index: {e}')
    (output/'publish-report.json').write_text(json.dumps({'published':published,'problems':problems},ensure_ascii=False,indent=2),encoding='utf-8')
    if not published: raise RuntimeError('No index was published: '+ '; '.join(problems))
    print('Published: '+', '.join(published),flush=True)
    if set(published) != {'github','gitee'}: print('镜像未完成；已可用平台保持发布。补传时使用同一文件及更高清单 revision。\n'+'\n'.join(problems),flush=True)
    return manifest

if __name__ == '__main__':
    import sys
    sys.stdout.reconfigure(encoding='utf-8')
    p=argparse.ArgumentParser(); p.add_argument('--spec',type=Path,required=True); p.add_argument('--revision',type=int,default=int(datetime.now(timezone.utc).strftime('%Y%m%d%H%M%S')))
    p.add_argument('--output',type=Path,required=True); p.add_argument('--config',type=Path,default=CONFIG); p.add_argument('--previous',type=Path)
    a=p.parse_args()
    try: publish(json.loads(a.spec.read_text(encoding='utf-8-sig')),a.revision,a.output,a.config,
                 json.loads(a.previous.read_text(encoding='utf-8-sig')) if a.previous else None)
    except Exception as e: raise SystemExit(f'Publication stopped: {e}') from None
