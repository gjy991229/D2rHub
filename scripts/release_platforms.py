"""Publishing adapters. Credentials remain in Windows DPAPI or gh's credential store."""
import hashlib
import json
import os
import shutil
from pathlib import Path
import subprocess
import requests

CONFIG = Path(os.environ.get('LOCALAPPDATA', str(Path.home()))) / 'D2RHub-Publisher' / 'config.json'

def windows_file_version(path):
    import ctypes
    from ctypes import wintypes
    api=ctypes.WinDLL('version',use_last_error=True)
    api.GetFileVersionInfoSizeW.argtypes=[wintypes.LPCWSTR,ctypes.POINTER(wintypes.DWORD)]
    api.GetFileVersionInfoSizeW.restype=wintypes.DWORD
    api.GetFileVersionInfoW.argtypes=[wintypes.LPCWSTR,wintypes.DWORD,wintypes.DWORD,wintypes.LPVOID]
    api.GetFileVersionInfoW.restype=wintypes.BOOL
    api.VerQueryValueW.argtypes=[wintypes.LPCVOID,wintypes.LPCWSTR,ctypes.POINTER(wintypes.LPVOID),ctypes.POINTER(wintypes.UINT)]
    api.VerQueryValueW.restype=wintypes.BOOL
    path=str(Path(path).resolve());size=api.GetFileVersionInfoSizeW(path,None)
    if not size or size>1024*1024: raise RuntimeError('Installer has no valid Windows version resource')
    buffer=ctypes.create_string_buffer(size)
    if not api.GetFileVersionInfoW(path,0,size,buffer): raise RuntimeError('Cannot read installer version')
    pointer=wintypes.LPVOID();length=wintypes.UINT()
    if not api.VerQueryValueW(buffer,'\\',ctypes.byref(pointer),ctypes.byref(length)) or length.value<52: raise RuntimeError('Invalid installer version resource')
    fields=(ctypes.c_uint32*13).from_address(pointer.value)
    if fields[0]!=0xFEEF04BD or fields[3]&65535: raise RuntimeError('Unsupported installer version format')
    return f'{fields[2]>>16}.{fields[2]&65535}.{fields[3]>>16}'

def sha256(path):
    h = hashlib.sha256()
    with open(path, 'rb') as f:
        for block in iter(lambda: f.read(1024 * 1024), b''): h.update(block)
    return h.hexdigest()

def credentials(config=CONFIG):
    data = json.loads(Path(config).read_text(encoding='utf-8-sig'))
    import ctypes
    from ctypes import wintypes
    class Blob(ctypes.Structure):
        _fields_=[('size',wintypes.DWORD),('data',ctypes.POINTER(ctypes.c_ubyte))]
    encrypted=bytes.fromhex(Path(data['gitee_token_dpapi']).read_text(encoding='ascii').strip())
    buffer=(ctypes.c_ubyte*len(encrypted)).from_buffer_copy(encrypted)
    source=Blob(len(encrypted),buffer); result=Blob()
    if not ctypes.windll.crypt32.CryptUnprotectData(ctypes.byref(source),None,None,None,None,0,ctypes.byref(result)):
        raise RuntimeError('Cannot decrypt this Windows user publishing credential')
    try: token=ctypes.string_at(result.data,result.size).decode('utf-16-le')
    finally:
        free=ctypes.windll.kernel32.LocalFree;free.argtypes=[ctypes.c_void_p];free.restype=ctypes.c_void_p
        free(ctypes.cast(result.data,ctypes.c_void_p))
    if not token: raise RuntimeError('Empty publisher credential')
    return data, token

class Platform:
    def __init__(self, name, repo, token):
        self.name, self.repo = name, repo
        self.base = ('https://api.github.com' if name == 'github' else 'https://gitee.com/api/v5') + '/repos/' + repo
        self.session = requests.Session()
        self.session.headers.update({'Authorization': 'Bearer ' + token, 'User-Agent': 'D2RHub-Publisher/2'})

    def call(self, method, suffix, missing=False, **kwargs):
        if self.name == 'github':
            args=['gh','api','--method',method,'repos/'+self.repo+suffix]
            payload=kwargs.get('json')
            if payload is not None: args+=['--input','-']
            result=subprocess.run(args,input=json.dumps(payload) if payload is not None else None,text=True,capture_output=True,encoding='utf-8')
            if result.returncode:
                if missing and '404' in result.stderr: return None
                raise RuntimeError(f'github: {method} {suffix}: API request failed')
            return json.loads(result.stdout) if result.stdout.strip() else None
        r = self.session.request(method, self.base + suffix, timeout=(15, 300), **kwargs)
        if missing and r.status_code == 404: return None
        if not r.ok: raise RuntimeError(f'{self.name}: {method} {suffix}: HTTP {r.status_code}')
        return r.json() if r.content else None

    def release(self, tag, create=True):
        r = self.call('GET', '/releases/tags/' + tag, missing=True)
        if r is None and create:
            data = dict(tag_name=tag, name=tag, body='D2RHub public download assets. Availability is controlled by the verified update manifests.', target_commitish='main', prerelease=True)
            if self.name == 'github': data.update(make_latest='false')
            r = self.call('POST', '/releases', json=data)
        return r

    def assets(self, release):
        endpoint = f"/releases/{release['id']}/" + ('assets?per_page=100' if self.name == 'github' else 'attach_files?page=1&per_page=100')
        return self.call('GET', endpoint)

    def upload(self, release, path):
        path = Path(path)
        if self.name == 'github':
            result=subprocess.run(['gh','release','upload',release['tag_name'],str(path),'--repo',self.repo],capture_output=True,text=True)
            if result.returncode: raise RuntimeError('github: asset upload failed')
            return next(a for a in self.assets(release) if a['name']==path.name)
        with path.open('rb') as stream:
            return self.call('POST', f"/releases/{release['id']}/attach_files", files={'file': (path.name, stream, 'application/octet-stream')})

    def publish_body(self, release, body):
        return self.call('PATCH', f"/releases/{release['id']}", json={'body':body,'name':release['name'],'tag_name':release['tag_name'],'prerelease':True})

def powershell(script, timeout):
    exe=shutil.which('pwsh') or shutil.which('powershell')
    if not exe: raise RuntimeError('PowerShell is required for anonymous download verification')
    environment=os.environ.copy()
    if Path(exe).name.lower()=='powershell.exe':
        environment['PSModulePath']=str(Path(os.environ['SystemRoot'])/'System32/WindowsPowerShell/v1.0/Modules')+';'+str(Path(os.environ['ProgramFiles'])/'WindowsPowerShell/Modules')
    prefix="$ErrorActionPreference='Stop'; $ProgressPreference='SilentlyContinue'; [Net.ServicePointManager]::SecurityProtocol=[Net.SecurityProtocolType]::Tls12; "
    return subprocess.run([exe,'-NoProfile','-Command',prefix+script],capture_output=True,timeout=timeout,env=environment)

def anonymous_verify(url, size, digest):
    # Fresh PowerShell request: no Authorization, cookies, or publishing credentials.
    import tempfile
    with tempfile.TemporaryDirectory(prefix='d2rhub-publish-') as temp:
        target=Path(temp)/'payload'
        script="$ErrorActionPreference='Stop'; $ProgressPreference='SilentlyContinue'; Invoke-WebRequest -UseBasicParsing -Uri '"+url.replace("'","''")+"' -OutFile '"+str(target).replace("'","''")+"' -TimeoutSec 90"
        result=powershell(script,100)
        if result.returncode: raise RuntimeError('Anonymous download failed')
        if target.stat().st_size != size or sha256(target) != digest: raise RuntimeError('Anonymous download size/hash mismatch (possibly a login page)')
    return True

def anonymous_json(url):
    import tempfile
    with tempfile.TemporaryDirectory(prefix='d2rhub-publish-index-') as temp:
        target=Path(temp)/'index.json'
        script="Invoke-WebRequest -UseBasicParsing -Uri '"+url.replace("'","''")+"' -OutFile '"+str(target).replace("'","''")+"' -TimeoutSec 20"
        result=powershell(script,30)
        if result.returncode or target.stat().st_size>1024*1024: raise RuntimeError('Anonymous index read failed')
        return json.loads(target.read_bytes())
