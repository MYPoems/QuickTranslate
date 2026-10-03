import {invoke} from "@tauri-apps/api/core";
import {listen} from "@tauri-apps/api/event";
import {getCurrentWindow} from "@tauri-apps/api/window";
import type {VocabularyEntry,VocabularyView,VocabularyQuizView,VocabularyQuizResult,WordCard} from "../types";
import {SpeechControls} from "../speech/controls";
import {mountCollector} from "./collect";
import "./vocabulary.css";

const root=document.querySelector<HTMLElement>("#app")!;
let current:VocabularyEntry|undefined;let view:VocabularyView;let mode:"browse"|"review"|"quiz"="browse";
let revealed=false;let activeQuiz:VocabularyQuizView|undefined;let choice=-1;let offset=0;let busy=false;let loadRevision=0;let dueTimer:number|undefined;
const el=<T extends HTMLElement>(selector:string)=>root.querySelector<T>(selector)!;
const errorText=(error:unknown)=>error instanceof Error?error.message:String((error as {message?:string})?.message||error);
const speech=new SpeechControls("vocabulary",(message,error)=>{const node=el("#book-speaking");if(node){node.textContent=message;node.hidden=!message;node.dataset.error=String(!!error);}});
const text=(tag:string,value:string,className="")=>{const node=document.createElement(tag);node.textContent=value;if(className)node.className=className;return node;};
const button=(label:string,action:()=>void,id="")=>{const node=document.createElement("button");node.type="button";node.textContent=label;if(id)node.id=id;node.addEventListener("click",action);return node;};
const dueText=(e:VocabularyEntry)=>e.nextDueAt?`下次：${new Date(e.nextDueAt*1000).toLocaleString()}`:"";
function status(message:string,error=false){el("#book-status").textContent=message;el("#book-status").dataset.error=String(error);}
function stopSpeech(){speech.stop();const node=el("#book-speaking");if(node){node.textContent="";node.hidden=true;}}
async function run(action:()=>Promise<void>){if(busy)return;busy=true;root.dataset.busy="true";try{await action();}catch(e){status(errorText(e),true);}finally{busy=false;root.dataset.busy="false";await loadBook();}}
function confirmAction(message:string):Promise<boolean>{const dialog=el<HTMLDialogElement>("#book-confirm");el("#confirmation-text").textContent=message;dialog.showModal();return new Promise(resolve=>{const finish=(value:boolean)=>{dialog.close();el("#confirm-yes").onclick=null;el("#confirm-no").onclick=null;dialog.oncancel=null;resolve(value);};el("#confirm-yes").onclick=()=>finish(true);el("#confirm-no").onclick=()=>finish(false);dialog.oncancel=()=>finish(false);});}
async function abandon(){stopSpeech();if(activeQuiz){const token=activeQuiz.token;activeQuiz=undefined;await invoke("abandon_vocabulary_quiz",{token}).catch(()=>{});}mode="browse";root.dataset.quiz="false";revealed=false;renderDetail();}

export function mountVocabulary(){
  root.innerHTML=`<main class="book-shell"><header class="book-header app-header"><div><p class="eyebrow">QUICKTRANSLATE</p><h1>生词本</h1></div><button id="book-add">＋ 添加生词</button></header>
    <nav class="book-tabs" aria-label="生词筛选"><button data-filter="learning" aria-pressed="true">待学习</button><button data-filter="due">到期复习</button><button data-filter="test">待测试</button><button data-filter="mastered">已掌握</button></nav>
    <p id="book-counts"></p><p id="book-status" role="status"></p>
    <section class="book-workspace"><aside><input id="book-search" type="search" placeholder="搜索英文单词" maxlength="100" aria-label="搜索生词" /><div id="book-list"></div><div class="book-pages"><button id="book-prev">上一页</button><span id="book-page"></span><button id="book-next">下一页</button></div></aside><article id="book-detail" aria-label="词卡"></article></section>
    <p id="book-speaking" role="status" hidden></p>
    <details class="book-maintenance"><summary>复习设置与本地备份</summary><p>首次学习不计次；三次有效复习后再等待同样间隔进行测试。提前查看不计次。掌握后归档，不自动永久删除。</p>
      <form id="book-rules"><label>最短间隔（小时）<input name="intervalHours" type="number" min="4" max="168" required /></label><label>每日复习上限（UTC 日）<input name="dailyLimit" type="number" min="1" max="100" required /></label><button>保存规则</button></form>
      <p>降低间隔不会提前解锁已有计划；延长间隔会延后尚未到期的计划。清理翻译历史不影响生词本。</p>
      <div class="book-tools"><button id="book-export">导出 JSON</button><button id="book-csv">导出阅读用 CSV</button><button id="book-copy">复制备份</button><button id="book-import">确认合并导入…</button><button id="book-save-file">保存为文件</button></div>
      <textarea id="book-backup" rows="5" spellcheck="false" placeholder="JSON 备份包括词卡、来源及复习/测试进度；导入不覆盖同名现有词条。CSV 不用于恢复进度。" aria-label="生词本备份"></textarea><label>载入 JSON 文件<input id="book-backup-file" type="file" accept=".json,application/json" /></label>
    </details><dialog id="book-confirm"><h2>请确认</h2><p id="confirmation-text"></p><div><button id="confirm-no">取消</button><button id="confirm-yes">确认</button></div></dialog></main>`;
  const sidebar=el(".book-workspace aside");sidebar.prepend(el(".book-tabs"),el("#book-counts"));
  let filter="learning";let debounce:number|undefined;
  const collector=mountCollector(root,()=>({source:"",translation:""}));
  el("#book-add").addEventListener("click",()=>void collector.open().catch(e=>status(errorText(e),true)));
  root.querySelectorAll<HTMLButtonElement>('[data-filter]').forEach(b=>b.addEventListener("click",()=>void run(async()=>{await abandon();filter=b.dataset.filter!;offset=0;current=undefined;root.querySelectorAll('[data-filter]').forEach(n=>n.setAttribute("aria-pressed",String(n===b)));await loadBook();})));
  el<HTMLInputElement>("#book-search").addEventListener("input",()=>{window.clearTimeout(debounce);debounce=window.setTimeout(()=>{offset=0;void loadBook();},180);});
  el("#book-prev").addEventListener("click",()=>{offset=Math.max(0,offset-100);void loadBook();});el("#book-next").addEventListener("click",()=>{offset+=100;void loadBook();});
  el<HTMLFormElement>("#book-rules").addEventListener("submit",e=>{e.preventDefault();void run(async()=>{const data=new FormData(el<HTMLFormElement>("#book-rules"));await invoke("set_vocabulary_rules",{rules:{intervalHours:Number(data.get("intervalHours")),dailyLimit:Number(data.get("dailyLimit"))}});status("规则已保存，不会提前解锁已有词条");});});
  el("#book-export").addEventListener("click",()=>void run(async()=>{el<HTMLTextAreaElement>("#book-backup").value=await invoke<string>("export_vocabulary");status("JSON 备份已生成，含复习进度，不含 API Key");}));
  el("#book-copy").addEventListener("click",()=>void run(async()=>{const value=el<HTMLTextAreaElement>("#book-backup").value;if(!value.trim())throw new Error("请先导出备份");await invoke("copy_translation",{text:value});status("备份已复制");}));
  el("#book-import").addEventListener("click",()=>void run(async()=>{const json=el<HTMLTextAreaElement>("#book-backup").value;const parsed=JSON.parse(json) as {schemaVersion:number;entries:unknown[]};if(parsed.schemaVersion!==1||!Array.isArray(parsed.entries))throw new Error("请选择有效的生词本 JSON 备份");if(!await confirmAction(`合并导入 ${parsed.entries.length} 条记录？同名词条不覆盖，其他词条保留原进度。`))return;const count=await invoke<number>("import_vocabulary",{json,confirmed:true});status(`已导入 ${count} 词；同名现有词条未覆盖`);}));
  el<HTMLInputElement>("#book-backup-file").addEventListener("change",()=>void run(async()=>{const file=el<HTMLInputElement>("#book-backup-file").files?.[0];if(!file)return;if(file.size>20*1024*1024)throw new Error("备份最多 20 MiB");el<HTMLTextAreaElement>("#book-backup").value=await file.text();status("文件已载入，尚未导入，请确认合并导入");}));
  el("#book-csv").addEventListener("click",()=>void run(async()=>{const backup=JSON.parse(await invoke<string>("export_vocabulary")) as {entries:VocabularyEntry[]};el<HTMLTextAreaElement>("#book-backup").value=vocabularyCsv(backup.entries);status("CSV 已生成；可保存或复制，不用于进度恢复");}));
  el("#book-save-file").addEventListener("click",()=>{const value=el<HTMLTextAreaElement>("#book-backup").value;if(!value.trim()){status("请先导出备份",true);return;}const isJson=value.trim().startsWith("{");const url=URL.createObjectURL(new Blob([value],{type:isJson?"application/json":"text/csv;charset=utf-8"}));const a=document.createElement("a");a.href=url;a.download=`QuickTranslate-vocabulary-${new Date().toISOString().slice(0,10)}.${isJson?"json":"csv"}`;a.click();window.setTimeout(()=>URL.revokeObjectURL(url),60000);});
  void listen("vocabulary-changed",()=>{if(!busy)void loadBook();});
  void listen("vocabulary-hidden",()=>{collector.close();void abandon();window.clearTimeout(dueTimer);});
  document.addEventListener("visibilitychange",()=>{if(document.hidden){void abandon();window.clearTimeout(dueTimer);}else void loadBook();});
  void getCurrentWindow().onFocusChanged(({payload})=>{if(payload)void loadBook();});
  root.dataset.filter=filter;void loadBook();
}
async function loadBook(){
  const request=++loadRevision;try{const filter=root.querySelector<HTMLButtonElement>('[data-filter][aria-pressed="true"]')?.dataset.filter||"learning";
    const next=await invoke<VocabularyView>("list_vocabulary",{query:el<HTMLInputElement>("#book-search").value,filter,offset});if(request!==loadRevision)return;view=next;
    el("#book-counts").textContent=`${view.total} 词 · ${view.due} 词到期 · ${view.tests} 词待测试 · ${view.mastered} 词已掌握`;
    el<HTMLInputElement>('[name="intervalHours"]').value=String(view.rules.intervalHours);el<HTMLInputElement>('[name="dailyLimit"]').value=String(view.rules.dailyLimit);
    const list=el("#book-list");list.replaceChildren();
    for(const e of view.entries){const row=button(e.word,()=>void run(async()=>{await abandon();current=await invoke<VocabularyEntry>("get_vocabulary_entry",{id:e.id});renderDetail();}));row.className="book-word";row.dataset.selected=String(current?.id===e.id);row.append(text("small",`${({new:"首次学习",review:"待复习",test:"待测试",mastered:"已掌握"})[e.status]} · ${e.reviews}/3`));list.append(row);}
    if(!view.entries.length)list.append(text("p","此筛选暂无词条。可从翻译悬浮窗收藏，或点击添加生词。","book-empty"));
    el<HTMLButtonElement>("#book-prev").disabled=offset===0;el<HTMLButtonElement>("#book-next").disabled=view.entries.length<100;el("#book-page").textContent=`第 ${Math.floor(offset/100)+1} 页`;
    if(!current && view.entries.length){current=view.entries[0];renderDetail();}
    else if(current && mode!=="quiz"){try{const latest=await invoke<VocabularyEntry>("get_vocabulary_entry",{id:current.id});if(request!==loadRevision)return;if(latest.revision!==current.revision){current=latest;revealed=false;mode="browse";renderDetail();}}catch{current=undefined;renderDetail();}}
    else if(!current)renderDetail();
    window.clearTimeout(dueTimer);if(!document.hidden && mode!=="quiz"){const future=view.entries.map(e=>e.nextDueAt||0).filter(v=>v>view.now);if(future.length)dueTimer=window.setTimeout(()=>{renderDetail();void loadBook();},Math.min(2147483647,(Math.min(...future)-view.now+1)*1000));}
  }catch(e){if(request===loadRevision)status(errorText(e),true);}
}
function renderDetail(){
  stopSpeech();const outer=el("#book-detail");outer.replaceChildren();const detail=text("div","","book-detail-content");outer.append(detail);const e=current;if(!e){detail.append(text("h2","你的阅读，会留下词汇。"),text("p","收藏后的词卡和复习记录仅保存在本机。模型生成需要网络；已有词卡可离线学习。","book-empty"));return;}
  const heading=text("div","","section-heading");heading.append(text("h2",e.word,"book-headword"),speech.button("朗读单词",e.word,"speak-word"));detail.append(heading,text("p",`${e.reviews}/3 次有效复习 · ${dueText(e)}`,"book-meta"));
  if(e.generationError)detail.append(text("p",e.generationError,"book-error"));
  if(!e.card){detail.append(text("p",e.generationState==="generating"?"模型正在生成词卡…": "词条已保存，尚未生成词卡。只发送本词和必要来源句，使用当前翻译模型，可能产生费用。"));const gen=button("生成词卡",()=>void generate(false,false),"generate-card");gen.disabled=e.generationState==="generating";detail.append(gen,button("删除未学习词条…",()=>void run(async()=>{if(!await confirmAction("永久删除这个未学习词条及来源句？"))return;await invoke("delete_vocabulary",{id:e.id,revision:e.revision,confirmed:true});current=undefined;status("词条已删除");}),"delete-pending"));return;}
  const card=e.card;
  if(mode==="review" && !revealed){detail.append(text("h3","先回忆中文释义，再翻面核对。"),button("查看答案",()=>{revealed=true;renderDetail();},"reveal-card"));return;}
  detail.append(text("p",`英 ${card.ipaUk||"未提供"} · 美 ${card.ipaUs||"未提供"} · ${card.partOfSpeech}`,"book-ipa"));
  if(card.lemma!==e.word)detail.append(button(`基本词形：${card.lemma} · 确认采用并去重…`,()=>void run(async()=>{if(!await confirmAction("请根据语境确认基本词形。相同基本词会合并来源并保留目标词进度；无目标词时重新生成词卡并从首次学习开始。"))return;current=await invoke("adopt_vocabulary_lemma",{id:e.id,revision:e.revision});renderDetail();}),"adopt-lemma"));
  detail.append(text("h3","当前语境"),text("p",card.contextMeaning,"book-meaning"));const definitions=text("ul","");card.definitions.forEach(d=>definitions.append(text("li",d)));detail.append(definitions);
  if(e.sources.length){detail.append(text("h3","阅读中的来源"));for(const s of e.sources){const block=text("div","","book-example");block.append(readingLine(highlight(s.sentence,e.word),"朗读来源句",s.sentence));if(s.translation)block.append(readingLine(text("p",s.translation),"朗读来源译文",s.translation));detail.append(block);}}
  detail.append(text("h3","例句"));for(const example of card.examples){const block=text("div","","book-example");block.append(readingLine(highlight(example.english,e.word),"朗读英文例句",example.english),readingLine(text("p",example.chinese),"朗读中文例句",example.chinese));detail.append(block);}
  if(card.collocations.length)detail.append(text("p",`常用搭配：${card.collocations.join(" · ")}`));
  detail.append(text("p",`${e.userEdited?"已手动编辑":"AI 生成，请核对音标、释义和题目"} · ${e.provider} ${e.model}`,"book-meta"));
  const learning=text("div","","book-learning-actions");
  if(mode==="review"&&revealed){for(const [label,rating] of [["记得 · 计一次","remember"],["模糊 · 不增加","fuzzy"],["忘记 · 退一步","forgot"]])learning.append(button(label,()=>void review(rating),`rate-${rating}`));}
  else if(e.status==="new")learning.append(button("完成首次学习（不计复习）",()=>void review("study"),"finish-study"));
  else if(e.status==="review"){const start=button("开始到期复习",()=>{mode="review";revealed=false;renderDetail();},"start-review");start.disabled=(e.nextDueAt||Infinity)>Date.now()/1000;learning.append(start);}
  else if(e.status==="test"){const start=button("开始两题测试",()=>void run(async()=>{stopSpeech();activeQuiz=await invoke("begin_vocabulary_quiz",{id:e.id,revision:e.revision});mode="quiz";root.dataset.quiz="true";renderQuiz(false);}),"start-quiz");start.disabled=(e.nextDueAt||Infinity)>Date.now()/1000;learning.append(start);}
  else learning.append(text("p","已掌握：已从待学习列表移出，可重新学习或手动删除。"));
  outer.append(learning);
  const tools=text("div","","book-secondary-tools");tools.append(button("重新学习…",()=>void run(async()=>{if(!await confirmAction("重置该词复习次数，保留词卡和来源，从首次学习开始？"))return;await reviewAction("restart");}),"restart-word"),button("编辑词卡…",()=>editCard(),"edit-card"),button("重新生成词卡…",()=>void generate(true,false),"regenerate-card"),button("修复 / 更换测试题…",()=>void generate(true,true),"repair-quizzes"),button("永久删除…",()=>void run(async()=>{if(!await confirmAction("永久删除这个词及其学习记录？掌握归档无需删除。"))return;await invoke("delete_vocabulary",{id:e.id,revision:e.revision,confirmed:true});current=undefined;stopSpeech();status("词条已删除");}),"delete-word"));const more=document.createElement("details");more.className="book-more";more.append(text("summary","更多操作"),tools);learning.append(more);
}
function readingLine(content:HTMLElement,label:string,value:string){const line=text("div","","book-reading-line");line.append(content,speech.button(label,value));return line;}
function highlight(value:string,word:string){const p=document.createElement("p");const escaped=word.replace(/[.*+?^${}()|[\]\\]/g,"\\$&");const regex=new RegExp(`(\\b${escaped}\\b)`,"gi");for(const part of value.split(regex)){p.append(part.toLowerCase()===word.toLowerCase()?text("mark",part):document.createTextNode(part));}return p;}
async function reviewAction(rating:string){if(!current)return;stopSpeech();current=await invoke("review_vocabulary",{id:current.id,revision:current.revision,token:crypto.randomUUID(),rating});mode="browse";revealed=false;renderDetail();status(rating==="study"?"首次学习完成；到期后才能计入第一次复习":"学习进度已保存");}
async function review(rating:string){await run(()=>reviewAction(rating));}
async function generate(force:boolean,quizOnly:boolean){await run(async()=>{if(!current)return;const e=current;if(force && !await confirmAction(quizOnly?"重新生成两组测试题？保留复习进度，不上传整个生词本，可能产生费用。":"重新生成词卡可能产生费用，手动编辑将被替换。已有复习次数时须先重新学习。"))return;status("正在生成，请稍候…");current=await invoke("generate_vocabulary",{id:e.id,revision:e.revision,force,quizOnly});renderDetail();status(current?.generationError||"词卡已保存",!!current?.generationError);});}
function editCard(){
  if(!current?.card)return;const e=current;const original=e.card!;stopSpeech();const detail=el("#book-detail");
  detail.replaceChildren(text("h2",`编辑 ${e.word}`),text("p","保存会重置学习进度。音标不确定可留空；更改词义请同步核对下方题库，或保存后点击更换测试题。"));
  const form=document.createElement("form");form.className="card-editor";
  const fields=new Map<string,HTMLInputElement|HTMLTextAreaElement>();
  const field=(name:string,label:string,value:string,multiline=false)=>{const wrapper=document.createElement("label");wrapper.append(document.createTextNode(label));const input=multiline?document.createElement("textarea"):document.createElement("input");input.id=`edit-${name}`;input.value=value;if(input instanceof HTMLTextAreaElement)input.rows=3;input.maxLength=4000;wrapper.append(input);form.append(wrapper);fields.set(name,input);};
  field("lemma","基本词形（不会自动更名合并）",original.lemma);field("partOfSpeech","词性",original.partOfSpeech);
  field("ipaUk","英式音标",original.ipaUk);field("ipaUs","美式音标",original.ipaUs);
  field("contextMeaning","当前语境释义",original.contextMeaning,true);field("definitions","释义（每行一条）",original.definitions.join("\n"),true);
  field("collocations","搭配（每行一条，可空）",original.collocations.join("\n"),true);
  for(let i=0;i<3;i++){field(`english-${i}`,`例句 ${i+1} · 英文`,original.examples[i]?.english||"",true);field(`chinese-${i}`,`例句 ${i+1} · 中文`,original.examples[i]?.chinese||"",true);}
  const advanced=document.createElement("details");advanced.className="card-quiz-editor";advanced.append(text("summary","高级：核对 / 编辑测试题 JSON"));const quizzes=document.createElement("textarea");quizzes.id="card-quizzes";quizzes.rows=12;quizzes.value=JSON.stringify(original.quizzes,null,2);advanced.append(quizzes);form.append(advanced);
  const controls=text("div","","book-detail-actions");const save=button("保存并重新学习",()=>{},"save-card");save.type="submit";controls.append(save,button("取消",renderDetail));form.append(controls);
  form.addEventListener("submit",event=>{event.preventDefault();void run(async()=>{const value=(key:string)=>fields.get(key)!.value.trim();const lines=(key:string)=>value(key).split("\n").map(v=>v.trim()).filter(Boolean);
    const examples=Array.from({length:3},(_,i)=>({english:value(`english-${i}`),chinese:value(`chinese-${i}`)})).filter(x=>x.english||x.chinese);
    const card:WordCard={...original,lemma:value("lemma"),partOfSpeech:value("partOfSpeech"),ipaUk:value("ipaUk"),ipaUs:value("ipaUs"),contextMeaning:value("contextMeaning"),definitions:lines("definitions"),collocations:lines("collocations"),examples,quizzes:JSON.parse(quizzes.value)};
    current=await invoke("save_vocabulary_card",{id:e.id,revision:e.revision,card});mode="browse";renderDetail();status("修改已保存，复习进度已重置");
  });});detail.append(form);
}
function renderQuiz(spelling:boolean){
  const q=activeQuiz;if(!q)return;const detail=el("#book-detail");detail.replaceChildren(text("h2",spelling?"第 2 题 · 拼写":"第 1 题 · 语境释义"));
  if(!spelling){detail.append(text("p",q.meaningPrompt));const options=text("div","","quiz-options");q.options.forEach((option,index)=>options.append(button(option,()=>{choice=index;renderQuiz(true);},`quiz-option-${index}`)));detail.append(options);}
  else {detail.append(text("p",q.cloze),text("p",q.hint,"book-meta"));const form=document.createElement("form");const input=document.createElement("input");input.id="quiz-spelling";input.autocomplete="off";input.spellcheck=false;input.maxLength=48;input.required=true;input.setAttribute("aria-label","填写英文单词");const submit=button("提交测试",()=>{});submit.type="submit";form.append(input,submit);form.addEventListener("submit",event=>{event.preventDefault();void run(async()=>{const result=await invoke<VocabularyQuizResult>("submit_vocabulary_quiz",{token:q.token,choice,spelling:input.value});activeQuiz=undefined;mode="browse";root.dataset.quiz="false";current=result.entry;renderDetail();status(result.passed?"两题通过！已归档至已掌握。":`未通过（释义${result.meaningCorrect?"正确":"错误"}，拼写${result.spellingCorrect?"正确":"错误"}）。退回 2/3，间隔后复习再测试。`);});});detail.append(form);input.focus();}
  detail.append(button("退出测试（不计进度）",()=>void run(async()=>{await abandon();renderDetail();}),"quit-quiz"),button("题目有问题（不计失败）",()=>void run(async()=>{await abandon();current=await invoke("get_vocabulary_entry",{id:q.entryId});renderDetail();status("本次测试已取消，次数不变。可点击修复 / 更换测试题，核对后再测试。");}),"report-quiz"));
}
export function vocabularyCsv(entries:VocabularyEntry[]):string {
  const cell=(v:string)=>`"${(/^[=+\-@\t\r]/.test(v)?"'"+v:v).replace(/"/g,'""')}"`;
  return "\uFEFF"+[["word","lemma","ipa_uk","ipa_us","definitions","context_meaning","status","reviews"],...entries.map(e=>[e.word,e.card?.lemma||"",e.card?.ipaUk||"",e.card?.ipaUs||"",e.card?.definitions.join("；")||"",e.card?.contextMeaning||"",e.status,String(e.reviews)])].map(row=>row.map(cell).join(",")).join("\r\n");
}
