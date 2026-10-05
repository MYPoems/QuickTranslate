import { invoke } from "@tauri-apps/api/core";
import type { VocabularyEntry } from "../types";
import { icon } from "../ui/icons";
import "./collect.css";

export function englishCandidates(text:string):string[] {
  return [...new Set((text.replace(/’/g,"'").match(/[A-Za-z]+(?:['-][A-Za-z]+)*/g)||[])
    .filter(word=>word.length<=48 && !/^[A-Z]{2,}$/.test(word)).map(word=>word.toLowerCase()))].slice(0,80);
}
export function sentenceFor(text:string,word:string):string {
  const escaped=word.replace(/[.*+?^${}()|[\]\\]/g,"\\$&");
  const index=text.search(new RegExp(`\\b${escaped}\\b`,"i"));
  if(index<0)return "";
  const left=text.slice(0,index).search(/[^.!?。！？\n]*$/);const right=text.slice(index).search(/[.!?。！？\n]/);
  const end=right<0?text.length:index+right+1;
  return text.slice(Math.max(left,index-500),Math.min(end,index+700)).trim();
}
const errorText=(error:unknown)=>error instanceof Error?error.message:String((error as {message?:string})?.message||error);
export function mountCollector(root:HTMLElement,source:()=>{source:string;translation:string},popup=false) {
  const dialog=document.createElement("dialog");dialog.id="collect-dialog";dialog.className="collect-dialog";
  dialog.setAttribute("aria-labelledby","collect-title");
  if(popup)dialog.dataset.host="popup";
  dialog.innerHTML=`<form method="dialog"><header class="collect-header"><h2 id="collect-title">${icon("book")}加入生词本</h2><button type="button" id="cancel-collect" aria-label="关闭收藏" title="关闭">${icon("close")}</button></header>
    <div class="collect-body">
      <p class="collect-note">从原文选取生词，或手动添加英文单词。</p>
      <label>手动添加<input id="collect-word" maxlength="48" placeholder="例如 architecture" autocomplete="off" /></label>
      <section id="collect-candidates-section"><p class="section-label">从原文选词</p><div id="word-candidates" class="word-candidates" aria-label="句子中的候选生词"></div></section>
      <details id="collect-source-options"><summary>来源句与收藏说明</summary><div class="collect-options">
        <label>来源句（单词上下文）<textarea id="collect-context" maxlength="1200" rows="2"></textarea></label>
        <p class="collect-note">单个单词使用此来源句；批量收藏自动为每个单词提取原句。仅单词和对应上下文会发送给模型。缩写可手动填写，相同词形去重；基本词形需在词卡中确认。</p>
      </div></details>
      <div class="collect-generation"><label class="collect-check"><input id="generate-on-collect" type="checkbox" checked aria-describedby="collect-generation-note" />生成 AI 词卡与缓存题库</label><p id="collect-generation-note" class="collect-note">使用当前翻译模型，需联网且可能产生费用；失败仍保留单词。AI 音标和题目需要核对。</p></div>
    </div>
    <footer class="collect-footer"><p id="collect-status" role="status"></p><div class="collect-actions"><span id="collect-count" aria-live="polite">已选 0 / 20</span><div><button type="button" id="dismiss-collect">取消</button><button type="button" id="confirm-collect">收藏选中词</button></div></div></footer></form>`;
  (popup?root.querySelector(".popup-shell")||root:root).append(dialog);let snapshot={source:"",translation:""};let busy=false;let epoch=0;
  const el=<T extends HTMLElement>(selector:string)=>dialog.querySelector<T>(selector)!;
  const close=()=>{epoch++;if(dialog.open)dialog.close();if(popup)void invoke("set_vocabulary_collection_open",{open:false}).catch(()=>{});};
  dialog.addEventListener("cancel",event=>{event.preventDefault();close();});
  el("#cancel-collect").addEventListener("click",close);
  el("#dismiss-collect").addEventListener("click",close);
  const selectedWords=()=>[...new Set([el<HTMLInputElement>("#collect-word").value.trim(),...Array.from(dialog.querySelectorAll<HTMLInputElement>('#word-candidates input:checked')).map(c=>c.value)].filter(Boolean))];
  const refreshCount=()=>{el("#collect-count").textContent=`已选 ${selectedWords().length} / 20`;};
  el("#collect-word").addEventListener("input",refreshCount);
  el("#word-candidates").addEventListener("change",refreshCount);
  const open=async (selected="",batch=false)=>{
    if(busy)return;snapshot=source();epoch++;el("#collect-status").textContent="";
    el<HTMLInputElement>("#collect-word").value=batch?"":selected.trim();el<HTMLTextAreaElement>("#collect-context").value=selected?sentenceFor(snapshot.source,selected):"";
    const candidates=el("#word-candidates");candidates.replaceChildren();
    if(batch)for(const word of englishCandidates(snapshot.source)){const label=document.createElement("label");const check=document.createElement("input");check.type="checkbox";check.value=word;label.append(check,document.createTextNode(word));candidates.append(label);}
    el("#collect-candidates-section").hidden=!candidates.childElementCount;
    el<HTMLDetailsElement>("#collect-source-options").open=false;refreshCount();
    if(popup)await invoke("set_vocabulary_collection_open",{open:true});dialog.showModal();el<HTMLInputElement>("#collect-word").focus();
  };
  el("#confirm-collect").addEventListener("click",()=>void (async()=>{
    if(busy)return;const ownEpoch=epoch;
    const words=selectedWords();refreshCount();
    if(!words.length||words.length>20){el("#collect-status").textContent="请选取 1–20 个单词";return;}
    if(words.some(word=>! /^[A-Za-z]+(?:['’-][A-Za-z]+)*$/.test(word)||word.length>48)){el("#collect-status").textContent="请输入单个英文词，不要填写整句";return;}
    busy=true;el<HTMLButtonElement>("#confirm-collect").disabled=true;
    try {
      const drafts:VocabularyEntry[]=[];
      for(const word of words){const context=words.length===1?el<HTMLTextAreaElement>("#collect-context").value:sentenceFor(snapshot.source,word);
        drafts.push(await invoke<VocabularyEntry>("collect_vocabulary",{word,sentence:context,translation:words.length===1?snapshot.translation.slice(0,1200):""}));}
      if(el<HTMLInputElement>("#generate-on-collect").checked){
        // Two workers only. Collection survives closing the dialog or generation failure.
        const queue=drafts.filter(e=>!e.card && e.generationState!=="generating");
        for(let worker=0;worker<2;worker++)void (async()=>{while(queue.length){const e=queue.shift()!;try{await invoke("generate_vocabulary",{id:e.id,revision:e.revision,force:false,quizOnly:false});}catch {/* status remains in the durable book */}}})();
      }
      if(ownEpoch===epoch){el("#collect-status").textContent=`已收藏 ${drafts.length} 个词，可在生词本查看生成状态`;}
    }catch(error){if(ownEpoch===epoch)el("#collect-status").textContent=errorText(error);}
    finally{busy=false;el<HTMLButtonElement>("#confirm-collect").disabled=false;}
  })());
  return {open,close,dialog};
}
