"""Out-of-build approved-weight fetch only, no app runtime use.
Read the official ZIP by ranges and extract ONLY its MIT scratch detector.
ZIP CRC and the export tool's exact checkpoint SHA protect the extracted bytes.
"""
import urllib.request,io,zipfile,hashlib,pathlib,json
url='https://github.com/microsoft/Bringing-Old-Photos-Back-to-Life/releases/download/v1.0/global_checkpoints.zip'
size=2036400762
class Remote(io.RawIOBase):
    def __init__(self): self.pos=0
    def seekable(self): return True
    def seek(self,offset,whence=0):
        self.pos=offset+(self.pos if whence==1 else size if whence==2 else 0); return self.pos
    def tell(self): return self.pos
    def read(self,n=-1):
        n=min(n if n>=0 else size,size-self.pos)
        if n<=0:return b''
        req=urllib.request.Request(url,headers={'Range':f'bytes={self.pos}-{self.pos+n-1}'})
        with urllib.request.urlopen(req) as r:
            assert r.status==206 and r.headers['Content-Range'].startswith(f'bytes {self.pos}-'),str(r.headers)
            data=r.read()
        assert len(data)==n,(len(data),n)
        self.pos+=n;return data
with zipfile.ZipFile(Remote()) as z:
    info=z.getinfo('checkpoints/detection/FT_Epoch_latest.pt')
    print(info,flush=True)
    p=pathlib.Path('target/restore-export/FT_Epoch_latest.pt')
    h=hashlib.sha256()
    with z.open(info) as src,p.open('wb') as dst:
        while data:=src.read(1024*1024): dst.write(data);h.update(data)
    print(json.dumps({'source':url,'inner':info.filename,'bytes':p.stat().st_size,'sha256':h.hexdigest()}),flush=True)
