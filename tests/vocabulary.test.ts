import {test} from "node:test";
import assert from "node:assert/strict";
import {build} from "esbuild";
import {JSDOM} from "jsdom";
import type {VocabularyEntry} from "../src/types";
const settle=async()=>{for(let i=0;i<8;i++)await new Promise<void>(r=>setImmediate(r));};
const entry=(overrides:Partial<VocabularyEntry>={}):VocabularyEntry=>({id:1,word:"architecture",card:{word:"architecture",lemma:"architecture",ipaUk:"",ipaUs:"",partOfSpeech:"n.",definitions:["建筑学"],contextMeaning:"建筑学",examples:[{english:"She studies architecture.",chinese:"她学习建筑学。"}],collocations:[],quizzes:[]},sources:[],status:"new",reviews:0,nextDueAt:null,studiedAt:null,createdAt:1,updatedAt:1,revision:1,contentRevision:1,generationState:"ready",generationError:null,model:"fixture",provider:"test",generatedAt:1,userEdited:false,activeQuiz:null,quizAttempts:0,...overrides});
async function mount(kind:"collect"|"vocabulary", initial=entry()) {
  const dom=new JSDOM('<div id="app"></div>',{url:"http://localhost"});
  const globals=globalThis as unknown as Record<string,unknown>;
  globals.AudioContext=class {async resume(){}};
  for(const name of ["window","document","HTMLElement","HTMLInputElement","HTMLSelectElement","HTMLFormElement","HTMLTextAreaElement","HTMLDialogElement","FormData"]){globals[name]=(dom.window as unknown as Record<string,unknown>)[name];}
  dom.window.HTMLDialogElement.prototype.showModal=function(){this.open=true;};dom.window.HTMLDialogElement.prototype.close=function(){this.open=false;};
  dom.window.HTMLElement.prototype.scrollIntoView=()=>{};
  const calls:Array<{command:string;args:Record<string,unknown>}>=[];
  let value=initial;const handlers=new Map<string,(e:{payload:unknown})=>void>();
  const qa={async listen(name:string,fn:(e:{payload:unknown})=>void){handlers.set(name,fn);return()=>handlers.delete(name);},async invoke(command:string,args:Record<string,unknown>={}) {
    calls.push({command,args});
    if(command==="get_speech_preferences")return {provider:"offline",rate:100,chineseVoice:"",englishVoice:"",bilingual:false,threads:4};
    if(command==="synthesize_speech")throw new Error("fixture voice unavailable");
    if(command==="list_vocabulary")return {entries:[value],total:1,due:1,tests:1,mastered:value.status==="mastered"?1:0,now:Math.floor(Date.now()/1000),rules:{intervalHours:4,dailyLimit:20}};
    if(command==="get_vocabulary_entry")return value;
    if(command==="collect_vocabulary")return entry({word:String(args.word),card:null,generationState:"pending"});
    if(command==="generate_vocabulary")throw new Error("fixture network unavailable");
    if(command==="review_vocabulary"){value={...value,status:"review",reviews:args.rating==="study"?0:value.reviews+1,revision:value.revision+1,nextDueAt:Math.floor(Date.now()/1000)+14400};return value;}
    if(command==="begin_vocabulary_quiz")return {token:"fixture-quiz",entryId:1,meaningPrompt:"What does architecture mean?",options:["建筑学","食物","天气","交通"],cloze:"She studies ____.",hint:"建筑学"};
    if(command==="submit_vocabulary_quiz"){value={...value,status:"mastered",revision:value.revision+1};return {passed:true,meaningCorrect:true,spellingCorrect:true,entry:value};}
    if(command==="export_vocabulary")return JSON.stringify({schemaVersion:1,entries:[value]});
    if(command==="import_vocabulary")return 1;
    return undefined;
  }};globals.__vocabQa=qa;
  const result=await build({entryPoints:[`src/vocabulary/${kind}.ts`],bundle:true,write:false,format:"esm",platform:"browser",loader:{".css":"empty"},plugins:[{name:"stub",setup(b){b.onResolve({filter:/^@tauri-apps\/api\//},a=>({path:a.path,namespace:"qa"}));b.onLoad({filter:/.*/,namespace:"qa"},()=>({contents:`export class Channel {onmessage=()=>{}};export const invoke=(...a)=>globalThis.__vocabQa.invoke(...a);export const listen=(...a)=>globalThis.__vocabQa.listen(...a);export const getCurrentWindow=()=>({onFocusChanged:async()=>()=>{}});`}));}}]});
  const module=await import(`data:text/javascript;base64,${Buffer.from(result.outputFiles[0].text+`\n//${Math.random()}`).toString("base64")}`);
  const root=dom.window.document.querySelector<HTMLElement>("#app")!;
  const collector=kind==="collect"?module.mountCollector(root,()=>({source:"Architecture makes a curious student resilient. Architecture matters.",translation:"建筑学很重要。"}),true):undefined;
  if(kind==="vocabulary")module.mountVocabulary();await settle();
  const el=<T extends HTMLElement>(s:string)=>root.querySelector<T>(s)!;
  return {dom,root,module,collector,qa,calls,el,handlers,close:()=>dom.window.close()};
}
test("candidate words deduplicate, ignore acronyms and select the actual source sentence",async()=>{
  const m=await mount("collect");try{assert.deepEqual(m.module.englishCandidates("API Curious curious one's well-known"),["curious","one's","well-known"]);assert.equal(m.module.sentenceFor("First sentence. She is curious. Last.","curious"),"She is curious.");}finally{m.close();}
});
test("collection is explicit, retains context, and rejected generation does not erase the draft",async()=>{
  const m=await mount("collect");try{await m.collector.open("architecture");assert.equal(m.calls.some(c=>c.command==="collect_vocabulary"||c.command==="generate_vocabulary"),false);m.el("#confirm-collect").click();await settle();assert.equal(m.calls.find(c=>c.command==="collect_vocabulary")?.args.sentence,"Architecture makes a curious student resilient.");assert.ok(m.calls.some(c=>c.command==="generate_vocabulary"));assert.equal(m.calls.some(c=>c.command==="delete_vocabulary"),false);assert.match(m.el("#collect-status").textContent||"",/已收藏/);m.collector.dialog.dispatchEvent(new m.dom.window.Event("cancel",{cancelable:true}));assert.equal(m.collector.dialog.open,false);await settle();assert.equal(m.calls.at(-1)?.args.open,false);}finally{m.close();}
});
test("batch selection supports local-only collection and per-word sentences",async()=>{
  const m=await mount("collect");try{await m.collector.open("",true);m.el<HTMLInputElement>("#generate-on-collect").checked=false;for(const c of Array.from(m.root.querySelectorAll<HTMLInputElement>('#word-candidates input')).filter(c=>["architecture","curious"].includes(c.value)))c.checked=true;m.el("#confirm-collect").click();await settle();assert.equal(m.calls.filter(c=>c.command==="collect_vocabulary").length,2);assert.equal(m.calls.some(c=>c.command==="generate_vocabulary"),false);}finally{m.close();}
});
test("review requires revealing answers and early reviews stay disabled",async()=>{
  const m=await mount("vocabulary",entry({status:"review",reviews:1,nextDueAt:1}));try{m.el("#start-review").click();assert.doesNotMatch(m.el("#book-detail").textContent||"",/建筑学/);assert.equal(m.el("#rate-remember"),null);m.el("#reveal-card").click();assert.match(m.el("#book-detail").textContent||"",/建筑学/);m.el("#rate-remember").click();await settle();assert.equal(m.calls.find(c=>c.command==="review_vocabulary")?.args.rating,"remember");assert.equal(m.el<HTMLButtonElement>("#start-review").disabled,true);}finally{m.close();}
});
test("quiz hides recognition before spelling, sends both answers, and archives on pass",async()=>{
  const m=await mount("vocabulary",entry({status:"test",reviews:3,nextDueAt:1}));try{m.el("#start-quiz").click();await settle();assert.match(m.el("#book-detail").textContent||"",/architecture/);m.el("#quiz-option-0").click();assert.doesNotMatch(m.el("#book-detail").textContent||"",/architecture/);assert.equal(m.el("#quiz-option-0"),null);m.el<HTMLInputElement>("#quiz-spelling").value="architecture";m.el<HTMLFormElement>("#book-detail form").dispatchEvent(new m.dom.window.Event("submit",{cancelable:true}));await settle();assert.deepEqual(m.calls.find(c=>c.command==="submit_vocabulary_quiz")?.args,{token:"fixture-quiz",choice:0,spelling:"architecture"});assert.match(m.el("#book-status").textContent||"",/已归档/);assert.equal(m.root.dataset.quiz,"false");}finally{m.close();}
});
test("untrusted card text stays text, CSV escapes formulas, and import needs confirmation",async()=>{
  const hostile=entry();hostile.card!.contextMeaning='<img src=x onerror=alert(1)>';hostile.card!.definitions=["=HYPERLINK(foo)"];
  const m=await mount("vocabulary",hostile);try{assert.equal(m.root.querySelector("img"),null);assert.match(m.module.vocabularyCsv([hostile]),/'=HYPERLINK/);m.el("#book-export").click();await settle();m.el("#book-import").click();await settle();assert.equal(m.el<HTMLDialogElement>("#book-confirm").open,true);assert.equal(m.calls.some(c=>c.command==="import_vocabulary"),false);m.el("#confirm-yes").click();await settle();assert.equal(m.calls.find(c=>c.command==="import_vocabulary")?.args.confirmed,true);}finally{m.close();}
});
test("card editor provides ordinary fields and submits cached quizzes without hidden model calls",async()=>{
  const m=await mount("vocabulary");try{m.el("#edit-card").click();assert.equal(m.el<HTMLInputElement>("#edit-lemma").value,"architecture");m.el<HTMLTextAreaElement>("#edit-definitions").value="建筑学\n建筑风格";m.el<HTMLFormElement>(".card-editor").dispatchEvent(new m.dom.window.Event("submit",{cancelable:true}));await settle();const call=m.calls.find(c=>c.command==="save_vocabulary_card");assert.deepEqual((call?.args.card as {definitions:string[]}).definitions,["建筑学","建筑风格"]);assert.equal(m.calls.some(c=>c.command==="generate_vocabulary"),false);}finally{m.close();}
});
test("pending draft deletion is available but never sent before confirmation",async()=>{
  const m=await mount("vocabulary",entry({card:null,generationState:"pending"}));try{m.el("#delete-pending").click();await settle();assert.equal(m.calls.some(c=>c.command==="delete_vocabulary"),false);m.el("#confirm-no").click();await settle();assert.equal(m.calls.some(c=>c.command==="delete_vocabulary"),false);}finally{m.close();}
});

test("word and both example languages have independent icon reads; tools remain collapsed",async()=>{
  const m=await mount("vocabulary");try{
    assert.equal(m.el<HTMLDetailsElement>(".book-more").open,false);
    assert.equal(m.el("#book-pause"),null);
    for(const label of ["朗读单词","朗读英文例句","朗读中文例句"]){m.el<HTMLButtonElement>(`[aria-label="${label}"]`).click();await settle();}
    assert.deepEqual(m.calls.filter(c=>c.command==="synthesize_speech").map(c=>c.args.text),["architecture","She studies architecture.","她学习建筑学。"]);
    assert.equal(m.calls.some(c=>c.command==="review_vocabulary"||c.command==="generate_vocabulary"),false);
    assert.equal(m.el<HTMLButtonElement>("#speak-word").getAttribute("aria-pressed"),"false");
  }finally{m.close();}
});
