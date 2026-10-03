import { test } from "node:test";
import assert from "node:assert/strict";
import { build } from "esbuild";
import { JSDOM } from "jsdom";

async function fixture() {
  const dom = new JSDOM("<!doctype html><html><body></body></html>");
  let receive: (event: {payload: unknown}) => void = () => {};
  let finish: (value: unknown) => void = () => {};
  const qa = { async listen(_name: string, callback: typeof receive) { receive = callback; }, async invoke() { return new Promise(resolve => { finish = resolve; }); } };
  const globals = globalThis as unknown as Record<string, unknown>; globals.document = dom.window.document; globals.__appearanceQa = qa;
  const built = await build({ entryPoints:["src/appearance.ts"], bundle:true,write:false,format:"esm",plugins:[{name:"qa",setup(builder){
    builder.onResolve({filter:/^@tauri-apps\/api\//},args=>({path:args.path,namespace:"qa"}));
    builder.onLoad({filter:/.*/,namespace:"qa"},()=>({contents:"export const invoke=(...a)=>globalThis.__appearanceQa.invoke(...a); export const listen=(...a)=>globalThis.__appearanceQa.listen(...a);"}));
  }}] });
  const module = await import(`data:text/javascript;base64,${Buffer.from(built.outputFiles[0].text+`\n//${Math.random()}`).toString("base64")}`);
  return {dom,module,qa,emit:(value:unknown)=>receive({payload:value}),finish:(value:unknown)=>finish(value)};
}

test("appearance normalizes invalid values and only changes background CSS alpha",async()=>{
  const {dom,module}=await fixture();try{
    assert.deepEqual(module.normalizeAppearance(),{theme:"system",popupOpacity:96});
    assert.deepEqual(module.normalizeAppearance({theme:"unknown",popupOpacity:0}),{theme:"system",popupOpacity:70});
    assert.deepEqual(module.normalizeAppearance({theme:"light",popupOpacity:110}),{theme:"light",popupOpacity:100});
    module.applyAppearance({theme:"dark",popupOpacity:80});
    assert.equal(dom.window.document.documentElement.dataset.theme,"dark");
    assert.equal(dom.window.document.documentElement.style.getPropertyValue("--popup-opacity"),"80%");
    assert.equal(dom.window.document.documentElement.style.opacity,"");
  }finally{dom.window.close();}
});

test("live appearance event takes precedence over stale initial preferences",async()=>{
  const m=await fixture();try{
    const pending=m.module.initializeAppearance();await new Promise(resolve=>setImmediate(resolve));
    m.emit({theme:"dark",popupOpacity:75});m.finish({theme:"light",popupOpacity:100});await pending;
    assert.equal(m.dom.window.document.documentElement.dataset.theme,"dark");
    assert.equal(m.dom.window.document.documentElement.style.getPropertyValue("--popup-opacity"),"75%");
  }finally{m.dom.window.close();}
});
