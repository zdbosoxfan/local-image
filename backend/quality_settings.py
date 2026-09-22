"""Local, authenticated-by-origin settings UI for the RapidRAW connector.

Integrate with ``from quality_settings import router as quality_settings_router``
and ``app.include_router(quality_settings_router)`` in main.py. The engine must
reload config.WORKFLOW_FILE for every generation (as build_workflow already does).
"""

import asyncio
import json
import logging
import os
import secrets
import tempfile
from pathlib import Path, PurePosixPath
from typing import Literal

import aiohttp
from fastapi import APIRouter, HTTPException, Request
from fastapi.responses import HTMLResponse, JSONResponse
from pydantic import BaseModel, ConfigDict, Field

import engine
from engine import config


router = APIRouter()
logger = logging.getLogger("API")
_write_lock = asyncio.Lock()
_csrf_token = secrets.token_urlsafe(32)
_local_hosts = {"127.0.0.1", "localhost", "::1"}

# object_info lists filenames, not checkpoint architectures. Offer only models
# whose SDXL architecture and compatibility with this workflow are known.
QUALITY_CHECKPOINT = "RealVisXL_V5.0_fp16.safetensors"
FAST_CHECKPOINT = "XL_RealVisXL_V5.0_Lightning.safetensors"
COMPATIBLE_CHECKPOINTS = {QUALITY_CHECKPOINT, FAST_CHECKPOINT}


class QualitySettings(BaseModel):
    model_config = ConfigDict(extra="forbid", strict=True, allow_inf_nan=False)

    checkpoint: str = Field(min_length=1, max_length=512)
    max_edge: Literal[1024, 1280, 1536, 2048]
    steps: int = Field(ge=1, le=60)
    cfg: float = Field(ge=1, le=10)
    sampler: Literal["dpmpp_sde", "dpmpp_2m", "euler"]
    scheduler: Literal["karras", "normal", "ddim_uniform"]
    context: float = Field(ge=1.05, le=2)
    mask_blend: int = Field(ge=0, le=64)
    denoise: float = Field(ge=0.1, le=1)


def _local_request(request: Request, require_origin: bool = False) -> None:
    # This also prevents a remote DNS name resolving to loopback from passing
    # the origin test. These settings are intended to be opened on this PC.
    if request.url.hostname not in _local_hosts:
        raise HTTPException(403, "Open AI settings using localhost or 127.0.0.1.")
    expected_origin = f"{request.url.scheme}://{request.url.netloc}"
    origin = request.headers.get("origin")
    if (require_origin and not origin) or (origin and origin != expected_origin):
        raise HTTPException(403, "Settings can only be changed from their local page.")
    if request.headers.get("sec-fetch-site") == "cross-site":
        raise HTTPException(403, "Open the local AI settings page directly.")


def _read_workflow() -> tuple[Path, bytes, dict]:
    path = Path(config.WORKFLOW_FILE)
    try:
        original = path.read_bytes()
        workflow = json.loads(original.decode("utf-8-sig"))
        _settings_from_workflow(workflow)
        return path, original, workflow
    except (OSError, UnicodeError, ValueError, KeyError, TypeError):
        logger.exception("Could not read the AI settings workflow")
        raise HTTPException(500, "The AI workflow could not be read. Check the connector log.")


def _settings_from_workflow(workflow: dict) -> dict:
    sample = workflow["28"]["inputs"]
    crop = workflow["36"]["inputs"]
    return {
        "checkpoint": workflow["1"]["inputs"]["ckpt_name"],
        "max_edge": workflow["37"]["inputs"]["value"],
        "steps": sample["steps"],
        "cfg": sample["cfg"],
        "sampler": sample["sampler_name"],
        "scheduler": sample["scheduler"],
        "context": crop["context_from_mask_extend_factor"],
        "mask_blend": crop["mask_blend_pixels"],
        "denoise": sample["denoise"],
    }


async def _installed_checkpoints() -> list[str]:
    try:
        async with aiohttp.ClientSession(timeout=aiohttp.ClientTimeout(total=10)) as session:
            async with session.get(f"{config.http_url}/object_info/CheckpointLoaderSimple") as response:
                response.raise_for_status()
                data = await response.json()
        names = data["CheckpointLoaderSimple"]["input"]["required"]["ckpt_name"][0]
        if not isinstance(names, list) or not all(isinstance(name, str) for name in names):
            raise ValueError("Invalid checkpoint list")
        return sorted({name for name in names if _basename(name) in COMPATIBLE_CHECKPOINTS})
    except (aiohttp.ClientError, asyncio.TimeoutError, KeyError, TypeError, ValueError):
        logger.exception("Could not get installed checkpoints from ComfyUI")
        raise HTTPException(503, "ComfyUI is unavailable. Start RapidRAW with AI, then reload this page.")


def _basename(name: str) -> str:
    return PurePosixPath(name.replace("\\", "/")).name


def _presets(checkpoints: list[str]) -> dict:
    names = {_basename(name): name for name in checkpoints}
    return {
        "quality": {
            "available": QUALITY_CHECKPOINT in names,
            "settings": {
                "checkpoint": names.get(QUALITY_CHECKPOINT, QUALITY_CHECKPOINT),
                "max_edge": 1536, "steps": 32, "cfg": 3.5,
                "sampler": "dpmpp_sde", "scheduler": "karras",
                "context": 1.5, "mask_blend": 32, "denoise": 1.0,
            },
        },
        "fast": {
            "available": FAST_CHECKPOINT in names,
            "settings": {
                "checkpoint": names.get(FAST_CHECKPOINT, FAST_CHECKPOINT),
                "max_edge": 1280, "steps": 5, "cfg": 1.5,
                "sampler": "dpmpp_sde", "scheduler": "karras",
                "context": 1.5, "mask_blend": 32, "denoise": 1.0,
            },
        },
    }


def _save_settings(settings: QualitySettings) -> dict:
    path, original, workflow = _read_workflow()
    workflow["1"]["inputs"]["ckpt_name"] = settings.checkpoint
    sample = workflow["28"]["inputs"]
    sample.update(steps=settings.steps, cfg=settings.cfg,
                  sampler_name=settings.sampler, scheduler=settings.scheduler,
                  denoise=settings.denoise)
    workflow["36"]["inputs"].update(
        context_from_mask_extend_factor=settings.context,
        mask_blend_pixels=settings.mask_blend,
    )
    workflow["37"]["inputs"]["value"] = settings.max_edge
    temporary_path = None
    try:
        backup = path.with_name(path.name + ".before-quality-settings")
        try:
            with backup.open("xb") as stream:
                try:
                    stream.write(original)
                    stream.flush()
                    os.fsync(stream.fileno())
                except OSError:
                    # Remove an incomplete backup while leaving the live workflow
                    # untouched. Never replace an already existing backup.
                    stream.close()
                    backup.unlink(missing_ok=True)
                    raise
        except FileExistsError:
            pass
        # A temporary file in the same directory permits atomic replacement;
        # generation sees either the complete old workflow or the complete new one.
        with tempfile.NamedTemporaryFile(mode="w", encoding="utf-8", newline="\n",
                                         prefix=".quality-", suffix=".json",
                                         dir=path.parent, delete=False) as stream:
            temporary_path = Path(stream.name)
            json.dump(workflow, stream, ensure_ascii=False, indent=2)
            stream.write("\n")
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(temporary_path, path)
    except OSError:
        logger.exception("Could not save AI settings")
        raise HTTPException(500, "Settings could not be saved. Check the connector log.")
    finally:
        if temporary_path is not None and temporary_path.exists():
            try:
                temporary_path.unlink(missing_ok=True)
            except OSError:
                logger.warning("Could not remove an unused temporary settings file")
    return _settings_from_workflow(workflow)


@router.get("/api/quality-settings")
async def get_quality_settings(request: Request):
    _local_request(request)
    _, _, workflow = _read_workflow()
    checkpoints = await _installed_checkpoints()
    return JSONResponse({"settings": _settings_from_workflow(workflow),
                         "checkpoints": checkpoints, "presets": _presets(checkpoints)},
                        headers={"Cache-Control": "no-store"})


@router.post("/api/quality-settings")
async def set_quality_settings(request: Request, settings: QualitySettings):
    _local_request(request, require_origin=True)
    supplied_token = request.headers.get("x-quality-csrf", "")
    if not secrets.compare_digest(supplied_token.encode("utf-8"), _csrf_token.encode("ascii")):
        raise HTTPException(403, "Reload this settings page and try saving again.")
    checkpoints = await _installed_checkpoints()
    if settings.checkpoint not in checkpoints:
        raise HTTPException(422, "Choose an installed, compatible SDXL checkpoint from the list.")
    async with _write_lock:
        active = _save_settings(settings)
    return JSONResponse({"settings": active, "message": "Saved. Your next AI edit will use these settings."},
                        headers={"Cache-Control": "no-store"})


@router.get("/api/quality-diagnostics")
async def get_quality_diagnostics(request: Request):
    _local_request(request)
    try:
        # The same engine helper supplies the execution plan and the saved-mask
        # preview, so diagnostic region counts cannot diverge from generation.
        getter = getattr(engine, "get_last_diagnostics", None)
        diagnostic = await asyncio.to_thread(getter) if getter else None
        if not isinstance(diagnostic, dict):
            diagnostic = {"status": "unavailable"}
    except Exception:
        logger.exception("Could not read last mask diagnostics")
        diagnostic = {"status": "unavailable"}
    return JSONResponse(diagnostic, headers={"Cache-Control": "no-store"})


@router.get("/settings", response_class=HTMLResponse)
async def quality_settings_page(request: Request):
    _local_request(request)
    nonce = secrets.token_urlsafe(24)
    page = _PAGE.replace("__NONCE__", nonce).replace("__CSRF__", _csrf_token)
    return HTMLResponse(page, headers={
        "Cache-Control": "no-store",
        "X-Frame-Options": "DENY",
        "X-Content-Type-Options": "nosniff",
        "Referrer-Policy": "no-referrer",
        "Content-Security-Policy": (
            "default-src 'none'; base-uri 'none'; frame-ancestors 'none'; "
            f"style-src 'nonce-{nonce}'; script-src 'nonce-{nonce}'; "
            "connect-src 'self'; form-action 'self'"
        ),
    })


_PAGE = r"""<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>RapidRAW AI settings</title>
<style nonce="__NONCE__">
:root{color-scheme:dark;font-family:system-ui,-apple-system,"Segoe UI",sans-serif;background:#101217;color:#edf0f6}
*{box-sizing:border-box}body{margin:0}main{max-width:880px;margin:auto;padding:48px 24px 56px}
.eyebrow{color:#97b8ff;font-size:12px;letter-spacing:.16em;font-weight:650;text-transform:uppercase}
h1{font-size:32px;letter-spacing:-.025em;margin:10px 0 10px}h2{font-size:18px;margin:0 0 20px}
p{line-height:1.6;color:#b4bdcb;margin:8px 0 0}.intro{max-width:670px;margin-bottom:28px}
.card{border:1px solid #323846;border-radius:16px;padding:24px;background:#181c24;margin-bottom:18px}
.grid{display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:22px}.wide{grid-column:1/-1}
label{display:block;font-size:14px;font-weight:600;margin:0 0 9px}
input,select,button{font:inherit}input,select{width:100%;border:1px solid #465062;border-radius:8px;padding:11px 12px;background:#11151d;color:#edf0f6;min-height:44px}
input:focus-visible,select:focus-visible,button:focus-visible,summary:focus-visible{outline:3px solid #96bbff;outline-offset:3px}
.hint{font-size:13px;line-height:1.55;color:#aab5c6;margin:9px 0 0}.preset-hint{padding:14px 16px;border-radius:9px;background:#202a3c;color:#c5d5f1;font-size:14px;margin:16px 0 24px}
fieldset{border:0;padding:0;margin:0;min-width:0}fieldset:disabled{opacity:.65}
summary{font-weight:600;cursor:pointer;min-height:32px}details .grid{padding-top:21px}
.footer{display:flex;align-items:center;gap:18px;flex-wrap:wrap;margin-top:24px}button{cursor:pointer;border:0;border-radius:9px;min-height:46px;padding:12px 21px;font-weight:650;background:#a9c7ff;color:#10203b}button:disabled{opacity:.45;cursor:wait}
#status{font-size:14px;line-height:1.5;flex:1;min-width:210px;color:#b9c5d8}#status[data-state="error"]{color:#ffb2b2}#status[data-state="ok"]{color:#9fe3bd}
.small{font-size:13px;margin-top:22px;color:#9aa8bb}a{color:#a9c7ff}a:focus-visible{outline:3px solid #96bbff;outline-offset:3px}
.diagnostics{margin-top:28px}.diagnostics dl{display:grid;grid-template-columns:minmax(145px,1fr) 2fr;gap:10px 18px;font-size:14px;line-height:1.55}.diagnostics dt{color:#aab5c6}.diagnostics dd{margin:0;overflow-wrap:anywhere}.diagnostics button{background:#303b4e;color:#e5edfb;font-size:13px;min-height:40px;padding:9px 15px;margin-top:12px}
.regions{margin:15px 0 0;padding-left:22px;font-size:14px;line-height:1.8;color:#c9d2df}
@media(max-width:600px){main{padding:30px 16px}.card{padding:20px}.grid{grid-template-columns:1fr}h1{font-size:28px}.footer{align-items:stretch}button{width:100%}.diagnostics dl{grid-template-columns:1fr;gap:3px}.diagnostics dd{margin-bottom:9px}}
</style></head><body><main>
<div class="eyebrow">RapidRAW · Local AI</div><h1>AI editing settings</h1>
<p class="intro"><strong>Remove Brush uses FLUX.2 Klein with a specialized object-removal adapter automatically. No prompt is needed.</strong> The settings below apply to edits with a typed prompt. This custom local panel edits the connector's workflow. Changes apply after saving; an edit already running keeps its current settings.</p>
<form id="settings-form"><fieldset id="fields" disabled>
<section class="card" aria-labelledby="model-heading"><h2 id="model-heading">Model &amp; detail</h2>
<label for="preset">Starting preset</label><select id="preset"><option value="custom">Custom settings</option><option value="quality">Quality — more detail, slower</option><option value="fast">Fast — quick previews</option></select>
<p class="preset-hint">Quality uses the full RealVisXL model, more sampling steps, and a larger working area. You can adjust either preset before saving.</p>
<div class="grid"><div class="wide"><label for="checkpoint">SDXL model</label><select id="checkpoint" name="checkpoint" required></select><p class="hint" id="model-hint">Lists installed RealVisXL models checked for compatibility with this integration.</p></div>
<div class="wide"><label for="max_edge">Maximum generated edge</label><select id="max_edge" name="max_edge"><option value="1024">1024 pixels — lighter</option><option value="1280">1280 pixels — Fast preset</option><option value="1536">1536 pixels — Quality preset</option><option value="2048">2048 pixels — wider shapes</option></select><p class="hint">The longest edge of each generated region is limited by this value. Wide masked areas larger than this are rescaled; smaller selections retain more detail. Dimensions follow the selection's shape, with an overall 1536 × 1536 pixel budget. At 2048, a wide region can be longer but a square remains at most 1536 × 1536. Your final photo keeps its original dimensions.</p></div>
<div><label for="steps">Sampling steps</label><input id="steps" name="steps" type="number" min="1" max="60" step="1" required><p class="hint">Quality: 32. Lightning / Fast: 5. More steps take longer and do not always add detail.</p></div>
<div><label for="cfg">Prompt guidance (CFG)</label><input id="cfg" name="cfg" type="number" min="1" max="10" step="0.1" required><p class="hint">Quality: 3.5. Lightning / Fast: 1.5. Very high guidance can make results harsh.</p></div></div></section>
<section class="card"><details><summary>Advanced settings</summary><div class="grid">
<div><label for="sampler">Sampler</label><select id="sampler" name="sampler"><option value="dpmpp_sde">DPM++ SDE</option><option value="dpmpp_2m">DPM++ 2M</option><option value="euler">Euler</option></select><p class="hint">For full RealVisXL V5, the author recommends SDE/Karras with 30+ steps, or 2M/Karras with 50+.</p></div>
<div><label for="scheduler">Scheduler</label><select id="scheduler" name="scheduler"><option value="karras">Karras</option><option value="normal">Normal</option><option value="ddim_uniform">DDIM uniform</option></select></div>
<div><label for="context">Context around selection</label><input id="context" name="context" type="number" min="1.05" max="2" step="0.01" required><p class="hint">1.50 is the default for prompted edits. Larger values show more surroundings to help match the background; smaller values devote more resolution to the selection.</p></div>
<div><label for="mask_blend">Mask edge blend (pixels)</label><input id="mask_blend" name="mask_blend" type="number" min="0" max="64" step="1" required><p class="hint">32 is a starting point for softer transitions at the selection boundary.</p></div>
<div class="wide"><label for="denoise">Edit strength (denoise)</label><input id="denoise" name="denoise" type="number" min="0.1" max="1" step="0.05" required><p class="hint">1.0 allows a full replacement inside the mask. Lower values retain more of the original content.</p></div>
</div></details></section>
</fieldset><div class="footer"><button id="save" type="submit" disabled>Save settings</button><div id="status" role="status" aria-live="polite">Loading current settings…</div></div></form>
<section class="card diagnostics" aria-labelledby="diagnostic-heading"><h2 id="diagnostic-heading">Mask received from RapidRAW</h2><p class="hint">The source canvas size is the whole photo. Selection bounds enclose all marked areas, including empty gaps between separate groups. Each processing group gets its own generated patch.</p><p id="diagnostic-status" class="hint" role="status" aria-live="polite">Loading the latest mask information…</p><dl id="diagnostic-details" hidden></dl><ul id="regions" class="regions" hidden></ul><button id="refresh-diagnostics" type="button">Refresh mask information</button></section>
<p class="small"><strong>GPU removal:</strong> In RapidRAW, open Inpainting (K), choose <strong>Remove Brush</strong>, cover the whole object and click <strong>Remove</strong>. Blank prompts on other generative selections also use the specialized removal model. A typed prompt uses the SDXL settings above.</p>
<p class="small">The connector documents workflow customization in its <a href="https://github.com/CyberTimon/RapidRAW-AI-Connector#customization" target="_blank" rel="noopener noreferrer">official README</a>. Model author guidance: <a href="https://huggingface.co/SG161222/RealVisXL_V5.0" target="_blank" rel="noopener noreferrer">RealVisXL V5</a> and <a href="https://huggingface.co/SG161222/RealVisXL_V5.0_Lightning" target="_blank" rel="noopener noreferrer">V5 Lightning</a>. The Quality sampler and step count follow the model author's guidance; resolution, context, and CFG 3.5 are local choices tested for this setup.</p>
</main><script nonce="__NONCE__">
'use strict';
const csrf = '__CSRF__';
const keys = ['checkpoint','max_edge','steps','cfg','sampler','scheduler','context','mask_blend','denoise'];
const textKeys = new Set(['checkpoint','sampler','scheduler']);
const byId = id => document.getElementById(id);
const fields = byId('fields'), save = byId('save'), status = byId('status'), preset = byId('preset');
let presets = {}, installed = [], busy = false;
let settingsLoaded = false, loadInFlight = false, loadTimer = null;
function message(text, state=''){status.textContent=text;status.dataset.state=state;}
function readForm(){return Object.fromEntries(keys.map(key => [key,textKeys.has(key)?byId(key).value:Number(byId(key).value)]));}
function identifyPreset(settings){return Object.keys(presets).find(name => keys.every(key => typeof settings[key]==='number'?Math.abs(settings[key]-presets[name].settings[key])<0.000001:settings[key]===presets[name].settings[key]))||'custom';}
function fill(settings){for(const key of keys){byId(key).value=typeof settings[key]==='number'?String(Number(settings[key].toFixed(4))):settings[key];}preset.value=identifyPreset(settings);}
function refreshSave(){save.disabled=busy||!installed.includes(byId('checkpoint').value);}
async function api(method='GET', payload, url='/api/quality-settings'){
  const options={method,credentials:'same-origin',cache:'no-store'};
  if(payload){options.headers={'Content-Type':'application/json','X-Quality-CSRF':csrf};options.body=JSON.stringify(payload);}
  let response;
  try{response=await fetch(url,options);}catch{const error=new Error('Could not reach the AI connector.');error.status=0;throw error;}
  let data;
  try{data=await response.json();}catch{const error=new Error('The connector returned an unreadable response. Reload this page.');error.status=response.status;throw error;}
  if(!response.ok){let detail=data.detail;if(Array.isArray(detail)){detail=detail.map(item=>`${item.loc.filter(part=>part!=='body').join('.')}: ${item.msg}`).join('; ');}const error=new Error(typeof detail==='string'?detail:'Settings could not be loaded or saved.');error.status=response.status;throw error;}
  return data;
}
preset.addEventListener('change',()=>{if(preset.value==='custom')return;const selected=presets[preset.value];if(!selected?.available){message('That preset model is not installed. Choose an available model.','error');preset.value='custom';return;}fill(selected.settings);refreshSave();message('Preset selected. Save to apply it.');});
for(const key of keys){byId(key).addEventListener('input',()=>{preset.value=identifyPreset(readForm());refreshSave();message('Unsaved changes.');});}
byId('settings-form').addEventListener('invalid',event=>{const details=event.target.closest('details');if(details)details.open=true;},true);
byId('settings-form').addEventListener('submit',async event=>{event.preventDefault();if(busy||!event.currentTarget.reportValidity())return;busy=true;fields.disabled=true;refreshSave();message('Saving…');try{const data=await api('POST',readForm());fill(data.settings);message(data.message,'ok');loadDiagnostics();}catch(error){message(error.message,'error');}finally{busy=false;fields.disabled=false;refreshSave();}});
async function load(){
  if(settingsLoaded||loadInFlight)return;
  if(loadTimer!==null){clearTimeout(loadTimer);loadTimer=null;}
  loadInFlight=true;
  try{
    const data=await api();presets=data.presets;installed=data.checkpoints;
    const checkpoint=byId('checkpoint');checkpoint.replaceChildren();
    for(const name of installed){checkpoint.add(new Option(name,name));}
    if(!installed.includes(data.settings.checkpoint)){const missing=new Option(data.settings.checkpoint+' (unavailable)',data.settings.checkpoint);missing.disabled=true;checkpoint.add(missing);}
    for(const [name,value] of Object.entries(presets)){preset.querySelector(`option[value="${name}"]`).disabled=!value.available;}
    fill(data.settings);settingsLoaded=true;fields.disabled=false;refreshSave();
    if(!installed.length){message('No supported SDXL models are available. Start ComfyUI and reload.','error');}
    else if(!installed.includes(data.settings.checkpoint)){message('The saved model is unavailable. Choose an installed model and save.','error');}
    else{message('Current settings loaded.');}
  }catch(error){
    if(error.status===503||error.status===0){
      message(error.status===503?'Waiting for ComfyUI to start. Retrying automatically…':'Reconnecting to the AI connector. Retrying automatically…');
      loadTimer=setTimeout(()=>{loadTimer=null;load();},3000);
    }else{message(error.message,'error');}
  }finally{loadInFlight=false;}
}
function boxText(box){return Array.isArray(box)&&box.length===4?`${box[2]-box[0]} × ${box[3]-box[1]} px, at x ${box[0]}, y ${box[1]}`:'No selected pixels';}
function sizeText(size){return Array.isArray(size)&&size.length===2?`${size[0]} × ${size[1]} px`:'Unavailable';}
async function loadDiagnostics(){const button=byId('refresh-diagnostics'), note=byId('diagnostic-status'), details=byId('diagnostic-details'), regions=byId('regions');button.disabled=true;try{const d=await api('GET',null,'/api/quality-diagnostics');details.replaceChildren();regions.replaceChildren();details.hidden=true;regions.hidden=true;if(!Array.isArray(d.canvas_size)){note.textContent='No mask information is available yet. Run an AI edit in RapidRAW, then refresh.';return;}let state=({saved_mask_preview:'Saved mask preview using current settings',planning:'Planning the latest edit',running:'Generating the latest edit',complete:'Latest edit completed',error:'Latest edit ended with an error'})[d.status]||'Latest received mask';const timestamp=d.timestamp?new Date(d.timestamp):null;if(timestamp&&!Number.isNaN(timestamp.getTime()))state+=` · ${timestamp.toLocaleString()}`;note.textContent=state;const entries=[['Whole source canvas',sizeText(d.canvas_size)],['All selection bounds',boxText(d.selection_bbox)],['Selected coverage',Number.isFinite(d.selected_fraction)?`${(d.selected_fraction*100).toFixed(2)}% of canvas`:'Unavailable'],['Separate processing groups',Number.isInteger(d.region_count)?String(d.region_count):'Unavailable']];for(const [name,value] of entries){const dt=document.createElement('dt'),dd=document.createElement('dd');dt.textContent=name;dd.textContent=value;details.append(dt,dd);}details.hidden=false;for(const [index,region] of (d.regions||[]).entries()){const li=document.createElement('li');li.textContent=`Group ${index+1}: selection ${boxText(region.bbox)} → generated ${sizeText(region.generation_size)}`;regions.append(li);}regions.hidden=!regions.childElementCount;}catch(error){note.textContent=error.message;}finally{button.disabled=false;}}
byId('refresh-diagnostics').addEventListener('click',loadDiagnostics);
load();loadDiagnostics();
</script></body></html>"""
