#!/usr/bin/env python3
"""Permanent CC0 decoder comparison driver. No C/reference program enters the Cargo build.
Use --fetch, --references, --run, --robustness, --timings (explicit opt-in), --threads 4.
"""
import argparse, csv, ctypes, hashlib, json, os, re, resource, shutil, subprocess, urllib.request, urllib.parse
from pathlib import Path
LIBRAW_HARNESS = r'''/* External reference only: compiled under target/, never in Cargo or the shipped app. */
#include <libraw/libraw.h>
int main(int argc,char **argv) {
 if(argc!=3)return 2;
 libraw_data_t *r=libraw_init(0);r->rawparams.options &= ~LIBRAW_RAWOPTIONS_CONVERTFLOAT_TO_INT;int e=libraw_open_file(r,argv[1]);if(!e)e=libraw_unpack(r);
 if(e){fprintf(stderr,"%s\n",libraw_strerror(e));libraw_close(r);return 1;}
 char name[4096];snprintf(name,sizeof(name),"%s.meta",argv[2]);FILE *m=fopen(name,"w");if(!m)return 2;
 unsigned cpp=(r->rawdata.raw_image||r->rawdata.float_image)?1:((r->rawdata.color3_image||r->rawdata.float3_image)?3:(r->idata.colors>=1&&r->idata.colors<=4?r->idata.colors:4));
 fprintf(m,"%u\t%u\t%u\n%u\t%u\t%u\t%u\n",r->sizes.raw_width,r->sizes.raw_height,cpp,r->sizes.left_margin,r->sizes.top_margin,r->sizes.width,r->sizes.height);
 for(int i=0;i<4;i++)fprintf(m,"%u%c",r->color.black+r->color.cblack[i],i==3?'\n':'\t');
 fprintf(m,"%u\n%.9g\t%.9g\t%.9g\n%d\n%s\n%s\n",r->color.maximum,r->color.cam_mul[0],r->color.cam_mul[1],r->color.cam_mul[2],r->sizes.flip,r->idata.make,r->idata.model);
 fprintf(m,"%u\t%u\n",r->color.cblack[4],r->color.cblack[5]);
 for(unsigned i=0;i<r->color.cblack[4]*r->color.cblack[5] && i+6<LIBRAW_CBLACK_SIZE;i++)fprintf(m,"%u\t",r->color.cblack[i+6]);
 fprintf(m,"\n");
 for(int y=0;y<6;y++)for(int x=0;x<6;x++)fprintf(m,"%d",libraw_COLOR(r,y,x));
 fprintf(m,"\n");fclose(m);
 snprintf(name,sizeof(name),"%s.f32",argv[2]);FILE *out=fopen(name,"wb");if(!out)return 2;
 if(!r->rawdata.raw_image&&!r->rawdata.float_image&&!r->rawdata.color3_image&&!r->rawdata.color4_image&&!r->rawdata.float3_image&&!r->rawdata.float4_image){libraw_close(r);fclose(out);return 3;}
 for(unsigned y=0;y<r->sizes.raw_height;y++)for(unsigned x=0;x<r->sizes.raw_width;x++)for(unsigned s=0;s<cpp;s++){
  unsigned i=y*r->sizes.raw_width+x;float v;
  if(r->rawdata.float3_image)v=r->rawdata.float3_image[i][s];
  else if(r->rawdata.float4_image)v=r->rawdata.float4_image[i][s];
  else if(r->rawdata.float_image)v=r->rawdata.float_image[i];
  else if(r->rawdata.raw_image)v=r->rawdata.raw_image[y*(r->sizes.raw_pitch/2)+x];
  else if(r->rawdata.color3_image)v=r->rawdata.color3_image[i][s];else v=r->rawdata.color4_image[i][s];
  fwrite(&v,4,1,out);
 }
 fclose(out);libraw_close(r);return 0;
}
'''

ROOT = Path(__file__).resolve().parents[1]
MANIFEST = ROOT / 'crates/lc-raw/tests/fixtures/decoder-comparison-samples.tsv'

def run(args, log, env=None, limit=False):
    def cap(): resource.setrlimit(resource.RLIMIT_AS,(768*1024**2,768*1024**2))
    with log.open('w') as f:
        p=subprocess.run(args,cwd=ROOT,env=env,stdout=f,stderr=subprocess.STDOUT,preexec_fn=cap if limit else None)
    return p.returncode

def libraw_reference(cache, rows):
    library=ctypes.CDLL('libraw.so.25');library.libraw_version.restype=ctypes.c_char_p
    version=library.libraw_version().decode();tag=version.split('-')[0]
    build=ROOT/'target/refvec/decoder-comparison-libraw';build.mkdir(parents=True,exist_ok=True)
    header=build/'libraw';header.mkdir(exist_ok=True)
    for name in ['libraw.h','libraw_types.h','libraw_const.h','libraw_version.h','libraw_datastream.h','libraw_internal.h','libraw_alloc.h']:
        p=header/name
        if not p.exists():p.write_bytes(urllib.request.urlopen(f'https://raw.githubusercontent.com/LibRaw/LibRaw/{tag}/libraw/{name}').read())
    # Reference C exists only under target and is never compiled by Cargo.
    (build/'main.c').write_text(LIBRAW_HARNESS)
    subprocess.run(['cc','-O2','-I',str(build),str(build/'main.c'),'/usr/lib64/libraw.so.25','-o',str(build/'reference')],check=True)
    schema=hashlib.sha256(LIBRAW_HARNESS.encode()).hexdigest()
    for row in rows:
        prefix=cache/(row['id']+'.libraw')
        if prefix.with_suffix('.libraw.f32').exists() and prefix.with_suffix('.libraw.f32').stat().st_size > 0 and prefix.with_suffix('.libraw.schema').exists() and prefix.with_suffix('.libraw.schema').read_text()==schema:continue
        status=run([str(build/'reference'),str(cache/row['file']),str(prefix)],cache/(row['id']+'.libraw.log'))
        if status==0:prefix.with_suffix('.libraw.schema').write_text(schema)
    (cache/'libraw-version.txt').write_text(version+'\n')

def rendered_references(cache, rows, threads):
    env=os.environ.copy();env['OMP_NUM_THREADS']=str(threads)
    env['XDG_CONFIG_HOME']=str(cache/'reference-config');env['XDG_CACHE_HOME']=str(cache/'reference-cache')
    (cache/'reference-config').mkdir(exist_ok=True);(cache/'reference-cache').mkdir(exist_ok=True)
    for row in rows:
        src=cache/row['file']
        for tool,app,args in [('darktable','org.darktable.Darktable',['--command=darktable-cli']),('rawtherapee','com.rawtherapee.RawTherapee',['--command=rawtherapee-cli'])]:
            dest=cache/f'{row["id"]}.{tool}.tif';log=cache/f'{row["id"]}.{tool}.log'
            if dest.exists():continue
            if tool=='darktable':
                command=['flatpak','run','--filesystem='+str(cache),'--env=XDG_CONFIG_HOME='+env['XDG_CONFIG_HOME'],'--env=XDG_CACHE_HOME='+env['XDG_CACHE_HOME'],'--env=OMP_NUM_THREADS='+str(threads),*args,app,str(src),str(dest),'--width','512','--height','512','--apply-custom-presets','false','--core','--disable-opencl','--configdir',str(cache/'reference-config/darktable'),'--cachedir',str(cache/'reference-cache/darktable'),'--conf','plugins/imageio/format/tiff/bpp=16','--conf','write_sidecar_files=never']
            else:
                profile=cache/'reference-neutral.pp3';profile.write_text('[Version]\nVersion=352\n[Resize]\nEnabled=true\nAppliesTo=Full image\nMethod=Lanczos\nDataSpecified=3\nWidth=512\nHeight=512\nAllowUpscaling=false\n')
                command=['flatpak','run','--filesystem='+str(cache),'--env=XDG_CONFIG_HOME='+env['XDG_CONFIG_HOME'],'--env=XDG_CACHE_HOME='+env['XDG_CACHE_HOME'],'--env=OMP_NUM_THREADS='+str(threads),*args,app,'-q','-t','-b16','-Y','-p',str(profile),'-o',str(dest),'-c',str(src)]
            status=run(command,log,env)
            print(row['id'],tool,'cached-render' if status==0 and dest.exists() else 'failed',flush=True)
    # RGB exports do not expose CFA. This explicit capability ledger prevents bogus CFA comparisons.
    (cache/'reference-capabilities.json').write_text(json.dumps({'libraw':'unpacked CFA + metadata','darktable-cli':'rendered RGB only; CFA comparison unavailable through CLI','rawtherapee-cli':'rendered RGB only; CFA comparison unavailable through CLI','dcraw_emu':shutil.which('dcraw_emu') or 'not installed'},indent=2)+'\n')

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--manifest',type=Path,default=MANIFEST);p.add_argument('--fetch',action='store_true');p.add_argument('--references',action='store_true');p.add_argument('--renders',action='store_true');p.add_argument('--run',action='store_true');p.add_argument('--robustness',action='store_true');p.add_argument('--backends',nargs='+',choices=['ours','rawler','adapter'],default=['ours','rawler','adapter']);p.add_argument('--timings',action='store_true');p.add_argument('--threads',type=int,default=4);p.add_argument('--ids',nargs='*');p.add_argument('--cache',type=Path,default=ROOT/'target/raw-comparison');a=p.parse_args()
    cache=a.cache.resolve();cache.mkdir(parents=True,exist_ok=True)
    with a.manifest.open() as f:rows=[r for r in csv.DictReader(f,delimiter='\t') if not a.ids or r['id'] in a.ids]
    for row in rows:
        path=cache/row['file']
        if a.fetch and not path.exists():
            previous=ROOT/'target/rawdecode'/row['file']
            data=previous.read_bytes() if previous.exists() else urllib.request.urlopen(urllib.parse.quote(row['url'],safe=':/'),timeout=120).read()
            if hashlib.sha256(data).hexdigest()!=row['sha256']:raise RuntimeError('source hash mismatch '+row['file'])
            path.write_bytes(data)
        if path.exists() and hashlib.sha256(path.read_bytes()).hexdigest()!=row['sha256']:raise RuntimeError('source hash mismatch '+row['file'])
    if a.references:libraw_reference(cache,rows)
    if a.renders:rendered_references(cache,rows,a.threads)
    logical=os.cpu_count()
    topology=set()
    for cpu in Path('/sys/devices/system/cpu').glob('cpu[0-9]*'):
        try:topology.add(((cpu/'topology/physical_package_id').read_text().strip(),(cpu/'topology/core_id').read_text().strip()))
        except OSError:pass
    (cache/'machine.json').write_text(json.dumps({'logical_cores':logical,'physical_cores':len(topology),'decoder_workers':a.threads,'timings_enabled':a.timings},indent=2)+'\n')
    env=os.environ.copy();env.update(RAW_COMPARE_MANIFEST=str(a.manifest.resolve()),RAW_COMPARE_DIR=str(cache),RAW_COMPARE_THREADS=str(a.threads),RAW_COMPARE_IDS=','.join(r['id'] for r in rows),RAW_COMPARE_TIMINGS='1' if a.timings else '0')
    cargo=[str(Path.home()/'.cargo/bin/cargo'),'+1.98.1','test','--offline','-p','lightcraft-raw','--lib']
    if a.run:
        status=run([*cargo,'decoder_compare_cc0','--','--ignored','--nocapture','--test-threads=1'],cache/'comparison-run.log',env)
        if status:raise SystemExit(status)
    if a.robustness:
        status=run([*cargo,'decoder_compare_build_fuzz_corpus','--','--ignored','--nocapture'],cache/'fuzz-build.log',env)
        if status:raise SystemExit(status)
        log=(cache/'fuzz-build.log').read_text()
        match=re.search(r'Running unittests.*?\((target/debug/deps/[^)]+)\)',log)
        if not match:raise RuntimeError('cannot identify the test executable in cargo output')
        binary=ROOT/match[1]
        statuses=json.loads((cache/'fuzz-process-status.json').read_text()) if len(a.backends)<3 and (cache/'fuzz-process-status.json').exists() else {}
        for backend in a.backends:
            e=env.copy();e['RAW_COMPARE_FUZZ_BACKEND']=backend
            skip=0;segments=[]
            for attempt in range(512):
                e['RAW_COMPARE_FUZZ_SKIP']=str(skip)
                memory=cache/f'fuzz-{backend}-{attempt}.memory'
                command=['/usr/bin/time','-f','peak_rss_kib=%M','-o',str(memory),str(binary),'decoder_compare_fuzz_child','--ignored','--nocapture','--test-threads=1']
                code=run(command,cache/f'fuzz-{backend}-{attempt}.log',e,True)
                segment={'start':skip,'exit':code,'memory':memory.read_text() if memory.exists() else None}
                if code in [0,101] and (cache/f'robustness-{backend}.tsv').exists():
                    segment['result']=(cache/f'robustness-{backend}.tsv').read_text()
                    segments.append(segment);break
                checkpoint=(cache/f'fuzz-{backend}-checkpoint.tsv').read_text().split()
                segment['checkpoint']=checkpoint
                segment['failed_case']=(cache/f'fuzz-{backend}-progress.txt').read_text()
                skip=int(checkpoint[0])+1
                segments.append(segment)
            statuses[backend]=segments
        (cache/'fuzz-process-status.json').write_text(json.dumps(statuses,indent=2)+'\n')
        ledger={}
        for backend,segments in statuses.items():
            totals=[0,0,0,0];peak=0
            for segment in segments:
                checkpoint=segment.get('checkpoint')
                values=checkpoint[1:5] if checkpoint else (segment['result'].splitlines()[-1].split('\t')[1:5] if 'result' in segment else [0]*4)
                totals=[old+int(value) for old,value in zip(totals,values)]
                match=re.search(r'peak_rss_kib=(\d+)',segment['memory'] or '')
                if match:peak=max(peak,int(match[1]))
            ledger[backend]={'ok':totals[0],'errors':totals[1],'escaped_panics':totals[2],'internal_panics':totals[3],'allocation_aborts':sum(s['exit']==134 for s in segments),'peak_rss_kib':peak,'completed':segments[-1]['exit'] in [0,101] and 'result' in segments[-1]}
        (cache/'robustness-summary.json').write_text(json.dumps(ledger,indent=2)+'\n')

if __name__=='__main__':main()
