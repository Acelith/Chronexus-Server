const {test}=require('node:test');
const assert=require('node:assert/strict');
const {readFileSync}=require('node:fs');
const {join}=require('node:path');
const {JSDOM}=require('jsdom');
const root=join(__dirname,'../web');
function fixture(locale='en',role='gm'){
  const dom=new JSDOM('<html><body></body></html>',{url:'http://localhost/',runScripts:'outside-only'}),w=dom.window;
  w.localStorage.setItem('chronexus-web-locale',locale);w.eval(readFileSync(join(root,'i18n.js'),'utf8'));w.eval(readFileSync(join(root,'board.js'),'utf8'));
  w.HTMLCanvasElement.prototype.getContext=()=>new Proxy({}, {get:(_,key)=>()=>{},set:()=>true});
  const el=(tag,attrs={},...children)=>{const e=w.document.createElement(tag);for(const[k,v]of Object.entries(attrs)){if(v===undefined||v===null||v===false)continue;if(k==='text')e.textContent=v;else if(k.startsWith('on'))e.addEventListener(k.slice(2),v);else e.setAttribute(k,v===true?'':v);}for(const c of children.flat())if(c!==undefined&&c!==null&&c!==false)e.append(c instanceof w.Node?c:w.document.createTextNode(String(c)));return e;};
  const s={id:'s',name:'Untranslated scene',width:1000,height:700,mapId:null,grid:{kind:'square',pixels:50,offset:{x:0,y:0},color:'#cab582',opacity:.4,distance:{value:5,unit:'ft'},diagonal:'equal',conversion:'table'},freeMove:true,difficult:[]};
  const state={revision:5,activeScene:'s',scenes:[s],templates:[],tokens:[{id:'p',name:'Hero <script>',sceneId:'s',ownerId:'self',position:{x:1,y:1},speed:{value:30,unit:'ft'},size:1,spent:0,diagonals:0,hidden:false}]},requests=[];
  const board=w.ChronexusBoard.create({el,t:w.ChronexusI18n.t,api:async(method,path,body)=>{requests.push({method,path,body});return method==='GET'?state:{revision:6};},tableId:'table',role,account:'self',seats:[{accountId:'self',role:'player',displayName:'Player'}],confirm:async()=>true,active:()=>true});
  w.document.body.append(board.root);board.update(state);board.connected(true);return{dom,w,board,state,requests};
}
const settle=()=>new Promise(resolve=>setTimeout(resolve,20));
test('both conversion conventions, all canonical values and all ten decimal locales',()=>{
  for(const locale of ['en','it','de','fr','es','pt-BR','ja','ko','zh-Hans','ar']){
    const f=fixture(locale);try{
      for(const convention of ['table','exact'])for(const value of [0,5,10,15,30,60,120,5280]){
        const maths=f.w.ChronexusBoard,m=maths.convert(value,'ft','m',convention);
        assert.ok(Math.abs(maths.convert(m,'m','ft',convention)-value)<1e-9);
        f.state.scenes[0].grid.conversion=convention;f.board.update(f.state);
        const formatted=f.board.distance(value,'ft');assert.ok(formatted.includes(new Intl.NumberFormat(locale,{maximumFractionDigits:2}).format(m)),locale+': '+formatted);
      }
      assert.equal(f.w.document.documentElement.dir,locale==='ar'?'rtl':'ltr');
      for(const label of f.w.document.querySelectorAll('label'))assert.ok(label.textContent.trim());
      assert.equal(f.w.document.querySelector('script'),null);
    }finally{f.board.dispose();f.dom.window.close();}
  }
});
test('square and both hex grids round-trip, and alternating cost keeps parity across drags',()=>{
  const f=fixture();try{const m=f.w.ChronexusBoard,g=f.state.scenes[0].grid;
    for(const kind of ['square','hexFlat','hexPointy'])for(const p of [{x:0,y:0},{x:2,y:3},{x:-1,y:4}]){g.kind=kind;const actual=m.snap(m.pixel(p,g),g);assert.equal(actual.x,p.x);assert.equal(actual.y,p.y);}
    g.kind='square';g.diagonal='alternating';assert.equal(m.cost({x:0,y:0},[{x:1,y:1}],f.state.scenes[0],1),10);
    const area={origin:{x:2,y:2},shape:'circle',size:{value:5,unit:'ft'},angle:0,coverage:'center'};assert.equal(m.templateContains({x:3,y:2},area,g),true);assert.equal(m.templateContains({x:4,y:2},area,g),false);
  }finally{f.dom.window.close();}
});
test('a player can move through keyboard or position fields; disconnection freezes writes',async()=>{
  const f=fixture('it','player');try{assert.equal(f.w.document.querySelector('details'),null);
    f.w.document.querySelector('.token-roster button').click();const canvas=f.w.document.querySelector('canvas');
    canvas.dispatchEvent(new f.w.KeyboardEvent('keydown',{key:'ArrowRight',cancelable:true}));await settle();
    assert.equal(f.requests[0].body.command.type,'move');assert.equal(f.requests[0].body.command.path[0].x,2);
    assert.equal(f.requests[0].body.baseRevision,5);
    f.board.connected(false);const before=f.requests.length;canvas.dispatchEvent(new f.w.KeyboardEvent('keydown',{key:'ArrowRight',cancelable:true}));await settle();assert.equal(f.requests.length,before);
    assert.match(f.w.document.querySelector('[role="status"]').textContent,/Connessione persa/);
  }finally{f.dom.window.close();}
});
test('newer state wins and untrusted token names stay text',()=>{
  const f=fixture();try{const newer=structuredClone(f.state);newer.revision=8;newer.tokens[0].name='<img src=x onerror=alert(1)>';f.board.update(newer);f.board.update(f.state);assert.equal(f.board.snapshot.revision,8);assert.equal(f.w.document.querySelector('.token-roster img'),null);assert.match(f.w.document.querySelector('.token-roster').textContent,/<img/);}finally{f.dom.window.close();}
});
test('every static board label is registered; enum labels exist in all ten catalogs',()=>{
  const f=fixture();try{const source=readFileSync(join(root,'board.js'),'utf8');const keys=[...source.matchAll(/\b(?:t|button|field)\(\s*['"]([^'"]+)['"]/g)].map(m=>m[1]);for(const key of keys){if(key.endsWith('.'))continue;assert.equal(f.w.ChronexusI18n.has(key),true,key);}assert.deepEqual(Array.from(f.w.ChronexusI18n.missing()),[]);}finally{f.dom.window.close();}
});
