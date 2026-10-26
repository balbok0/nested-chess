import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { randomUUID } from 'node:crypto';

const [origin='http://localhost:8080',project='nested-chess']=process.argv.slice(2);
const compose=(...args)=>execFileSync('docker',['compose','-p',project,...args],{stdio:['ignore','pipe','pipe'],timeout:90_000});
const waitFor=async check=>{
  const deadline=Date.now()+20_000;
  while(Date.now()<deadline) {try {if(await check())return;}catch {} await new Promise(resolve=>setTimeout(resolve,200));}
  throw new Error('Timed out waiting for recovery');
};
async function request(path,token,body,method=body===undefined?'GET':'POST') {
  const response=await fetch(origin+path,{method,headers:{...(token?{Authorization:`Bearer ${token}`}:{}),...(body===undefined?{}:{'Content-Type':'application/json'})},body:body===undefined?undefined:JSON.stringify(body),signal:AbortSignal.timeout(10_000)}).catch(error=>{throw new Error(`${method} ${path} failed`,{cause:error});});
  const data=await response.json();
  return {status:response.status,data,replica:response.headers.get('x-nested-replica')};
}
const expect=async(...args)=>{const result=await request(...args);assert.equal(result.status,200,JSON.stringify(result.data));return result;};
async function stream(id,token) {
  const {data}=await expect(`/api/v1/mobile/games/${id}/ticket`,token,{},'POST');
  const socket=new WebSocket(`${origin.replace(/^http/,'ws')}/api/v1/mobile/games/${id}/stream?ticket=${data.ticket}`);
  const messages=[];let closed=false;
  socket.addEventListener('message',event=>messages.push(JSON.parse(event.data)));
  socket.addEventListener('close',()=>{closed=true;});
  socket.addEventListener('error',()=>{closed=true;});
  return {socket,async seq(seq) {await waitFor(()=>{assert.equal(closed,false,'WebSocket closed');return messages.some(m=>(m.snapshot?.info.seq??m.event?.seq)===seq);});}};
}
const replicas=new Set();
for(let i=0;i<6;i++)replicas.add((await expect('/api/v1/health')).replica);
assert.deepEqual([...replicas].sort(),['app-a','app-b']);
const a=(await expect('/api/v1/mobile/guest',null,{})).data;
const b=(await expect('/api/v1/mobile/guest',null,{})).data;
const created=(await expect('/api/v1/games',a.token,{clock:{seconds:900,increment:10}})).data;
const id=created.info.id;
const joined=(await expect(`/api/v1/games/${id}/join`,b.token,{})).data;
const [white,black]=joined.info.white.id===a.user.id?[a,b]:[b,a];
let watch=await stream(id,black.token);await watch.seq(1);
const first={request_id:randomUUID(),expected_seq:1,action:{kind:'move',board:0,uci:'e2e4'}};
const accepted=(await expect(`/api/v1/games/${id}/commands`,white.token,first)).data;
await watch.seq(2);
assert.deepEqual((await expect(`/api/v1/games/${id}/commands`,white.token,first)).data,accepted);
execFileSync('docker',['compose','-p',project,'exec','-T','db','psql','-U','nested','-d','nested','-c',"SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE usename='nested' AND query LIKE 'LISTEN%' AND pid<>pg_backend_pid()"],{stdio:'pipe'});
const moves=await Promise.all(['e7e5','d7d5'].map(uci=>request(`/api/v1/games/${id}/commands`,black.token,{request_id:randomUUID(),expected_seq:2,action:{kind:'move',board:1,uci}})));
assert.deepEqual(moves.map(m=>m.status).sort(),[200,409]);
await watch.seq(3);
console.log('Round-robin routing, retries, move races and listener recovery passed.');
watch.socket.close();
compose('stop','--timeout','0','app-a');
await waitFor(async()=>{const result=await expect(`/api/v1/games/${id}`,white.token);return result.replica==='app-b'&&result.data.info.seq===3;});
const before=(await expect(`/api/v1/games/${id}`,white.token)).data;
compose('stop','--timeout','0','app-b');
compose('up','--no-deps','-d','--wait','app-a','app-b');
await waitFor(async()=>{const result=await request('/api/v1/ready');return result.status===200;});
const after=(await expect(`/api/v1/games/${id}`,white.token)).data;
assert.equal(after.info.status,'active');assert.equal(after.info.seq,3);assert.equal(after.state.ply,2);
assert.equal(after.info.running_since_ms,before.info.running_since_ms);
assert.ok(after.info.server_ms>before.info.server_ms);
assert.equal(after.save,before.save);
console.log('Hard stops and all-replica restart preserve positions, sessions and running clocks.');
watch=await stream(id,black.token);await watch.seq(3);
compose('stop','db');
await waitFor(async()=>{const result=await fetch(origin+'/');return result.status===200;});
const html=await (await fetch(origin+'/')).text();
const wasm=html.match(/src="([^"]+\.js)"/);
assert.ok(wasm,'Static bundle is served without PostgreSQL');
compose('start','db');
await waitFor(async()=>(await request('/api/v1/ready')).status===200);
const resigned=(await expect(`/api/v1/games/${id}/commands`,white.token,{request_id:randomUUID(),expected_seq:3,action:{kind:'resign'}})).data;
assert.equal(resigned.info.result,'black');await watch.seq(4);watch.socket.close();
assert.equal((await expect(`/api/v1/games/${id}`,black.token)).data.info.seq,4);
console.log('Database restart and cross-replica event catch-up passed.');
