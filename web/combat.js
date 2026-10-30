// Explicit table-only combat controls. No roll writes HP or a character sheet.
(function(){
  const conditions=['blinded','charmed','deafened','frightened','grappled','incapacitated','invisible','paralyzed','petrified','poisoned','prone','restrained','stunned','unconscious','exhaustion'];
  function create({el,t,role,account,send,getBoard,getSelected,getScene,field,button,confirm}){
    const root=el('section',{class:'combat-tracker','aria-label':t('combat')}),heading=el('h2',{text:t('combat')}),round=el('p',{role:'status'}),roster=el('ol',{class:'combat-order'}),conditionList=el('ul',{class:'plain'}),logs=el('ol',{class:'combat-log'});
    const pick=el('select',{multiple:true,size:4,'aria-label':t('combatants')}),form=el('form',{onsubmit:e=>e.preventDefault()}),details=el('details',{},el('summary',{text:t('combatControls')}),form);
    const ownedButton=(key,fn)=>{const b=button(key,fn);b.setAttribute('data-owned-only','true');return b;};
    const act=action=>send({type:'combat',action});
    const selected=()=>getBoard()?.tokens.find(v=>v.id===getSelected());
    const mine=()=>selected()?.ownerId===account;
    const amount=field(form,'amount',1,'number'),hp=field(form,'hitPoints',0,'number'),maxHp=field(form,'maxHitPoints',0,'number'),ac=field(form,'armorClass',10,'number'),showHp=field(form,'showHitPoints',false,'checkbox');
    const tokenStatus=field(form,'tokenStatus','');
    const note=role==='gm'?field(form,'gmNote',''):null;
    form.append(ownedButton('loadVitals',()=>{const v=getBoard()?.vitals?.find(v=>v.tokenId===getSelected());if(!v||!mine())return;hp.value=v.hp??0;maxHp.value=v.maxHp??0;ac.value=v.ac??10;showHp.checked=v.showHp;tokenStatus.value=v.status??'';if(note)note.value=v.gmNote??'';}));
    const setVitals=async(delta=null)=>{const token=selected();if(!token||!mine())return;
      const previous=getBoard()?.vitals?.find(v=>v.tokenId===token.id);
      const value=delta===null?Number(hp.value):(previous?.hp??Number(hp.value))+delta;
      if(!await confirm(t('applyHitPoints'),[t('tokenOnly')],t('applyHitPoints')))return;
      return act({type:'setVitals',tokenId:token.id,hp:value,maxHp:delta===null?Number(maxHp.value):previous?.maxHp??Number(maxHp.value),ac:delta===null?Number(ac.value):previous?.ac??null,showHp:delta===null?showHp.checked:previous?.showHp??false,status:delta===null?tokenStatus.value:previous?.status??null,gmNote:role==='gm'?(delta===null?note.value:previous?.gmNote??null):null});
    };
    form.append(ownedButton('saveVitals',()=>setVitals()),ownedButton('damage',()=>setVitals(-Math.abs(Number(amount.value)))),ownedButton('healing',()=>setVitals(Math.abs(Number(amount.value)))));
    const initiative=field(form,'initiative',0,'number'),secret=role==='gm'?field(form,'secretRoll',false,'checkbox'):null;
    form.append(ownedButton('setInitiative',()=>mine()&&act({type:'initiative',tokenId:getSelected(),value:Number(initiative.value),roll:false,secret:secret?.checked??false})),ownedButton('rollInitiative',()=>mine()&&act({type:'initiative',tokenId:getSelected(),value:Number(initiative.value),roll:true,secret:secret?.checked??false})));
    const count=field(form,'diceCount',1,'number'),sides=field(form,'diceSides',20,'number'),modifier=field(form,'modifier',0,'number'),label=field(form,'rollLabel','');
    form.append(ownedButton('rollDice',()=>mine()&&act({type:'roll',tokenId:getSelected(),count:Number(count.value),sides:Number(sides.value),modifier:Number(modifier.value),secret:secret?.checked??false,label:label.value.trim()})));
    const condition=field(form,'condition','prone','text',conditions.map(v=>[v,t('condition.'+v)])),duration=field(form,'roundsDuration',1,'number');
    form.append(button('applyCondition',()=>selected()&&act({type:'setCondition',condition:{id:window.ChronexusBoard.id(),tokenId:getSelected(),name:condition.value,rounds:Number(duration.value)||null,expired:false}})));
    const gm=el('div',{class:'combat-actions'});
    if(role==='gm')gm.append(el('label',{},t('combatants'),pick),button('startCombat',()=>getScene()&&act({type:'start',sceneId:getScene().id,tokenIds:[...pick.selectedOptions].map(v=>v.value)})),button('nextTurn',()=>act({type:'next'})),button('delayTurn',()=>act({type:'delay'})),button('addCombatant',()=>selected()&&act({type:'add',tokenId:getSelected()})),button('removeCombatant',()=>selected()&&act({type:'remove',tokenId:getSelected()})),button('endCombat',async()=>{if(await confirm(t('endCombat'),[t('tokenOnly')],t('endCombat')))act({type:'end'});}));
    root.append(heading,round,gm,roster,el('h3',{text:t('conditions')}),conditionList,role==='spectator'?null:details,el('h3',{text:t('combatLog')}),logs,el('p',{class:'muted',text:t('tokenOnly')}));
    const fmt=v=>new Intl.NumberFormat(window.ChronexusI18n.locale).format(v);
    function update(value){
      const c=value.combat,scene=getScene();
      const chosen=new Set([...pick.selectedOptions].map(v=>v.value));pick.replaceChildren(...value.tokens.filter(v=>v.sceneId===scene?.id).map(v=>el('option',{value:v.id,selected:chosen.has(v.id),text:v.name})));
      round.textContent=c?.running?(c.round?t('combatRound',{round:fmt(c.round)}):t('initiativePhase')):t('combatEmpty');
      roster.replaceChildren(...(c?.order??[]).map((v,index)=>{
        const token=value.tokens.find(t=>t.id===v.tokenId),vitals=value.vitals?.find(t=>t.tokenId===v.tokenId),name=token?.name??'';
        const row=el('li',{'aria-current':c.current===v.tokenId?'step':undefined,class:c.current===v.tokenId?'current-turn':undefined},el('strong',{text:name}),el('span',{text:` · ${v.initiative===null?'—':fmt(v.initiative)}`}),vitals?.status?el('span',{text:' · '+vitals.status}):null,vitals?.hp===undefined?null:el('span',{text:` · ${t('hitPoints')}: ${fmt(vitals.hp)}/${fmt(vitals.maxHp)}`}));
        if(role==='gm'){
          const reorder=offset=>{const ids=c.order.map(v=>v.tokenId),other=index+offset;if(other<0||other>=ids.length)return;[ids[index],ids[other]]=[ids[other],ids[index]];act({type:'order',tokenIds:ids});};
          row.append(button('moveEarlier',()=>reorder(-1)),button('moveLater',()=>reorder(1)));
        }return row;
      }));
      conditionList.replaceChildren(...(c?.conditions??[]).map(v=>el('li',{},el('span',{text:`${value.tokens.find(t=>t.id===v.tokenId)?.name??''}: ${t('condition.'+v.name)}${v.rounds===null?'':` · ${t('combatRound',{round:fmt(v.rounds)})}`}${v.expired?' · '+t('conditionExpired'):''}`}),
        role==='gm'||value.tokens.find(t=>t.id===v.tokenId)?.ownerId===account?button('remove',()=>act({type:'removeCondition',conditionId:v.id})):null)));
      logs.replaceChildren(...(value.combatLog??[]).map(v=>el('li',{},el('strong',{text:t('combatLog.'+v.kind)}),document.createTextNode(` ${value.tokens.find(t=>t.id===v.tokenId)?.name??''}${v.amount===null?'':` · ${new Intl.NumberFormat(window.ChronexusI18n.locale).format(v.amount)}`}${v.dice.length?` [${v.dice.map(fmt).join(', ')}]`:''}${v.label?' · '+(conditions.includes(v.label)?t('condition.'+v.label):v.label):''}`))));
    }
    return{root,update};
  }
  window.ChronexusCombat={create};
})();
