// Isolated browser QA: renders real page code/styles with fixtures, never user data or cloud APIs.
// npm run qa:ui -- requires Playwright installed locally or QUICKTRANSLATE_PLAYWRIGHT_PATH.
import { build } from "esbuild";
import { createServer } from "node:http";
import { createRequire } from "node:module";
import { mkdir } from "node:fs/promises";
import { resolve } from "node:path";
import assert from "node:assert/strict";
const require = createRequire(import.meta.url);
const { chromium } = require(process.env.QUICKTRANSLATE_PLAYWRIGHT_PATH || "playwright");
const pages = new Map();
for (const page of ["popup", "settings", "history", "vocabulary"]) {
  const result = await build({ stdin: { contents: `import "./src/styles/global.css"; import {defaultSpeech} from "./src/speech/reader"; import {mount${page[0].toUpperCase()+page.slice(1)}} from "./src/${page}/${page}";
    const handlers=new Map(); window.qa={calls:[],emit(name,payload){handlers.get(name)?.forEach(fn=>fn({payload}));},async listen(name,fn){if(!handlers.has(name))handlers.set(name,new Set());handlers.get(name).add(fn);return()=>handlers.get(name)?.delete(fn);},async invoke(command,args={}){
      this.calls.push({command,args});
      const speech={...defaultSpeech,provider:"offline"};
      const settings={appearance:{theme:"dark",popupOpacity:96},speech,provider:"阿里云百炼",baseUrl:"https://dashscope.aliyuncs.com/compatible-mode/v1",model:"qwen-turbo",globalShortcut:"Alt+Q",ocrShortcut:"Alt+W",ocrEngine:"cloud",ocrLanguage:"auto",cloudOcrBaseUrl:"https://dashscope.aliyuncs.com/compatible-mode/v1",cloudOcrModel:"qwen3.5-ocr",apiKeyConfigured:false,cloudOcrApiKeyConfigured:false,cloudSpeechApiKeyConfigured:false,paddleOcrInstalled:false,autoStartEnabled:false};
      if(command==="get_settings"||command==="save_settings")return {...settings,...args.update};
      if(command==="get_speech_preferences")return speech;
      if(command==="list_speech_voices")return [];
      if(command==="get_speech_plugin_status")return {installed:false,phase:"idle",downloaded:0,downloadBytes:171837079,version:"fixture",message:""};
      if(command==="get_paddle_ocr_plugin_status")return {installed:false,version:"fixture",installedBytes:0,downloadBytes:31200000};
      if(command==="get_update_state")return {phase:"idle",version:"",downloaded:0,message:"",releaseNotes:""};
      if(command==="get_popup_pinned")return false;
      if(command==="list_translation_history")return [
        {id:1,sourceText:"Stay curious. Keep learning.",translation:"保持好奇，持续学习。",sourceLanguage:"en",targetLanguage:"zh",provider:"阿里云百炼",model:"qwen-turbo",createdAt:1788200000,favorite:true},
        {id:2,sourceText:"A small step every day makes a difference.",translation:"每天迈出一小步，就会有所不同。",sourceLanguage:"en",targetLanguage:"zh",provider:"阿里云百炼",model:"qwen-turbo",createdAt:1788100000,favorite:false}];
      const entry={id:1,word:"curious",card:{word:"curious",lemma:"curious",ipaUk:"/ˈkjʊəriəs/",ipaUs:"/ˈkjʊriəs/",partOfSpeech:"adj.",contextMeaning:"好奇的；求知欲强的",definitions:["想了解或学习新事物的", "不寻常的；奇特的"],examples:[{english:"She is curious about how things work.",chinese:"她对事物如何运作充满好奇。"},{english:"Stay curious and keep exploring.",chinese:"保持好奇，继续探索。"}],collocations:["curious about", "a curious mind"],quizzes:[]},sources:[{sentence:"Stay curious. Keep learning.",translation:"保持好奇，持续学习。"}],status:"review",reviews:1,nextDueAt:1,studiedAt:1,createdAt:1,updatedAt:1,revision:1,contentRevision:1,generationState:"ready",generationError:null,model:"qwen-turbo",provider:"阿里云百炼",generatedAt:1,userEdited:false,activeQuiz:null,quizAttempts:0};
      if(command==="list_vocabulary")return {entries:[entry],total:12,due:3,tests:1,mastered:8,now:Date.now()/1000,rules:{intervalHours:4,dailyLimit:20}};
      if(command==="get_vocabulary_entry")return entry;
      if(command==="synthesize_speech")throw new Error("隔离测试：不请求真实语音服务");
      return undefined;
    }};
    document.body.className="${page}-window";document.documentElement.dataset.theme="dark";
    mount${page[0].toUpperCase()+page.slice(1)}();
    if("${page}"==="popup")setTimeout(()=>window.qa.emit("translation-state",{requestId:1,status:"success",sourceKind:"selection",result:{sourceText:"Stay curious. Keep learning.",translation:"保持好奇，持续学习。",detectedLanguage:"english",targetLanguage:"chinese",provider:"阿里云百炼",model:"qwen-turbo",cached:true}}),0);
  `, resolveDir: process.cwd(), loader: "ts" }, bundle: true, write: false, outdir: "ui-qa", format: "esm", platform: "browser", plugins: [{ name: "tauri-fixture", setup(builder) {
    builder.onResolve({filter:/^@tauri-apps\/api\//}, args=>({path:args.path,namespace:"qa"}));
    builder.onLoad({filter:/.*/,namespace:"qa"},()=>({contents:`export class Channel {onmessage=()=>{}};export const invoke=(...a)=>window.qa.invoke(...a);export const listen=(...a)=>Promise.resolve().then(()=>window.qa.listen(...a));export const emit=(...a)=>window.qa.emit(...a);export const getCurrentWindow=()=>({onFocusChanged:async()=>()=>{}});`}));
  }}] });
  pages.set(page, { js: result.outputFiles.find(file=>file.path.endsWith(".js")).text, css:result.outputFiles.find(file=>file.path.endsWith(".css")).text });
}
const server=createServer((request,response)=>{
  const [page, asset]=request.url.slice(1).split(".");const data=pages.get(page);
  if(!data){response.writeHead(404);response.end();return;}
  response.setHeader("Content-Type",asset==="js"?"text/javascript":asset==="css"?"text/css":"text/html; charset=utf-8");
  response.end(asset?data[asset]:`<!DOCTYPE html><html><head><meta charset="utf-8"><link rel="stylesheet" href="/${page}.css"></head><body><div id="app"></div><script type="module" src="/${page}.js"></script></body></html>`);
});
await new Promise(resolve=>server.listen(0,"127.0.0.1",resolve));
const output=resolve("artifacts/ui-qa");await mkdir(output,{recursive:true});
const browser=await chromium.launch({channel:"msedge",headless:true});
try {
  for(const [kind,width,height] of [["popup",520,380],["popup",360,180],["settings",820,680],["settings",460,560],["history",860,640],["history",560,460],["vocabulary",860,640],["vocabulary",620,500]]) {
    const page=await browser.newPage({viewport:{width,height}}), errors=[];
    page.on("pageerror",error=>errors.push(error.message));
    await page.goto(`http://127.0.0.1:${server.address().port}/${kind}`);
    await page.waitForSelector(kind==="popup"?".translation":kind==="settings"?'[data-settings-tab="general"]':kind==="history"?"#history-read-source":"#speak-word");
    if(kind==="settings")await page.locator('[data-settings-tab="general"]').click();
    for(const theme of ["dark","light"]) {
      await page.evaluate(theme=>document.documentElement.dataset.theme=theme,theme);
      assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),true,`${kind} ${width} horizontal overflow`);
      assert.equal(await page.evaluate(()=>document.documentElement.scrollHeight<=innerHeight),true,`${kind} ${height} vertical overflow`);
      const corners=await page.locator(`.${kind==="vocabulary"?"book":kind}-shell`).evaluate(node=>{
        const style=getComputedStyle(node);return [style.borderTopLeftRadius,style.borderBottomLeftRadius,style.borderBottomRightRadius];
      });assert.deepEqual(corners,["14px","14px","14px"]);
      await page.screenshot({path:resolve(output,`${kind}-${width}-${theme}.png`)});
    }
    if(kind==="popup") {
      const shell=await page.locator(".popup-shell").boundingBox(); assert.deepEqual([shell.x,shell.y,shell.width,shell.height],[0,0,width,height],"popup has no outer inset/frame");
      assert.equal(await page.locator(".popup-shell").evaluate(node=>getComputedStyle(node).boxShadow),"none");
      await page.locator("#more").click();
      const bounds=await page.locator("#popup-menu").boundingBox();assert.ok(bounds.x>=0&&bounds.y>=0&&bounds.x+bounds.width<=width&&bounds.y+bounds.height<=height);
      await page.screenshot({path:resolve(output,`popup-${width}-menu.png`)});
      await page.keyboard.press("Escape");assert.equal(await page.locator("#popup-menu").isHidden(),true);
      await page.evaluate(()=>window.qa.emit("translation-state",{requestId:2,status:"success",sourceKind:"selection",result:{sourceText:"pneumonoultramicroscopicsilicovolcanoconiosis",translation:"矽肺病",detectedLanguage:"english",targetLanguage:"chinese",provider:"fixture",model:"test",cached:false}}));
      assert.equal(await page.locator(".actions").evaluate(node=>node.scrollWidth<=node.clientWidth),true,"contextual collect footer overflow");
    }
    if(kind==="settings") {
      await page.locator('[name="appearanceTheme"]').selectOption("dark");
      await page.locator('[name="popupTransparency"]').fill("30");await page.locator('[name="popupTransparency"]').dispatchEvent("input");
      assert.equal(await page.locator(".appearance-sample").evaluate(node=>getComputedStyle(node).opacity),"1");
      assert.match(await page.locator(".appearance-sample").evaluate(node=>getComputedStyle(node).backgroundColor),/0\.7/);
      for(const tab of ["translation","ocr","speech","learning","maintenance"]) { await page.locator(`[data-settings-tab="${tab}"]`).click(); assert.equal(await page.locator(`#settings-${tab}`).isVisible(),true); }
    }
    assert.deepEqual(errors,[],`${kind} uncaught errors`);console.log(`PASS ${kind} ${width}×${height} light/dark, bounds and rounded corners`);await page.close();
  }
  console.log(`Screenshots: ${output}`);
} finally {await browser.close();await new Promise(resolve=>server.close(resolve));}
