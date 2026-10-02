import { invoke } from "@tauri-apps/api/core";
import type { VocabularyEntry } from "../types";
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
  dialog.innerHTML=`<form method="dialog"><header><h2>加入生词本</h2><button type="button" id="cancel-collect" aria-label="关闭收藏">×</button></header>
    <p class="collect-note">选择你想学习的英文词。缩写不自动列出，可手动填写；相同词形去重。基本词形需在词卡中确认。</p>
    <label>手动单词<input id="collect-word" maxlength="48" placeholder="例如 architecture" autocomplete="off" /></label>
    <div id="word-candidates" class="word-candidates" aria-label="句子中的候选生词"></div>
    <label>来源句（仅此上下文会发给模型）<textarea id="collect-context" maxlength="1200" rows="2"></textarea></label>
    <label class="collect-check"><input id="generate-on-collect" type="checkbox" checked />收藏后生成 AI 词卡与缓存题库</label>
    <p class="collect-note">使用当前翻译模型；可能产生费用。失败仍保留单词。词卡内容可编辑，AI 音标和题目需要核对。</p>
    <p id="collect-status" role="status"></p><button type="button" id="confirm-collect">收藏选中词</button></form>`;
  root.append(dialog);let snapshot={source:"",translation:""};let busy=false;let epoch=0;
  const el=<T extends HTMLElement>(selector:string)=>dialog.querySelector<T>(selector)!;
  const close=()=>{epoch++;if(dialog.open)dialog.close();if(popup)void invoke("set_vocabulary_collection_open",{open:false}).catch(()=>{});};
  dialog.addEventListener("cancel",event=>{event.preventDefault();close();});
  el("#cancel-collect").addEventListener("click",close);
  const open=async (selected="",batch=false)=>{
    if(busy)return;snapshot=source();epoch++;el("#collect-status").textContent="";
    el<HTMLInputElement>("#collect-word").value=batch?"":selected.trim();el<HTMLTextAreaElement>("#collect-context").value=selected?sentenceFor(snapshot.source,selected):"";
    const candidates=el("#word-candidates");candidates.replaceChildren();
    if(batch)for(const word of englishCandidates(snapshot.source)){const label=document.createElement("label");const check=document.createElement("input");check.type="checkbox";check.value=word;label.append(check,document.createTextNode(word));candidates.append(label);}
    if(popup)await invoke("set_vocabulary_collection_open",{open:true});dialog.showModal();el<HTMLInputElement>("#collect-word").focus();
  };
  el("#confirm-collect").addEventListener("click",()=>void (async()=>{
    if(busy)return;const ownEpoch=epoch;
    const words=[...new Set([el<HTMLInputElement>("#collect-word").value.trim(),...Array.from(dialog.querySelectorAll<HTMLInputElement>('#word-candidates input:checked')).map(c=>c.value)].filter(Boolean))];
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
