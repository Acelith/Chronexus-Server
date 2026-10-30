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
  function create({el,t,api,tableId,role,account,seats,confirm,active}) {
    const root=el('section',{class:'board-workspace','aria-label':t('board')});
    const notice=el('p',{class:'note',role:'status',text:t('liveConnecting')});
    const status=el('div',{role:'status'}), tools=el('div',{class:'board-tools'});
    const canvas=el('canvas',{class:'battle-map',width:1000,height:700,tabindex:0,'aria-label':t('boardKeys')});
    const summary=el('p',{class:'muted',text:t('boardKeys')});
    const roster=el('div',{class:'token-roster'}), editor=el('aside',{class:'board-editor'});
    const sceneSelect=el('select',{'aria-label':t('scene')});
    const tool=el('select',{'aria-label':t('tool')},...['move','ruler',...(role==='gm'?['calibrate','terrain']:[]),'circle','cone','square','line','cylinder'].map(v=>el('option',{value:v,text:t('tool.'+v)})));
    let board=null,online=false,busy=false,disposed=false,selected=null,chosenScene=null,start=null,preview=null,measurement=null,calibration=null,pending=null,primary='ft';
    const images=new Map(),urls=new Set(),controls=[];
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
    const button=(key,fn)=>{const b=el('button',{type:'button',text:t(key),onclick:fn});controls.push(b);return b;};
    const syncDisabled=()=>controls.forEach(b=>b.disabled=!online||busy);
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
    tools.append(sceneSelect,tool,coverage,el('label',{},t('templateSize'),templateSize),el('label',{},t('primaryUnit'),unit),el('label',{},t('converter'),converter),converted,convention);
    sceneSelect.onchange=()=>{chosenScene=sceneSelect.value;selected=null;draw();};
    const copy=el('button',{type:'button',text:t('copyResult'),onclick:async()=>{try{await navigator.clipboard.writeText(converted.textContent);status.textContent=t('done');}catch{status.textContent=t('boardFailed');}}});tools.append(copy);
    if(role!=='spectator')tools.append(button('shareTemplate',()=>{if(measurement&&scene()&&role!=='spectator')send({type:'saveTemplate',template:measurement});}));
    const templateList=el('div',{class:'template-list'});
    root.append(el('h2',{text:t('board')}),notice,tools,el('div',{class:'board-layout'},el('div',{class:'board-stage'},canvas,summary,roster,templateList),editor),status);
    async function loadImage(assetId){
      if(images.has(assetId))return;images.set(assetId,null);
      try{const blob=await api('GET',endpoint+'/maps/'+encodeURIComponent(assetId),undefined,true);if(!current())return;
        const url=URL.createObjectURL(blob);urls.add(url);const image=new Image();image.onload=()=>{if(current()){images.set(assetId,image);draw();}};image.src=url;
      }catch{if(current())status.textContent=t('boardFailed');}
    }
    function draw(){
      if(!current())return;convertDisplay();const s=scene();
      sceneSelect.replaceChildren(...(board?.scenes??[]).map(v=>el('option',{value:v.id,selected:v.id===s?.id,text:v.name})));
      roster.replaceChildren(...tokens().map(v=>{
        const b=el('button',{type:'button','aria-pressed':v.id===selected,text:`${v.name} · ${distance(v.speed.value,v.speed.unit)}${v.hidden?' · '+t('hidden'):''}`,onclick:()=>{selected=v.id;draw();}});
        return el('div',{},b,el('span',{class:v.spent>convert(v.speed.value,v.speed.unit,s.grid.distance.unit,s.grid.conversion)?'error':'muted',text:distance(v.spent)+' / '+distance(v.speed.value,v.speed.unit)}));
      }));
      templateList.replaceChildren(...(board?.templates??[]).filter(v=>v.sceneId===s?.id).map(v=>el('div',{},t('tool.'+v.shape)+' '+distance(v.size.value,v.size.unit), (role==='gm'||v.ownerId===account)?button('remove',()=>send({type:'removeTemplate',templateId:v.id})):null)));
      if(!s){summary.textContent=t('sceneEmpty');return;}
      summary.textContent=t('boardKeys')+' '+t('gridScale')+': '+distance(s.grid.distance.value)+' · '+t('conversion.'+s.grid.conversion);
      const renderScale=Math.min(1,2048/s.width,2048/s.height);canvas.width=Math.round(s.width*renderScale);canvas.height=Math.round(s.height*renderScale);const ctx=canvas.getContext('2d');if(!ctx)return;ctx.scale(renderScale,renderScale);
      ctx.fillStyle='#292b28';ctx.fillRect(0,0,s.width,s.height);
      if(s.mapId){if(images.get(s.mapId))ctx.drawImage(images.get(s.mapId),0,0,s.width,s.height);else loadImage(s.mapId);}
      ctx.save();ctx.strokeStyle=s.grid.color;ctx.globalAlpha=s.grid.opacity;ctx.lineWidth=1;
      const g=s.grid;
      if(g.kind==='square'){ctx.beginPath();for(let x=((g.offset.x%g.pixels)+g.pixels)%g.pixels;x<s.width;x+=g.pixels){ctx.moveTo(x,0);ctx.lineTo(x,s.height);}for(let y=((g.offset.y%g.pixels)+g.pixels)%g.pixels;y<s.height;y+=g.pixels){ctx.moveTo(0,y);ctx.lineTo(s.width,y);}ctx.stroke();}
      else if(g.kind.startsWith('hex')){const radius=g.pixels/2;let n=0;for(let q=-256;q<512&&n<10000;q++)for(let r=-256;r<512&&n<10000;r++){const p=pixel({x:q,y:r},g);if(p.x<-radius||p.y<-radius||p.x>s.width+radius||p.y>s.height+radius)continue;n++;ctx.beginPath();for(let i=0;i<7;i++){const a=(i*60+(g.kind==='hexPointy'?30:0))*Math.PI/180;const x=p.x+radius*Math.cos(a),y=p.y+radius*Math.sin(a);i?ctx.lineTo(x,y):ctx.moveTo(x,y);}ctx.stroke();}}
      ctx.restore();ctx.fillStyle='rgba(160,100,40,0.4)';for(const p of s.difficult){const c=pixel(p,g);ctx.fillRect(c.x-g.pixels/2,c.y-g.pixels/2,g.pixels,g.pixels);}
      for(const v of tokens()){const p=pixel(v.position,g),r=v.size*g.pixels/2;ctx.beginPath();ctx.arc(p.x,p.y,r,0,2*Math.PI);ctx.fillStyle=v.hidden?'#736151':v.ownerId===account?'#245c68':'#803c39';ctx.fill();ctx.lineWidth=v.id===selected?4:2;ctx.strokeStyle=v.id===selected?'#f6db8b':'#f3e7cb';ctx.stroke();
        if(v.portraitId){const image=images.get(v.portraitId);if(image){ctx.save();ctx.beginPath();ctx.arc(p.x,p.y,r-2,0,2*Math.PI);ctx.clip();ctx.drawImage(image,p.x-r,p.y-r,r*2,r*2);ctx.restore();}else loadImage(v.portraitId);}
        ctx.font=`${Math.max(12,g.pixels/4)}px Georgia`;ctx.textAlign='center';ctx.fillStyle='#fff8e6';ctx.fillText(v.name,p.x,p.y+r+16);}
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
      if(preview&&start){const a=pixel(start,g),b=pixel(preview,g);ctx.beginPath();ctx.strokeStyle='#f6db8b';ctx.lineWidth=3;ctx.moveTo(a.x,a.y);ctx.lineTo(b.x,b.y);ctx.stroke();}

    }
    const coordinate=e=>{const box=canvas.getBoundingClientRect();return {x:(e.clientX-box.left)*scene().width/box.width,y:(e.clientY-box.top)*scene().height/box.height};};
    const movable=v=>v&&(role==='gm'||v.ownerId===account)&&role!=='spectator';
    canvas.onpointerdown=e=>{if(!scene())return;const p=coordinate(e);start=snap(p,scene().grid);preview=start;calibration=tool.value==='calibrate'?p:null;
      if(tool.value==='move'){const hit=tokens().findLast(v=>{const q=pixel(v.position,scene().grid);return Math.hypot(q.x-p.x,q.y-p.y)<v.size*scene().grid.pixels/2;});selected=hit?.id??selected;start=selectedToken()?.position??start;}
      canvas.setPointerCapture?.(e.pointerId);draw();};
    canvas.onpointermove=e=>{if(!start||!scene())return;preview=snap(coordinate(e),scene().grid);
      if(['circle','cone','square','line','cylinder'].includes(tool.value)){const a=pixel(start,scene().grid),b=pixel(preview,scene().grid);measurement={id:id(),sceneId:scene().id,ownerId:account,shape:tool.value,origin:start,angle:Math.atan2(b.y-a.y,b.x-a.x)*180/Math.PI,size:{value:Number(templateSize.value)||0,unit:primary},coverage:coverage.value,hidden:false};}
      draw();try{summary.textContent=distance(cost(start,path(start,preview,scene().grid),scene(),selectedToken()?.diagonals??0));}catch{summary.textContent=t('pathLimit');}};
    canvas.onpointerup=e=>{if(!start||!scene())return;const end=snap(coordinate(e),scene().grid),s=scene();
      if(tool.value==='move'&&movable(selectedToken())&&online){try{const steps=path(start,end,s.grid);if(steps.length)send({type:'move',tokenId:selected,path:steps});}catch{status.textContent=t('pathLimit');}}
      if(tool.value==='terrain'&&role==='gm'&&online){const cells=s.difficult.some(p=>p.x===end.x&&p.y===end.y)?s.difficult.filter(p=>p.x!==end.x||p.y!==end.y):[...s.difficult,end];send({type:'saveScene',scene:{...s,difficult:cells}});}
      if(tool.value==='calibrate'&&role==='gm'&&online){const a=calibration,b=coordinate(e),pixels=Math.abs(b.x-a.x)/2;if(pixels>=8)send({type:'saveScene',scene:{...s,grid:{...s.grid,pixels,offset:a}}});}
      start=null;calibration=null;if(tool.value==='move'||tool.value==='terrain'){preview=null;draw();}};
    canvas.onkeydown=e=>{if(!['ArrowUp','ArrowDown','ArrowLeft','ArrowRight'].includes(e.key)||!online||!movable(selectedToken()))return;e.preventDefault();const p={...selectedToken().position},step=scene().grid.kind==='none'?scene().grid.pixels:1;p.x+=e.key==='ArrowLeft'?-step:e.key==='ArrowRight'?step:0;p.y+=e.key==='ArrowUp'?-step:e.key==='ArrowDown'?step:0;send({type:'move',tokenId:selected,path:[p]});};
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
      editor.append(el('details',{},el('summary',{text:t('sceneEditor')}),form));
    }
    function tokenEditor(){
      const form=el('form',{onsubmit:e=>e.preventDefault()}),name=field(form,'tokenName',''),owner=field(form,'tokenOwner',account,'text',seats.filter(s=>s.role!=='spectator').map(s=>[s.accountId,s.displayName]));
      const x=field(form,'positionX',1,'number'),y=field(form,'positionY',1,'number'),size=field(form,'tokenSize',1,'number'),speed=field(form,'speed',30,'number'),units=field(form,'units','ft','text',unitOptions),hidden=field(form,'hidden',false,'checkbox');
      let editing=null,portraitId=null;
      form.append(button('editToken',()=>{const v=selectedToken();if(!v)return;editing=v.id;name.value=v.name;owner.value=v.ownerId;x.value=v.position.x;y.value=v.position.y;size.value=v.size;speed.value=v.speed.value;units.value=v.speed.unit;hidden.checked=v.hidden;portraitId=v.portraitId;}));
      form.append(button('newToken',()=>{editing=null;portraitId=null;name.value='';}));
      const portrait=field(form,'portrait','','file');portrait.accept='image/png,image/jpeg,image/webp';
      form.append(button('portrait',async()=>{const file=portrait.files?.[0];if(!file||file.size>32*1024*1024)return;try{portraitId=(await api('POST',endpoint+'/maps',file)).id;status.textContent=t('mapReady');}catch(e){showError(e);}}));
      form.append(button('saveToken',()=>scene()&&send({type:'saveToken',token:{id:editing??id(),sceneId:scene().id,ownerId:owner.value,name:name.value.trim(),position:{x:read(x),y:read(y)},size:read(size),speed:{value:read(speed),unit:units.value},hidden:hidden.checked,portraitId,characterId:null,spent:0,diagonals:0}})));
      form.append(button('remove',async()=>{if(selected&&await confirm(t('remove'),[t('removeTokenConfirm')],t('remove')))send({type:'removeToken',tokenId:selected});}));
      editor.append(el('details',{},el('summary',{text:t('tokenEditor')}),form));
    }
    if(role==='gm'){sceneEditor();tokenEditor();}
    function update(value){if(!current()||(board&&value.revision<board.revision))return;board=value;if(chosenScene&&!value.scenes.some(s=>s.id===chosenScene))chosenScene=null;if(selected&&!tokens().some(v=>v.id===selected))selected=null;draw();}
    function connected(value){online=value;notice.textContent=t(value?'liveConnected':'liveReconnecting');syncDisabled();}
    function dispose(){disposed=true;for(const url of urls)URL.revokeObjectURL(url);urls.clear();images.clear();}
    return {root,update,connected,dispose,send,refresh,get snapshot(){return board;},get selected(){return selected;},distance,editor,status,button,field};
  }
  window.ChronexusBoard={create,convert,pixel,snap,path,cost,templateContains,id};
})();
