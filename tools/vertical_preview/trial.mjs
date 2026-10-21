import { randomUUID } from 'node:crypto';
import { createRequire } from 'node:module';
import { mkdir, writeFile, readFile, access } from 'node:fs/promises';
import { spawn } from 'node:child_process';
import path from 'node:path';
const [playwrightModule, endpoint, outputLibrary, appProcessId, label] = process.argv.slice(2);
if (!playwrightModule || !endpoint || !outputLibrary || !/^\d+$/.test(appProcessId ?? '') || !/^[ab]-[1-5]$/.test(label ?? '')) throw new Error('Expected playwright module, CDP endpoint, isolated output library, app PID, a|b-1..5');
const diagnostic = process.env.CLIP_DIAGNOSTIC || "";
if (diagnostic && !["gpu-upload-off", "gpu-draw-off", "gpu-no-flip", "copy-off", "filter-off", "cap320", "blur-off", "colour-off", "promote", "context-filter", "wrapper-filter", "backdrop-filter", "context160", "context320"].includes(diagnostic)) throw new Error("Unknown diagnostic");
const trialName = diagnostic ? `diagnostic-${diagnostic}-${label}` : label;
const root = path.resolve(`build/perf/qb-clip-010/trials/${trialName}`);
await mkdir(root, { recursive: false });
const { chromium } = createRequire(import.meta.url)(playwrightModule);
const browser = await chromium.connectOverCDP(endpoint);
try {
  const page = browser.contexts()[0].pages().find(p => p.url().startsWith('http://tauri.localhost'));
  if (!page) throw new Error('Production page missing');
  const session = await browser.contexts()[0].newCDPSession(page);
  await session.send('Emulation.setDeviceMetricsOverride', { width:1440,height:900,deviceScaleFactor:1,mobile:false });
  await page.bringToFront();
  if (!(await page.locator('body').innerText()).includes('CLIP WORKBENCH')) {
    await page.getByRole('button', { name:/Open .* recording from/ }).click({ force:true });
    await page.getByRole('button', { name:'Clip',exact:true }).waitFor();
    await page.getByRole('button', { name:'Clip',exact:true }).click({ force:true });
    const endpoint = page.getByRole('button', { name:/Clip ends at/ });
    if (await endpoint.getAttribute('aria-label') !== 'Clip ends at 0:05') throw new Error('Unexpected initial clip endpoint');
    // Each complete press moves exactly one 60fps source frame; no timed key hold.
    for (let frame = 0; frame < 1200; frame++) await endpoint.press('ArrowRight');
    if (await endpoint.getAttribute('aria-label') !== 'Clip ends at 0:25') throw new Error('Exact clip endpoint setup failed');
    await page.getByRole('button', { name:'Export clip',exact:true }).click({ force:true });
  }
  await page.getByRole('button', { name:/^EXPORT (?:\d+ )?MP4S?$/i }).waitFor();
  await page.waitForFunction(expected => document.body.innerText.replaceAll('\\','/').includes(expected.replaceAll('\\','/')+'/clips/'), outputLibrary.replace(/\/$/,''));
  const text = (await page.locator('body').innerText()).replaceAll('\\','/');
  await writeFile(path.join(root,'workbench.txt'),text);
  if (!text.includes(outputLibrary.replaceAll('\\','/').replace(/\/$/,'')+'/clips/') || !text.includes('0:25')) throw new Error('Isolated output/range guard failed');
  const pause = page.getByRole('button', { name:'PAUSE',exact:true });
  if (await pause.count()) await pause.click({ force:true });
  await page.getByRole('group', { name:'Export formats' }).locator('div').filter({ has:page.locator(':scope > label') }).filter({ hasText:'TikTok' }).getByRole('button').click({ force:true });
  await page.waitForFunction(() => [...document.querySelectorAll('video')].every(v=>v.readyState>=2 && !v.seeking));
  await page.getByRole('slider', { name:'Clip preview position' }).fill('0');
  await page.waitForFunction(() => [...document.querySelectorAll('video')].every(v=>!v.seeking && v.currentTime<.05));
  if (!diagnostic) {
    await page.getByRole('slider', { name:'Clip preview position' }).fill('4000');
    await page.waitForFunction(() => [...document.querySelectorAll('video')].every(v=>!v.seeking && Math.abs(v.currentTime-4)<.04));
    await page.screenshot({path:path.join(root,'settled-4000.png')});
    await page.getByRole('slider', { name:'Clip preview position' }).fill('0');
    await page.waitForFunction(() => [...document.querySelectorAll('video')].every(v=>!v.seeking && v.currentTime<.05));
  }
  if (diagnostic) await page.evaluate(mode => {
    const canvas = document.querySelector('canvas');
    if (!canvas) throw new Error('Diagnostic requires canvas');
    if (mode.startsWith('gpu-')) {
      const gl = canvas.getContext('webgl');
      if (mode === 'gpu-upload-off') gl.texSubImage2D = () => {};
      if (mode === 'gpu-draw-off') gl.drawArrays = () => {};
      if (mode === 'gpu-no-flip') gl.pixelStorei(gl.UNPACK_FLIP_Y_WEBGL, false);
    }
    if (mode === 'copy-off') canvas.getContext('2d').drawImage = () => {};
    if (mode === 'filter-off') canvas.style.filter = 'none';
    if (mode === 'context160' || mode === 'context320') {
      const cap=mode==='context160'?160:320;
      canvas.height=Math.round(canvas.height*cap/canvas.width);canvas.width=cap;
      canvas.style.filter='none';
      const scale=Math.max(canvas.clientWidth/canvas.width,canvas.clientHeight/canvas.height);
      canvas.getContext('2d').filter=`blur(${17/scale}px) brightness(.48) saturate(.82)`;
    }
    if (mode === 'wrapper-filter') {
      const wrapper=document.createElement('div');
      Object.assign(wrapper.style,{position:'absolute',inset:'-26px',filter:'blur(17px) brightness(.48) saturate(.82)',willChange:'filter',transform:'translateZ(0)'});
      canvas.before(wrapper);wrapper.append(canvas);
      Object.assign(canvas.style,{inset:'0',width:'100%',height:'100%',filter:'none',willChange:'auto',transform:'none'});
    }
    if (mode === 'backdrop-filter') {
      canvas.style.filter='none';
      const overlay=document.createElement('div');
      Object.assign(overlay.style,{position:'absolute',inset:'-26px',backdropFilter:'blur(17px) brightness(.48) saturate(.82)',pointerEvents:'none'});
      canvas.after(overlay);
    }
    if (mode === 'context-filter') {
      canvas.style.filter = 'none';
      const scale = Math.max(canvas.clientWidth/canvas.width,canvas.clientHeight/canvas.height);
      canvas.getContext('2d').filter = `blur(${17/scale}px) brightness(.48) saturate(.82)`;
    }
    if (mode === 'blur-off') canvas.style.filter = 'brightness(.48) saturate(.82)';
    if (mode === 'colour-off') canvas.style.filter = 'blur(17px)';
    if (mode === 'promote') { canvas.style.willChange = 'transform, filter'; canvas.style.transform += ' translateZ(0)'; }
    if (mode === 'cap320') { canvas.height = Math.round(canvas.height * 320 / canvas.width); canvas.width = 320; }
  }, diagnostic);
  const waitFile = async (name, attempts=200) => { for(let i=0;i<attempts;i++){ try {await access(path.join(root,name));return;} catch{} await new Promise(r=>setTimeout(r,50)); } throw new Error(`Timeout waiting for ${name}`); };
  let samplerExit;
  if (process.env.CLIP_CPU_WORKER) {
    await writeFile(path.join(path.resolve(process.env.CLIP_CPU_WORKER), `${trialName}-${randomUUID()}.request.json`), JSON.stringify({AppProcessId:Number(appProcessId),TrialRoot:root}));
    samplerExit = null;
  } else {
    const sampler = spawn('C:/Windows/System32/WindowsPowerShell/v1.0/powershell.exe', ['-NoProfile','-ExecutionPolicy','Bypass','-File',path.resolve('tools/vertical_preview/cpu.ps1'),'-AppProcessId',appProcessId,'-TrialRoot',root], { windowsHide:true,stdio:'ignore' });
    samplerExit = new Promise(resolve=>sampler.on('exit',resolve));
  }
  await waitFile('cpu.ready');
  await page.getByRole('button', { name:'PLAY CLIP',exact:true }).click({ force:true });
  await new Promise(r=>setTimeout(r,5000));
  const cpuUnavailable = (await readFile(path.join(root,'cpu.ready'),'utf8')).trim()==='unavailable';
  if (!cpuUnavailable) { await writeFile(path.join(root,'start'),'start'); await waitFile('cpu.started'); }
  await page.evaluate(() => {
    const snap=v=>({time:v.currentTime,decoded:v.webkitDecodedFrameCount,frames:v.getVideoPlaybackQuality().totalVideoFrames,dropped:v.getVideoPlaybackQuality().droppedVideoFrames,paused:v.paused});
    const rows=[...document.querySelectorAll('video')].map(v=>{
      const row={v,role:v.hasAttribute('aria-label')?'foreground':'backdrop',start:snap(v),callbacks:0,seeks:0,pauses:0,id:0};
      row.seek=()=>row.seeks++; row.pause=()=>row.pauses++;
      v.addEventListener('seeking',row.seek);v.addEventListener('pause',row.pause);
      const cb=()=>{row.callbacks++;row.id=v.requestVideoFrameCallback(cb);};row.id=v.requestVideoFrameCallback(cb);return row;
    });
    window.__clipTrial={snap,rows,start:performance.now(),visibility:document.visibilityState,hidden:0};
    window.__clipTrial.visibilityListener=()=>{if(document.hidden)window.__clipTrial.hidden++;};
    document.addEventListener('visibilitychange',window.__clipTrial.visibilityListener);
  });
  await new Promise(r=>setTimeout(r,10000));
  const result=await page.evaluate(()=>{
    const o=window.__clipTrial;document.removeEventListener('visibilitychange',o.visibilityListener);
    const videos=o.rows.map(r=>{r.v.cancelVideoFrameCallback(r.id);r.v.removeEventListener('seeking',r.seek);r.v.removeEventListener('pause',r.pause);return{role:r.role,start:r.start,end:o.snap(r.v),callbacks:r.callbacks,seeks:r.seeks,pauses:r.pauses};});
    const r={elapsed_ms:performance.now()-o.start,visibility:o.visibility,hidden:o.hidden,videos,backdrop_failed:document.body.innerText.includes("The blurred background could not be drawn"),canvas_count:document.querySelectorAll('canvas').length};delete window.__clipTrial;return r;
  });
  if (!cpuUnavailable) await writeFile(path.join(root,'stop'),'stop');
  if (samplerExit) await samplerExit;
  else await waitFile('cpu.json',400);
  result.cpu=JSON.parse((await readFile(path.join(root,'cpu.json'),'utf8')).replace(/^\uFEFF/,''));
  result.runtime=browser.version();result.label=label;result.diagnostic=diagnostic;
  result.media_valid=!result.backdrop_failed && result.visibility==='visible' && result.hidden===0 && result.videos.every(v=>v.seeks===0 && v.pauses===0 && !v.start.paused && !v.end.paused && v.end.decoded>v.start.decoded && v.end.frames>v.start.frames);
  result.valid=result.cpu.valid && result.media_valid;
  await pause.click({ force:true });
  await writeFile(path.join(root,'result.json'),JSON.stringify(result,null,2));
  console.log(JSON.stringify({label,valid:result.valid,cpu:result.cpu.logical_cores,reasons:result.cpu.reasons,videos:result.videos,canvas_count:result.canvas_count},null,2));
} finally { await browser.close(); }
