"""Validate the real public release transport; credentials never enter reports."""
import argparse
import base64
import json
from pathlib import Path
from release_platforms import credentials, Platform, anonymous_verify, sha256

parser=argparse.ArgumentParser()
parser.add_argument("--asset",type=Path,required=True)
parser.add_argument("--tag",required=True)
args=parser.parse_args()
config, token = credentials()
p = Platform('gitee', config['gitee_repo'], token)
repo = p.call('GET', '')
if repo['default_branch'] is None:
    content = '# D2RHub resources\n\nPublic download mirror. Executables and Mods are Release attachments; source code remains on GitHub.\n'
    p.call('POST', '/contents/README.md', json={'content':base64.b64encode(content.encode()).decode(), 'message':'Initialize resource mirror', 'branch':'main'})
release = p.release(args.tag)
path = args.asset
existing = next((a for a in p.assets(release) if a.get('name') == path.name), None)
asset = existing or p.upload(release, path)
print(json.dumps({'release_id': release['id'], 'asset':{k:asset[k] for k in ('name','size','browser_download_url')}},ensure_ascii=False),flush=True)
url = asset['browser_download_url']
anonymous_verify(url, path.stat().st_size, sha256(path))
print('Anonymous executable download and SHA-256 verification: PASS',flush=True)
