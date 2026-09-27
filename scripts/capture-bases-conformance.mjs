// Run against an isolated Obsidian 1.14.2 disposable vault with CDP enabled.
// Invokes the running app's registered CLI handlers; stores no app source.
import { createRequire } from 'node:module';
import { readFile, writeFile } from 'node:fs/promises';
const require = createRequire(new URL('../crates/crucible-web/web/package.json', import.meta.url));
const { chromium } = require('playwright');
const root = new URL('../assets/fixtures/bases/', import.meta.url);
const cases = JSON.parse(await readFile(new URL('expressions.json', root), 'utf8'));
const browser = await chromium.connectOverCDP(process.env.OBSIDIAN_CDP ?? 'http://127.0.0.1:19222');
try {
  const page = browser.contexts()[0].pages().find(p => p.url().startsWith('app://obsidian.md'));
  if (!page) throw new Error('Obsidian vault must be open');
  const result = await page.evaluate(async cases => {
    if (!app.vault.getName().startsWith('crucible-bases-conformance') && app.vault.adapter.basePath !== '/tmp/crucible-obsidian-oracle/vault') throw new Error('Refusing a vault not named crucible-bases-conformance*');
    if (!document.title.includes('Obsidian 1.14.2')) throw new Error('Compatibility target is Obsidian 1.14.2');
    const put = async (path, text) => { const f=app.vault.getFileByPath(path); if(f) await app.vault.modify(f,text); else await app.vault.create(path,text); };
    if (!app.vault.getFolderByPath('notes')) await app.vault.createFolder('notes');
    await put('notes/a.md', '---\nstatus: todo\ntags: [work/deep]\n---\n[[notes/a]]\n');
    await new Promise(resolve => setTimeout(resolve, 1500));
    const output=[];
    for (const item of cases) {
      const path=`Case-${item.id}.base`;
      await put(path, JSON.stringify({filters:'file.path == "notes/a.md"',formulas:{result:item.expression},properties:{'formula.result':{displayName:'result'}},views:[{type:'table',name:'Case',order:['formula.result']}]}));
      try { output.push({...item, output:JSON.parse(await app.cli.handlers.get('base:query').handler({path,view:'Case',format:'json'}))}); }
      catch(error){output.push({...item,error:String(error)});}
    }
    return {version:document.title.match(/Obsidian ([\d.]+)/)?.[1], timezone:Intl.DateTimeFormat().resolvedOptions().timeZone, transport:'running Obsidian registered base:query CLI handler via CDP', cases:output};
  }, cases);
  await writeFile(new URL('obsidian-1.14.2-expressions.json', root), JSON.stringify(result,null,2)+'\n');
  console.log(`Captured ${result.cases.length} expressions from ${result.version}`);
  const queries = JSON.parse(await readFile(new URL('queries.json', root), 'utf8'));
  const queryResults = await page.evaluate(async corpus => {
    if (!app.vault.getFolderByPath('cases')) await app.vault.createFolder('cases');
    const put = async (path, text) => { const f=app.vault.getFileByPath(path); if(f) await app.vault.modify(f,text); else await app.vault.create(path,text); };
    for (const [path,text] of Object.entries(corpus.files)) await put(path,text);
    await new Promise(resolve => setTimeout(resolve, 1500));
    const cases=[];
    for (const item of corpus.cases) {
      const path=`Corpus-${item.id}.base`;
      await put(path,JSON.stringify(item.base));
      const formats={};
      for (const format of ['json','csv','tsv','md','paths']) {
        try { formats[format]={output:await app.cli.handlers.get('base:query').handler({path,view:item.base.views?.length?'Case':undefined,format})}; }
        catch(error) { formats[format]={error:String(error)}; }
      }
      cases.push({...item,path,formats});
    }
    return {version:document.title.match(/Obsidian ([\d.]+)/)?.[1],timezone:Intl.DateTimeFormat().resolvedOptions().timeZone,files:corpus.files,cases};
  },queries);
  await writeFile(new URL('obsidian-1.14.2-queries.json',root),JSON.stringify(queryResults,null,2)+'\n');
  console.log(`Captured ${queryResults.cases.length} queries in five formats`);
  const creation = JSON.parse(await readFile(new URL('creation.json',root),'utf8'));
  const created = await page.evaluate(async corpus => {
    for (const folder of ['created','templates']) if (!app.vault.getFolderByPath(folder)) await app.vault.createFolder(folder);
    const put = async (path,text) => { const f=app.vault.getFileByPath(path); if(f) await app.vault.modify(f,text); else await app.vault.create(path,text); };
    for (const [path,text] of Object.entries(corpus.files)) await put(path,text);
    for (const file of app.vault.getMarkdownFiles()) if(file.basename.startsWith('Oracle-')) await app.vault.delete(file);
    await new Promise(resolve => setTimeout(resolve,1500));
    const cases=[];
    for (const item of corpus.cases) {
      const path=`Create-${item.id}.base`;
      await put(path,JSON.stringify(item.base));
      try {
        const output=await app.cli.handlers.get('base:create').handler({path,view:'Case',name:`Oracle-${item.id}`,content:'Body\n'});
        const file=app.vault.getMarkdownFiles().find(f=>f.basename===`Oracle-${item.id}`);
        cases.push({...item,output,path:file?.path,bytes:file?await app.vault.read(file):null});
      } catch(error) { cases.push({...item,error:String(error)}); }
    }
    return {version:document.title.match(/Obsidian ([\d.]+)/)?.[1],files:corpus.files,cases};
  },creation);
  await writeFile(new URL('obsidian-1.14.2-creation.json',root),JSON.stringify(created,null,2)+'\n');
  console.log(`Captured ${created.cases.length} entry creations`);
  const summaryResults = await page.evaluate(async () => {
    const names=['Average','Min','Max','Sum','Range','Median','Stddev','Earliest','Latest','Checked','Unchecked','Empty','Filled','Unique'];
    const base={filters:"file.inFolder('cases')",formulas:{when:"date('2024-01-01') + duration(score + 'd')"},views:[{type:'table',name:'Case',order:['file.name','note.score','note.checked','note.labels','formula.when'],groupBy:{property:'note.status',direction:'ASC'}}]};
    const path='Summaries.base';let file=app.vault.getFileByPath(path);
    if(file) await app.vault.modify(file,JSON.stringify(base)); else file=await app.vault.create(path,JSON.stringify(base));
    const leaf=app.workspace.getLeaf(true);await leaf.openFile(file);
    await new Promise(resolve=>setTimeout(resolve,1000));
    const view=leaf.view.controller.view;
    const summaries=[];
    for(const name of names) {
      const property=['Earliest','Latest'].includes(name)?'formula.when':['Checked','Unchecked'].includes(name)?'note.checked':['Empty','Filled','Unique'].includes(name)?'note.labels':'note.score';
      const value=rows=>String(view.data.getSummaryValue(view.config,rows,property,name));
      summaries.push({name,property,all:value(view.data.data),empty:value([]),groups:view.data.groupedData.map(g=>({group:String(g.key),value:value(g.entries)}))});
    }
    return {version:document.title.match(/Obsidian ([\d.]+)/)?.[1],transport:'running native table view summary evaluation',base,summaries};
  });
  await writeFile(new URL('obsidian-1.14.2-summaries.json',root),JSON.stringify(summaryResults,null,2)+'\n');
  console.log(`Captured ${summaryResults.summaries.length} summaries`);
  const moves = await page.evaluate(async () => {
    if (!app.vault.getFolderByPath('moves')) await app.vault.createFolder('moves');
    const put = async (path,text) => { const f=app.vault.getFileByPath(path); if(f) {await app.vault.modify(f,text); return f;} return app.vault.create(path,text); };
    await put('moves/donor.md','---\nstatus: done\nlabels: [b]\nchecked: false\n---\nDonor\n');
    const cases=[];
    for (const item of [
      {id:'status',key:'status',value:'done',header:'status: todo'},
      {id:'empty-group',key:'status',value:null,header:'status: todo'},
      {id:'list-group',key:'labels',value:'b',header:'labels: [a, c]'},
      {id:'missing-list-group',key:'labels',value:'b',header:'status: todo'},
      {id:'boolean-group',key:'checked',value:false,header:'checked: true'},
    ]) {
      const before=`---\n${item.header}\nkeep: unchanged\n---\nBody 🦀\n`;
      const file=await put('moves/item.md',before);
      const base={filters:'file.inFolder("moves")',views:[{type:'kanban',name:'Board',order:['file.name'],groupBy:{property:`note.${item.key}`,direction:'ASC'}}]};
      const source=await put('Native-Moves.base',JSON.stringify(base));
      const leaf=app.workspace.getLeaf(true); await leaf.openFile(source); await new Promise(r=>setTimeout(r,800));
      const view=leaf.view.controller.view;
      const target=item.value===null?view.data.data[0].getValue('note.absent'):view.data.groupedData.find(g=>String(g.key)===String(item.value))?.key;
      if(!target) throw new Error(`Missing move target ${item.id}`);
      await view.moveCardToColumn(file,target);
      cases.push({...item,before,after:await app.vault.read(file)});
      leaf.detach();
    }
    return {version:document.title.match(/Obsidian ([\d.]+)/)?.[1],transport:'running native kanban moveCardToColumn used by card drops',property_types:{labels:'multitext'},cases};
  });
  await writeFile(new URL('obsidian-1.14.2-moves.json',root),JSON.stringify(moves,null,2)+'\n');
  console.log(`Captured ${moves.cases.length} native kanban moves`);
} finally { await browser.close(); }
