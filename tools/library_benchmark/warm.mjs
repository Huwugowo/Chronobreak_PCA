// Small production-WebView check: prepared data, local artwork, real rendering.
// Uses an isolated WebView profile and never mutates recordings or starts capture.
import { createRequire } from 'node:module';
import { spawn } from 'node:child_process';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
const [modulePath, executable, artifact, profile] = process.argv.slice(2);
const libraryOnly = process.argv.includes('--library-only');
if (!profile) throw new Error('Expected Playwright module, executable, new artifact directory, and WebView profile');
const { chromium } = createRequire(import.meta.url)(modulePath);
const root = process.cwd(), preprod = path.join(root, 'build/preprod');
const config = await readFile(path.join(preprod, 'config.toml'), 'utf8');
if (!/^auto_delete_days\s*=\s*0\s*$/m.test(config)) throw new Error('Preprod retention must be disabled');
await mkdir(artifact, { recursive: false });
const endpoint = 'http://127.0.0.1:9225';
if (await fetch(endpoint+'/json/version').then(()=>true,()=>false)) throw new Error('Diagnostic port already in use');
const started = Date.now();
const child = spawn(path.resolve(executable), [], { cwd: path.dirname(path.resolve(executable)), windowsHide: true,
  env: { ...process.env, LEAGUE_REPLAY_CONFIG: path.join(preprod,'config.toml'),
    LEAGUE_REPLAY_OUTPUT_PATH: path.join(preprod,'library'), LEAGUE_REPLAY_APP_DATA_DIR: path.join(preprod,'app-data'),
    CHRONOBREAK_DISABLE_RECORDER_SUPERVISION: '1', WEBVIEW2_USER_DATA_FOLDER: path.resolve(profile),
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: '--remote-debugging-port=9225' } });
let errors = ''; child.stderr.on('data', data => { errors += data.toString(); });
let browser, page;
const replays=[];
try {
  const deadline = Date.now()+20000;
  while (!await fetch(endpoint+'/json/version').then(()=>true,()=>false)) {
    if (Date.now()>deadline || child.exitCode !== null) throw new Error('WebView did not start');
    await new Promise(resolve=>setTimeout(resolve,10));
  }
  browser = await chromium.connectOverCDP(endpoint);
  while (!(page=browser.contexts()[0]?.pages().find(p=>p.url().startsWith('http://tauri.localhost')))) {
    if (Date.now()>deadline) throw new Error('Production page missing');
    await new Promise(resolve=>setTimeout(resolve,10));
  }
  await page.evaluate(() => {
    const epoch=()=>performance.timeOrigin+performance.now();
    const probe={attached:epoch(),cards:null,images:null,replay:null,click:null};
    window.__warmDisplay=probe;
    let paintingLibrary=false, paintingReplay=false;
    const readyImages=images=>images.length && images.every(img=>img.complete&&(img.naturalWidth>0||img.hidden));
    const painted=async images=>{
      await Promise.all(images.filter(img=>!img.hidden).map(img=>img.decode()));
      // The second callback follows a frame in which decoded images could paint.
      await new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve)));
    };
    const frame=()=>{
      const cards=[...document.querySelectorAll('section[aria-label="Recorded matches"] article')];
      const list=document.querySelector('section[aria-label="Recorded matches"]');
      const settled=cards.length===8 && list?.getAttribute('aria-busy')==='false'
        && !document.body.textContent.includes('Loading match data');
      if(settled && !probe.cards)requestAnimationFrame(()=>{probe.cards??=epoch();});
      const images=cards.flatMap(card=>[...card.querySelectorAll('img')]);
      if(settled && readyImages(images) && !probe.images && !paintingLibrary) {
        paintingLibrary=true;
        void painted(images).then(()=>{
          probe.images=epoch();probe.imageCount=images.filter(img=>img.naturalWidth>0).length;
          probe.placeholderCount=images.filter(img=>img.hidden).length;
        }).catch(error=>{probe.error=String(error);});
      }
      const viewer=document.querySelector('[data-testid="viewer-surface"]');
      const details=viewer?.getAttribute('data-details-status');
      const replayImages=[...(viewer?.querySelectorAll('img')??[])]
        .filter(img=>img.getClientRects().length && getComputedStyle(img).visibility!=='hidden');
      if(probe.click && !probe.replay && !paintingReplay && details==='ready'
        && document.querySelector('[data-testid="replay-timeline"]')
        && (!replayImages.length||readyImages(replayImages))) {
        paintingReplay=true;
        void painted(replayImages).then(()=>{probe.replay=epoch();})
          .catch(error=>{probe.error=String(error);});
      }
      if(!viewer)paintingReplay=false;
      requestAnimationFrame(frame);
    };requestAnimationFrame(()=>requestAnimationFrame(frame));
  });
  await page.waitForFunction(()=>window.__warmDisplay.images!==null,{},{timeout:20000});
  const library = await page.evaluate(()=>structuredClone(window.__warmDisplay));
  if(!libraryOnly)for(let index=0;index<8;index++) {
    await page.waitForFunction(index=>{
      const list=document.querySelector('section[aria-label="Recorded matches"]');
      const button=document.querySelectorAll('article button[aria-label^="Open "]')[index];
      return list?.getAttribute('aria-busy')==='false' && button && !button.disabled;
    },index);
    // Click in the page's event loop so the timestamp includes its real handler.
    const label=await page.evaluate(index=>{
      const button=document.querySelectorAll('article button[aria-label^="Open "]')[index];
      if(!button)throw new Error('Recording button missing');
      window.__warmDisplay.replay=null;
      window.__warmDisplay.click=performance.timeOrigin+performance.now();button.click();
      return button.getAttribute('aria-label');
    },index);
    await page.waitForFunction(()=>window.__warmDisplay.replay!==null,{},{timeout:15000});
    replays.push(await page.evaluate(label=>({label,painted_details_and_artwork_ms:
      window.__warmDisplay.replay-window.__warmDisplay.click}),label));
    console.log(JSON.stringify(replays.at(-1)));
    await page.getByRole('button',{name:'Back to Match History',exact:true}).click();
    await page.waitForFunction(()=>document.querySelectorAll('section[aria-label="Recorded matches"] article').length===8);
  }
  const result=await page.evaluate(()=>structuredClone(window.__warmDisplay));
  const report={process_started_epoch_ms:started,observer_attached_ms:result.attached-started,
    cards_painted_ms:library.cards-started,cards_and_images_painted_ms:library.images-started,
    card_count:8,image_count:library.imageCount,unavailable_artwork_placeholders:library.placeholderCount,
    replays,
    runtime:browser.version(),exe:path.resolve(executable)};
  await writeFile(path.join(artifact,'summary.json'),JSON.stringify(report,null,2));
  console.log(JSON.stringify(report));
  if(process.argv.includes('--assert-budget') && (report.cards_and_images_painted_ms>=1000
      || replays.some(replay=>replay.painted_details_and_artwork_ms>=1000)))
    throw new Error('Cached painted display exceeded the one-second budget');
} catch(error) {
  if(page)await writeFile(path.join(artifact,'failure.json'),JSON.stringify({started,replays,...await page.evaluate(()=>({
    probe:window.__warmDisplay,text:document.body.innerText,
    images:[...document.images].map(img=>({src:img.src,complete:img.complete,width:img.naturalWidth,hidden:img.hidden}))
  }))},null,2));
  throw error;
} finally {
  if(browser)await browser.close();
  child.kill();
  await writeFile(path.join(artifact,'stderr.log'),errors);
}
