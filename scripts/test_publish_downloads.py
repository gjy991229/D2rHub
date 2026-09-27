import copy
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

spec=importlib.util.spec_from_file_location('publisher',Path(__file__).with_name('publish-downloads.py'))
publisher=importlib.util.module_from_spec(spec);spec.loader.exec_module(publisher)

class FakePlatform:
    stores={}; uploads=[]; fail_gitee=False
    def __init__(self,name,repo,token):self.name=name;self.repo=repo;self.base='https://'+name;self.stores.setdefault(name,{})
    def release(self,tag,create=True):
        if self.name=='gitee' and self.fail_gitee:raise RuntimeError('mirror offline')
        store=self.stores[self.name]
        if tag not in store and create:store[tag]={'id':tag,'tag_name':tag,'name':tag,'body':'pending','files':{}}
        return store.get(tag)
    def assets(self,release):return list(release['files'].values())
    def upload(self,release,path):
        self.uploads.append((self.name,Path(path).name));data=Path(path).read_bytes();url=f"{self.base}/{release['id']}/{Path(path).name}"
        item={'name':Path(path).name,'size':len(data),'browser_download_url':url,'data':data};release['files'][Path(path).name]=item;return item
    def publish_body(self,release,body):release['body']=body

def anonymous_json(url):
    platform=url.split('/')[2]
    if '/releases/tags/' in url:return FakePlatform.stores[platform][url.rsplit('/',1)[-1]]
    _,_,_,tag,name=url.split('/',4)
    return json.loads(FakePlatform.stores[platform][tag]['files'][name]['data'])

class PublishingTests(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory();self.root=Path(self.temp.name);self.file=self.root/'Hub-1.0.0-setup.exe';self.file.write_bytes(b'stable bytes')
        FakePlatform.stores={};FakePlatform.uploads=[];FakePlatform.fail_gitee=False
        self.spec={'kind':'software','product':'D2RHub','platform':'windows-x86_64','assets':[{'id':'hub','version':'1.0.0','file':str(self.file),'release_tag':'v1.0.0'}]}
        self.patches=[patch.object(publisher,'Platform',FakePlatform),patch.object(publisher,'credentials',return_value=({'github_repo':'a/b','gitee_repo':'a/b'},'not-a-real-token')),patch.object(publisher,'anonymous_verify',return_value=True),patch.object(publisher,'anonymous_json',side_effect=anonymous_json),patch.object(publisher,'windows_file_version',return_value='1.0.0')]
        for p in self.patches:p.start()
    def tearDown(self):
        for p in self.patches:p.stop()
        self.temp.cleanup()
    def publish(self,revision):return publisher.publish(self.spec,revision,self.root/'out',self.root/'config')
    def test_repeat_skips_binaries_and_same_version_different_content_is_rejected(self):
        self.publish(1);self.publish(1)
        self.assertEqual(sum(name==self.file.name for _,name in FakePlatform.uploads),2)
        self.file.write_bytes(b'different bytes');before=copy.deepcopy(FakePlatform.stores)
        with self.assertRaises(RuntimeError):self.publish(2)
        self.assertEqual(before,FakePlatform.stores)
    def test_mirror_failure_keeps_github_published_and_retry_uploads_only_missing_binary(self):
        FakePlatform.fail_gitee=True;first=self.publish(1);self.assertEqual([m['platform'] for m in first['assets'][0]['mirrors']],['github'])
        FakePlatform.fail_gitee=False;second=self.publish(2)
        self.assertEqual(len(second['assets'][0]['mirrors']),2)
        self.assertEqual(sum(name==self.file.name for _,name in FakePlatform.uploads),2)
    def test_older_revision_cannot_advance_either_pointer(self):
        self.publish(5)
        with self.assertRaises(RuntimeError):self.publish(4)
    def test_higher_revision_cannot_publish_an_older_software_version(self):
        self.publish(1)
        self.spec['assets'][0]['version']='0.9.9';self.spec['assets'][0]['release_tag']='v0.9.9'
        with self.assertRaises(RuntimeError):self.publish(2)
    def test_mislabeled_installer_is_rejected_before_upload(self):
        self.spec['assets'][0]['version']='2.0.0'
        with self.assertRaises(RuntimeError):self.publish(1)
        self.assertEqual(FakePlatform.uploads,[])

if __name__=='__main__':unittest.main()
