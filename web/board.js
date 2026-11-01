// Table-only canvas and keyboard controls. Server views are already audience-filtered.
(function () {
  const id = () => {
    const bytes = crypto.getRandomValues(new Uint8Array(16));
    const millis = Date.now();
    for (let i = 0; i < 6; i++) bytes[i] = Math.floor(millis / 2 ** ((5 - i) * 8)) & 255;
    bytes[6] = (bytes[6] & 15) | 0x70; bytes[8] = (bytes[8] & 63) | 0x80;
    const h = [...bytes].map(n => n.toString(16).padStart(2, '0')).join('');
    return `${h.slice(0,8)}-${h.slice(8,12)}-${h.slice(12,16)}-${h.slice(16,20)}-${h.slice(20)}`;
  };
  const factor = convention => convention === 'exact' ? 0.3048 : 0.3;
  const convert = (value, from, to, convention) => from === to ? value : from === 'ft' ? value * factor(convention) : value / factor(convention);
  function pixel(p, g) {
    const s = g.pixels / 2;
    const [x,y] = g.kind === 'hexPointy' ? [Math.sqrt(3)*(p.x+p.y/2)*s, 1.5*p.y*s] :
      g.kind === 'hexFlat' ? [1.5*p.x*s, Math.sqrt(3)*(p.y+p.x/2)*s] :
      g.kind === 'none' ? [p.x,p.y] : [p.x*g.pixels,p.y*g.pixels];
    return {x:x+g.offset.x,y:y+g.offset.y};
  }
  function snap(p, g) {
    const x=p.x-g.offset.x,y=p.y-g.offset.y,s=g.pixels/2;
    if (g.kind === 'none') return {x,y};
    if (g.kind === 'square') return {x:Math.round(x/g.pixels),y:Math.round(y/g.pixels)};
    const q=g.kind==='hexPointy'?(Math.sqrt(3)*x/3-y/3)/s:2*x/3/s;
    const r=g.kind==='hexPointy'?2*y/3/s:(-x/3+Math.sqrt(3)*y/3)/s;
    let rx=Math.round(q),ry=Math.round(-q-r),rz=Math.round(r);
    const dx=Math.abs(rx-q),dy=Math.abs(ry+q+r),dz=Math.abs(rz-r);
    if (dx>dy&&dx>dz) rx=-ry-rz; else if(dy<=dz) rz=-rx-ry;
    return {x:rx,y:rz};
  }
  function path(from, to, g) {
    if(g.kind==='none') return [to];
    const count=g.kind==='square'?Math.max(Math.abs(to.x-from.x),Math.abs(to.y-from.y)):
      (Math.abs(to.x-from.x)+Math.abs(to.y-from.y)+Math.abs(to.x-from.x+to.y-from.y))/2;
    if(count>100) throw Error('pathLimit');
    return Array.from({length:count},(_,i)=>{
      const n=(i+1)/count,p={x:from.x+(to.x-from.x)*n,y:from.y+(to.y-from.y)*n};
      return g.kind==='square'?{x:Math.round(p.x),y:Math.round(p.y)}:snap(pixel(p,g),g);
    });
  }
  function cost(from, steps, scene, diagonals=0) {
    let value=0;
    for(const p of steps){const dx=Math.abs(p.x-from.x),dy=Math.abs(p.y-from.y),g=scene.grid;
      let cells;
      if(g.kind==='none') cells=Math.hypot(dx,dy)/g.pixels;
      else if(g.kind.startsWith('hex')) cells=(dx+dy+Math.abs(p.x-from.x+p.y-from.y))/2;
      else if(dx&&dy){diagonals++;cells=g.diagonal==='alternating'?(diagonals%2?1:2):g.diagonal==='euclidean'?Math.SQRT2:1;}
      else cells=Math.max(dx,dy);
      value+=cells*g.distance.value*(scene.difficult.some(q=>q.x===p.x&&q.y===p.y)?2:1);from=p;
    } return value;
  }
  function templateContains(p, area, g) {
    const origin=pixel(area.origin,g), cell=pixel(p,g), angle=area.angle*Math.PI/180;
    const dx=cell.x-origin.x,dy=cell.y-origin.y;
    const x=dx*Math.cos(angle)+dy*Math.sin(angle),y=-dx*Math.sin(angle)+dy*Math.cos(angle);
    const radius=convert(area.size.value,area.size.unit,g.distance.unit,g.conversion)/g.distance.value*g.pixels;
    // Centre coverage is exact. Touch coverage deliberately includes the cell's
    // bounding radius; the UI labels this conservative convention.
    const margin=area.coverage==='touch'?g.pixels/Math.SQRT2:0;
    if(area.shape==='circle'||area.shape==='cylinder')return Math.hypot(x,y)<=radius+margin;
    if(area.shape==='square')return x>=-margin&&x<=radius+margin&&Math.abs(y)<=radius/2+margin;
    if(area.shape==='line')return x>=-margin&&x<=radius+margin&&Math.abs(y)<=g.pixels/2+margin;
    return Math.hypot(x,y)<=radius+margin&&x>=-margin&&Math.abs(y)<=Math.max(0,x)*Math.tan(Math.PI/6)+margin;
  }
  // One 24 px stroke icon set, built with DOM calls (never markup strings). Decorative only:
  // every control also carries its translated name.
  const ICON={pan:'M12 3v18M3 12h18M9 6l3-3 3 3M9 18l3 3 3-3M6 9l-3 3 3 3M18 9l3 3-3 3',move:'M5 3l14 8-6 2-2 6z',ping:'M12 9a3 3 0 1 0 0 6a3 3 0 1 0 0-6M12 3a9 9 0 1 0 0 18a9 9 0 1 0 0-18',ruler:'M3 17L17 3l4 4L7 21zM7 13l2 2M10 10l2 2M13 7l2 2',calibrate:'M4 4h16v16H4zM12 4v16M4 12h16',terrain:'M3 20l6-10 4 6 3-4 5 8z',reveal:'M2 12s4-7 10-7 10 7 10 7-4 7-10 7S2 12 2 12zM12 9a3 3 0 1 0 0 6a3 3 0 1 0 0-6',cover:'M2 12s4-7 10-7 10 7 10 7-4 7-10 7S2 12 2 12zM3 3l18 18',brushReveal:'M15 3l6 6-8 8-6-6zM7 11l-4 4v6h6l4-4',brushCover:'M15 3l6 6-8 8-6-6zM7 11l-4 4v6h6l4-4M3 3l18 18',polygon:'M12 3l9 7-3 11H6L3 10z',circle:'M12 3a9 9 0 1 0 0 18a9 9 0 1 0 0-18',cone:'M3 12l18-8v16z',square:'M4 4h16v16H4z',line:'M3 19L19 3M5 21L21 5',cylinder:'M5 6a7 3 0 1 0 14 0a7 3 0 1 0-14 0M5 6v12a7 3 0 0 0 14 0V6',zoomOut:'M5 12h14',zoomIn:'M5 12h14M12 5v14',zoomFit:'M4 9V4h5M20 9V4h-5M4 15v5h5M20 15v5h-5',fullscreen:'M3 8V3h5M21 8V3h-5M3 16v5h5M21 16v5h-5M9 9h6v6H9z'};
  function icon(name){const ns='http://www.w3.org/2000/svg',svg=document.createElementNS(ns,'svg'),path=document.createElementNS(ns,'path');
    for(const[k,v]of Object.entries({viewBox:'0 0 24 24',width:'22',height:'22','aria-hidden':'true',focusable:'false',class:'vtt-icon-svg'}))svg.setAttribute(k,v);
    path.setAttribute('d',ICON[name]);svg.append(path);return svg;}
  let instances=0;
  function create({el,t,api,tableId,role,account,seats,confirm,active,ping}) {
    const uid=++instances;
    const root=el('section',{class:'board-workspace vtt','aria-label':t('board')});
    const notice=el('p',{class:'note vtt-notice',role:'status',text:t('liveConnecting')});
    const status=el('div',{role:'status',class:'vtt-status'}), tools=el('div',{class:'board-tools'});
    const canvas=el('canvas',{class:'battle-map',width:1000,height:700,tabindex:0,'aria-label':t('boardKeys')});
    const summary=el('p',{class:'muted vtt-summary',text:t('boardKeys')});
    const pingStatus=el('p',{class:'sr-only','aria-live':'polite'});
    const roster=el('div',{class:'token-roster'}), editor=el('div',{class:'board-editor'});
    const sceneSelect=el('select',{'aria-label':t('scene')});
    const toolNames=['pan','move',...(role==='spectator'||!ping?[]:['ping']),'ruler',...(role==='gm'?['calibrate','terrain','reveal','cover','brushReveal','brushCover','polygon']:[]),'circle','cone','square','line','cylinder'];
    const tool={value:'move'};
    const palette=el('div',{class:'vtt-palette',role:'toolbar','aria-label':t('tool'),'aria-orientation':'vertical'});
    const toolButtons=toolNames.map(v=>el('button',{type:'button',class:'vtt-tool','data-tool':v,'aria-pressed':String(v===tool.value),title:t('tool.'+v),onclick:()=>chooseTool(v)},icon(v),el('span',{class:'sr-only',text:t('tool.'+v)})));
    palette.append(...toolButtons);
    palette.onkeydown=e=>{const i=toolButtons.indexOf(document.activeElement);if(i<0||!['ArrowUp','ArrowDown','Home','End'].includes(e.key))return;e.preventDefault();const n=e.key==='Home'?0:e.key==='End'?toolButtons.length-1:(i+(e.key==='ArrowDown'?1:-1)+toolButtons.length)%toolButtons.length;toolButtons[n].focus();};
    const stage=el('div',{class:'vtt-stage'},canvas);
    const viewport=el('div',{class:'vtt-viewport','data-tool':tool.value},stage);
    function chooseTool(v){if(!toolNames.includes(v))return;tool.value=v;if(v!=='polygon')polygon=[];measurement=['circle','cone','square','line','cylinder'].includes(v)?measurement:null;for(const b of toolButtons)b.setAttribute('aria-pressed',String(b.dataset.tool===v));viewport.dataset.tool=v;paint();}
    // View transform: CSS translate/scale on the stage. Pointer maths reads the transformed box.
    let view={x:0,y:0,z:1,fitted:null},panning=null,spaceHeld=false;
    const zoomLabel=el('output',{class:'vtt-zoom-level','aria-live':'off'});
    const applyView=()=>{stage.style.transform=`translate(${view.x}px,${view.y}px) scale(${view.z})`;zoomLabel.textContent=new Intl.NumberFormat(window.ChronexusI18n.locale,{style:'percent'}).format(view.z);};
    const fit=()=>{const s=scene();if(!s)return;const w=viewport.clientWidth||s.width,h=viewport.clientHeight||s.height;const z=Math.max(0.1,Math.min(4,w/s.width,h/s.height));view={x:(w-s.width*z)/2,y:(h-s.height*z)/2,z,fitted:s.id};applyView();};
    const zoomAt=(factor,cx=viewport.clientWidth/2,cy=viewport.clientHeight/2)=>{const z=Math.max(0.1,Math.min(4,view.z*factor));view.x=cx-(cx-view.x)*z/view.z;view.y=cy-(cy-view.y)*z/view.z;view.z=z;applyView();};
    viewport.addEventListener('wheel',e=>{if(!scene())return;e.preventDefault();const b=viewport.getBoundingClientRect();zoomAt(e.deltaY<0?1.15:1/1.15,e.clientX-b.left,e.clientY-b.top);},{passive:false});
    // Middle or right button, Space+drag and the pan tool move the view and never the map state.
    viewport.addEventListener('pointerdown',e=>{if(!(e.button===1||e.button===2||spaceHeld||tool.value==='pan'))return;e.stopPropagation();e.preventDefault();panning={id:e.pointerId,x:e.clientX,y:e.clientY,vx:view.x,vy:view.y};viewport.setPointerCapture?.(e.pointerId);viewport.classList.add('panning');},true);
    viewport.addEventListener('pointermove',e=>{if(!panning||e.pointerId!==panning.id)return;e.stopPropagation();view.x=panning.vx+e.clientX-panning.x;view.y=panning.vy+e.clientY-panning.y;applyView();},true);
    const endPan=e=>{if(!panning||e.pointerId!==panning.id)return;e.stopPropagation();panning=null;viewport.classList.remove('panning');};
    viewport.addEventListener('pointerup',endPan,true);viewport.addEventListener('pointercancel',endPan,true);
    viewport.addEventListener('contextmenu',e=>e.preventDefault());
    const zoomButton=(key,fn)=>el('button',{type:'button',class:'vtt-icon',title:t(key),'aria-label':t(key),onclick:fn},icon(key));
    const fullscreen=el('button',{type:'button',class:'vtt-icon',title:t('fullscreen'),'aria-label':t('fullscreen'),'aria-pressed':'false',onclick:()=>{try{if(document.fullscreenElement===root)document.exitFullscreen?.();else root.requestFullscreen?.();}catch{}}});
    fullscreen.append(icon('fullscreen'));document.addEventListener?.('fullscreenchange',()=>fullscreen.setAttribute('aria-pressed',String(document.fullscreenElement===root)));
    const zoomBar=el('div',{class:'vtt-zoom',role:'group','aria-label':t('zoom')},zoomButton('zoomOut',()=>zoomAt(1/1.25)),zoomLabel,zoomButton('zoomIn',()=>zoomAt(1.25)),zoomButton('zoomFit',fit),fullscreen);
    // Sidebar: one tab per panel; the host adds chat and players to the same list.
    const tabs=el('div',{class:'vtt-tabs',role:'tablist','aria-label':t('panels')}),panelsBox=el('div',{class:'vtt-panels'}),panelMap=new Map();
    let activePanel=null,savedPanel=null;try{savedPanel=localStorage.getItem('chronexus-table-panel');}catch{}
    function openPanel(key,remember=true){if(!panelMap.has(key))return;activePanel=key;for(const[k,v]of panelMap){const on=k===key;v.tab.setAttribute('aria-selected',String(on));v.tab.tabIndex=on?0:-1;v.body.hidden=!on;}if(remember)try{localStorage.setItem('chronexus-table-panel',key);}catch{}}
    function panel(key,first=false){
      if(panelMap.has(key))return panelMap.get(key).body;
      const tabId=`vtt-tab-${uid}-${key}`,panelId=`vtt-panel-${uid}-${key}`;
      const tab=el('button',{type:'button',role:'tab',id:tabId,'aria-controls':panelId,'aria-selected':'false',tabindex:'-1',text:t('tab.'+key),onclick:()=>openPanel(key)});
      const body=el('div',{role:'tabpanel',id:panelId,'aria-labelledby':tabId,class:'vtt-panel vtt-panel-'+key,tabindex:'0',hidden:true});
      if(first){tabs.prepend(tab);panelsBox.prepend(body);}else{tabs.append(tab);panelsBox.append(body);}
      panelMap.set(key,{tab,body});
      if(!activePanel||key===savedPanel||(first&&activePanel!==savedPanel))openPanel(key,false);
      return body;
    }
    tabs.onkeydown=e=>{const list=[...panelMap.keys()],i=list.indexOf(activePanel);if(!['ArrowLeft','ArrowRight','Home','End'].includes(e.key)||i<0)return;e.preventDefault();const rtl=getComputedStyle?.(tabs).direction==='rtl',forward=(e.key==='ArrowRight')!==rtl;const n=e.key==='Home'?0:e.key==='End'?list.length-1:(i+(forward?1:-1)+list.length)%list.length;openPanel(list[n]);panelMap.get(list[n]).tab.focus();};
    const sidebar=el('aside',{class:'vtt-sidebar','aria-label':t('panels')},tabs,panelsBox);
    panel('tokens');panel('combat');panel('measure');if(role==='gm'){panel('scene');panel('fog');panel('library');}
    // Pings are transient: drawn for a few seconds, announced politely, never stored.
    const pings=[];let pingFrame=0;
    function showPing(frame){const s=scene();if(!current()||!s||frame?.sceneId!==s.id||!Number.isFinite(frame.x)||!Number.isFinite(frame.y))return;pings.push({x:frame.x,y:frame.y,name:String(frame.authorName??'').slice(0,100),mine:frame.authorId===account,until:Date.now()+2500});if(pings.length>20)pings.shift();pingStatus.textContent=t('pinged',{name:String(frame.authorName??'').slice(0,100)});animate();}
    function animate(){if(pingFrame||!current())return;const next=globalThis.requestAnimationFrame??(fn=>setTimeout(fn,16));pingFrame=next(()=>{pingFrame=0;const now=Date.now();while(pings.length&&pings[0].until<now)pings.shift();paint();if(pings.length)animate();});}
    const sendPing=p=>{const s=scene();if(!ping||!online||role==='spectator'||!s||s.id!==board?.activeScene)return;try{ping({sceneId:s.id,x:Math.max(0,Math.min(s.width,p.x)),y:Math.max(0,Math.min(s.height,p.y))});}catch{}};
    const clone=value=>JSON.parse(JSON.stringify(value));
    let advanced=null,polygon=[],imageGeneration=0;
    let board=null,online=false,busy=false,disposed=false,selected=null,chosenScene=null,start=null,preview=null,measurement=null,calibration=null,pending=null,primary='ft';
    const images=new Map(),urls=new Set(),thumbUrls=new Set(),thumbnails=new Map();
    let thumbnailQueue=Promise.resolve();
    const thumbnailFor=assetId=>{if(!thumbnails.has(assetId)){const request=thumbnailQueue.then(async()=>{if(!current())return null;const blob=await api('GET',endpoint+'/maps/'+encodeURIComponent(assetId)+'/thumbnail',undefined,true);if(!current())return null;const url=URL.createObjectURL(blob);thumbUrls.add(url);return url;});thumbnailQueue=request.catch(()=>null);thumbnails.set(assetId,request);request.catch(()=>thumbnails.delete(assetId));}return thumbnails.get(assetId);};
    try{primary=localStorage.getItem('chronexus-table-unit')==='m'?'m':'ft';}catch{}
    const current=()=>!disposed&&active();
    const scene=()=>board?.scenes.find(s=>s.id===(chosenScene??board.activeScene))??board?.scenes[0];
    const tokens=()=>board?.tokens.filter(v=>v.sceneId===scene()?.id)??[];
    const selectedToken=()=>tokens().find(v=>v.id===selected);
    const number=v=>new Intl.NumberFormat(window.ChronexusI18n.locale,{maximumFractionDigits:2}).format(v);
    const distance=(v,unit=scene()?.grid.distance.unit??'ft',convention=scene()?.grid.conversion??'table')=>{
      const a=convert(v,unit,primary,convention), other=primary==='ft'?'m':'ft';
      return `${number(a)} ${primary} · ${number(convert(v,unit,other,convention))} ${other}`;
    };
    const button=(key,fn)=>{const b=el('button',{type:'button',text:t(key),onclick:fn,'data-board-write':true,disabled:!online||busy});return b;};
    const syncDisabled=()=>root.querySelectorAll('[data-board-write]').forEach(b=>b.disabled=!online||busy||(b.hasAttribute('data-owned-only')&&selectedToken()?.ownerId!==account));
    const showError=e=>{status.replaceChildren(el('p',{class:'error',role:'alert',text:t('boardFailed')}));if(pending)status.append(button('retry',()=>run(pending)));};
    const endpoint=`/api/v1/tables/${encodeURIComponent(tableId)}`;
    async function refresh(){const value=await api('GET',endpoint+'/board');if(current())update(value);}
    async function run(op) {
      if(!current()||!online||busy)return;
      busy=true;syncDisabled();pending=op;status.textContent=t('working');
      try{await api('POST',endpoint+'/board/operations',op);pending=null;await refresh();if(current())status.textContent=t('done');}
      catch(e){if(e.status===409||e.status===403||e.status===400){pending=null;try{await refresh();}catch{} }if(current())showError(e);}
      finally{busy=false;syncDisabled();}
    }
    const send=command=>run(pending??{operationId:id(),baseRevision:board.revision,command});
    function field(form,key,value,type='text',options) {
      const input=options?el('select',{},...options.map(([v,label])=>el('option',{value:v,text:label,selected:v===String(value)}))):el('input',{type,value,step:type==='number'?'any':undefined});
      input.id='board-'+key+'-'+id();form.append(el('label',{for:input.id,text:t(key)}),input);return input;
    }
    const read=input=>Number(input.value);
    const unitOptions=[['ft','ft'],['m','m']];
    const unit=el('select',{'aria-label':t('primaryUnit')},...unitOptions.map(([value,text])=>el('option',{value,text,selected:value===primary})));
    unit.onchange=()=>{primary=unit.value;try{localStorage.setItem('chronexus-table-unit',primary);}catch{}draw();};
    const converter=el('input',{type:'number',step:'any',value:30,'aria-label':t('converter')});
    const converted=el('output',{'aria-live':'polite'}),convention=el('span',{class:'muted'});
    const coverage=el('select',{'aria-label':t('coverage')},...['center','touch'].map(value=>el('option',{value,text:t('coverage.'+value)})));
    const templateSize=el('input',{type:'number',min:0,max:10000,value:15,step:'any','aria-label':t('templateSize')});
    const convertDisplay=()=>{converted.textContent=distance(Number(converter.value)||0,primary);convention.textContent=t('conversion.'+(scene()?.grid.conversion??'table'));};
    converter.oninput=convertDisplay;
    tools.append(coverage,el('label',{},t('templateSize'),templateSize),el('label',{},t('converter'),converter),converted,convention);
    sceneSelect.onchange=()=>{chosenScene=sceneSelect.value;selected=null;draw();tracker?.update(board);syncDisabled();};
    const copy=el('button',{type:'button',text:t('copyResult'),onclick:async()=>{try{await navigator.clipboard.writeText(converted.textContent);status.textContent=t('done');}catch{status.textContent=t('boardFailed');}}});tools.append(copy);
    if(role!=='spectator')tools.append(button('shareTemplate',()=>{if(measurement&&scene()&&role!=='spectator')send({type:'saveTemplate',template:measurement});}));
    const templateList=el('div',{class:'template-list'});
    const unitLabel=el('label',{class:'vtt-unit'},t('primaryUnit'),unit);
    root.append(el('h2',{class:'sr-only',text:t('board')}),el('div',{class:'vtt-topbar'},notice,sceneSelect,unitLabel,zoomBar),
      el('div',{class:'vtt-body'},palette,el('div',{class:'vtt-center board-stage'},viewport,summary,pingStatus,status),sidebar));
    panel('tokens').append(roster,editor);panel('measure').append(tools,templateList);
    async function loadImage(assetId){
      if(images.has(assetId))return;images.set(assetId,null);
      const generation=imageGeneration;
      try{const blob=await api('GET',endpoint+'/maps/'+encodeURIComponent(assetId),undefined,true);if(!current())return;
        if(generation!==imageGeneration)return;
        const url=URL.createObjectURL(blob);urls.add(url);const image=new Image();image.onload=()=>{if(current()&&generation===imageGeneration){images.set(assetId,image);draw();}};image.src=url;
      }catch{if(current()&&generation===imageGeneration){images.delete(assetId);status.textContent=t('boardFailed');}}
    }
    function draw(){
      if(!current())return;convertDisplay();const s=scene();
      sceneSelect.replaceChildren(...(board?.scenes??[]).map(v=>el('option',{value:v.id,selected:v.id===s?.id,text:v.name})));
      roster.replaceChildren(...tokens().map(v=>{
        const b=el('button',{type:'button','aria-pressed':v.id===selected,text:`${v.name} · ${distance(v.speed.value,v.speed.unit)}${v.hidden?' · '+t('hidden'):''}`,onclick:()=>{selected=v.id;draw();tracker?.update(board);syncDisabled();}});
        return el('div',{},b,el('span',{class:v.spent>convert(v.speed.value,v.speed.unit,s.grid.distance.unit,s.grid.conversion)?'error':'muted',text:distance(v.spent)+' / '+distance(v.speed.value,v.speed.unit)}));
      }));
      templateList.replaceChildren(...(board?.templates??[]).filter(v=>v.sceneId===s?.id).map(v=>el('div',{},t('tool.'+v.shape)+' '+distance(v.size.value,v.size.unit), (role==='gm'||v.ownerId===account)?button('remove',()=>send({type:'removeTemplate',templateId:v.id})):null)));
      if(!s){summary.textContent=t('sceneEmpty');return;}
      summary.textContent=t('boardKeys')+' '+t('gridScale')+': '+distance(s.grid.distance.value)+' · '+t('conversion.'+s.grid.conversion);
      paint();
    }
    function paint(){
      if(!current())return;const s=scene();if(!s)return;
      canvas.style.width=s.width+'px';canvas.style.height=s.height+'px';
      if(view.fitted!==s.id){view.fitted=s.id;(globalThis.requestAnimationFrame??setTimeout)(()=>{if(current())fit();});}
      const renderScale=Math.min(1,2048/s.width,2048/s.height);canvas.width=Math.round(s.width*renderScale);canvas.height=Math.round(s.height*renderScale);const ctx=canvas.getContext('2d');if(!ctx)return;ctx.scale(renderScale,renderScale);
      ctx.fillStyle='#292b28';ctx.fillRect(0,0,s.width,s.height);
      if(s.mapId){if(images.get(s.mapId))ctx.drawImage(images.get(s.mapId),0,0,s.width,s.height);else loadImage(s.mapId);}
      ctx.save();ctx.strokeStyle=s.grid.color;ctx.globalAlpha=s.grid.opacity;ctx.lineWidth=1;
      const g=s.grid;
      if(g.kind==='square'){ctx.beginPath();for(let x=((g.offset.x%g.pixels)+g.pixels)%g.pixels;x<s.width;x+=g.pixels){ctx.moveTo(x,0);ctx.lineTo(x,s.height);}for(let y=((g.offset.y%g.pixels)+g.pixels)%g.pixels;y<s.height;y+=g.pixels){ctx.moveTo(0,y);ctx.lineTo(s.width,y);}ctx.stroke();}
      else if(g.kind.startsWith('hex')){const radius=g.pixels/2;let n=0;for(let q=-256;q<512&&n<10000;q++)for(let r=-256;r<512&&n<10000;r++){const p=pixel({x:q,y:r},g);if(p.x<-radius||p.y<-radius||p.x>s.width+radius||p.y>s.height+radius)continue;n++;ctx.beginPath();for(let i=0;i<7;i++){const a=(i*60+(g.kind==='hexPointy'?30:0))*Math.PI/180;const x=p.x+radius*Math.cos(a),y=p.y+radius*Math.sin(a);i?ctx.lineTo(x,y):ctx.moveTo(x,y);}ctx.stroke();}}
      ctx.restore();ctx.fillStyle='rgba(160,100,40,0.4)';for(const p of s.difficult){const c=pixel(p,g);ctx.fillRect(c.x-g.pixels/2,c.y-g.pixels/2,g.pixels,g.pixels);}
      for(const v of tokens()){const p=pixel(v.position,g),r=v.size*g.pixels/2;
        if(v.artwork&&v.portraitId){const image=images.get(v.portraitId);if(image){const scale=Math.min(r*2/image.width,r*2/image.height);ctx.drawImage(image,p.x-image.width*scale/2,p.y-image.height*scale/2,image.width*scale,image.height*scale);if(v.id===selected){ctx.strokeStyle='#f6db8b';ctx.strokeRect(p.x-r,p.y-r,r*2,r*2);}}else loadImage(v.portraitId);continue;}
        ctx.beginPath();ctx.arc(p.x,p.y,r,0,2*Math.PI);ctx.fillStyle=v.hidden?'#736151':v.ownerId===account?'#245c68':'#803c39';ctx.fill();ctx.lineWidth=v.id===selected?4:2;ctx.strokeStyle=v.id===selected?'#f6db8b':'#f3e7cb';ctx.stroke();
        if(v.portraitId){const image=images.get(v.portraitId);if(image){ctx.save();ctx.beginPath();ctx.arc(p.x,p.y,r-2,0,2*Math.PI);ctx.clip();ctx.drawImage(image,p.x-r,p.y-r,r*2,r*2);ctx.restore();}else loadImage(v.portraitId);}
        ctx.font=`${Math.max(12,g.pixels/4)}px Georgia`;ctx.textAlign='center';ctx.fillStyle='#fff8e6';ctx.fillText(v.name,p.x,p.y+r+16);
        const effects=board?.combat?.conditions?.filter(c=>c.tokenId===v.id)??[];if(effects.length){ctx.beginPath();ctx.arc(p.x+r,p.y-r,12,0,Math.PI*2);ctx.fillStyle=effects.some(c=>c.expired)?'#b94d39':'#1f5f8b';ctx.fill();ctx.fillStyle='#ffffff';ctx.font='14px Georgia';ctx.fillText(number(effects.length),p.x+r,p.y-r+5);}
      }
      const areas=[...(board?.templates??[]).filter(a=>a.sceneId===s.id),...(measurement?[measurement]:[])];
      for(const area of areas){
        const a=pixel(area.origin,g),r=convert(area.size.value,area.size.unit,g.distance.unit,g.conversion)/g.distance.value*g.pixels;
        ctx.save();ctx.translate(a.x,a.y);ctx.rotate(area.angle*Math.PI/180);ctx.strokeStyle='#f6db8b';ctx.fillStyle='rgba(246,219,139,.16)';ctx.lineWidth=2;ctx.beginPath();
        if(['circle','cylinder'].includes(area.shape))ctx.arc(0,0,r,0,2*Math.PI);
        else if(area.shape==='cone'){ctx.moveTo(0,0);ctx.arc(0,0,r,-Math.PI/6,Math.PI/6);ctx.closePath();}
        else ctx.rect(0,-(area.shape==='line'?g.pixels:r)/2,r,area.shape==='line'?g.pixels:r);
        ctx.fill();ctx.stroke();ctx.restore();
        if(g.kind!=='none') {let count=0;const min=snap({x:Math.max(0,a.x-r-g.pixels),y:Math.max(0,a.y-r-g.pixels)},g),max=snap({x:Math.min(s.width,a.x+r+g.pixels),y:Math.min(s.height,a.y+r+g.pixels)},g);
          for(let x=Math.min(min.x,max.x)-2;x<=Math.max(min.x,max.x)+2&&count<10000;x++)for(let y=Math.min(min.y,max.y)-2;y<=Math.max(min.y,max.y)+2&&count<10000;y++){count++;if(!templateContains({x,y},area,g))continue;const c=pixel({x,y},g);ctx.fillStyle='rgba(246,219,139,.22)';ctx.fillRect(c.x-g.pixels/3,c.y-g.pixels/3,g.pixels*2/3,g.pixels*2/3);}}
      }
      if(preview&&start){const a=pixel(start,g),b=pixel(preview,g);ctx.beginPath();ctx.strokeStyle='#f6db8b';ctx.lineWidth=3;ctx.moveTo(a.x,a.y);ctx.lineTo(b.x,b.y);ctx.stroke();
        // Drag ghost: where the selected token would land. The server still validates the path.
        const v=selectedToken();if(tool.value==='move'&&movable(v)&&(preview.x!==start.x||preview.y!==start.y)){const r=v.size*g.pixels/2;ctx.save();ctx.globalAlpha=.55;ctx.beginPath();ctx.arc(b.x,b.y,r,0,2*Math.PI);ctx.fillStyle=v.ownerId===account?'#245c68':'#803c39';ctx.fill();ctx.setLineDash?.([6,4]);ctx.lineWidth=3;ctx.strokeStyle='#f6db8b';ctx.stroke();ctx.restore();ctx.font=`${Math.max(14,g.pixels/3.5)}px Georgia`;ctx.textAlign='center';ctx.fillStyle='#fff8e6';ctx.strokeStyle='#1b1712';ctx.lineWidth=4;ctx.strokeText?.(summary.textContent,b.x,b.y-r-10);ctx.fillText(summary.textContent,b.x,b.y-r-10);}}
      const fog=board?.fog?.find(f=>f.sceneId===s.id);
      if(fog){const visible=new Set(fog.visible);ctx.fillStyle=role==='gm'?'rgba(0,0,0,.55)':'#000';for(let y=0;y<fog.rows;y++)for(let x=0;x<fog.columns;x++)if(!visible.has(y*fog.columns+x))ctx.fillRect(x*s.width/fog.columns,y*s.height/fog.rows,Math.ceil(s.width/fog.columns),Math.ceil(s.height/fog.rows));}
      if(role==='gm'){const config=board?.fogEditor?.find(f=>f.sceneId===s.id);for(const w of config?.walls??[]){ctx.strokeStyle=w.open?'#4c9e84':w.door?'#dcb46d':'#e7d8b5';ctx.lineWidth=3;ctx.beginPath();ctx.moveTo(w.a.x,w.a.y);ctx.lineTo(w.b.x,w.b.y);ctx.stroke();}if(polygon.length){ctx.strokeStyle='#f6db8b';ctx.beginPath();polygon.forEach((p,i)=>i?ctx.lineTo(p.x,p.y):ctx.moveTo(p.x,p.y));ctx.stroke();}}
      const now=Date.now();
      for(const p of pings){const age=1-(p.until-now)/2500;ctx.save();ctx.strokeStyle=p.mine?'#7fd1e8':'#f6db8b';for(let ring=0;ring<3;ring++){const phase=(age*1.5+ring/3)%1;ctx.globalAlpha=Math.max(0,1-phase);ctx.lineWidth=4;ctx.beginPath();ctx.arc(p.x,p.y,g.pixels*(0.3+phase*1.6),0,2*Math.PI);ctx.stroke();}ctx.globalAlpha=1;ctx.font=`${Math.max(14,g.pixels/3.5)}px Georgia`;ctx.textAlign='center';ctx.fillStyle='#fff8e6';ctx.strokeStyle='#1b1712';ctx.lineWidth=4;ctx.strokeText?.(p.name,p.x,p.y-g.pixels);ctx.fillText(p.name,p.x,p.y-g.pixels);ctx.restore();}
    }
    const coordinate=e=>{const box=canvas.getBoundingClientRect();return {x:(e.clientX-box.left)*scene().width/box.width,y:(e.clientY-box.top)*scene().height/box.height};};
    const movable=v=>v&&(role==='gm'||v.ownerId===account)&&role!=='spectator';
    root.addEventListener('keydown',e=>{if(e.code==='Space'&&e.target===canvas){spaceHeld=true;e.preventDefault();}});
    root.addEventListener('keyup',e=>{if(e.code==='Space')spaceHeld=false;});
    canvas.onpointerdown=e=>{if(!scene())return;const p=coordinate(e);if(tool.value==='ping'||(e.altKey&&tool.value!=='polygon')){sendPing(p);return;}start=snap(p,scene().grid);preview=start;calibration=tool.value==='calibrate'?p:null;
      if(tool.value==='move'){const hit=tokens().findLast(v=>{const q=pixel(v.position,scene().grid);return Math.hypot(q.x-p.x,q.y-p.y)<v.size*scene().grid.pixels/2;});selected=hit?.id??selected;start=selectedToken()?.position??start;}
      if(role==='gm'&&['reveal','cover','brushReveal','brushCover'].includes(tool.value))calibration=p;
      if(role==='gm'&&tool.value==='polygon'){if(polygon.length<64)polygon.push(p);}
      canvas.setPointerCapture?.(e.pointerId);draw();tracker?.update(board);syncDisabled();};
    canvas.onpointercancel=()=>{start=null;preview=null;calibration=null;paint();};
    canvas.onpointermove=e=>{if(!start||!scene())return;preview=snap(coordinate(e),scene().grid);
      if(['circle','cone','square','line','cylinder'].includes(tool.value)){const a=pixel(start,scene().grid),b=pixel(preview,scene().grid);measurement={id:id(),sceneId:scene().id,ownerId:account,shape:tool.value,origin:start,angle:Math.atan2(b.y-a.y,b.x-a.x)*180/Math.PI,size:{value:Number(templateSize.value)||0,unit:primary},coverage:coverage.value,hidden:false};}
      try{summary.textContent=distance(cost(start,path(start,preview,scene().grid),scene(),selectedToken()?.diagonals??0));}catch{summary.textContent=t('pathLimit');}paint();};
    canvas.onpointerup=e=>{if(!start||!scene())return;const end=snap(coordinate(e),scene().grid),s=scene();
      if(tool.value==='move'&&movable(selectedToken())&&online){try{const steps=path(start,end,s.grid);if(steps.length)send({type:'move',tokenId:selected,path:steps});}catch{status.textContent=t('pathLimit');}}
      if(tool.value==='terrain'&&role==='gm'&&online){const cells=s.difficult.some(p=>p.x===end.x&&p.y===end.y)?s.difficult.filter(p=>p.x!==end.x||p.y!==end.y):[...s.difficult,end];send({type:'saveScene',scene:{...s,difficult:cells}});}
      if(tool.value==='calibrate'&&role==='gm'&&online){const a=calibration,b=coordinate(e),pixels=Math.abs(b.x-a.x)/2;if(pixels>=8)send({type:'saveScene',scene:{...s,grid:{...s.grid,pixels,offset:a}}});}
      if(role==='gm'&&online&&calibration&&['reveal','cover','brushReveal','brushCover'].includes(tool.value)){const b=coordinate(e),a=calibration;let points;
        if(tool.value.startsWith('brush')){const radius=Math.max(1,Number(advanced?.brush.value)||25);const angle=Math.atan2(b.y-a.y,b.x-a.x);points=[];for(let i=0;i<=12;i++){const v=angle-Math.PI/2+i*Math.PI/12;points.push({x:b.x+Math.cos(v)*radius,y:b.y+Math.sin(v)*radius});}for(let i=0;i<=12;i++){const v=angle+Math.PI/2+i*Math.PI/12;points.push({x:a.x+Math.cos(v)*radius,y:a.y+Math.sin(v)*radius});}}
        else points=[a,{x:b.x,y:a.y},b,{x:a.x,y:b.y}];
        points=points.map(p=>({x:Math.max(0,Math.min(s.width,p.x)),y:Math.max(0,Math.min(s.height,p.y))}));advanced?.paint(points,['reveal','brushReveal'].includes(tool.value));}
      start=null;calibration=null;if(tool.value==='move'||tool.value==='terrain'){preview=null;draw();}};
    canvas.onkeydown=e=>{
      if(e.key==='+'||e.key==='='){e.preventDefault();zoomAt(1.25);return;}
      if(e.key==='-'){e.preventDefault();zoomAt(1/1.25);return;}
      if(e.key==='0'){e.preventDefault();fit();return;}
      if((e.key==='p'||e.key==='P')&&scene()){const v=selectedToken();sendPing(v?pixel(v.position,scene().grid):{x:scene().width/2,y:scene().height/2});return;}
      if(!['ArrowUp','ArrowDown','ArrowLeft','ArrowRight'].includes(e.key)||!online||!movable(selectedToken()))return;e.preventDefault();const p={...selectedToken().position},step=scene().grid.kind==='none'?scene().grid.pixels:1;p.x+=e.key==='ArrowLeft'?-step:e.key==='ArrowRight'?step:0;p.y+=e.key==='ArrowUp'?-step:e.key==='ArrowDown'?step:0;send({type:'move',tokenId:selected,path:[p]});};
    // Position fields provide a click/tap alternative to dragging and arrow keys.
    const moveForm=el('form',{onsubmit:e=>e.preventDefault()}),mx=field(moveForm,'positionX',0,'number'),my=field(moveForm,'positionY',0,'number');
    const moveButton=button('tool.move',()=>{const v=selectedToken();if(!movable(v))return;try{send({type:'move',tokenId:v.id,path:path(v.position,{x:read(mx),y:read(my)},scene().grid)});}catch{status.textContent=t('pathLimit');}});
    moveForm.append(moveButton);if(role!=='spectator')editor.append(moveForm);
    function sceneEditor(){
      const form=el('form',{onsubmit:e=>e.preventDefault()}),name=field(form,'sceneName',''),width=field(form,'mapWidth',1000,'number'),height=field(form,'mapHeight',700,'number');
      const kind=field(form,'gridKind','square','text',['square','hexFlat','hexPointy','none'].map(v=>[v,t('grid.'+v)]));
      const pixels=field(form,'cellPixels',50,'number'),ox=field(form,'offsetX',0,'number'),oy=field(form,'offsetY',0,'number');
      const scale=field(form,'gridScale',5,'number'),units=field(form,'units','ft','text',unitOptions);
      const diagonal=field(form,'diagonal','equal','text',['equal','alternating','euclidean'].map(v=>[v,t('diagonal.'+v)]));
      const convention=field(form,'conversion','table','text',['table','exact'].map(v=>[v,t('conversion.'+v)]));
      const color=field(form,'gridColor','#cab582','color'),opacity=field(form,'opacity',0.4,'number'),free=field(form,'freeMove',true,'checkbox');free.checked=true;
      let editing=null,mapId=null;
      form.append(button('editScene',()=>{const s=scene();if(!s)return;editing=s.id;mapId=s.mapId;name.value=s.name;width.value=s.width;height.value=s.height;kind.value=s.grid.kind;pixels.value=s.grid.pixels;ox.value=s.grid.offset.x;oy.value=s.grid.offset.y;scale.value=s.grid.distance.value;units.value=s.grid.distance.unit;diagonal.value=s.grid.diagonal;convention.value=s.grid.conversion;color.value=s.grid.color;opacity.value=s.grid.opacity;free.checked=s.freeMove;}));
      form.append(button('newScene',()=>{editing=null;mapId=null;name.value='';}));
      form.append(button('saveScene',()=>{const original=board?.scenes.find(s=>s.id===editing);return send({type:'saveScene',scene:{id:editing??id(),name:name.value.trim(),width:read(width),height:read(height),mapId,grid:{kind:kind.value,pixels:read(pixels),offset:{x:read(ox),y:read(oy)},color:color.value,opacity:read(opacity),distance:{value:read(scale),unit:units.value},diagonal:diagonal.value,conversion:convention.value},freeMove:free.checked,difficult:original?.difficult??[]}});}));
      form.append(button('activateScene',()=>scene()&&send({type:'activateScene',sceneId:scene().id})));
      const upload=field(form,'uploadMap','','file');upload.accept='image/png,image/jpeg,image/webp,.uvtt,.dd2vtt';
      form.append(button('uploadMap',async()=>{const file=upload.files?.[0];if(!file||busy)return;busy=true;syncDisabled();try{
        if(/\.(uvtt|dd2vtt)$/i.test(file.name)){if(file.size>46*1024*1024)throw Error();const document=JSON.parse(await file.text());await api('POST',endpoint+'/uvtt',{operationId:id(),baseRevision:board.revision,sceneId:id(),name:name.value.trim()||file.name,document});await refresh();}
        else{if(file.size>32*1024*1024)throw Error();const asset=await api('POST',endpoint+'/maps',file);mapId=asset.id;width.value=asset.width;height.value=asset.height;status.textContent=t('mapReady');}
      }catch(e){showError(e);}finally{busy=false;syncDisabled();}}));
      panel('scene').append(el('h3',{text:t('sceneEditor')}),form);
    }
    function tokenEditor(){
      const form=el('form',{onsubmit:e=>e.preventDefault()}),name=field(form,'tokenName',''),owner=field(form,'tokenOwner',account,'text',seats.filter(s=>s.role!=='spectator').map(s=>[s.accountId,s.displayName]));
      const x=field(form,'positionX',1,'number'),y=field(form,'positionY',1,'number'),size=field(form,'tokenSize',1,'number'),speed=field(form,'speed',30,'number'),units=field(form,'units','ft','text',unitOptions),hidden=field(form,'hidden',false,'checkbox');
      let editing=null,portraitId=null,artwork=false;
      form.append(button('editToken',()=>{const v=selectedToken();if(!v)return;editing=v.id;artwork=!!v.artwork;name.value=v.name;owner.value=v.ownerId;x.value=v.position.x;y.value=v.position.y;size.value=v.size;speed.value=v.speed.value;units.value=v.speed.unit;hidden.checked=v.hidden;portraitId=v.portraitId;}));
      form.append(button('newToken',()=>{editing=null;portraitId=null;artwork=false;name.value='';}));
      const portrait=field(form,'portrait','','file');portrait.accept='image/png,image/jpeg,image/webp';
      form.append(button('portrait',async()=>{const file=portrait.files?.[0];if(!file||file.size>32*1024*1024)return;try{portraitId=(await api('POST',endpoint+'/maps',file)).id;status.textContent=t('mapReady');}catch(e){showError(e);}}));
      form.append(button('saveToken',()=>scene()&&send({type:'saveToken',token:{id:editing??id(),sceneId:scene().id,ownerId:owner.value,name:name.value.trim(),position:{x:read(x),y:read(y)},size:read(size),speed:{value:read(speed),unit:units.value},hidden:hidden.checked,portraitId,characterId:null,spent:0,diagonals:0,artwork}})));
      form.append(button('remove',async()=>{if(selected&&await confirm(t('remove'),[t('removeTokenConfirm')],t('remove')))send({type:'removeToken',tokenId:selected});}));
      editor.append(el('details',{},el('summary',{text:t('tokenEditor')}),form));
    }
    function advancedEditor(){
      const section=(key,...nodes)=>{if(key==='fogEditor'){panel('fog').append(el('h3',{text:t(key)}),...nodes);return;}panel('library').append(el('details',{open:key==='monsterLibrary'},el('summary',{text:t(key)}),...nodes));};
      const form=()=>el('form',{onsubmit:e=>e.preventDefault()});
      const config=()=>clone(board?.fogEditor?.find(f=>f.sceneId===scene()?.id)??{sceneId:scene()?.id,enabled:true,vision:false,strokes:[],walls:[],eyes:[],lights:[]});
      const fogForm=form(),enabled=field(fogForm,'fogEnabled',true,'checkbox'),vision=field(fogForm,'visionEnabled',false,'checkbox'),brush=field(fogForm,'brushPixels',25,'number');enabled.checked=true;
      const saveFog=fog=>scene()&&send({type:'saveFog',fog});
      const paint=(points,reveal)=>{const f=config();f.enabled=true;f.strokes.push({points,reveal});return saveFog(f);};
      fogForm.append(button('save',()=>{const f=config();f.enabled=enabled.checked;f.vision=vision.checked;return saveFog(f);}),button('undoFog',()=>{const f=config();f.strokes.pop();return saveFog(f);}),button('finishPolygon',()=>{if(polygon.length>=3){const points=polygon;polygon=[];return paint(points,true);}}),button('cancel',()=>{polygon=[];draw();}),button('revealAll',()=>scene()&&paint([{x:0,y:0},{x:scene().width,y:0},{x:scene().width,y:scene().height},{x:0,y:scene().height}],true)),button('coverAll',async()=>{if(scene()&&await confirm(t('coverAll'),[t('coverAllConfirm')],t('coverAll'))){const f=config();f.enabled=true;f.strokes=[];return saveFog(f);}}));
      const x1=field(fogForm,'positionX',0,'number'),y1=field(fogForm,'positionY',0,'number'),x2=field(fogForm,'endX',100,'number'),y2=field(fogForm,'endY',100,'number'),door=field(fogForm,'door',false,'checkbox');
      fogForm.append(button('addWall',()=>{const f=config();f.walls.push({a:{x:read(x1),y:read(y1)},b:{x:read(x2),y:read(y2)},door:door.checked,open:false});return saveFog(f);}),button('revealRectangle',()=>paint([{x:read(x1),y:read(y1)},{x:read(x2),y:read(y1)},{x:read(x2),y:read(y2)},{x:read(x1),y:read(y2)}],true)));
      const range=field(fogForm,'visionRange',60,'number'),rangeUnit=field(fogForm,'units','ft','text',unitOptions),dark=field(fogForm,'darkvision',false,'checkbox'),rangeOutput=el('output');
      const rangeDisplay=()=>rangeOutput.textContent=distance(read(range),rangeUnit.value);range.oninput=rangeDisplay;rangeUnit.onchange=rangeDisplay;fogForm.append(rangeOutput);
      fogForm.append(button('setVision',()=>{const v=selectedToken();if(!v)return;const f=config();f.eyes=f.eyes.filter(e=>e.tokenId!==v.id);f.eyes.push({tokenId:v.id,range:{value:read(range),unit:rangeUnit.value},darkvision:dark.checked});return saveFog(f);}),button('addLight',()=>{const f=config();f.lights.push({position:{x:read(x1),y:read(y1)},range:{value:read(range),unit:rangeUnit.value}});return saveFog(f);}));
      fogForm.append(button('importGeometry',()=>scene()&&send({type:'importVision',sceneId:scene().id})));
      const walls=el('div'),lights=el('div'),eyes=el('div');section('fogEditor',el('p',{class:'muted',text:t('fogHelp')}),fogForm,walls,lights,eyes);
      const libraryForm=form(),name=field(libraryForm,'tokenName',''),hp=field(libraryForm,'hitPoints',1,'number'),ac=field(libraryForm,'armorClass',10,'number'),speed=field(libraryForm,'speed',30,'number'),size=field(libraryForm,'tokenSize',1,'number');
      const monsterUnit=field(libraryForm,'units','ft','text',unitOptions),monsterDistance=el('output');const showMonsterDistance=()=>monsterDistance.textContent=distance(read(speed),monsterUnit.value);speed.oninput=showMonsterDistance;monsterUnit.onchange=showMonsterDistance;libraryForm.append(monsterDistance);
      const stat=el('textarea',{rows:8,'aria-label':t('statBlock')}),sourceText=el('pre',{class:'source-attribution'});libraryForm.append(el('label',{},t('statBlock'),stat),sourceText);
      let draftId=null,draftSource=null,draftPortrait=null;
      const fill=m=>{draftId=m.id;draftSource=m.source;draftPortrait=m.portraitId;name.value=m.name;hp.value=m.hp;ac.value=m.ac;speed.value=m.speed.value;monsterUnit.value=m.speed.unit;showMonsterDistance();size.value=m.size;stat.value=m.statBlock;sourceText.textContent=m.source?`${m.source.author}\n${m.source.edition}\n${m.source.license}\n${m.source.url}`:'';};
      libraryForm.append(button('newMonster',()=>fill({id:null,name:'',hp:1,ac:10,speed:{value:30,unit:'ft'},size:1,statBlock:'',source:null,portraitId:null})),button('saveMonster',()=>send({type:'saveMonster',monster:{id:draftId??id(),name:name.value.trim(),hp:read(hp),ac:read(ac),speed:{value:read(speed),unit:monsterUnit.value},size:read(size),statBlock:stat.value,source:draftSource,portraitId:draftPortrait}})));
      const monsterList=el('div'),encounterList=el('div'),members=el('div'),encounterForm=form(),encounterName=field(encounterForm,'encounterName','');let encounterId=null,spawns=[];
      const px=field(encounterForm,'positionX',1,'number'),py=field(encounterForm,'positionY',1,'number'),concealed=field(encounterForm,'hidden',false,'checkbox');
      const renderSpawns=()=>members.replaceChildren(...spawns.map((m,i)=>el('div',{},board?.monsters?.find(v=>v.id===m.monsterId)?.name??t('remove'),button('remove',()=>{spawns.splice(i,1);renderSpawns();}))));
      encounterForm.append(button('newEncounter',()=>{encounterId=null;encounterName.value='';spawns=[];renderSpawns();}),button('saveEncounter',()=>send({type:'saveEncounter',encounter:{id:encounterId??id(),name:encounterName.value.trim(),members:spawns}})));
      section('monsterLibrary',libraryForm,monsterList);section('encounterLibrary',encounterForm,members,encounterList);
      const assetForm=form(),assetName=field(assetForm,'assetName',''),file=field(assetForm,'importComputer','','file');file.accept='image/png,image/jpeg,image/webp';
      const assetSize=field(assetForm,'tokenSize',1,'number'),assetHidden=field(assetForm,'hidden',false,'checkbox');
      const gallery=el('div',{class:'asset-gallery'}),searchForm=form(),provider=field(searchForm,'catalog','assets','text',[['assets',t('assetLibrary')],['monsters',t('monsterLibrary')]]),query=field(searchForm,'search',''),results=el('div',{'aria-live':'polite',class:'asset-results'});
      const localImport=async()=>{const f=file.files?.[0];if(!f||!online||busy||f.size>32*1024*1024)return;busy=true;syncDisabled();let a;try{a=await api('POST',endpoint+'/maps',f);}catch(e){showError(e);}finally{busy=false;syncDisabled();}if(a&&current())return send({type:'saveAsset',asset:{id:a.id,name:assetName.value.trim()||f.name.slice(0,100),source:null}});};
      assetForm.append(button('importComputer',localImport));
      const lookup=async(entry)=>{if(!online||busy)return;busy=true;syncDisabled();status.textContent=t('working');let response;try{response=await api('POST',endpoint+'/catalog',{kind:provider.value,query:query.value.trim(),entry:entry??null});if(!current())return;if(response.monster){fill(response.monster);status.textContent=t('reviewImport');}else if(!response.asset){results.replaceChildren(...response.entries.map(item=>el('div',{},el('span',{text:item.name}),item.source?el('small',{text:`${item.source.author} · ${item.source.license}`}):null,button('import',()=>{if(provider.value==='assets')assetName.value=item.name.slice(0,100);return lookup(item.id);}))));if(!response.entries.length)results.append(el('p',{text:t('catalogEmpty')}));status.textContent=t('done');}}catch(e){showError(e);}finally{busy=false;syncDisabled();}if(response?.asset&&current())return send({type:'saveAsset',asset:{id:response.asset.id,name:assetName.value.trim()||t('assetLibrary'),source:response.source}});};
      searchForm.append(el('p',{class:'muted',text:t('catalogPrivacy')}),button('search',()=>lookup()));section('assetLibrary',assetForm,gallery);section('publicCatalog',searchForm,results);
      function updateLibrary(){
        const f=config();enabled.checked=f.enabled;vision.checked=f.vision;rangeDisplay();
        walls.replaceChildren(...f.walls.map((w,i)=>el('div',{},el('span',{text:`${t(w.door?'door':'wall')} ${number(i+1)}`}),w.door?button(w.open?'closeDoor':'openDoor',()=>{const next=config();next.walls[i].open=!next.walls[i].open;return saveFog(next);}):null,button('remove',()=>{const next=config();next.walls.splice(i,1);return saveFog(next);}))));
        lights.replaceChildren(...f.lights.map((l,i)=>el('div',{},t('light')+' '+distance(l.range.value,l.range.unit),button('remove',()=>{const next=config();next.lights.splice(i,1);return saveFog(next);}))));
        eyes.replaceChildren(...f.eyes.map((e,i)=>el('div',{},(board?.tokens.find(v=>v.id===e.tokenId)?.name??'')+' '+distance(e.range.value,e.range.unit),button('remove',()=>{const next=config();next.eyes.splice(i,1);return saveFog(next);}))));
        monsterList.replaceChildren(...(board?.monsters??[]).map(m=>el('div',{},el('strong',{text:m.name}),el('span',{text:distance(m.speed.value,m.speed.unit)}),button('editToken',()=>fill(m)),button('addToEncounter',()=>{spawns.push({monsterId:m.id,position:{x:read(px),y:read(py)},hidden:concealed.checked});renderSpawns();}),button('remove',async()=>{if(await confirm(t('remove'),[m.name],t('remove')))return send({type:'removeLibrary',id:m.id,kind:'monster'});}))));
        encounterList.replaceChildren(...(board?.encounters??[]).map(e=>el('div',{},el('strong',{text:e.name}),button('editToken',()=>{encounterId=e.id;encounterName.value=e.name;spawns=clone(e.members);renderSpawns();}),button('deployEncounter',async()=>{if(scene()&&await confirm(t('deployEncounter'),[e.name],t('deployEncounter')))return send({type:'deployEncounter',sceneId:scene().id,encounterId:e.id});}),button('remove',async()=>{if(await confirm(t('remove'),[e.name],t('remove')))return send({type:'removeLibrary',id:e.id,kind:'encounter'});}))));
        gallery.replaceChildren(...(board?.assets??[]).map(a=>{const image=el('img',{alt:a.name,width:80,height:80,loading:'lazy'});thumbnailFor(a.id).then(url=>{if(current()&&url)image.src=url;}).catch(()=>{});return el('div',{},image,el('strong',{text:a.name}),a.source?el('small',{text:`${a.source.author} · ${a.source.license} · ${a.source.url}`}):null,button('placeAsset',()=>scene()&&send({type:'saveToken',token:{id:id(),sceneId:scene().id,ownerId:account,name:a.name,position:{x:read(px),y:read(py)},size:read(assetSize),speed:{value:0,unit:'ft'},hidden:assetHidden.checked,portraitId:a.id,characterId:null,spent:0,diagonals:0,artwork:true}})),button('usePortrait',()=>{draftPortrait=a.id;status.textContent=t('reviewImport');}));}));renderSpawns();
      }
      return {paint,brush,update:updateLibrary};
    }
    if(role==='gm'){sceneEditor();tokenEditor();advanced=advancedEditor();}
    const tracker=window.ChronexusCombat?.create({el,t,role,account,send,getBoard:()=>board,getSelected:()=>selected,getScene:scene,field,button,confirm});if(tracker)panel('combat').append(tracker.root);
    function update(value){if(!current()||(board&&value.revision<board.revision))return;if(!board||value.activeScene!==board.activeScene||(role!=='gm'&&JSON.stringify(value.fog)!==JSON.stringify(board.fog))){imageGeneration++;images.clear();for(const url of urls)URL.revokeObjectURL(url);urls.clear();}board=value;if(chosenScene&&!value.scenes.some(s=>s.id===chosenScene))chosenScene=null;if(selected&&!tokens().some(v=>v.id===selected))selected=null;if(measurement&&measurement.sceneId!==scene()?.id)measurement=null;draw();tracker?.update(value);advanced?.update();syncDisabled();}
    function connected(value){online=value;notice.textContent=t(value?'liveConnected':'liveReconnecting');syncDisabled();}
    function dispose(){disposed=true;pings.length=0;for(const url of urls)URL.revokeObjectURL(url);urls.clear();images.clear();for(const url of thumbUrls)URL.revokeObjectURL(url);thumbUrls.clear();thumbnails.clear();}
    applyView();
    return {root,update,connected,dispose,send,refresh,get snapshot(){return board;},get selected(){return selected;},distance,editor,status,button,field,panel,openPanel,showPing,fit};
  }
  window.ChronexusBoard={create,convert,pixel,snap,path,cost,templateContains,id};
})();
